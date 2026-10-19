//! The event loop: a window, a swapchain, and frames going onto a screen.
//!
//! This is the impure half of the crate. Everything it can hand to a pure
//! function it does — [`PointerTranslator`], [`Lifecycle`], [`Scale`] and
//! [`FrameLog`] are all tested without a window — and what is left here is the
//! wiring that genuinely needs one.

use std::mem::ManuallyDrop;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use accesskit_winit::Event as AccessEvent;
// Android reaches AccessKit through `a11y_android` instead, so the winit
// adapter is not merely unused there — it is the one that cannot work.
#[cfg(not(target_os = "android"))]
use accesskit_winit::Adapter as AccessAdapter;
use vieww_foundation::crash::{CrashContext, Crashes};
use vieww_foundation::{Color, FileDrag, ImeEvent, PointerButton, PointerEvent, Size, ViewMetrics};
use vieww_paint::FrameScheduler;
use vieww_render::element::Runtime;
use vieww_render::{next_action, FrameDriver, LoopAction, WindowKey};
use winit::application::ApplicationHandler;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
use winit::dpi::LogicalSize;
use winit::dpi::{PhysicalPosition, PhysicalSize};
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy};
#[cfg(target_os = "android")]
use winit::platform::android::activity::AndroidApp;
use winit::window::{Window, WindowId};

use crate::input::{PointerTranslator, WheelDelta};
use crate::insets::{InsetWatch, SystemInsets};
use crate::keys::KeyTranslator;
use crate::lifecycle::Lifecycle;
use crate::native::{NativeError, NativeRenderer, NativeSurface, PresentPath};
use crate::perf_hint::PerfHint;
use crate::rasterizer_choice::{Calibration, Launch, Measuring, Rasterizer};
use crate::scale::Scale;
use crate::stats::{FrameLog, FrameReport};
use crate::windows::{PlatformWindows, Requests};

/// Why an application could not run.
///
/// The two platform failures carry their reason as text rather than as winit's
/// own error types. Holding `winit::error::EventLoopError` here would make winit's
/// version part of this crate's contract for anyone who so much as matches on the
/// error — and neither type is one a caller can branch on usefully. [`Gpu`] keeps
/// its payload because [`NativeError`] is ours, and its variants *are* worth
/// matching on ([`NativeError::NoAdapter`] is how a headless runner is
/// recognised — the same role `vieww_paint::gpu::GpuError::NoAdapter` used to
/// play before this crate's renderer moved off vello).
///
/// [`Gpu`]: PlatformError::Gpu
#[derive(Debug)]
pub enum PlatformError {
    /// The platform would not give us an event loop. On Android this means the
    /// activity is not attached.
    EventLoop(String),
    /// The platform would not give us a window.
    Window(String),
    /// The window exists but cannot be drawn into.
    Gpu(NativeError),
}

impl std::fmt::Display for PlatformError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EventLoop(error) => write!(f, "creating an event loop: {error}"),
            Self::Window(error) => write!(f, "creating a window: {error}"),
            Self::Gpu(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for PlatformError {}

/// Everything that can be delivered to the loop from outside it.
///
/// It was `accesskit_winit::Event` alone until a `Task` needed to wake the loop
/// from a worker thread. `winit` has no bare "wake up" on an `EventLoopProxy` —
/// the only way in is a user event — so the type had to widen to hold both.
///
/// `Adapter::with_event_loop_proxy` is generic over `T: From<Event>`, which is
/// what makes that widening possible without wrapping the adapter.
#[derive(Debug)]
enum UserEvent {
    /// A screen reader attaching, or asking for something.
    Access(AccessEvent),
    /// The same, from Android's injected delegate rather than from
    /// `accesskit_winit`. A separate variant because it is a different type
    /// carrying the same two questions — see [`crate::a11y_android`] for why
    /// Android does not go through the winit adapter at all.
    #[cfg(target_os = "android")]
    AndroidAccess(crate::a11y_android::Request),
    /// Off-thread work finished and there may be nothing else asking for a
    /// frame. See [`Waker`].
    Wake,
}

impl From<AccessEvent> for UserEvent {
    fn from(event: AccessEvent) -> Self {
        Self::Access(event)
    }
}

#[cfg(target_os = "android")]
impl From<crate::a11y_android::Request> for UserEvent {
    fn from(request: crate::a11y_android::Request) -> Self {
        Self::AndroidAccess(request)
    }
}

/// Whichever AccessKit adapter this platform can actually use.
///
/// Aliased rather than branched at every call site: `update_if_active` and
/// `process_event` have the same shape on both, so only construction differs.
#[cfg(target_os = "android")]
type PlatformA11y = crate::a11y_android::AndroidAdapter;
#[cfg(not(target_os = "android"))]
type PlatformA11y = AccessAdapter;

/// Asks a running event loop for a frame, from any thread.
///
/// [`App::waker`] hands one out *before* the loop exists, because that is when
/// an application needs it: the waker has to be in hand to be passed into the
/// tree that `run`'s build closure creates. So it starts empty and is filled in
/// when the loop is built.
///
/// A wake before then is dropped, and correctly — there is no window, no
/// surface and no tree, so there is no frame to ask for. The first frame draws
/// everything anyway.
#[derive(Clone, Default)]
pub struct Waker {
    proxy: Arc<Mutex<Option<EventLoopProxy<UserEvent>>>>,
}

impl std::fmt::Debug for Waker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Waker")
            .field("attached", &self.locked().is_some())
            .finish()
    }
}

impl Waker {
    /// The proxy, with poisoning ignored — see `CrashContext::lock` for the
    /// same reasoning. A panicking thread must not also cost the application
    /// its ability to wake up.
    fn locked(&self) -> std::sync::MutexGuard<'_, Option<EventLoopProxy<UserEvent>>> {
        self.proxy
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
    }

    /// Point this at a loop that now exists.
    fn attach(&self, proxy: EventLoopProxy<UserEvent>) {
        *self.locked() = Some(proxy);
    }
}

impl vieww_foundation::task::FrameWaker for Waker {
    fn wake(&self) {
        if let Some(proxy) = self.locked().as_ref() {
            // An error means the loop has already stopped, which is not a
            // failure: the application is closing and the frame this would have
            // asked for is one nobody will see.
            let _ = proxy.send_event(UserEvent::Wake);
        }
    }
}

/// A vieww cursor as winit's.
///
/// Exhaustive over the closed list [`Cursor`](vieww_foundation::Cursor) defines,
/// so adding a shape there is a compile error here rather than a silent arrow.
/// The names line up because both follow CSS — see `Cursor::css_name`.
const fn cursor_icon(cursor: vieww_foundation::Cursor) -> winit::window::CursorIcon {
    use vieww_foundation::Cursor as C;
    use winit::window::CursorIcon as W;
    match cursor {
        C::Default => W::Default,
        C::Text => W::Text,
        C::Pointer => W::Pointer,
        C::Move => W::Move,
        C::ResizeColumn => W::ColResize,
        C::ResizeRow => W::RowResize,
        C::Wait => W::Wait,
        C::NotAllowed => W::NotAllowed,
        C::Crosshair => W::Crosshair,
        C::Grab => W::Grab,
        C::Grabbing => W::Grabbing,
        // `Cursor` is `#[non_exhaustive]`, so this arm is required from outside
        // its own crate. The arrow is the safe answer for a shape this backend
        // has not been taught yet.
        _ => W::Default,
    }
}

/// When the event loop should stop.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum Stop {
    /// When the window is closed. What an application wants.
    #[default]
    OnClose,
    /// After this many frames have reached the screen. What a measurement
    /// wants: an unattended run that ends by itself and reports.
    AfterFrames(u64),
}

/// What [`App::on_frame`] stores.
///
/// Named rather than written inline because `Option<Box<dyn FnMut(&FrameLog)>>`
/// is past `clippy::type_complexity`, and because the thing deserved a name: it
/// is the only per-frame door into a running application.
type FrameHook = Box<dyn FnMut(&FrameLog)>;

/// What [`App::after_frame`] stores.
///
/// Takes the driver rather than a narrower view because the point of running
/// *after* a frame is to read what the frame produced — the semantics tree, the
/// layout, the damage — and which of those a caller wants is not this crate's
/// business to guess.
type AfterFrameHook = Box<dyn FnMut(&FrameDriver)>;

/// What [`App::before_frame`] stores.
///
/// The only hook that takes the driver **mutably**, because it is the only one
/// whose job is to change the tree rather than read it — see `App::before_frame`
/// for why hot reload needs that and the other two deliberately do not have it.
type BeforeFrameHook = Box<dyn FnMut(&mut FrameDriver)>;

/// Asked whether a window may close, and told how big it is when it does.
///
/// See [`App::on_close_request`]. The size is logical, not physical, because it
/// is a size an application will hand back to [`App::size`] on the next launch
/// and that one is logical too.
type CloseHook = Box<dyn FnMut(CloseRequest) -> Closing>;

/// What the platform knows about a window that is being asked to close.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CloseRequest {
    /// The window's logical width, for an application that restores geometry.
    pub width: f32,
    /// The window's logical height.
    pub height: f32,
    /// `true` when this is the application's last window, and so when letting
    /// it go ends the process. An editor with unsaved work wants to stop this
    /// one and can usually let the others go.
    pub last: bool,
}

/// The answer to a [`CloseRequest`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Closing {
    /// Let the window go. The default, and what happens with no hook installed.
    #[default]
    Allowed,
    /// Keep the window. The application is expected to have put something on
    /// screen explaining why — a hook that vetoes silently is a window whose
    /// close button appears broken.
    Vetoed,
}

/// An application: a window, and a widget tree inside it.
///
/// ```no_run
/// use vieww_foundation::Color;
/// use vieww_platform_winit::App;
/// use vieww_widget::prelude::*;
///
/// App::new()
///     .title("hello")
///     .run(|driver| {
///         driver.elements().set_root(ColoredBox::new(Color::BLUE));
///     })
///     .unwrap();
/// ```
pub struct App {
    title: String,
    size: Size,
    background: Color,
    /// The theme published above the application's tree — see [`App::theme`].
    theme: Option<std::rc::Rc<vieww_widget::ThemeData>>,
    refresh_hz: f32,
    /// Whether [`App::refresh_rate`] was called. Detection defers to it.
    refresh_stated: bool,
    /// Whether each frame is planned and, when the plan is complete,
    /// rasterised by the GPU — see [`App::prefer_gpu`].
    prefer_gpu: bool,
    /// Measure both rasterizers on this device and keep the faster — see
    /// [`App::auto_rasterizer`]. Overrides `prefer_gpu` when set.
    auto_rasterizer: bool,
    stop: Stop,
    /// Called once per frame each, in the order they were added. See
    /// [`App::on_frame`].
    on_frame: Vec<FrameHook>,
    /// [`App::after_frame`].
    after_frame: Vec<AfterFrameHook>,
    /// Installed in `run_on`, not here — a panic hook is process-wide, and an
    /// `App` that was built and then dropped without running must not have
    /// changed the process. See [`App::report_crashes`].
    crashes: Option<Crashes>,
    /// Handed out by [`App::waker`] and attached to the loop in `run_on`.
    waker: Waker,
    /// Set only by [`App::run_android`], and taken out again in `run_on`. It
    /// rides along on the config because `run_on` is shared with every other
    /// platform and must not grow a parameter that only one of them can supply.
    #[cfg(target_os = "android")]
    android: Option<AndroidApp>,
    /// Asked before a window closes. See [`App::on_close_request`].
    on_close: Option<CloseHook>,
    /// Run before each frame is built, able to change the tree.
    ///
    /// The one hook that takes `&mut FrameDriver`. See [`App::before_frame`].
    before_frame: Vec<BeforeFrameHook>,
    /// The reactive graph **every** window's driver shares.
    ///
    /// Created here rather than inside the first `FrameDriver`, because with
    /// more than one window it can no longer belong to any of them: a document
    /// open in two windows has to be one document, which means one graph. It is
    /// also what `open_windows` is minted from, and a signal from a different
    /// graph than the trees reading it marks nothing pending — the window menu
    /// would never rebuild and every test of it would pass.
    runtime: Runtime,
    /// Handed out by [`App::windows`] and drained by the runner, exactly as
    /// [`waker`](Self::waker) is handed out and attached.
    windows: PlatformWindows,
}

/// Hand-written, and `Clone` is gone, because a hook is a closure: closures are
/// neither, and an `App` that could be cloned would have handed two windows the
/// same hook expecting one of them to own it.
impl std::fmt::Debug for App {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("App")
            .field("title", &self.title)
            .field("size", &self.size)
            .field("background", &self.background)
            .field("refresh_hz", &self.refresh_hz)
            .field("refresh_stated", &self.refresh_stated)
            .field("stop", &self.stop)
            .field("on_frame", &self.on_frame.len())
            .field("after_frame", &self.after_frame.len())
            .field("crashes", &self.crashes)
            .finish_non_exhaustive()
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

impl App {
    /// An 800x600 window at 60Hz.
    #[must_use]
    pub fn new() -> Self {
        let waker = Waker::default();
        let runtime = Runtime::new();
        // Cloning the waker shares it: the copy the queue holds and the copy
        // `run_on` attaches to the loop are the same handle, so a window asked
        // for from a menu wakes a loop that is asleep.
        let windows = PlatformWindows::new(&runtime, waker.clone());
        Self {
            title: "vieww".to_owned(),
            size: Size::new(800.0, 600.0),
            background: Color::WHITE,
            theme: None,
            refresh_hz: 60.0,
            refresh_stated: false,
            prefer_gpu: false,
            auto_rasterizer: false,
            stop: Stop::OnClose,
            on_frame: Vec::new(),
            after_frame: Vec::new(),
            crashes: None,
            waker,
            #[cfg(target_os = "android")]
            android: None,
            before_frame: Vec::new(),
            on_close: None,
            runtime,
            windows,
        }
    }

    /// The [`Windows`](vieww_render::Windows) service this application will be
    /// driven by, available **before** the loop exists.
    ///
    /// ```no_run
    /// # use vieww_foundation::SharedServices;
    /// # use vieww_platform_winit::{services, App};
    /// # use vieww_render::Windows;
    /// let app = App::new();
    /// let mut registry = services::platform();
    /// registry.provide::<dyn Windows>(app.windows());
    /// ```
    ///
    /// The same shape as [`waker`](Self::waker), and for the same reason: the
    /// service goes into the tree, and `run` consumes the `App`, so a builder
    /// method would hand it back only after it was too late to use.
    ///
    /// On Android and iOS the handle exists and **refuses** — an application
    /// does not own its windows there, the activity or the scene is the window.
    /// A tree that hides its "New Window" item when the request comes back
    /// unsupported therefore behaves correctly on a phone with no code of its
    /// own.
    #[must_use]
    pub fn windows(&self) -> Rc<PlatformWindows> {
        // Cloning shares the queue rather than copying it — `PlatformWindows`
        // is a handle around one `Rc<Requests>`.
        Rc::new(self.windows.clone())
    }

    /// The window's title.
    #[must_use]
    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = title.into();
        self
    }

    /// The window's size, in logical pixels.
    ///
    /// A request, not a guarantee: a tiling window manager, a phone, and a
    /// maximised window all ignore it, which is why nothing downstream reads
    /// this and everything reads the size the window reports back.
    #[must_use]
    pub const fn size(mut self, size: Size) -> Self {
        self.size = size;
        self
    }

    /// What the window is before anything is drawn on it.
    ///
    /// Prefer [`theme`](Self::theme), which sets this *and* the theme the
    /// widgets read, from one value. Setting only this is how a window ends up
    /// painted for one theme and populated for another — see [`theme`](Self::theme).
    #[must_use]
    pub const fn background(mut self, color: Color) -> Self {
        self.background = color;
        self
    }

    /// The theme the application draws in: the window's background **and** the
    /// theme every widget in the tree reads, from one value.
    ///
    /// # Why this exists
    ///
    /// These were two settings and nothing tied them together.
    /// [`background`](Self::background) painted the window, and the widget tree
    /// got whatever `Theme` the application remembered to mount — with
    /// `ThemeData::of` falling back to `ThemeData::light()` when it mounted
    /// none.
    ///
    /// The project template set `background(ThemeData::dark().colors.surface)`
    /// and mounted no `Theme`. So a scaffolded application ran with a **dark
    /// window** and **light-themed widgets**: body text in near-black ink on a
    /// near-black ground, invisible, next to a filled button drawing its own
    /// accent — which was, literally, the only thing visible on the screen.
    ///
    /// And it passed review, because vieww Studio's preview wraps the screen in
    /// a `Theme` of its own. The preview was right and the built application
    /// was wrong, which is the one failure a preview exists to prevent.
    ///
    /// One value, both consumers:
    ///
    /// ```ignore
    /// App::new().title("my-app").theme(ThemeData::dark())
    /// ```
    ///
    /// Changing it moves the window and the widgets together, which is what the
    /// template's own comment already claimed — "so the window's background
    /// cannot drift from what the widgets draw on" — and could not deliver with
    /// two settings.
    ///
    /// A `Theme` *inside* the tree still wins, so a dark section of a light
    /// screen works exactly as before.
    #[must_use]
    pub fn theme(mut self, theme: impl Into<std::rc::Rc<vieww_widget::ThemeData>>) -> Self {
        let theme = theme.into();
        self.background = theme.colors.surface;
        self.theme = Some(theme);
        self
    }

    /// The display's refresh rate, for the frame budget.
    ///
    /// Only the budget: pacing comes from the swapchain, which blocks on vsync
    /// whatever this says. Setting it wrong mis-states the jank count and
    /// nothing else.
    ///
    /// # Panics
    ///
    /// If `hz` is not finite and positive.
    #[must_use]
    /// Setting this **turns display detection off.** An application that states
    /// a rate means it, and a value quietly replaced by whatever panel the
    /// window landed on would be a setting that works until somebody plugs in a
    /// monitor.
    pub fn refresh_rate(mut self, hz: f32) -> Self {
        assert!(hz.is_finite() && hz > 0.0, "refresh rate must be positive");
        self.refresh_hz = hz;
        self.refresh_stated = true;
        self
    }

    /// Plan every frame, and rasterise on the GPU when the plan is complete.
    ///
    /// Off by default, because the CPU rasterizer is this framework's own
    /// and its quality is the bar; the GPU path is the one `vieww-gpu`
    /// plans and `vieww-hal`'s `SceneRenderer` executes — one batched draw
    /// for a whole frame, edges covered analytically. A frame the planner
    /// cannot express falls back to the CPU rasterizer in full, per frame:
    /// turning this on never turns the CPU path off, it only makes the GPU
    /// available to the frames it can draw.
    ///
    /// What it costs today: the GPU's frame is read back into host memory
    /// and uploaded like the CPU's would be, and there is no damage-driven
    /// repaint — every GPU frame is a full one. See
    /// [`NativeRenderer::present_planned`](crate::native::NativeRenderer::present_planned)
    /// for the shape and the follow-up that makes it fast rather than
    /// merely correct.
    #[must_use]
    pub const fn prefer_gpu(mut self) -> Self {
        self.prefer_gpu = true;
        self
    }

    /// Choose the CPU or the GPU rasterizer by measuring them on this device,
    /// instead of deciding in advance with [`prefer_gpu`](Self::prefer_gpu).
    ///
    /// The first launch runs the CPU path and measures it; if it fits
    /// comfortably in the frame budget it is kept and no GPU driver is ever
    /// loaded. Otherwise the next launch measures the GPU path, and the GPU is
    /// kept only if it is clearly faster. The answer is kept in a small file in
    /// the application's data directory and reused until the build, the OS or
    /// the window size changes. See [`crate::rasterizer_choice`] for the rules
    /// and the Device Farm numbers they were checked against, and
    /// [`rasterizer_choice`](crate::rasterizer_choice()) for what was chosen.
    #[must_use]
    pub const fn auto_rasterizer(mut self) -> Self {
        self.auto_rasterizer = true;
        self
    }

    /// Close the window after this many frames have reached the screen.
    ///
    /// For measuring rather than for shipping: it makes a run finite, so a test
    /// or a benchmark can open a window, drive it, and report without a human
    /// closing anything.
    #[must_use]
    pub const fn exit_after_frames(mut self, frames: u64) -> Self {
        self.stop = Stop::AfterFrames(frames);
        self
    }

    /// Run `hook` once per frame, with what the frames so far have cost.
    ///
    /// **Additive.** Calling it twice installs two hooks, run in the order they
    /// were added, because a diagnostic and a test harness both want this and
    /// neither should have to know about the other. A setter that silently
    /// replaced the first one would fail by *quietly doing less*, which is the
    /// worst way for a diagnostic to fail.
    ///
    /// The counterpart to `build`, which runs exactly once: this is the per-frame
    /// door into a running application, and it exists because
    /// `vieww_widget::PerformanceOverlay` had no way in. The overlay takes a
    /// `Vec<Duration>`, [`FrameLog::work_samples`] produces one, and nothing
    /// joined them — the driver is only handed out at startup. (Named in
    /// backticks rather than linked for the reason `FrameLog` itself does it:
    /// `vieww-widget` is a dev-dependency here, so the link would not resolve.)
    ///
    /// ```no_run
    /// # use std::time::Duration;
    /// # use vieww_platform_winit::App;
    /// # use vieww_element::Signal;
    /// # fn wire(samples: Signal<Vec<Duration>>) {
    /// App::new()
    ///     .on_frame(move |log| samples.set(log.work_samples()))
    ///     .run(|_driver| { /* a tree reading `samples` */ })
    ///     .unwrap();
    /// # }
    /// ```
    ///
    /// # It runs *before* the build phase, and that is the whole design
    ///
    /// A hook after the frame would write its signal into a tree that has
    /// already been built, so nothing would be on screen until something else
    /// asked for another frame — and the obvious fix, requesting one, is a loop
    /// that never sleeps. Running first means the write lands in the frame that
    /// is about to be built, at the cost of the samples being the *previous*
    /// frames'. An overlay is a report on frames that have happened; it cannot
    /// be anything else.
    ///
    /// # It does not manufacture frames
    ///
    /// A hook that writes a signal marks an element pending and so produces the
    /// frame it was called for, but nothing schedules the next one. An idle
    /// window stays idle with a hook installed, which is the point: an
    /// instrument that pegged the app at 60Hz would be measuring itself. To
    /// measure a frame rate, give the tree something that genuinely animates —
    /// a repeating `vieww_element::Animation` — and the hook then runs on every
    /// frame that animation asks for.
    ///
    /// Called on the UI thread, between frames, so writing signals is allowed
    /// for the reason a gesture handler's writes are: it is not inside a build.
    #[must_use]
    pub fn on_frame<H>(mut self, hook: H) -> Self
    where
        H: FnMut(&FrameLog) + 'static,
    {
        self.on_frame.push(Box::new(hook));
        self
    }

    /// Run `hook` after each presented frame, with the driver.
    ///
    /// The counterpart to [`on_frame`](Self::on_frame), and the difference is the
    /// whole reason both exist. `on_frame` runs *before* the build so what it
    /// writes appears in the frame about to be drawn; this runs *after* the
    /// present, so what it reads is what is on the screen — a laid-out tree, a
    /// settled semantics tree, real geometry.
    ///
    /// # What it is for
    ///
    /// Reading the application's own layout, which nothing else could do. An
    /// inspector, a screenshot annotator, or a test harness that needs to know
    /// **where a widget actually ended up** rather than being told a coordinate by
    /// somebody who guessed.
    ///
    /// That last one is not hypothetical: `ci/mobile/device-suite.sh` tapped
    /// `190,536` — correct for one 1080x2340 phone at 2.75x and silently wrong
    /// everywhere else, with a miss showing up as a check *absent* from the report
    /// rather than failing. A cross-platform framework cannot verify itself with a
    /// constant that only suits the author's handset. This is the door that lets
    /// the app answer the question instead.
    ///
    /// # Cost
    ///
    /// Nothing when no hook is registered. Note that
    /// [`FrameDriver::semantics`](vieww_render::FrameDriver::semantics) *builds* a
    /// tree each call rather than returning a cached one, so a hook that wants it
    /// every frame is paying for it every frame — ask on the frames you care
    /// about.
    ///
    /// ```no_run
    /// # use vieww_platform_winit::App;
    /// App::new()
    ///     .after_frame(|driver| {
    ///         for node in driver.semantics().nodes() {
    ///             let _where_it_is = node.bounds;
    ///         }
    ///     })
    ///     .run(|_driver| {})
    ///     .unwrap();
    /// ```
    #[must_use]
    pub fn after_frame<H>(mut self, hook: H) -> Self
    where
        H: FnMut(&FrameDriver) + 'static,
    {
        self.after_frame.push(Box::new(hook));
        self
    }

    /// Run `hook` immediately before each frame is built, with the driver
    /// **mutably**.
    ///
    /// The other two frame hooks deliberately cannot change anything:
    /// [`on_frame`](Self::on_frame) sees the log, [`after_frame`](Self::after_frame)
    /// sees a settled tree. This one is for the case that has to *replace* the
    /// tree, and hot reload is the reason it exists — a new build of the
    /// application arrives between frames and swaps the root.
    ///
    /// Before the build rather than after the present, because a root swapped
    /// after a frame would leave the screen showing the previous build until
    /// something else asked for another one.
    ///
    /// `vieww-reload` is the intended caller — its `Reloader::poll` answers
    /// with a new root, and this hook is where that root lands:
    ///
    /// ```text
    /// App::new().before_frame(move |driver| match reloader.poll() {
    ///     Reloaded::Root(root)    => driver.set_root(root),   // keep the tree
    ///     Reloaded::Restart(root) => driver.remount(root),    // and this drops it
    ///     Reloaded::Failed(error) => eprintln!("keeping the running build: {error}"),
    ///     Reloaded::Nothing       => {}
    /// })
    /// ```
    ///
    /// Written as text rather than a doctest on purpose: `vieww-reload` depends
    /// on this crate's neighbours, and adding it here to compile six lines of
    /// documentation would be a dependency edge that exists for a doc comment.
    #[must_use]
    pub fn before_frame<H>(mut self, hook: H) -> Self
    where
        H: FnMut(&mut FrameDriver) + 'static,
    {
        self.before_frame.push(Box::new(hook));
        self
    }

    /// Ask before a window closes, and veto it.
    ///
    /// # Why the platform has to own this
    ///
    /// Everything else an application does about closing happens *after* the
    /// window is gone. `WindowEvent::CloseRequested` reaches winit, the window
    /// is destroyed, and if it was the last one the loop exits — by the time
    /// any application code could look, there is nothing to keep. An editor
    /// with unsaved buffers therefore had no way to say "wait": the studio in
    /// this repository lost modified files to the window's close button with no
    /// dialog and no recovery file, and no amount of application-side code
    /// could have stopped it.
    ///
    /// The hook returns [`Closing::Vetoed`] to keep the window. It is called
    /// **before** anything is destroyed, and a veto leaves the window exactly
    /// as it was, so the next frame can draw a dialog over it.
    ///
    /// # The obligation a veto takes on
    ///
    /// A hook that vetoes must put something on screen, and must eventually
    /// stop vetoing. Neither can be enforced here — a window that refuses to
    /// close and says nothing is indistinguishable, from this layer, from one
    /// that is waiting for an answer. The pattern that works is a flag: veto
    /// once, raise the dialog, and let the dialog's own "close anyway" set the
    /// flag the hook checks first.
    ///
    /// ```no_run
    /// # use vieww_platform_winit::{App, Closing};
    /// # let confirmed = std::rc::Rc::new(std::cell::Cell::new(false));
    /// # let flag = confirmed.clone();
    /// App::new().on_close_request(move |request| {
    ///     if flag.get() || !request.last {
    ///         return Closing::Allowed;
    ///     }
    ///     // …raise the dialog…
    ///     Closing::Vetoed
    /// });
    /// ```
    #[must_use]
    pub fn on_close_request<H>(mut self, hook: H) -> Self
    where
        H: FnMut(CloseRequest) -> Closing + 'static,
    {
        self.on_close = Some(Box::new(hook));
        self
    }

    /// A handle that asks this application for a frame, from any thread.
    ///
    /// Hand it to [`vieww_foundation::task::Task::spawn`] so that work
    /// finishing on a worker thread causes a frame — without it the result sits
    /// in its channel until something else happens to draw, which on an idle
    /// screen is "until the user touches it".
    ///
    /// # Take it before `run`, not during
    ///
    /// `run` consumes the `App` and does not return until the window closes, so
    /// this is the only opportunity. That is why the handle is valid before the
    /// event loop exists: it starts empty, is attached when the loop is built,
    /// and drops wakes until then — which is harmless, because the first frame
    /// draws everything regardless.
    ///
    /// ```no_run
    /// # use vieww_platform_winit::App;
    /// let app = App::new();
    /// let waker = app.waker();
    /// app.run(move |_driver| {
    ///     // `waker` goes into the tree, and into every `Task` it spawns.
    /// })
    /// .unwrap();
    /// ```
    #[must_use]
    pub fn waker(&self) -> Waker {
        self.waker.clone()
    }

    /// Capture panics into `crashes`, and tell it what the platform knows.
    ///
    /// Without this, a panic in a shipped vieww app reaches nobody: in release
    /// the default `ErrorPolicy` lets it unwind and the process is gone, and on
    /// a phone the stderr the default hook prints to is discarded. With it, the
    /// panic is written wherever the reporter's sinks put it — including,
    /// crucially, to a file that outlives the process, so the *next* launch can
    /// send it somewhere. See [`crate::crash`] for the whole sequence.
    ///
    /// # It replaces the process's panic hook
    ///
    /// Installed at [`run`](Self::run), not here, so an `App` that is built and
    /// dropped leaves the process as it found it. The hook that was already
    /// there is chained rather than discarded — the default one still prints.
    ///
    /// # What the bridge adds to it
    ///
    /// Breadcrumbs only this layer can know, set as they change rather than
    /// every frame: the target platform, and the surface's size and density
    /// once there is a window. An application that wants breadcrumbs of its own
    /// takes [`Crashes::context_handle`] *before* handing the reporter over,
    /// and keeps writing to it.
    #[must_use]
    pub fn report_crashes(mut self, crashes: Crashes) -> Self {
        self.crashes = Some(crashes);
        self
    }

    /// Open the window and run until it closes.
    ///
    /// `build` is called once, after the window exists, with the driver already
    /// sized to it — so a tree that depends on the surface size gets the real
    /// one rather than the size that was asked for. It is *not* called again
    /// when the surface is recreated: the element tree outlives the surface,
    /// which is the whole reason Android can take one away and give it back
    /// without the application noticing.
    ///
    /// # Errors
    ///
    /// [`PlatformError`] if the window or its surface cannot be created. A
    /// failure *during* the loop ends it and is returned the same way.
    pub fn run<F>(self, build: F) -> Result<FrameReport, PlatformError>
    where
        F: FnOnce(&mut FrameDriver),
    {
        crate::startup::begin();
        // A user-event loop rather than a plain one, because that is how
        // AccessKit delivers a screen reader's requests: they arrive on
        // whatever thread the platform's accessibility service uses, and the
        // proxy is what gets them onto ours.
        let event_loop = EventLoop::<UserEvent>::with_user_event()
            .build()
            .map_err(|error| PlatformError::EventLoop(error.to_string()))?;
        self.run_on(event_loop, build)
    }

    /// Run inside an Android activity, driven by the `AndroidApp` that
    /// `android_main` was handed.
    ///
    /// The counterpart to [`run`](Self::run), and the only difference is where
    /// the event loop comes from: on Android it cannot be conjured, because the
    /// activity — and with it the looper, the asset manager and the surface —
    /// already exists by the time any Rust runs.
    ///
    /// Everything above this is unchanged, and deliberately so. The lifecycle
    /// this crate already implements *is* the Android one: the window is opened
    /// on `resumed` rather than at startup, and `suspended` drops the surface
    /// while the element tree lives on. A desktop window simply never exercises
    /// the interesting half.
    ///
    /// # Errors
    ///
    /// As [`run`](Self::run).
    #[cfg(target_os = "android")]
    pub fn run_android<F>(
        mut self,
        android: AndroidApp,
        build: F,
    ) -> Result<FrameReport, PlatformError>
    where
        F: FnOnce(&mut FrameDriver),
    {
        use winit::platform::android::EventLoopBuilderExtAndroid;

        crate::startup::begin();
        // A debug build on a phone is roughly an order of magnitude slower than
        // a release one, and it does not announce itself — it presents as "the
        // framework is slow", which is a report nobody can act on and which has
        // already cost one investigation. `[profile.release]` in the workspace
        // manifest tunes the release build; nothing there can stop a harness
        // from shipping the debug one, so this says so out loud.
        //
        // Android only, and a warning rather than an error: a debug build on a
        // device is a legitimate thing to want — it is how you attach a
        // debugger — so the point is that it be impossible to do by accident,
        // not impossible to do. On desktop the same line would fire on every
        // `cargo run` of every example and mean nothing.
        #[cfg(debug_assertions)]
        log::warn!(
            "vieww: this is a DEBUG build. Expect roughly 10x slower frames than \
             release; measure performance with `--release` before reading anything \
             into a frame report."
        );

        // Kept as well as handed over: the event loop needs it to exist at all,
        // and the safe area needs it every frame. Cloning shares the activity.
        self.android = Some(android.clone());

        let event_loop = EventLoop::<UserEvent>::with_user_event()
            .with_android_app(android)
            .build()
            .map_err(|error| PlatformError::EventLoop(error.to_string()))?;
        self.run_on(event_loop, build)
    }

    /// Drive `event_loop` to completion. Shared by every platform's entry point,
    /// so that what runs on a phone is the same code that runs on a desktop.
    fn run_on<F>(
        #[allow(unused_mut)] mut self,
        event_loop: EventLoop<UserEvent>,
        build: F,
    ) -> Result<FrameReport, PlatformError>
    where
        F: FnOnce(&mut FrameDriver),
    {
        // Before `config: self` moves it.
        #[cfg(target_os = "android")]
        let android = self.android.take();

        // The handle has to come out before `install` consumes the reporter,
        // and the platform's own breadcrumbs go in at the same moment — the
        // target is known now and never changes.
        // **No panic is invisible, even when the application installed no
        // reporter.** Shipped builds abort on panic (Cargo.toml's
        // `dist` profile), and an abort on a phone is an app that vanishes:
        // Android discards stderr, and an iPhone has no console. So without an
        // application's own `Crashes`, a default one with `LogSink` goes in —
        // the message and location reach logcat / the unified log before the
        // process ends. Once per process: a second `App` in the same process
        // (tests open many) must not stack a second hook.
        let crashes = self.crashes.take().or_else(|| {
            static DEFAULT_REPORTER: std::sync::Once = std::sync::Once::new();
            let mut fresh = None;
            DEFAULT_REPORTER.call_once(|| {
                fresh = Some(Crashes::new().with_sink(crate::crash::LogSink));
            });
            fresh
        });
        let crash_context = crashes.map(|crashes| {
            let context = crashes.context_handle();
            context.set("target", std::env::consts::OS);
            crashes.install();
            context
        });

        // The handle an application took before this call now points at a real
        // loop. Anything it spawned in the meantime woke nothing, which is
        // correct — there was no frame to ask for.
        self.waker.attach(event_loop.create_proxy());

        // Out before `config: self` moves it. Both are handles: the runtime is
        // the graph every window's driver will share, and the requests are the
        // queue the application's own `Windows` handle writes into.
        let runtime = self.runtime.clone();
        let requests = self.windows.requests();

        let mut runner = Runner {
            shared: Shared {
                proxy: event_loop.create_proxy(),
                config: self,
                crash_context,
                epoch: Instant::now(),
                failure: None,
                #[cfg(target_os = "android")]
                android,
                #[cfg(target_os = "android")]
                android_a11y_declined: false,
            },
            has_opened: false,
            auto_planned: None,
            last_report: None,
            build: Some(build),
            windows: Vec::new(),
            requests,
            runtime,
        };

        // Start-up is split at the platform's own hand-off: everything up to
        // here is ours; from `run_app` to `StartCause::Init` is the OS starting
        // the loop (on iOS, `UIApplicationMain` up to `didFinishLaunching`);
        // from there to `resumed` is the OS deciding the app may draw.
        crate::startup::mark("app setup");
        event_loop
            .run_app(&mut runner)
            .map_err(|error| PlatformError::EventLoop(error.to_string()))?;

        match runner.shared.failure {
            Some(error) => Err(error),
            // **The primary window's report**, which is the one every existing
            // caller means: with one window it is the only window, and with
            // several it is the one the application started with. A window that
            // never opened at all reports nothing rather than pretending to a
            // measurement it never took.
            // A window that is still open reports directly; one that has been
            // closed reports what it had at the moment it closed. The invented
            // empty report below is for the case it was always meant for — a
            // loop that never opened a window at all.
            None if runner.primary().is_none() && runner.last_report.is_some() => {
                Ok(runner.last_report.expect("just checked"))
            }
            None => Ok(runner.primary().map_or_else(
                // No window ever opened — a `Stop::AfterFrames(0)` run, or a
                // loop that exited before `resumed`. An empty log's report is
                // the honest answer: zero frames, rather than a number invented
                // for a run that never drew.
                || {
                    FrameLog::new(
                        Duration::from_secs_f32(1.0 / runner.shared.config.refresh_hz),
                        0,
                    )
                    .report()
                },
                |window| window.log.report(),
            )),
        }
    }
}

/// The window and everything derived from it.
///
/// One unit because they are created together and die together: on Android a
/// suspend takes the surface, and holding a renderer that outlives it is how a
/// backgrounded app crashes on the way back.
struct Gpu {
    window: Arc<Window>,
    /// `ManuallyDrop` only so that `VIEWW_KEEP_VULKAN` can skip it — see
    /// [`Gpu`]'s `Drop`. Dropped exactly once otherwise.
    renderer: ManuallyDrop<NativeRenderer>,
    surface: NativeSurface,
    /// The next present must repaint everything rather than trust damage.
    ///
    /// True on a first frame and after every resize. Both discard whatever
    /// the renderer was retaining — a resized buffer holds nothing worth
    /// keeping, and a frame's damage describes a difference from a *previous
    /// frame*, which on the first one does not exist. Getting this wrong
    /// leaves the window showing a stale or half-drawn picture that no
    /// further frame repairs, because nothing after it is damaged either.
    needs_full_repaint: bool,
    /// Whether this window's frames go through `present_planned` — the GPU
    /// rasterizer — rather than the CPU's `present_damaged`: `App::prefer_gpu`,
    /// or this device's answer under `App::auto_rasterizer`.
    planned: bool,
    /// This launch's rasterizer measurement, until it finishes.
    measuring: Option<Measuring>,
    /// The Android performance-hint session over the UI thread, under
    /// `VIEWW_PERF_HINT=1` — see `crate::perf_hint`. Started on the first
    /// presented frame, once the budget is known.
    hint: Option<PerfHint>,
    hint_tried: bool,
    /// Whether the GPU pipeline cache save has been queued after this
    /// surface's first GPU frame.
    pipelines_saved: bool,
    /// The UI thread's non-drawing chores, run after a frame with slack
    /// (`crate::frame_budget`). Drained in `Drop` before the renderer goes.
    chores: crate::frame_budget::FrameBudgetGuard<Chore>,
}

/// A deferred chore of one [`Gpu`] — see `crate::frame_budget`'s census.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Chore {
    /// Write the pipeline cache the first GPU frame filled.
    SavePipelineCache,
}

impl Gpu {
    fn run_chore(&self, chore: Chore) {
        match chore {
            Chore::SavePipelineCache => {
                let seed = self.renderer.pipeline_cache_seed().unwrap_or(0);
                let saved = self.renderer.save_pipeline_cache();
                crate::startup::note_pipeline_cache(seed, saved);
            }
        }
    }
}

/// Teardown order is not field order.
///
/// Rust would drop `window`, then `renderer` (destroying the Vulkan device and
/// instance), then `surface` — leaving a live swapchain and `VkSurfaceKHR`
/// behind a destroyed window and device. Mesa's Intel driver on Wayland
/// segfaults on exactly that when a dialog closes or the app exits. The
/// swapchain goes first, while both of its owners exist.
impl Drop for Gpu {
    fn drop(&mut self) {
        // Chores first, while the renderer they need still exists: a suspend
        // or a closing window has no next frame to wait for.
        let pending: Vec<Chore> = self.chores.drain().collect();
        for chore in pending {
            self.run_chore(chore);
        }
        self.surface.release(&self.renderer);
        // **`VIEWW_KEEP_VULKAN=1` keeps the device and instance alive.** A
        // diagnostic, not a setting: every window opens a `VkInstance` and a
        // `VkDevice` of its own (`VulkanDevice::for_window`), so closing one
        // window calls `vkDestroyInstance` while other windows' instances are
        // still live — and NVIDIA's driver then jumps through a null pointer
        // inside the *next* window's `vkDestroySwapchainKHR`. With this set
        // nothing is ever destroyed: if the segfault goes away, the driver's
        // per-process state is what the earlier teardown broke, and the fix is
        // one shared instance and device for every window rather than one
        // each. It leaks a device per window, so it is for one run of the
        // desktop suite and nothing else.
        //
        // It earned its keep a second time on 2026-10-15: Windows + NVIDIA
        // crashed at *process exit* after a good frame, and passed with this
        // set — which is how the cause was found. The shared device was being
        // destroyed by a thread-local's destructor at exit; it is now held
        // weakly and dies with its last window (see `native::SHARED_DEVICE`).
        if std::env::var_os("VIEWW_KEEP_VULKAN").is_none() {
            // SAFETY: the only place the renderer is dropped, and `Gpu` is
            // gone after this — nothing can reach the field again.
            unsafe { ManuallyDrop::drop(&mut self.renderer) };
        }
    }
}

/// Everything one window owns.
///
/// # Why this exists
///
/// `Runner` used to hold exactly these fields, singular, and that was the whole
/// of what made vieww single-window: `vieww-render`'s half of multi-window has
/// been done and tested since 2026-08-14, and the platform's half was one
/// struct's worth of "there is only ever one of these".
///
/// Everything here is genuinely per-surface. The scheduler paces one swapchain;
/// the driver owns one element tree; the pointer translator holds one set of
/// live touches; the IME pair describes one input method session; the insets
/// describe one window's furniture. Sharing any of them across two windows is a
/// bug with a plausible-looking implementation, which is why they moved together
/// rather than one at a time.
///
/// What is **not** here is in [`Shared`], and the split is exactly "does a second
/// window need its own?".
struct WindowState {
    /// The monitor this window's refresh rate was last read from — so a
    /// window dragged onto another display, or a Wayland surface that had not
    /// entered an output when it opened, re-reads it. See
    /// [`WindowState::display_may_have_changed`].
    display: Option<winit::monitor::MonitorHandle>,
    /// When [`display`](Self::display) was last compared against the window's
    /// current monitor from the draw path — see
    /// [`WindowState::recheck_display`].
    display_checked: Option<Instant>,
    /// The pointer shape currently set on this window.
    ///
    /// Remembered so a mouse moving *within* one widget costs nothing — see
    /// [`WindowState::apply_cursor`].
    cursor: vieww_foundation::Cursor,
    /// What the application calls this window.
    key: WindowKey,
    /// What winit calls it, which is how an event finds its way back here.
    id: WindowId,
    scheduler: FrameScheduler,
    /// **Not an `Option`, unlike the field it replaced.** A `WindowState` is
    /// created with its driver and dropped with its window, so the twenty-odd
    /// `if let Some(driver)` guards the old code carried were all asking a
    /// question that now has one answer. The surface below is still optional,
    /// because Android takes it away and gives it back.
    driver: FrameDriver,
    gpu: Option<Gpu>,
    input: PointerTranslator,
    /// Modifier state, which `winit` reports as changes rather than per event.
    keys: KeyTranslator,
    scale: Scale,
    /// The AccessKit adapter, once a window exists. Cheap when no assistive
    /// technology is running — an adapter nobody has activated does no work.
    accessibility: Option<PlatformA11y>,
    /// Whether the platform's input method is currently switched on.
    ///
    /// Tracked so it is toggled on a *change* rather than every frame: enabling
    /// an already-enabled IME re-opens the candidate window on some platforms,
    /// which flickers.
    ime_open: bool,
    /// Whether the input method is **mid-composition** right now.
    ///
    /// Distinct from [`Self::ime_open`], and the distinction is the whole point.
    /// An IME being *allowed* says nothing about whether it consumed the key
    /// that just arrived: on a keyboard composing nothing — every ASCII key on
    /// every layout, which is most typing on most machines — the platform hands
    /// the key straight through, with its text, and no preedit ever opens.
    /// Suppressing on `ime_open` therefore dropped every printable key press
    /// for as long as a field had focus, which is the whole of typing.
    ///
    /// Set by a non-empty preedit and cleared by a commit, an empty preedit or
    /// the session closing — so it is true exactly while there is composed text
    /// on screen that the key would otherwise be typed twice into.
    ime_composing: bool,
    /// The last answer the platform gave about its furniture.
    ///
    /// Cached rather than asked for per frame: the ask is a JNI round trip on
    /// Android, and it is answered on the frames `inset_watch` picks.
    insets: SystemInsets,
    /// Which frames are worth asking on.
    inset_watch: InsetWatch,
    log: FrameLog,
    lifecycle: Lifecycle,
    /// Whether this window has put a frame on screen yet. See
    /// [`WindowState::may_draw`].
    presented: bool,
    /// The earliest moment this window may draw its next frame, on a CPU
    /// present.
    ///
    /// A Vulkan swapchain paces the loop by blocking in `present` until vsync —
    /// which is why `about_to_wait` answers `Poll` after drawing. A CPU present
    /// (`crate::native::PresentPath::Cpu`) returns at once, so without this an
    /// animating window draws as fast as the CPU allows: measured under Xvfb,
    /// 12,700 frames a second of a band that moves a pixel per 16 ms. One frame
    /// per refresh interval (the scheduler's budget, which follows the
    /// monitor) is what the swapchain would have allowed.
    cpu_paced_until: Option<Duration>,
    /// What this window clears to.
    ///
    /// Per window rather than read from the config, because [`WindowSpec`](vieww_render::window::WindowSpec) lets a
    /// second window state its own and a dialog over a dark editor should not be
    /// forced to the editor's colour.
    background: Color,
}

/// Everything the application owns, regardless of how many windows it has.
///
/// Passed to [`WindowState`]'s methods rather than held by them, so that the
/// borrow checker enforces what the split means: a window may read the config
/// and run the hooks, and may not reach another window.
struct Shared {
    config: App,
    /// Breadcrumbs for a crash report, when an application asked for one. See
    /// [`App::report_crashes`].
    crash_context: Option<CrashContext>,
    /// Frame timestamps are measured from here rather than from the wall clock,
    /// so an animation's `t` is a small number and stays exact in `f32`.
    ///
    /// One epoch for the process, not one per window: two windows' frame logs
    /// are only comparable if their timestamps share an origin.
    epoch: Instant,
    /// Set when the loop has to stop for a reason that is not the user closing
    /// the window.
    failure: Option<PlatformError>,
    /// Gets a screen reader's requests onto the UI thread.
    proxy: EventLoopProxy<UserEvent>,
    /// The activity, kept for the one thing `winit` does not surface: where the
    /// system furniture is. Cloning it shares the activity rather than copying
    /// anything.
    #[cfg(target_os = "android")]
    android: Option<AndroidApp>,
    /// Whether installing the accessibility delegate has permanently failed.
    ///
    /// Remembered so a device that cannot carry it — too old an API, a delegate
    /// already installed — pays for one attempt rather than a handful of JNI
    /// calls on every frame it ever draws.
    #[cfg(target_os = "android")]
    android_a11y_declined: bool,
}

impl Shared {
    fn now(&self) -> Duration {
        self.epoch.elapsed()
    }
}

impl WindowState {
    /// Take the frame budget from the display this window actually landed on.
    ///
    /// **The budget was a 60Hz constant on every platform until 2026-08-14**, so
    /// on a 90 or 120Hz phone every jank verdict, every `FrameReport` and the
    /// performance overlay's budget line were all wrong together — consistently
    /// wrong, which is why nothing looked broken.
    ///
    /// Called on every open, not once: a window can move to another monitor, and
    /// a phone can change mode. Skipped entirely when the application stated a
    /// rate, because then it means it.
    ///
    /// **It writes this window's scheduler and log, and no longer the config.**
    /// With one window those were the same thing; with two on different monitors
    /// they are not, and the config's rate is the application's stated default
    /// rather than any particular surface's measurement.
    ///
    /// Pacing does not come from here and never did — the swapchain blocks on
    /// vsync whatever this says. This decides only what counts as *late*.
    /// Re-read the refresh rate if the window is now on a different monitor
    /// than the one it was read from.
    ///
    /// Opening a window was the only time the rate was read, so a window
    /// dragged from a 60Hz display onto a 144Hz one kept a 16.7ms budget and
    /// counted every frame inside 6.9ms as on time — and on Wayland, where a
    /// fresh surface may not have entered any output yet, `current_monitor`
    /// is `None` at open and the budget stayed at its default until the
    /// window was reopened. Called on `Moved`, on `ScaleFactorChanged`, and
    /// once a second from the draw path ([`recheck_display`](Self::recheck_display),
    /// because on Wayland the first two may never come); a move within one
    /// monitor costs one comparison.
    /// [`display_may_have_changed`](Self::display_may_have_changed), at most
    /// once a second, from frames the window is drawing anyway.
    ///
    /// # Why the events were not enough
    ///
    /// On GNOME/Wayland none of them fire for the case that matters. A fresh
    /// surface has entered no output when it opens, so `current_monitor` is
    /// `None` and the open-time read finds nothing; Wayland never sends
    /// `Moved`; and `ScaleFactorChanged` comes only when the scale *changes* —
    /// two 1x monitors never send it. A real two-monitor run on a GNOME/Wayland
    /// laptop (2026-10-14) printed no refresh-rate line at all, at open or
    /// after dragging the window across, while the desktop suite otherwise
    /// passed 21/21: the budget had silently stayed at its 60Hz default.
    ///
    /// Comparing monitor handles once a second is cheap, and it costs nothing
    /// for a window that is not drawing — a window nobody sees has no frame
    /// budget to get wrong.
    fn recheck_display(&mut self, shared: &Shared) {
        let now = Instant::now();
        if self
            .display_checked
            .is_some_and(|last| now.duration_since(last) < Duration::from_secs(1))
        {
            return;
        }
        self.display_checked = Some(now);
        self.display_may_have_changed(shared);
    }

    fn display_may_have_changed(&mut self, shared: &Shared) {
        let Some(window) = self.gpu.as_ref().map(|gpu| Arc::clone(&gpu.window)) else {
            return;
        };
        if window.current_monitor() != self.display {
            self.follow_display(shared, &window);
        }
    }

    fn follow_display(&mut self, shared: &Shared, window: &Window) {
        self.display = window.current_monitor();
        if shared.config.refresh_stated {
            return;
        }
        // Android first, and not as a fallback: **winit's Android backend
        // returns `None` unconditionally** — a hard-coded `FIXME no way to get
        // real refresh rate for now`. So on the platform this framework
        // actually ships to, asking winit can only ever fail, and the budget
        // stayed 60Hz on every phone regardless of its panel. Found by the log
        // line below never appearing on a real device.
        #[cfg(target_os = "android")]
        let detected = shared
            .android
            .as_ref()
            .and_then(crate::insets::display_refresh_hz);
        #[cfg(not(target_os = "android"))]
        let detected: Option<f32> = None;

        // Millihertz, and `None` on a platform that will not say — Wayland
        // often. Keeping the previous value is right: 60 is a better guess than
        // zero, and a display that answers later is picked up on the next open.
        let detected = detected.or_else(|| {
            window
                .current_monitor()
                .and_then(|monitor| monitor.refresh_rate_millihertz())
                .map(|millihertz| {
                    #[expect(
                        clippy::cast_precision_loss,
                        reason = "a refresh rate in millihertz is a small integer"
                    )]
                    let hz = millihertz as f32 / 1000.0;
                    hz
                })
        });
        let Some(hz) = detected else {
            // Say so when there *is* a monitor and it will not name a rate:
            // otherwise "no line" means either this or "never asked", and the
            // two cost a hardware run each to tell apart.
            if let Some(monitor) = &self.display {
                log::info!(
                    "vieww: {} is on {}, which reports no refresh rate; keeping the current budget",
                    self.key,
                    monitor
                        .name()
                        .unwrap_or_else(|| "an unnamed monitor".to_owned())
                );
            }
            return;
        };
        // A monitor reporting something absurd is a monitor to ignore, not a
        // reason to divide by it. `set_refresh_rate` ignores it too; the guard
        // here is so the log and the scheduler cannot disagree about whether it
        // was taken.
        if !hz.is_finite() || hz <= 0.0 {
            return;
        }
        self.scheduler.set_refresh_rate(hz);
        self.log.set_budget(self.scheduler.budget());
        log::info!(
            "vieww: {} reports {hz:.1}Hz; frame budget follows it",
            self.key
        );
    }

    /// Match the window's new size, in physical pixels.
    fn resize(&mut self, width: u32, height: u32) {
        let Some(gpu) = &mut self.gpu else { return };
        if width == 0 || height == 0 {
            return;
        }

        // The retained pixels are gone with the old size, so the next
        // present cannot trust damage — see `Gpu::needs_full_repaint`.
        gpu.needs_full_repaint = true;

        // The swapchain extent on MoltenVK is the layer's bounds times its
        // `contentsScale`; see `ios::match_layer_scale`.
        #[cfg(target_os = "ios")]
        crate::ios::match_layer_scale(&gpu.window);

        if let Err(error) = gpu.renderer.resize(&mut gpu.surface, width, height) {
            // A resize is not fatal to the application — the next present
            // will try again against whatever the surface settles on — but
            // it is worth a line in the log, since a surface that will not
            // rebuild is usually the first symptom of something worse.
            log::error!("vieww: resizing to {width}x{height} failed: {error}");
        }
        self.log.resize(u64::from(width) * u64::from(height));
        self.driver
            .resize(self.scale.to_logical_size(width, height));
        // A resize is a rotation, a fold, or a window being dragged onto
        // another monitor, and every one of those moves the furniture.
        self.inset_watch.disturbed();
        self.republish_insets();
        self.scheduler.request_frame();
    }

    /// Give the per-frame hooks the log, before anything is built.
    ///
    /// Before rather than after, so a signal a hook writes is picked up by the
    /// build phase of the frame it was called for. See [`App::on_frame`].
    ///
    /// # Every window runs them
    ///
    /// A hook is the application's, not the window's, and an application with an
    /// inspector open still wants to know what its main window cost. The log
    /// carries [`FrameLog::window`] so a hook that only cares about one of them
    /// can say so — which is why the fan-out did not need a breaking change to
    /// the hook signature.
    fn notify_frame(&mut self, shared: &mut Shared) {
        for hook in &mut shared.config.on_frame {
            hook(&self.log);
        }
    }

    /// Whether the loop may draw this window now: when it is visible, and —
    /// whatever the compositor says about visibility — once, if it has never
    /// put a frame on screen.
    ///
    /// # Why the first frame is drawn even when "hidden"
    ///
    /// On Wayland a toplevel does not exist on screen until its first buffer
    /// is committed. GNOME's focus-stealing prevention can open a window that
    /// has no activation token (anything started from a terminal, a test
    /// harness included) *below* the focused one, and report it occluded
    /// before it has drawn anything. Declining to draw a hidden window is
    /// right; declining its *first* frame left it with no buffer, so nothing
    /// could ever un-hide it: no `Occluded(false)`, no frame, for ever. That
    /// is the shape of `tests/wait_loop.rs`' idle scenario hanging with zero
    /// frames on a GNOME/Wayland laptop. One frame into a hidden surface is
    /// harmless (presenting to it is legal — see [`Lifecycle::has_surface`]);
    /// after it, hidden windows are left alone as before.
    fn may_draw(&self) -> bool {
        self.lifecycle.may_draw(self.presented)
    }

    /// Produce a frame and put it on the screen.
    fn draw(&mut self, shared: &mut Shared) {
        // Here, not in the `RedrawRequested` arm: most frames are drawn
        // directly from `about_to_wait` (see the comment at that call), so a
        // check placed only on `RedrawRequested` ran on the first map and
        // almost never again — the second two-monitor run on GNOME/Wayland
        // still printed no refresh-rate line in 18,200 frames.
        if !self.presented {
            crate::startup::mark("until first draw");
        }
        self.recheck_display(shared);
        // Before the hooks and before the build, for the same reason
        // `notify_frame` runs there: a keyboard that came up has to be in the
        // metrics the frame about to be built reads, not the one after it.
        // Most frames answer `false` here and cost nothing.
        if self.inset_watch.due() {
            self.refresh_insets(shared);
        }
        self.notify_frame(shared);

        // Before the build, and before `drive` below, so a tree swapped here is
        // the tree this frame shows. After the present it would leave the
        // previous build on screen until something else asked for a frame — on
        // an idle screen, indefinitely.
        for hook in &mut shared.config.before_frame {
            hook(&mut self.driver);
        }
        // The application's own hooks, apart from the framework's steps: on the
        // desktop example the clipboard hook's first X11 round trip was 100 ms
        // of a 150 ms start-up, which is the application's to spend.
        if !self.presented {
            crate::startup::mark("app before-frame hooks");
        }

        let Some(gpu) = &mut self.gpu else {
            return;
        };

        let now = shared.epoch.elapsed();
        let epoch = shared.epoch;
        let stats = self
            .driver
            .drive(&mut self.scheduler, now, || epoch.elapsed());
        if !self.presented && stats.is_some() {
            crate::startup::mark("first build and layout");
        }

        // Nothing ran, so there is nothing new to put on screen. An
        // un-occlusion just requests a frame (see the `Occluded` handler
        // below) and this same `stats.is_some()` check covers it.
        if stats.is_none() {
            return;
        }

        // Tells the compositor a frame is imminent, so it can time its own
        // work. Without it, Wayland pacing is measurably worse.
        gpu.window.pre_present_notify();

        let logical = self.driver.size();
        #[expect(
            clippy::cast_sign_loss,
            clippy::cast_possible_truncation,
            reason = "a window's logical size is a small positive number of pixels"
        )]
        let logical_size = (logical.width as u32, logical.height as u32);
        let started = Instant::now();
        // **Damage-driven presentation.** The renderer keeps the pixels it
        // produced last frame and repaints only the regions this frame's
        // damage names — see `crate::native::NativeRenderer::present_damaged`
        // and, for the rasteriser half, `render_retained`'s own docs.
        //
        // `needs_full_repaint` is set by a resize and by the first frame on a
        // surface: in both cases there is no retained content worth keeping,
        // and the damage would not describe the difference anyway.
        let damage = if gpu.needs_full_repaint {
            gpu.needs_full_repaint = false;
            None
        } else {
            Some(self.driver.damage())
        };
        // **Which rasterizer draws is the application's call, per config.**
        //
        // `prefer_gpu` routes through `present_planned`: the frame is planned
        // first, drawn by the GPU rasterizer when the plan is complete, and
        // drawn here by the CPU when it is not. The damage computed above is
        // only meaningful to the CPU half — the GPU path is a whole-frame
        // rasterizer — but it is computed unconditionally anyway so the
        // stats below keep one shape whichever path a frame took.
        let presented = if gpu.planned {
            gpu.renderer
                .present_planned(
                    &mut gpu.surface,
                    self.driver.scene(),
                    self.background,
                    logical_size,
                    self.scale,
                )
                .map(|frame| (frame.report, frame.drawn_on_gpu, frame.presented_directly))
        } else {
            gpu.renderer
                .present_damaged(
                    &mut gpu.surface,
                    self.driver.scene(),
                    damage,
                    self.background,
                    logical_size,
                    self.scale,
                )
                .map(|report| (report, false, false))
        };

        match presented {
            Ok((report, drawn_on_gpu, presented_directly)) => {
                // A wall-clock measurement around the call, unlike
                // `vieww_paint::gpu::GpuRenderer::present_with`'s own doc
                // warning against exactly that: that backend's
                // `get_current_texture` blocks on vsync *inside* the call it
                // was warning about timing, so wrapping it measured the frame
                // interval rather than the GPU's own work. This backend's
                // `present_pixels` is fully synchronous by design (see
                // `vieww-hal`'s `vulkan::swapchain` module docs) and returns
                // only once the frame is genuinely on the swapchain, so a
                // wall-clock duration around it is the raster-plus-present
                // cost, not the frame interval.
                let elapsed = started.elapsed();
                let factor = f64::from(self.scale.factor());
                // **The pixels this frame actually rasterised, not the ones it
                // could have.**
                //
                // This was the whole surface, unconditionally, which made
                // `FrameLog::full_repaints` — `rasterised / surface_pixels` —
                // a tautology: it could only ever equal the frame count. An
                // idle window reported "every frame repainted everything", and
                // so did a window that really did. Neither the retained path
                // working nor a regression in it could move the number, which
                // is the one thing a metric has to be able to do.
                //
                // `FrameStats::damage_area` has carried the answer since it was
                // added — its own doc says "nothing carried it out to where the
                // timings are read, so every performance conversation about this
                // framework was missing its denominator". This is that read. It
                // is logical px², so it scales by the factor twice.
                //
                // A `None` `damage` means this frame was presented as a full
                // repaint deliberately (first frame, or a resize), so the whole
                // surface is the honest figure there. A `None` `stats` means the
                // scheduler declined a frame and only the present happened, and
                // the driver annotated nothing.
                #[expect(
                    clippy::cast_sign_loss,
                    clippy::cast_possible_truncation,
                    reason = "pixel counts are small positive numbers"
                )]
                let pixels = {
                    let surface = ((f64::from(logical_size.0) * factor) as u64)
                        * ((f64::from(logical_size.1) * factor) as u64);
                    match (damage, stats) {
                        (Some(_), Some(stats)) => {
                            (f64::from(stats.damage_area) * factor * factor) as u64
                        }
                        _ => surface,
                    }
                };
                self.log
                    .record(stats, elapsed, pixels, Instant::now(), drawn_on_gpu);
                // The same work the frame log judges against the budget.
                let work = stats.map_or(elapsed, |stats| stats.total + elapsed);
                if let Some(measuring) = &mut gpu.measuring {
                    if measuring.observe(work, drawn_on_gpu, self.log.budget()) {
                        gpu.measuring = None;
                    }
                }
                if !gpu.hint_tried {
                    gpu.hint_tried = true;
                    gpu.hint = crate::perf_hint::start(self.log.budget());
                }
                if let Some(hint) = &mut gpu.hint {
                    hint.frame(work, self.log.budget());
                }
                // The pipelines exist once a GPU frame has drawn, and a phone
                // app is killed rather than dropped, so the next launch's head
                // start is written soon — but in a frame with slack, or at the
                // guard's deadline (`crate::frame_budget::MAX_WAIT`), never on
                // top of a frame that is already over budget.
                if drawn_on_gpu && !gpu.pipelines_saved {
                    gpu.pipelines_saved = true;
                    gpu.chores.defer(Chore::SavePipelineCache, shared.now());
                }
                if let Some(chore) = gpu
                    .chores
                    .after_frame(shared.now(), work, self.log.budget())
                {
                    gpu.run_chore(chore);
                }
                self.log.note_stages(gpu.renderer.last_stages());
                if tracing() {
                    let st = gpu.renderer.last_stages();
                    let ms = |d: Duration| d.as_secs_f64() * 1000.0;
                    trace_line(
                        shared.now(),
                        format_args!(
                            "frame stages ({}): scale {:.2} plan {:.2} cpu {:.2} | gpu upload {:.2} \
                             record {:.2} submit {:.2} wait {:.2} readback {:.2} | acquire {:.2} \
                             present {:.2} = {:.2} ms",
                            if drawn_on_gpu { "gpu" } else { "cpu" },
                            ms(st.scale),
                            ms(st.plan),
                            ms(st.cpu_raster),
                            ms(st.gpu.upload),
                            ms(st.gpu.record),
                            ms(st.gpu.submit),
                            ms(st.gpu.wait),
                            ms(st.gpu.readback),
                            ms(st.acquire),
                            ms(st.present),
                            ms(st.total()),
                        ),
                    );
                }
                if !self.presented {
                    crate::startup::first_frame();
                }
                self.presented = true;
                if presented_directly {
                    self.log.note_direct_present();
                }
                let _ = report;
                // After the frame, not before: the semantics describe what is
                // now on screen, and a screen reader announcing a control a
                // frame before it appears is a race the user can hear.
                self.publish_semantics(shared);
                // After the present *and* after the semantics, so a hook sees the
                // same settled tree a screen reader would.
                for hook in &mut shared.config.after_frame {
                    hook(&self.driver);
                }
            }
            // The swapchain rebuilds itself internally on an ordinary
            // resize race (see `vieww-hal`'s `present_pixels`); reaching
            // here means that rebuild itself failed, or the device is
            // genuinely gone. Not fatal to the application on its own — the
            // next frame tries again — but not silently swallowed either.
            Err(error @ NativeError::Present(_)) => {
                log::error!("vieww: presenting a frame failed: {error}");
                self.scheduler.request_frame();
            }
            Err(error) => shared.failure = Some(PlatformError::Gpu(error)),
        }
    }

    /// Route one translated pointer event into the tree.
    ///
    /// A frame is requested unconditionally rather than only when something
    /// changed. A handler that sets a signal already marks its element pending,
    /// but a handler that changes something the framework cannot see — a
    /// caret's blink phase, a scroll offset held outside a signal — would
    /// otherwise never reach the screen. The scheduler coalesces, so the cost
    /// of being wrong in this direction is one boolean.
    fn pointer(&mut self, event: Option<PointerEvent>) {
        let Some(event) = event else { return };
        // **The keys, stamped here and nowhere else.**
        //
        // Every pointer event the platform produces passes through this
        // function, and `keys` is the only thing in the process that knows what
        // is held — `winit` reports modifier *changes*, not state, so there is
        // nothing to ask at the point of use even if there were somewhere to
        // ask it. Stamping in the one chokepoint is what makes ⇧-click and
        // ⌘-click possible for an application without every recogniser, every
        // widget and every event constructor learning about a keyboard.
        let event = event.with_modifiers(self.keys.modifiers());
        self.driver.handle_pointer(&event);
        self.scheduler.request_frame();
        // **A tap is how focus moves on a phone**, and this was missing for as
        // long as the IME has existed. `sync_ime` used to be called only after
        // a key event and an IME event, which is enough on a desktop: tap a
        // field, press a key, and the key event opens the IME on its way past.
        // A phone has no key to press. The field focused, drew its caret, and
        // the platform was never asked for a keyboard — measured on a Redmi
        // Note 7 Pro as `mInputShown=false` under a visibly focused field, with
        // `view_insets` therefore stuck at zero no matter how correct the JNI
        // that reads them is.
        self.sync_ime();
    }

    /// Carry out what a screen reader asked for, and publish the result.
    ///
    /// Both halves matter. An action changes the tree, so the screen reader has
    /// to be told what it now looks like — otherwise it goes on reading the
    /// state from before the thing it just did.
    fn access_action(&mut self, shared: &mut Shared, request: &accesskit::ActionRequest) {
        let Some(action) = crate::a11y::from_action(request.action) else {
            // An action we do not carry out. Doing nothing is the honest
            // response; approximating it would do something the user did not
            // ask for.
            return;
        };
        let semantics = self.driver.semantics();
        let Some(target) = crate::a11y::render_id(request.target_node, &semantics) else {
            // A stale id from the screen reader's cache. They cache
            // aggressively and the tree moves on; finding nothing is the
            // correct outcome, and is why `node_id` packs the generation in.
            return;
        };
        // **The only evidence that a screen reader's own gesture reached the
        // tree.** Everything else about this path can be checked from inside
        // the app — `examples/shared/device_tests.rs` dispatches an `Activate`
        // and asserts the handler ran — but a self-dispatch says nothing about
        // whether `accesskit_android` *delivers* a TalkBack double-tap. That
        // half is only visible from outside, and this is what makes it visible:
        // `ci/mobile/a11y-android.sh --activate` waits for this line.
        //
        // Both outcomes, because they fail differently. Nothing at all means
        // the bridge never delivered; `handled=false` means it delivered and
        // the tree had nothing that would take it, which is a routing bug and
        // not a bridge one.
        let handled = self.driver.handle_semantic_action(target, action);
        log::info!("a11y: {action:?} delivered to a node, handled={handled}");
        if handled {
            self.scheduler.request_frame();
            self.publish_semantics(shared);
            // For the same reason a tap does it: a screen reader activating a
            // text field has moved focus, and expects a keyboard to follow.
            self.sync_ime();
        }
    }

    /// Tell the tree where the cursor is, and ask for a frame only if that
    /// changed something.
    ///
    /// The check matters: a mouse moving across a window produces an event per
    /// pixel, and asking for a frame on each would repaint continuously while
    /// nothing on screen was different.
    fn hover(&mut self) {
        let position = self.input.hover();
        if self.driver.handle_hover(position) {
            self.scheduler.request_frame();
        }
        self.apply_cursor(position);
    }

    /// Set the window's pointer shape from whatever is under it.
    ///
    /// # Why the last one is remembered
    ///
    /// `set_cursor` is a platform call, and this runs on every mouse move —
    /// which on a trackpad is a few hundred times a second. Sending the same
    /// icon over and over is work the compositor has to do nothing with, and on
    /// X11 it is a round trip. The comparison makes the common case — the
    /// pointer moving *within* one widget — free.
    ///
    /// A pointer that has left the window gets no call at all rather than a
    /// reset to the arrow: the cursor is not over this window any more, and
    /// whatever it is over owns its shape.
    fn apply_cursor(&mut self, position: Option<vieww_foundation::Offset>) {
        let Some(position) = position else {
            return;
        };
        let cursor = self.driver.cursor_at(position);
        if self.cursor == cursor {
            return;
        }
        self.cursor = cursor;
        // Through `gpu`, which is where the window handle lives: a suspended
        // window has no surface and no window, and setting a cursor on one that
        // is not on screen is a call with nowhere to land.
        if let Some(gpu) = self.gpu.as_ref() {
            gpu.window
                .set_cursor(winit::window::Cursor::Icon(cursor_icon(cursor)));
        }
    }

    /// Ask the platform where its furniture is, right now.
    ///
    /// # Android
    ///
    /// Real insets, through JNI — see [`crate::insets`] for why the cheaper
    /// `content_rect` route does not work and what was measured to establish
    /// that. The soft keyboard comes back from the same call.
    ///
    /// # iOS
    ///
    /// Real insets, through UIKit — see `src/ios.rs`, and note that nothing in
    /// it has run against a real UIKit yet.
    ///
    /// # Everywhere else
    ///
    /// Zero, and honestly so. `winit` has no cross-platform API for a cutout
    /// because its desktop backends have nothing to report: a desktop window
    /// genuinely has no furniture over it and no soft keyboard under it.
    fn system_insets(&self, shared: &Shared) -> SystemInsets {
        #[cfg(target_os = "android")]
        if let Some(android) = &shared.android {
            // `None` means the ask failed, or the device predates the API. Zero
            // is the same answer this gave before either way.
            return crate::insets::window_insets(android, self.scale.factor())
                .unwrap_or(SystemInsets::NONE);
        }
        #[cfg(target_os = "ios")]
        if let Some(gpu) = &self.gpu {
            return crate::ios::window_insets(&gpu.window).unwrap_or(SystemInsets::NONE);
        }
        let _ = shared;
        SystemInsets::NONE
    }

    /// Ask the platform, remember the answer, and republish the metrics.
    ///
    /// Split from [`sync_view_metrics`](Self::sync_view_metrics) because the two
    /// have very different costs. Asking is a JNI round trip or a pair of
    /// message sends; republishing is a comparison. Every frame does the second
    /// one; only the frames [`InsetWatch`](crate::insets::InsetWatch) picks do
    /// the first.
    fn refresh_insets(&mut self, shared: &Shared) {
        self.insets = self.system_insets(shared);
        self.inset_watch.observed(self.insets.keyboard);
        self.sync_view_metrics();
    }

    /// Republish the metrics from what was last asked, without asking again.
    ///
    /// For `resize`, which is reached from a borrow of the window list and has
    /// no [`Shared`] to hand. Not asking is also the right thing rather than a
    /// concession: a resize has already marked the watch disturbed, so the next
    /// frame does the real ask, and doing it here as well would be a JNI round
    /// trip inside an event burst that can carry dozens of resizes.
    fn republish_insets(&mut self) {
        self.sync_view_metrics();
    }

    /// Tell the tree what the surface is like, from what was last asked.
    fn sync_view_metrics(&mut self) {
        let insets = self.insets;
        let scale = self.scale.factor();
        let changed = self.driver.set_view_metrics(ViewMetrics {
            size: self.driver.surface().size(),
            device_pixel_ratio: scale,
            safe_area: insets.safe_area,
            // Separate from the safe area all the way down, and this is where
            // the separation is finally load-bearing: content is inset past a
            // cutout and expected to *scroll out from under* a keyboard.
            view_insets: insets.keyboard,
        });
        if changed {
            self.scheduler.request_frame();
        }
    }

    /// Route one key event into the tree.
    fn key(&mut self, event: &vieww_foundation::KeyEvent) {
        self.driver.handle_key(event);
        // Unconditionally, for the same reason a pointer does: a key that moved
        // focus changed what is drawn even when nothing "handled" it.
        self.scheduler.request_frame();
        self.sync_ime();
    }

    /// Match the platform's input method to what has focus.
    ///
    /// Called after anything that can move focus or the caret. Two things have
    /// to be told: whether an IME should be open at all — which on a phone is
    /// whether the soft keyboard comes up — and where the caret is, so a
    /// candidate window opens beside the text rather than in the corner.
    fn sync_ime(&mut self) {
        let Some(gpu) = &self.gpu else {
            return;
        };

        let wanted = self.driver.accepts_text();
        if wanted != self.ime_open {
            gpu.window.set_ime_allowed(wanted);
            self.ime_open = wanted;
            // The keyboard is about to animate in or out, and the height it
            // has *at this instant* is the old one. Watch it across the
            // animation rather than sampling once and believing it.
            self.inset_watch.disturbed();
        }

        if wanted {
            if let Some(caret) = self.driver.ime_cursor_area() {
                let caret = self.scale.to_physical_rect(caret);
                gpu.window.set_ime_cursor_area(
                    PhysicalPosition::new(caret.left, caret.top),
                    PhysicalSize::new(caret.width().max(1.0), caret.height().max(1.0)),
                );
            }
        }
    }

    /// Install the Android delegate, or arrange to try again.
    ///
    /// Called at window creation *and* after every frame, because the first
    /// attempt happens on `resumed` and the activity's view is not reliably
    /// attached to a window by then — and injecting into an unattached view
    /// fails **silently**, leaving a working adapter that Android never asks
    /// anything of. `Injection::NotYet` in `a11y_android` carries the detail.
    ///
    /// A permanent decline is remembered, so an old device pays for one attempt
    /// rather than a handful of JNI calls per frame for ever.
    #[cfg(target_os = "android")]
    fn try_android_accessibility(&mut self, shared: &mut Shared) {
        use crate::a11y_android::Injection;

        if self.accessibility.is_some() || shared.android_a11y_declined {
            return;
        }
        let Some(android) = shared.android.clone() else {
            return;
        };
        match crate::a11y_android::AndroidAdapter::with_event_loop_proxy(
            &android,
            shared.proxy.clone(),
        ) {
            Injection::Done(adapter) => self.accessibility = Some(adapter),
            Injection::NotYet => {}
            Injection::Declined => shared.android_a11y_declined = true,
        }
    }

    /// Move to a new lifecycle state, and tell the tree what it means.
    ///
    /// **The only place `self.lifecycle` is assigned**, which is the point of it
    /// existing. Capture and lifecycle are the same fact told twice, and there
    /// are six transition sites in this file — five window events and a
    /// suspend. Five of them remembering to call
    /// [`FrameDriver::set_capture`] and one forgetting is not a hypothetical
    /// failure; it is what happens to every rule that lives in call sites
    /// rather than in a setter, and the one that forgets is whichever arm is
    /// added last by somebody who has not read this comment.
    fn set_lifecycle(&mut self, next: Lifecycle) {
        self.lifecycle = next;
        let capture = next.capture();
        if !self.driver.set_capture(capture) {
            return;
        }
        log::debug!("capture: {capture} ({next})");
        // A mask that is not painted is not a mask. Unmasking always gets its
        // frame, because `Screen` only ever follows a transition back to being
        // visible and the scheduler is live again by then.
        //
        // Masking is the direction that cannot always be honoured, and this is
        // the known limit of doing it from lifecycle at all: by the time winit
        // reports `Suspended` on Android the surface is gone, so the frame that
        // would carry the mask has nowhere to go and the thumbnail the OS
        // already took is of the unmasked tree. `Hidden` — a desktop window
        // covered, an Android task still holding its surface — is the case this
        // does close, because the swapchain is still there to draw into.
        //
        // Closing the Android case properly needs `FLAG_SECURE` on the window,
        // which is a JNI call rather than a repaint, and it is recorded in
        // `TRACKER.md` rather than pretended away here.
        if next.should_draw() {
            self.scheduler.request_frame();
        }
    }

    /// Hand the current semantics tree to AccessKit.
    ///
    /// `update_if_active` is the important half: with no screen reader running
    /// the closure is never called, so building the semantics tree — a walk of
    /// the whole render tree — costs nothing on the overwhelming majority of
    /// machines. Building it unconditionally and then discarding it would be a
    /// per-frame tax paid by everyone for the benefit of no one.
    fn publish_semantics(&mut self, shared: &mut Shared) {
        // Before the early return below, because until this succeeds there is no
        // adapter and the early return is exactly what would stop us retrying.
        #[cfg(target_os = "android")]
        self.try_android_accessibility(shared);
        let _ = &shared;

        // **iOS**: AccessKit ships no iOS adapter (see `a11y.rs`'s module
        // doc), so the tree reaches VoiceOver through `ios_accessibility.rs`
        // instead — gated on VoiceOver itself running, for the same
        // no-screen-reader-no-cost reason the closure below is lazy. Before
        // the adapter early-return, for the same reason the Android hook is
        // there; and only with a live surface, because the bridge needs the
        // view the elements are parented to.
        #[cfg(target_os = "ios")]
        {
            let scale = self.scale;
            let surface = self.gpu.as_ref().map(|gpu| std::sync::Arc::clone(&gpu.window));
            if let Some(window) = surface {
                let semantics = self.driver.semantics();
                let published =
                    crate::ios_accessibility::publish_semantics(&window, &semantics, scale);
                if let Some(count) = published {
                    trace_line(
                        shared.now(),
                        format_args!("a11y: published {count} element(s) to VoiceOver"),
                    );
                }
            }
        }

        let Some(adapter) = &mut self.accessibility else {
            return;
        };
        let scale = self.scale;
        let driver = &self.driver;
        adapter.update_if_active(|| {
            let semantics = driver.semantics();
            let published = crate::a11y::tree_update(&semantics, scale);
            report_published_tree(&semantics, published.is_some());
            published.unwrap_or_else(crate::a11y::empty_update)
        });
    }

    /// Tell the gesture layer that every live pointer is gone.
    fn cancel_pointers(&mut self, shared: &Shared) {
        let now = shared.now();
        for event in self.input.cancel_all(now) {
            self.driver.handle_pointer(&event);
        }
    }

    /// Drop the surface, keeping the tree.
    ///
    /// The Android suspend path, and the reason [`Gpu`] is one unit: the
    /// platform is about to reclaim the surface, and a renderer that outlives it
    /// crashes on the way back.
    fn drop_surface(&mut self) {
        self.scheduler.cancel_frame();
        self.gpu = None;
    }
}

/// A winit window and the renderer bound to it, before either has a tree.
///
/// A struct rather than the six-tuple this was first written as: `clippy`'s
/// `type_complexity` is on by default and the tuple is exactly what it exists to
/// catch — six positional values of which two are `u32` and mean different
/// things.
struct NewSurface {
    window: Arc<Window>,
    renderer: NativeRenderer,
    surface: NativeSurface,
    /// Whether frames go through `present_planned` (the GPU rasterizer).
    planned: bool,
    /// This launch's rasterizer measurement, under `App::auto_rasterizer`.
    measuring: Option<Measuring>,
    scale: Scale,
    width: u32,
    height: u32,
}

struct Runner<F> {
    shared: Shared,
    /// Taken the first time the primary window appears. `None` afterwards, which
    /// is what makes a rebuilt surface not rebuild the tree.
    build: Option<F>,
    /// Every open window, in the order they opened. The primary is whichever
    /// carries [`WindowKey::PRIMARY`], which is the first one created and the
    /// last one destroyed.
    windows: Vec<WindowState>,
    /// What the application has asked for and this loop has not done yet, and
    /// the one [`WindowSet`](vieww_render::WindowSet) both halves read.
    requests: Rc<Requests>,
    /// The graph every window's driver shares, so a document open in two windows
    /// is one document.
    runtime: Runtime,
    /// The primary window's report, kept from just before it was destroyed.
    ///
    /// **`App::run` returned a fabricated empty report for every application
    /// that closed normally**, and that is not an edge case — it is what
    /// happens every time a user clicks the close button. `destroy` removes the
    /// window, `primary()` then answers `None`, and the fallback below invents
    /// `frames: 0` for a session that may have drawn for an hour. The whole
    /// point of the return value is the measurement, and it was being discarded
    /// at exactly the moment it was complete.
    ///
    /// Found by the programmatic-close fix above — the returned report read
    /// `frames: 0` for a run that had visibly drawn — but the user-close path
    /// had the same hole and had had it for longer.
    last_report: Option<FrameReport>,
    /// Whether a window has ever been on screen.
    ///
    /// Guards [`last_window_closed`](Self::last_window_closed) against the state
    /// every application passes through on the way up: before `resumed` there
    /// are no windows, and "no windows" must not mean "the application is over"
    /// while it still means "the application has not started". Android makes
    /// that gap wider than it looks — the window is opened on `resumed` rather
    /// than at startup, which is the whole reason that platform can take a
    /// surface away and give it back.
    has_opened: bool,
    /// Under `App::auto_rasterizer`, what the first window chose. Every later
    /// surface — a second window, an Android surface rebuilt after a suspend —
    /// draws the same way rather than calibrating again: a dialog's size is
    /// not the device's, and its measurement would overwrite the primary's.
    auto_planned: Option<bool>,
}

impl<F: FnOnce(&mut FrameDriver)> Runner<F> {
    /// The window an application means when it does not say which.
    ///
    /// [`App::run`]'s report comes from here, and so does [`Stop::AfterFrames`].
    /// With one window this is the only window; with several it is the one the
    /// application started with, which is the only choice that leaves every
    /// existing caller measuring what it measured before.
    fn primary(&self) -> Option<&WindowState> {
        self.windows
            .iter()
            .find(|window| window.key == WindowKey::PRIMARY)
            .or_else(|| self.windows.first())
    }

    /// The window a [`WindowKey`] names.
    ///
    /// There is deliberately no `by_id` beside this. Routing a winit event needs
    /// the **index** rather than the borrow — `window_event` holds it across
    /// calls that also need `&mut self.shared`, and a method handing back
    /// `&mut WindowState` would borrow the whole `Runner` for the rest of the
    /// match. `iter().position(..)` at the top of that function is the shape
    /// that works, so a lookup returning a reference exists only for the one
    /// caller that genuinely wants one.
    fn by_key(&mut self, key: WindowKey) -> Option<&mut WindowState> {
        self.windows.iter_mut().find(|window| window.key == key)
    }

    /// Create a winit window and everything that hangs off it.
    ///
    /// Everything except the tree: the caller supplies that, because the primary
    /// window's builder and a second window's are different types — `run` takes
    /// `FnOnce(&mut FrameDriver)` and [`Windows::open`](vieww_render::window::Windows::open) takes one that is also
    /// handed its own key.
    fn surface(
        &mut self,
        event_loop: &ActiveEventLoop,
        title: &str,
        size: Size,
    ) -> Result<NewSurface, PlatformError> {
        let attributes = Window::default_attributes()
            .with_title(title.to_owned())
            // AccessKit's adapter has to exist before the window is ever shown,
            // and panics if it does not. So the window is born invisible and
            // revealed by the caller, once the adapter is attached.
            .with_visible(false);
        // **A phone decides its own window.** On iOS, winit applies a
        // requested size to the `UIWindow` frame: the demo's 420x880 desktop
        // preview became a 420x880 window on a 393x852 iPhone 15 Pro. The frame
        // was drawn from the top-left and cropped at the right, while the
        // accessibility frames came out shifted by the difference (27, 28) — so
        // Maestro, aiming from the tree, tapped seven points above "Tap me"
        // (first iOS simulator run with a working GPU, 2026-10-14). On a phone
        // the requested size is ignored, as the iOS example's doc always said
        // it was; winit's Android backend ignores it anyway.
        #[cfg(not(any(target_os = "ios", target_os = "android")))]
        let attributes = attributes.with_inner_size(LogicalSize::new(
            f64::from(size.width),
            f64::from(size.height),
        ));
        // **Startup is traced step by step.** A gate run on a GNOME/Wayland
        // hybrid-GPU laptop hung here — `tests/wait_loop.rs`' idle scenario
        // printed no window event at all in 20 s while its waker thread fired
        // on time — and an untraced startup could not say whether the window,
        // the GPU selection or the swapchain was the step that never returned.
        trace_line(self.shared.now(), format_args!("startup: creating window"));
        let window = Arc::new(
            event_loop
                .create_window(attributes)
                .map_err(|error| PlatformError::Window(error.to_string()))?,
        );
        crate::startup::mark("window created");

        let scale = Scale::new(window.scale_factor());

        // A window can report 0x0 before its first configure — Wayland does —
        // and a zero-sized swapchain is a validation error, so the requested
        // size stands in until the first `Resized` says otherwise.
        let physical = window.inner_size();
        let (width, height) = if physical.width == 0 || physical.height == 0 {
            #[expect(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "a window size is a small positive number of pixels"
            )]
            (
                (size.width * scale.factor()).max(1.0) as u32,
                (size.height * scale.factor()).max(1.0) as u32,
            )
        } else {
            (physical.width, physical.height)
        };

        // A `vieww-hal` `VulkanDevice` of its own for every window — there is
        // no `vieww_paint::gpu::GpuContext` equivalent to share one across
        // windows yet (see `crate::native`'s module docs, and
        // `docs/RENDERER-MIGRATION.md`). One Vulkan device per window is
        // correct, just not `docs/AIMS.md` §B's *Cheaper* win the old vello
        // path had; a follow-up, not a defect.
        // **Which way frames reach the screen.** A window that does not want
        // the GPU presents its CPU frames straight to the compositor and never
        // loads a GPU driver — see `crate::native::PresentPath`.
        //
        // Under `auto_rasterizer` the choice is this device's calibration —
        // see `crate::rasterizer_choice`.
        let calibration = (self.shared.config.auto_rasterizer && self.auto_planned.is_none())
            .then(|| self.calibration(width, height))
            .flatten();
        let launch = calibration.as_ref().map(Calibration::launch);
        let planned = match (launch, self.auto_planned) {
            (Some(launch), _) => launch.rasterizer() == Rasterizer::Gpu,
            (None, Some(planned)) => planned,
            (None, None) => self.shared.config.prefer_gpu && !self.shared.config.auto_rasterizer,
        };
        let path = PresentPath::choose(planned);
        trace_line(
            self.shared.now(),
            format_args!(
                "startup: window created ({width}x{height}); opening the {path:?} present"
            ),
        );
        let opening = Instant::now();
        // Before any GPU device exists, so the device this window opens starts
        // from the pipelines an earlier launch compiled
        // (`vieww_hal::vulkan::pipeline_cache`).
        vieww_hal::vulkan::set_pipeline_cache_file(self.pipeline_cache_file());
        // And before the first renderer, so its blur and fill caches start at
        // this device tier's budgets (`quality::cache_budgets_for`).
        crate::quality::apply_cache_budgets();
        let opened = NativeRenderer::for_window_on(&window, width, height, path);
        // A device whose GPU cannot be opened has answered the question: under
        // `auto_rasterizer` that is the CPU path, kept, rather than an app that
        // will not start.
        let (opened, planned, launch, settled) = match (opened, &calibration) {
            (Err(error), Some(calibration)) if path == PresentPath::Vulkan => {
                log::warn!("vieww: the GPU path could not be opened ({error}); keeping the CPU path");
                calibration.settle(Rasterizer::Cpu);
                (
                    NativeRenderer::for_window_on(&window, width, height, PresentPath::Cpu),
                    false,
                    Some(Launch::Use(Rasterizer::Cpu)),
                    true,
                )
            }
            (opened, _) => (opened, planned, launch, false),
        };
        let (mut renderer, surface) = opened.map_err(PlatformError::Gpu)?;
        if self.shared.config.auto_rasterizer {
            self.auto_planned = Some(planned);
        }
        let measuring = match (calibration, launch) {
            (Some(calibration), Some(launch)) => {
                // `settle` has already said why.
                if !settled {
                    crate::rasterizer_choice::announce_launch(&calibration, launch);
                }
                Measuring::start(calibration, launch)
            }
            _ => None,
        };
        // **`VIEWW_PLANNER_SHADOW=<file>`: the hybrid planner, in shadow.**
        //
        // Every `present_planned` frame is also planned by
        // `vieww_render_planner`'s placement planner and logged — one JSON
        // line per frame — against what actually drew it, without changing
        // what is drawn. See `NativeRenderer::enable_planner_shadow`. A
        // diagnostic that cannot open its file says so and the app runs on.
        if let Some(path) = std::env::var_os("VIEWW_PLANNER_SHADOW") {
            let path = std::path::PathBuf::from(path);
            match renderer.enable_planner_shadow(&path) {
                Ok(()) => trace_line(
                    self.shared.now(),
                    format_args!("startup: planner shadow log -> {}", path.display()),
                ),
                Err(error) => eprintln!(
                    "vieww: VIEWW_PLANNER_SHADOW={} could not be opened ({error}); \
                     running without the planner shadow",
                    path.display()
                ),
            }
        }
        // **`VIEWW_PLANNER_AUTHORITATIVE=1`: let the planner's mode machine
        // run to completion.** It still changes nothing until its own gate
        // clears — `ShadowPlanner`'s doc has the machine and the numbers —
        // and requires the shadow log above, because the gate is fed by
        // that log's observations.
        if std::env::var("VIEWW_PLANNER_AUTHORITATIVE").as_deref() == Ok("1") {
            match renderer.enable_planner_authoritative() {
                Ok(()) => trace_line(
                    self.shared.now(),
                    format_args!("startup: planner authoritative enabled (gate-fed by the shadow log)"),
                ),
                Err(reason) => eprintln!("vieww: VIEWW_PLANNER_AUTHORITATIVE ignored: {reason}"),
            }
        }
        crate::startup::mark("present ready");
        trace_line(
            self.shared.now(),
            format_args!(
                "startup: {:?} present ready in {:?}: {}",
                surface.path(),
                opening.elapsed(),
                renderer.adapter_name()
            ),
        );

        Ok(NewSurface {
            window,
            renderer,
            surface,
            planned,
            measuring,
            scale,
            width,
            height,
        })
    }

    /// This device's rasterizer calibration file for a `width`×`height`
    /// window — `VIEWW_RENDERER_CACHE` if set, else the application's data
    /// directory. `None` where there is nowhere to keep it (an Android
    /// activity with no internal data path), and the window then takes the CPU
    /// path, as if `auto_rasterizer` had not been asked for.
    fn calibration(&self, width: u32, height: u32) -> Option<Calibration> {
        let file = if let Some(file) = std::env::var_os("VIEWW_RENDERER_CACHE") {
            std::path::PathBuf::from(file)
        } else {
            crate::rasterizer_choice::file_in(&self.data_dir()?)
        };
        Some(Calibration::new(file, width, height))
    }

    /// The application's own data directory: the activity's internal data
    /// path on Android, `storage::data_dir` elsewhere.
    fn data_dir(&self) -> Option<std::path::PathBuf> {
        #[cfg(target_os = "android")]
        {
            self.shared
                .android
                .as_ref()
                .and_then(AndroidApp::internal_data_path)
        }
        #[cfg(not(target_os = "android"))]
        {
            Some(crate::storage::data_dir())
        }
    }

    /// Where the GPU's pipeline cache is kept between launches —
    /// `VIEWW_PIPELINE_CACHE=<file>`, `=off` for none, else a file in the data
    /// directory named after the executable, as the rasterizer calibration is.
    fn pipeline_cache_file(&self) -> Option<std::path::PathBuf> {
        match std::env::var_os("VIEWW_PIPELINE_CACHE") {
            Some(value) if value == "off" => None,
            Some(file) => Some(std::path::PathBuf::from(file)),
            None => {
                let calibration = crate::rasterizer_choice::file_in(&self.data_dir()?);
                let name = calibration
                    .file_name()?
                    .to_string_lossy()
                    .replacen("rasterizer-", "pipelines-", 1)
                    .replace(".txt", ".bin");
                Some(calibration.with_file_name(name))
            }
        }
    }

    /// Attach the accessibility adapter for a window that has just been made.
    fn attach_accessibility(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _window: &Arc<Window>,
    ) -> Option<PlatformA11y> {
        // Android's adapter attaches to the activity's view rather than to the
        // window, so it needs the activity and not the `ActiveEventLoop`. It is
        // also allowed to decline — an old device, or a view that already has a
        // delegate — and an app that will not start is a far worse outcome than
        // one a screen reader cannot read.
        //
        // **The first attempt is expected to fail**, and that is not a defect:
        // the activity's view is usually not attached to a window yet on
        // `resumed`. It is retried after every frame until it takes.
        #[cfg(target_os = "android")]
        {
            None
        }
        #[cfg(not(target_os = "android"))]
        {
            Some(AccessAdapter::with_event_loop_proxy(
                _event_loop,
                _window,
                self.shared.proxy.clone(),
            ))
        }
    }

    /// Open one window and mount a tree in it.
    fn create(
        &mut self,
        event_loop: &ActiveEventLoop,
        key: WindowKey,
        title: &str,
        size: Size,
        background: Color,
        build: impl FnOnce(&mut FrameDriver),
    ) -> Result<(), PlatformError> {
        let NewSurface {
            window,
            renderer,
            surface,
            planned,
            measuring,
            scale,
            width,
            height,
        } = self.surface(event_loop, title, size)?;
        // Each step from here to the first event is traced too: the trace that
        // localised the wait_loop hang ended at `GPU ready` with nothing after
        // it for 20 s, and every step below could be the one — AccessKit's
        // D-Bus connection to the accessibility bus (a blocking call with a
        // 25 s default timeout when the bus has to be activated), the system
        // font scan on a cold disk cache, or the compositor.
        let accessibility = self.attach_accessibility(event_loop, &window);
        trace_line(
            self.shared.now(),
            format_args!("startup: accessibility attached"),
        );
        window.set_visible(true);
        trace_line(
            self.shared.now(),
            format_args!("startup: window made visible"),
        );
        crate::startup::mark("accessibility and show");

        let logical = scale.to_logical_size(width, height);
        let mut log = FrameLog::new(
            Duration::from_secs_f32(1.0 / self.shared.config.refresh_hz),
            0,
        );
        log.set_window(key);
        log.resize(u64::from(width) * u64::from(height));

        // `with_runtime`, not `new`: every window in this process shares one
        // reactive graph, so a signal written in one is seen by the other and a
        // document open twice is one document. This is also what makes
        // `open_windows` — minted from the same runtime — mark the right
        // elements pending.
        let mut driver = FrameDriver::with_runtime(logical, self.runtime.clone());
        // The system font fallback: the default store is embedded-only (Latin,
        // Hebrew, Arabic) and a windowed application is by definition on a
        // machine with fonts — without it a shipped app renders Chinese,
        // Japanese, Korean, Devanagari and Thai as blank space (see
        // `FontStore::with_system_fallback`).
        //
        // **In the background, not before the first frame.** This was
        // `use_system_fonts()`, a full scan of every system font on this
        // thread before the window could draw. Its cost is the machine's disk,
        // not vieww's: on a GNOME laptop whose font files an 11-minute build had
        // evicted from the page cache, it ran past `tests/wait_loop.rs`' 20 s
        // deadline — the startup trace ended at `window made visible` with
        // `system fonts loaded` never printed — while every warm run passed.
        // A user launching after a reboot paid the same. Now the first frames
        // draw with the embedded faces, and the scan's thread wakes this loop
        // when it is done so the next frame adopts the full fallback (see
        // `FrameDriver::use_system_fonts_in_background`).
        let proxy = self.shared.proxy.clone();
        driver.use_system_fonts_in_background(move || {
            // An error means the loop already stopped: nobody to show them to.
            let _ = proxy.send_event(UserEvent::Wake);
        });
        trace_line(
            self.shared.now(),
            format_args!("startup: system font scan started"),
        );
        // Before `build`, so the application's own `set_root` is published
        // under the theme rather than the theme having to re-mount the tree.
        if let Some(theme) = &self.shared.config.theme {
            driver.set_theme(std::rc::Rc::clone(theme));
        }
        build(&mut driver);
        crate::startup::mark("tree built");
        trace_line(self.shared.now(), format_args!("startup: tree built"));

        let mut input = PointerTranslator::new(Scale::ONE);
        input.set_scale(scale);

        let mut state = WindowState {
            key,
            cursor: vieww_foundation::Cursor::default(),
            id: window.id(),
            scheduler: FrameScheduler::new(self.shared.config.refresh_hz),
            driver,
            gpu: None,
            input,
            keys: KeyTranslator::new(),
            scale,
            accessibility,
            ime_open: false,
            ime_composing: false,
            insets: SystemInsets::NONE,
            inset_watch: InsetWatch::new(),
            log,
            lifecycle: Lifecycle::default(),
            presented: false,
            background,
            display: None,
            display_checked: None,
            cpu_paced_until: None,
        };

        state.follow_display(&self.shared, &window);
        trace_line(self.shared.now(), format_args!("startup: display read"));
        state.gpu = Some(Gpu {
            window,
            renderer: ManuallyDrop::new(renderer),
            surface,
            needs_full_repaint: true,
            planned,
            measuring,
            hint: None,
            hint_tried: false,
            pipelines_saved: false,
            chores: crate::frame_budget::FrameBudgetGuard::default(),
        });
        state.scheduler.request_frame();
        // The window's density is a fact the tree can read, and a `SafeArea`
        // needs it before the first layout rather than after it.
        state.refresh_insets(&self.shared);
        crate::startup::mark("window ready");
        trace_line(self.shared.now(), format_args!("startup: window ready"));

        if let Some(context) = &self.shared.crash_context {
            context.set("surface", format!("{}x{}", logical.width, logical.height));
            context.set("density", scale.factor().to_string());
        }

        self.windows.push(state);
        // Set here rather than in `resumed`, because this is the one place every
        // window comes through — the primary at startup, a window the
        // application asked for, and an Android surface rebuilt after a suspend.
        self.has_opened = true;
        Ok(())
    }

    /// Give a window its surface back after a suspend.
    ///
    /// The tree is untouched; only its pixels are in question. Android is the
    /// only platform that reaches this, and it only ever has the one window.
    fn reopen(&mut self, event_loop: &ActiveEventLoop, index: usize) -> Result<(), PlatformError> {
        let (title, size) = (self.shared.config.title.clone(), self.shared.config.size);
        let NewSurface {
            window,
            renderer,
            surface,
            planned,
            measuring,
            scale,
            width,
            height,
        } = self.surface(event_loop, &title, size)?;
        let accessibility = self.attach_accessibility(event_loop, &window);
        window.set_visible(true);

        {
            let shared = &self.shared;
            let state = &mut self.windows[index];
            state.id = window.id();
            state.scale = scale;
            state.input.set_scale(scale);
            state.accessibility = accessibility;
            state.log.resize(u64::from(width) * u64::from(height));
            state.driver.resize(scale.to_logical_size(width, height));
            // Before the window moves into `Gpu`, which is the only reason this
            // is ordered rather than grouped with the rest.
            state.follow_display(shared, &window);
            state.gpu = Some(Gpu {
                window,
                renderer: ManuallyDrop::new(renderer),
                surface,
                needs_full_repaint: true,
                planned,
                measuring,
                hint: None,
                hint_tried: false,
                pipelines_saved: false,
                chores: crate::frame_budget::FrameBudgetGuard::default(),
            });
            state.scheduler.request_frame();
        }

        // Split out because it needs `&self.shared` and the block above holds
        // `&mut self.windows`.
        let shared = &self.shared;
        self.windows[index].refresh_insets(shared);
        Ok(())
    }

    /// Everything the application asked for while the loop was busy or asleep.
    ///
    /// Drained in `about_to_wait`, which is the one callback that has an
    /// `ActiveEventLoop` and runs after every batch of events — so a window
    /// asked for from a button handler appears on the same turn of the loop as
    /// the click that asked for it.
    fn service_requests(&mut self, event_loop: &ActiveEventLoop) {
        // Bound before the loop, not iterated in place. Temporaries in a `for`
        // scrutinee live for the whole loop, so `for p in self.requests.take()`
        // would hold a borrow of `self.requests` across a body that needs
        // `&mut self`.
        let opens = self.requests.take_opens();
        let closes = self.requests.take_closes();

        for pending in opens {
            let spec = pending.spec;
            let key = pending.key;
            let build = pending.build;
            if let Err(error) = self.create(
                event_loop,
                key,
                spec.title(),
                spec.size(),
                spec.background(),
                |driver| build(key, driver),
            ) {
                // A window that could not be created is not an application
                // failure: the request came from a menu item, and the honest
                // outcome is that the window is not open. Forgetting it keeps
                // `open_windows` truthful, which is the whole contract.
                log::error!("vieww: {key} could not be opened: {error}");
                self.requests.forget(key);
            }
        }

        let closed_any = !closes.is_empty();
        for key in closes {
            self.destroy(key);
        }

        // **The half that was missing.** A programmatic close reaches here and
        // nowhere near the `CloseRequested` arm, so without this the last window
        // could be destroyed and the loop would carry on with nothing to show.
        // Guarded on having actually closed something so that the ordinary
        // no-requests turn — which is almost every turn — costs one boolean.
        if closed_any && self.last_window_closed() {
            event_loop.exit();
        }
    }

    /// Close a window and everything that belonged to it.
    ///
    /// The order is `WindowSet`'s: dialogs before the window they sit on, so a
    /// parent is never destroyed while a child still points at it.
    fn destroy(&mut self, key: WindowKey) {
        // Before anything is removed: this is the last moment the primary
        // window's log exists, and `App::run`'s whole return value is that log.
        // See `last_report`.
        if key == WindowKey::PRIMARY {
            if let Some(window) = self.primary() {
                self.last_report = Some(window.log.report());
            }
        }
        let focus_to = self.requests.focus_after_closing(key);
        // Bound first, for the reason `service_requests` gives.
        let doomed = self.requests.forget(key);
        for doomed in doomed {
            if let Some(index) = self.windows.iter().position(|w| w.key == doomed) {
                let mut state = self.windows.remove(index);
                state.set_lifecycle(state.lifecycle.detached());
                state.drop_surface();
            }
        }
        // Focus returns to the parent, which is the promise `WindowRole::Dialog`
        // makes and the one winit will not keep for us on any platform.
        if let Some(parent) = focus_to {
            if let Some(window) = self.by_key(parent) {
                if let Some(gpu) = &window.gpu {
                    gpu.window.focus_window();
                }
            }
        }
    }

    /// `true` when the application's last window has gone.
    ///
    /// **This rule used to live in one place and needed to be in two.** It was
    /// written inline in the `WindowEvent::CloseRequested` arm — the path where
    /// the *user* clicks the close button — and nowhere else. A close asked for
    /// by the application itself goes through `Requests` and is carried out by
    /// `service_requests`, which destroyed the window and never asked whether
    /// that was the last one. The result was a process with no window, no way to
    /// get one back, and an event loop still running: an application that closes
    /// itself from a background task — a sync finishing, a sign-out — stayed
    /// alive and invisible.
    ///
    /// Nothing about it was detectable without a real window, which is how it
    /// survived; `tests/wait_loop.rs` found it within an hour of existing.
    ///
    /// The two conditions are both here because they mean different things.
    /// `windows` is what is on screen; `requests` is the [`WindowSet`](vieww_render::window::WindowSet) including
    /// windows asked for and not yet created, so a close racing an open does not
    /// end the application early.
    ///
    /// [`WindowSet`]: vieww_render::WindowSet
    fn last_window_closed(&self) -> bool {
        self.has_opened && (self.requests.is_empty() || self.windows.is_empty())
    }

    /// `true` once the configured stopping point is reached.
    fn is_finished(&self) -> bool {
        self.shared.failure.is_some()
            || match self.shared.config.stop {
                Stop::OnClose => false,
                Stop::AfterFrames(frames) => self
                    .primary()
                    .is_some_and(|window| window.log.frames() >= frames),
            }
    }
}

/// A window that could not open: record why, say why, and stop.
///
/// # Said, not only recorded
///
/// `App::run` hands the recorded error back once `run_app` returns — but on
/// iOS **`run_app` never returns** (`UIApplicationMain` owns the process) and
/// `exit()` is ignored, so a GPU that failed to open left a black screen and
/// no word anywhere. That is what the first iOS simulator runs in CI showed: a
/// startup trace ending at "opening the GPU", an empty stderr, and an app that
/// was still "running". So the reason is printed here, on every platform, and
/// on iOS the process ends — a failure that looks like a failure.
fn fail_to_open(
    failure: &mut Option<PlatformError>,
    error: PlatformError,
    event_loop: &ActiveEventLoop,
) {
    eprintln!("vieww: the window could not open: {error}");
    *failure = Some(error);
    event_loop.exit();
    #[cfg(target_os = "ios")]
    std::process::exit(1);
}

impl<F: FnOnce(&mut FrameDriver)> ApplicationHandler<UserEvent> for Runner<F> {
    fn new_events(&mut self, _event_loop: &ActiveEventLoop, cause: winit::event::StartCause) {
        if matches!(cause, winit::event::StartCause::Init) {
            crate::startup::mark("event loop started");
        }
    }

    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.windows.is_empty() {
            crate::startup::mark("resumed");
            let build = self.build.take();
            let (title, size, background) = (
                self.shared.config.title.clone(),
                self.shared.config.size,
                self.shared.config.background,
            );
            if let Err(error) = self.create(
                event_loop,
                WindowKey::PRIMARY,
                &title,
                size,
                background,
                |driver| {
                    if let Some(build) = build {
                        build(driver);
                    }
                },
            ) {
                fail_to_open(&mut self.shared.failure, error, event_loop);
                return;
            }
        } else {
            // Android, coming back from a suspend: the trees are all still here
            // and their surfaces are not.
            for index in 0..self.windows.len() {
                if self.windows[index].gpu.is_none() {
                    if let Err(error) = self.reopen(event_loop, index) {
                        fail_to_open(&mut self.shared.failure, error, event_loop);
                        return;
                    }
                }
            }
        }

        for window in &mut self.windows {
            window.set_lifecycle(window.lifecycle.resumed());
            // Anything could have happened while this app was in the background
            // — a rotation, the navigation bar being switched to gestures, the
            // keyboard being dismissed by whatever had focus next. The insets on
            // the way back in are not the ones on the way out.
            window.inset_watch.disturbed();
        }
        if let Some(context) = &self.shared.crash_context {
            context.set("lifecycle", "resumed");
        }
    }

    fn suspended(&mut self, _event_loop: &ActiveEventLoop) {
        // In this order: the pointers have to be cancelled while the tree can
        // still receive them, and the surface has to go before the platform
        // reclaims it underneath us.
        let shared = &self.shared;
        for window in &mut self.windows {
            window.cancel_pointers(shared);
            window.set_lifecycle(window.lifecycle.suspended());
            window.drop_surface();
        }
        // Each window's `vieww-hal` `VulkanDevice` went with its `Gpu` above
        // — there is no separate shared context to drop a second time (see
        // `crate::native`'s module docs on why there is no
        // `vieww_paint::gpu::GpuContext` equivalent here). A renderer that
        // outlives its surface is how a backgrounded application crashes on
        // the way back, which is why `drop_surface` above takes the device
        // with it rather than trying to keep it warm for `resumed` to reuse.
        if let Some(context) = &self.shared.crash_context {
            // Worth a breadcrumb of its own: a crash while suspended is a
            // different bug from a crash on screen, and the two are otherwise
            // indistinguishable in a report.
            context.set("lifecycle", "suspended");
        }
    }

    /// The OS says memory is short.
    ///
    /// winit delivers this from Android's `onTrimMemory` and iOS's
    /// `didReceiveMemoryWarning`; the desktop backends never call it. See
    /// [`MemoryPressure`](vieww_foundation::MemoryPressure) for the mapping and
    /// for why iOS's single warning is `Critical` rather than `Moderate`.
    ///
    /// # Why the level is `Critical` and not read from the platform
    ///
    /// Because winit does not carry one. `memory_warning` is a bare
    /// notification with no severity attached, so the only honest reading of it
    /// is the more serious of the two things it can mean — on iOS it *is* the
    /// serious one, and on Android it collapses `RUNNING_MODERATE` in with the
    /// rest. Trimming too much costs one slow frame; trimming too little on the
    /// platform that sent this costs the process.
    ///
    /// If winit later exposes Android's level, this is the one place that
    /// changes: everything below already takes a `MemoryPressure`.
    fn memory_warning(&mut self, _event_loop: &ActiveEventLoop) {
        use vieww_foundation::MemoryPressure;

        const LEVEL: MemoryPressure = MemoryPressure::Critical;

        if let Some(context) = &self.shared.crash_context {
            // A memory warning shortly before a crash is most of the diagnosis,
            // and it is invisible in a report without this.
            context.set("memory_warning", "critical");
        }

        // Every window's driver: the text caches and the retained scene.
        // There is no shared GPU context to trim a second time here — see
        // `crate::native`'s module docs on why each window's `vieww-hal`
        // device is its own.
        for window in &mut self.windows {
            window.driver.trim_memory(LEVEL);
        }
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: UserEvent) {
        use accesskit_winit::WindowEvent as AccessWindowEvent;

        let event = match event {
            UserEvent::Access(event) => event,
            // The same two questions as below, arriving from the injected
            // Android delegate. Answered by the same two methods, so the only
            // thing this arm does is unwrap a different envelope.
            #[cfg(target_os = "android")]
            UserEvent::AndroidAccess(request) => {
                // Android has one window, so the primary is the only candidate.
                if let Some(index) = self
                    .windows
                    .iter()
                    .position(|w| w.key == WindowKey::PRIMARY)
                {
                    let (windows, shared) = (&mut self.windows, &mut self.shared);
                    match request {
                        crate::a11y_android::Request::InitialTree => {
                            windows[index].publish_semantics(shared);
                        }
                        crate::a11y_android::Request::Action(request) => {
                            windows[index].access_action(shared, &request);
                        }
                    }
                }
                if self.is_finished() {
                    event_loop.exit();
                }
                return;
            }
            // A worker finished, or a window was asked for. Neither can mark
            // anything pending from over there, so all this does is guarantee a
            // frame happens — the waiting element's `take_pending` runs in it
            // and notices the value, and `about_to_wait` services the queue.
            UserEvent::Wake => {
                for window in &mut self.windows {
                    window.scheduler.request_frame();
                }
                if self.is_finished() {
                    event_loop.exit();
                }
                return;
            }
        };

        // AccessKit reports the window the request is about, which is how a
        // screen reader driving a dialog reaches the dialog's tree rather than
        // its parent's.
        let target = self
            .windows
            .iter()
            .position(|window| window.id == event.window_id);
        if let Some(index) = target {
            let (windows, shared) = (&mut self.windows, &mut self.shared);
            match event.window_event {
                // A screen reader has just attached. It needs the tree
                // immediately; there is no frame coming, because nothing
                // changed.
                AccessWindowEvent::InitialTreeRequested => {
                    windows[index].publish_semantics(shared);
                }
                // A screen reader asked us to do something — activate a control,
                // increment a slider, scroll a list. This is the whole reason a
                // screen reader user can *use* an application rather than only
                // hear it described.
                AccessWindowEvent::ActionRequested(request) => {
                    windows[index].access_action(shared, &request);
                }
                AccessWindowEvent::AccessibilityDeactivated => {}
            }
        }
        if self.is_finished() {
            event_loop.exit();
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        let now = self.shared.now();

        // `Occluded` carries the one bit that decides whether this window may
        // draw at all, so it is the event the trace names with its payload.
        if let WindowEvent::Occluded(hidden) = event {
            trace_line(now, format_args!("event Occluded({hidden})"));
        } else {
            trace_line(now, format_args!("event {}", event_name(&event)));
        }

        let Some(index) = self.windows.iter().position(|w| w.id == window_id) else {
            // An event for a window that has already gone. Ordinary during a
            // close, and nothing here can act on it.
            return;
        };

        // A parent with a dialog on it takes no input, which is the promise
        // `WindowRole::Dialog` makes and the one no desktop will keep for us
        // portably. Geometry and lifecycle still get through, because a window
        // that stopped resizing itself while a sheet was up would be visibly
        // broken; only what a *user* does is refused.
        let inert = self.requests.is_inert(self.windows[index].key)
            && matches!(
                event,
                WindowEvent::CursorMoved { .. }
                    | WindowEvent::MouseInput { .. }
                    | WindowEvent::MouseWheel { .. }
                    | WindowEvent::Touch(_)
                    | WindowEvent::KeyboardInput { .. }
                    | WindowEvent::Ime(_)
            );
        if inert {
            return;
        }

        // The adapter watches for focus and window-geometry changes it has to
        // report on its own. Given the event before we act on it, so it cannot
        // observe a window state we have already moved past.
        {
            let window = &mut self.windows[index];
            if let (Some(adapter), Some(gpu)) = (&mut window.accessibility, &window.gpu) {
                adapter.process_event(&gpu.window, &event);
            }
        }

        match event {
            WindowEvent::CloseRequested => {
                // Asked before anything is destroyed, because after the destroy
                // there is nothing left to keep. See `App::on_close_request`.
                let size = self.windows[index].driver.size();
                let last = self.windows.len() <= 1;
                if let Some(hook) = self.shared.config.on_close.as_mut() {
                    let request = CloseRequest {
                        width: size.width,
                        height: size.height,
                        last,
                    };
                    if hook(request) == Closing::Vetoed {
                        // Left exactly as it was, so the next frame can draw
                        // whatever the veto is waiting on. The frame is asked
                        // for here rather than left to the application: the
                        // close button is not an input event that would have
                        // caused one, so without this the dialog would not
                        // appear until the user moved the mouse.
                        self.windows[index].scheduler.request_frame();
                        return;
                    }
                }
                let key = self.windows[index].key;
                self.destroy(key);
                // The application ends when its last window does, which is what
                // closing the only window has always meant. The rule itself is
                // in `last_window_closed`, because `service_requests` needs the
                // same one and having two copies is what produced the bug that
                // method documents.
                if self.last_window_closed() {
                    event_loop.exit();
                }
                return;
            }
            WindowEvent::Resized(size) => self.windows[index].resize(size.width, size.height),
            WindowEvent::Moved(_) => {
                let shared = &self.shared;
                self.windows[index].display_may_have_changed(shared);
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                let shared = &self.shared;
                self.windows[index].display_may_have_changed(shared);
                let window = &mut self.windows[index];
                window.scale = Scale::new(scale_factor);
                window.input.set_scale(window.scale);
                // The new physical size arrives in a `Resized` of its own, but
                // the logical size has already changed underneath the tree.
                if let Some(gpu) = &window.gpu {
                    let size = gpu.window.inner_size();
                    let (width, height) = (size.width, size.height);
                    window.resize(width, height);
                }
            }
            WindowEvent::Focused(focused) => {
                let shared = &self.shared;
                let window = &mut self.windows[index];
                window.set_lifecycle(window.lifecycle.focused(focused));
                if !focused {
                    // A window losing focus mid-gesture never sees the release.
                    window.cancel_pointers(shared);
                    // And never sees the modifiers come up either, so a Control
                    // held while alt-tabbing away would still be held on the
                    // way back and turn the next letter into a shortcut.
                    window.keys.release_modifiers();
                }
            }
            // One event per file, and no position on any of them — which is
            // what makes the whole feature window-level. `vieww_foundation`'s
            // `file_drop` module carries the reasoning.
            WindowEvent::HoveredFile(_) => {
                let window = &mut self.windows[index];
                if window.driver.set_file_drag(FileDrag::Hovering) {
                    window.scheduler.request_frame();
                }
            }
            WindowEvent::HoveredFileCancelled => {
                let window = &mut self.windows[index];
                if window.driver.set_file_drag(FileDrag::Idle) {
                    window.scheduler.request_frame();
                }
            }
            WindowEvent::DroppedFile(path) => {
                let window = &mut self.windows[index];
                // Gathered, not delivered: a four-file drop arrives as four
                // events and is one action. The hover state going idle is
                // what ends the gathering, below.
                window.driver.push_dropped_file(path);
                // A drop implies the hover is over — winit does not send a
                // cancel after a successful drop, so a zone left lit is what
                // happens if this is omitted.
                window.driver.set_file_drag(FileDrag::Idle);
                window.scheduler.request_frame();
            }
            WindowEvent::Occluded(occluded) => {
                let window = &mut self.windows[index];
                window.set_lifecycle(window.lifecycle.occluded(occluded));
                if !occluded {
                    // No persistent target to re-blit without repainting the
                    // way `vieww_paint::gpu::WindowSurface` had (see
                    // `crate::native`'s module docs) — a requested frame
                    // repaints in full, which for this backend is what every
                    // frame already does.
                    window.scheduler.request_frame();
                }
            }
            // A guard rather than an `if` inside the arm, because clippy's
            // `collapsible_match` is denied in this workspace. A window that may
            // not draw falls through to the catch-all below and does nothing,
            // which is what the `if` did.
            WindowEvent::RedrawRequested if self.windows[index].may_draw() => {
                let (windows, shared) = (&mut self.windows, &mut self.shared);
                windows[index].draw(shared);
            }
            WindowEvent::CursorMoved { position, .. } => {
                let window = &mut self.windows[index];
                // **Android's touch exploration arrives here**, not as a touch.
                // With TalkBack on, a finger dragged over the screen is a hover,
                // and this is the only place it can be handed to the screen
                // reader. Physical pixels, deliberately: the delegate works in
                // the view's own coordinate space, and `input.cursor_moved`
                // below converts to logical for the tree.
                //
                // Does nothing until `ci/tools/patch-winit.sh` has been applied —
                // stock winit 0.30.13 never delivers a hover on Android at all.
                #[cfg(target_os = "android")]
                if let Some(adapter) = &mut window.accessibility {
                    #[expect(
                        clippy::cast_possible_truncation,
                        reason = "a window coordinate is a small number of pixels"
                    )]
                    adapter.on_hover(
                        crate::a11y_android::ACTION_HOVER_MOVE,
                        position.x as f32,
                        position.y as f32,
                    );
                }
                let event = window.input.cursor_moved((position.x, position.y), now);
                window.pointer(event);
                window.hover();
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let Some(button) = pointer_button(button) else {
                    // A back or forward thumb button. Carrying it would mean a
                    // vocabulary for buttons nothing in the framework reacts to.
                    return;
                };
                let window = &mut self.windows[index];
                let event = window
                    .input
                    .mouse_button(button, state == ElementState::Pressed, now);
                window.pointer(event);
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let window = &mut self.windows[index];
                let delta = match delta {
                    MouseScrollDelta::LineDelta(x, y) => WheelDelta::Lines(x, y),
                    MouseScrollDelta::PixelDelta(position) => {
                        let scale = window.scale.factor();
                        WheelDelta::Pixels((position.x as f32) / scale, (position.y as f32) / scale)
                    }
                };
                if let Some(event) = window.input.scroll(delta, now) {
                    if window.driver.handle_scroll(&event) {
                        window.scheduler.request_frame();
                    }
                }
            }
            WindowEvent::CursorLeft { .. } => {
                let window = &mut self.windows[index];
                // The exit edge, which is what tells a screen reader to drop its
                // focus rectangle rather than leave it on the last control the
                // finger crossed.
                #[cfg(target_os = "android")]
                if let Some(adapter) = &mut window.accessibility {
                    adapter.on_hover(crate::a11y_android::ACTION_HOVER_EXIT, 0.0, 0.0);
                }
                let event = window.input.cursor_left(now);
                window.pointer(event);
                window.hover();
            }
            WindowEvent::Touch(touch) => {
                let window = &mut self.windows[index];
                // `Force::normalized` is done here, at the one call site that
                // is allowed to know `winit::event::Force` exists —
                // `input.rs`'s own module doc explains why its translator
                // takes a plain `Option<f32>` instead.
                #[expect(
                    clippy::cast_possible_truncation,
                    reason = "a normalized force is 0.0..=1.0; f64->f32 here costs no precision that matters"
                )]
                let pressure = touch.force.map(|force| force.normalized() as f32);
                let event = window.input.touch(
                    touch.id,
                    touch.phase.into(),
                    (touch.location.x, touch.location.y),
                    now,
                    pressure,
                );
                window.pointer(event);
            }

            WindowEvent::ModifiersChanged(modifiers) => {
                self.windows[index].keys.set_modifiers(modifiers.state());
            }
            // A key that produced a composition is the input method's, not
            // ours: `Ime::Preedit` will report the result. Handling both
            // types the letter twice.
            //
            // **Gated on composing rather than on the IME being open.** Those
            // are not the same fact, and using the second cost every keystroke:
            // an allowed IME that is composing nothing — ASCII on a Latin
            // layout, which is most typing — passes the key through with its
            // text and opens no preedit, so suppressing on `ime_open` swallowed
            // every printable press for as long as a field had focus. Verified
            // against winit 0.30 on X11 on 2026-08-16 by logging both arms: the
            // press arrived carrying `text: Some("a")` while `Ime::Enabled` had
            // been delivered and no `Preedit` ever was.
            WindowEvent::KeyboardInput { event, .. }
                if !self.windows[index].ime_composing || !event.state.is_pressed() =>
            {
                let window = &mut self.windows[index];
                if let Some(key) = window.keys.key(&event, now) {
                    window.key(&key);
                }
            }
            WindowEvent::Ime(event) => {
                let window = &mut self.windows[index];
                let event = crate::keys::ime(&event);
                // Before the driver sees it, so a preedit that arrives in the
                // same burst as its key is already suppressing by the time the
                // key is looked at.
                window.ime_composing =
                    matches!(&event, ImeEvent::Preedit { text, .. } if !text.is_empty());
                if window.driver.handle_ime(&event) {
                    window.scheduler.request_frame();
                }
                window.sync_ime();
            }

            _ => {}
        }

        if self.is_finished() {
            event_loop.exit();
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if self.is_finished() {
            event_loop.exit();
            return;
        }

        // Before the decision below, so a window asked for during this batch of
        // events is on screen and drawing on the same turn of the loop rather
        // than after the next thing the user does.
        self.service_requests(event_loop);

        if self.windows.is_empty() {
            event_loop.set_control_flow(ControlFlow::Wait);
            return;
        }

        // **The decision itself lives in `vieww_render::next_action`**, so that
        // `LoopHarness` drives the same code with no window, no GPU and no
        // compositor. A test that reimplemented this rule would be testing the
        // reimplementation — which is how both fixes reverted on 2026-08-11 came
        // to look correct against a green suite.
        //
        // Asked per window, and drawn per window. Two windows sharing one
        // runtime is exactly the case `FrameDriver::owes_rebuild` was written
        // for: input arriving at one leaves the other pending with nobody
        // asking, and the answer is that every window is asked every turn.
        // Whether a window that drew presents through a swapchain, whose
        // blocking present paces a `Poll` loop. A CPU-presented window does
        // not block and is paced by `wake_at` instead.
        let mut poll = false;
        // The soonest moment any window says it will next want a frame. Only
        // consulted when nothing wants one *now* — see `LoopAction::SleepUntil`
        // and the `ControlFlow` decision at the bottom of this function.
        let mut wake_at: Option<Duration> = None;
        for index in 0..self.windows.len() {
            // Recorded before the decision, because `next_action` re-requests on
            // the driver's behalf and would otherwise make its own input
            // unreadable.
            //
            // `needs_frame`, not `is_animating`: a finger held still on the
            // screen sends no events, and the deadline a long press is waiting
            // on can only be reached by a frame going past. Asking the narrower
            // question here is what made a long press unreachable in a real
            // window while passing every test, since a test draws frames in a
            // row.
            //
            // `vieww_paint::gpu::WindowSurface` used to carry a
            // `needs_present` flag this loop deliberately excluded from
            // `pending` — a window that could not present kept setting it on
            // every attempt, and treating it as a reason to draw turned a
            // blocking acquire into a spin. `crate::native::NativeSurface`
            // has no equivalent flag to exclude: `NativeRenderer::present`
            // either succeeds, silently rebuilds the swapchain and succeeds,
            // or returns a real error — there is no "try again next tick"
            // state to spin on. The states that make presenting possible
            // again (`Occluded(false)`, a resume, a resize, any input) still
            // request a frame of their own, same as before.
            let window = &mut self.windows[index];
            let pending = window.driver.needs_frame();
            let requested = window.scheduler.is_frame_requested();
            let can_draw = window.may_draw();
            let key = window.key;

            let now = self.shared.now();
            let mut action =
                next_action(&mut window.scheduler, Some(&window.driver), can_draw, now);
            // A CPU-presented window that drew less than a refresh interval ago
            // waits for the next one instead — see `cpu_paced_until`. The
            // request stays pending (only drawing clears it), so the frame is
            // deferred, never dropped.
            if action == LoopAction::Draw {
                if let Some(until) = window.cpu_paced_until.filter(|&until| until > now) {
                    action = LoopAction::SleepUntil(until);
                }
            }
            if let LoopAction::SleepUntil(at) = action {
                wake_at = Some(wake_at.map_or(at, |held: Duration| held.min(at)));
            }

            // The line that says whether the loop is about to sleep, and on
            // whose authority. A stall is one `WAIT` followed by silence; a
            // livelock is one `poll` followed by silence, and the *next* line's
            // timestamp is how long it lasted. Both read the same deduplicated
            // way.
            trace_line(
                self.shared.now(),
                format_args!(
                    "{} {key} requested={requested} pending={pending} should_draw={can_draw}",
                    match action {
                        LoopAction::Draw => "poll",
                        LoopAction::SleepUntil(_) => "TIMED",
                        LoopAction::Sleep => "WAIT",
                    },
                ),
            );

            if action == LoopAction::Draw {
                // **Draw here, rather than asking for a `RedrawRequested` and
                // drawing when it arrives.** The round trip through the window
                // was a deadlock: `request_redraw` is a no-op while winit
                // already holds a redraw as pending, so a redraw dropped once —
                // by the compositor, by occlusion, by pressure — leaves
                // `scheduler.requested` true for ever, every later
                // `request_redraw` no-ops against the phantom pending one, and
                // no redraw can arrive again.
                //
                // Measured in `examples/grid.rs`: a frame requested at 36.4s,
                // then `poll requested=true` on every iteration to 37.7s with no
                // `RedrawRequested` and no frame, `should_draw` true throughout,
                // and input still arriving at the mouse's 10.5ms report rate. A
                // window frozen mid-content and fully responsive underneath.
                //
                // Drawing directly makes the request *self-consuming*: `pulse`
                // clears it, so `wanted` goes false and the next
                // `about_to_wait` reaches `Wait`. It cannot be stranded, because
                // nothing outside this function has to cooperate.
                //
                // `RedrawRequested` still calls `draw` and is still needed: the
                // compositor asks for repaints nobody requested — first map,
                // expose, a resize. That path now finds `requested` already
                // false, and `draw` itself early-outs when nothing in the
                // tree changed (`stats.is_none()` — see `WindowState::draw`).
                //
                // **Only a lost redraw is fixed here.** A frame that is never
                // requested was the separate bug, and it is closed: layout
                // settles in place before painting, so a value discovered during
                // layout no longer needs a follow-up frame to reach the screen.
                // See
                // `a_signal_written_from_layout_is_shown_by_the_frame_that_wrote_it`.
                let (windows, shared) = (&mut self.windows, &mut self.shared);
                let window = &mut windows[index];
                window.draw(shared);
                if window
                    .gpu
                    .as_ref()
                    .is_some_and(|gpu| gpu.surface.path() == PresentPath::Cpu)
                {
                    let next = now + window.scheduler.budget();
                    window.cpu_paced_until = Some(next);
                    wake_at = Some(wake_at.map_or(next, |held: Duration| held.min(next)));
                } else {
                    poll = true;
                }
            } else {
                // And tell the log, because from inside it a long interval and a
                // stall are the same number. This decision is the only thing
                // that knows the difference: the gap that starts here is one
                // nobody was waiting on. Without it an untouched session reports
                // its idle time as its worst frame — 1979.50ms over 216 seconds,
                // and 48302.83ms in the run that verified the layout-write fix.
                self.windows[index].log.about_to_sleep();
            }
        }

        // A 120 Hz iPhone draws at 60 unless something asks for more while it
        // runs; the display link asks while frames are being drawn and stops
        // asking when the loop goes idle (`crate::ios::display_link_active`).
        #[cfg(target_os = "ios")]
        crate::ios::display_link_active(poll || wake_at.is_some());

        if poll {
            // Poll rather than Wait: the swapchain blocks on vsync inside
            // `present`, so the loop is paced by the display rather than by a
            // timer we would have to keep in step with it.
            event_loop.set_control_flow(ControlFlow::Poll);
        } else if let Some(at) = wake_at {
            // **Something is animating, but not yet.** A text caret is the
            // case: it blinks twice a second and answers `is_animating` for as
            // long as it exists, so before `LoopAction::SleepUntil` existed the
            // loop had no way to say "yes, but in 400ms" and answered `Poll`
            // for ever — one core at 100% on a window nobody was touching.
            //
            // `WaitUntil` sleeps like `Wait` and is additionally woken by the
            // deadline, so a keystroke still arrives immediately. The deadline
            // is on the frame timeline and the epoch is what converts it.
            //
            // Saturating: a deadline already past is a wake now, not a panic.
            event_loop.set_control_flow(ControlFlow::WaitUntil(self.shared.epoch + at));
        } else {
            // Nothing is moving anywhere. Sleep until something happens — this
            // is why an idle window costs no CPU.
            event_loop.set_control_flow(ControlFlow::Wait);
        }
    }

    fn exiting(&mut self, _event_loop: &ActiveEventLoop) {
        for window in &mut self.windows {
            window.set_lifecycle(window.lifecycle.detached());
            window.drop_surface();
        }
    }
}

/// Say what shape of tree was handed to the platform adapter.
///
/// # Why this exists
///
/// On a Redmi Note 7 Pro with TalkBack running, the adapter activates and a root
/// node reaches Android with the window's bounds — and **nothing underneath it
/// does**, while `driver.semantics()` on the same frames demonstrably holds a
/// button and a text field, because the device suite locates them there and
/// prints their coordinates.
///
/// The two possibilities have completely different fixes, and no dump can tell
/// them apart from outside:
///
/// - **a thin count here** — the tree handed to `tree_update` is not the tree
///   the suite walks, so this is two different trees or two different moments;
/// - **a full count here** — the nodes left correctly and the loss is inside the
///   platform adapter, and the question becomes how it exposes virtual children.
///
/// # Only when it changes
///
/// This is called once per frame while a screen reader is attached, so it prints
/// only when the shape moves. A line a frame would bury the transition that
/// matters, which is the same reason `RenderViewport::report_extents` guards
/// itself — and the count settling is itself the interesting part.
fn report_published_tree(semantics: &vieww_render::SemanticsTree, mapped: bool) {
    use std::cell::Cell;

    thread_local! {
        static LAST: Cell<Option<(usize, usize, bool)>> = const { Cell::new(None) };
    }

    // The root's own child count, from the source side: `accesskit::Node` has no
    // getter for what `set_children` was given, and this is the same number.
    let children = semantics
        .root()
        .and_then(|root| semantics.node(root))
        .map_or(0, |node| node.children.len());

    let now = (semantics.len(), children, mapped);
    if LAST.get() == Some(now) {
        return;
    }
    LAST.set(Some(now));

    log::info!(
        "a11y: published {} node(s), root has {} child(ren){}",
        semantics.len(),
        children,
        if mapped {
            ""
        } else {
            " — tree_update declined, this is the empty fallback"
        }
    );
}

/// `true` when `VIEWW_TRACE_FRAMES` is set in the environment.
///
/// The same switch as [`vieww_render::FrameDriver`]'s own trace, deliberately, so
/// one variable turns on both halves and the two streams interleave in order on
/// stderr. Read once — this is consulted per window event.
fn tracing() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("VIEWW_TRACE_FRAMES").is_some())
}

/// Print `what`, with a timestamp, unless it is identical to the line before it.
///
/// # This suppression is the difference between an instrument and a fault
///
/// The first version printed unconditionally. `stderr` is unbuffered, so a line
/// per iteration of a `ControlFlow::Poll` loop is a write syscall per iteration —
/// measured at **~23,000 lines per second**, and it starved the very
/// `RedrawRequested` delivery the trace was watching for: `examples/grid.rs` went
/// from 80 presented frames and working scrolling to one frame and a dead window,
/// with the *trace* as the only change. The observer became the bug, which is the
/// oldest mistake in this file and now has a fifth entry.
///
/// Repeats carry no information here anyway. Every question this answers — did an
/// event arrive, did the loop sleep, how long was the gap — is answered by the
/// **transitions** and their timestamps. A thousand identical `poll` lines say
/// exactly what one says, followed by the next line's clock.
fn trace_line(now: Duration, what: std::fmt::Arguments<'_>) {
    use std::cell::RefCell;

    if !tracing() {
        return;
    }
    let line = format!("{what}");
    thread_local! {
        static LAST: RefCell<String> = const { RefCell::new(String::new()) };
    }
    LAST.with_borrow_mut(|last| {
        if *last == line {
            return;
        }
        last.clear();
        last.push_str(&line);
        eprintln!("{:>8.3} {line}", now.as_secs_f64());
    });
}

/// A window event's name, for the trace.
///
/// # Why this is a hand-written match rather than `{event:?}`
///
/// `WindowEvent` is `#[non_exhaustive]` and its `Debug` prints the payload, which
/// for `CursorMoved` is a coordinate pair per pixel of travel. The question this
/// instrument exists to answer is *which events arrive during a stall*, and a
/// thousand lines of coordinates is how that answer gets buried.
fn event_name(event: &WindowEvent) -> &'static str {
    match event {
        WindowEvent::CloseRequested => "CloseRequested",
        WindowEvent::RedrawRequested => "RedrawRequested",
        WindowEvent::CursorMoved { .. } => "CursorMoved",
        WindowEvent::CursorEntered { .. } => "CursorEntered",
        WindowEvent::CursorLeft { .. } => "CursorLeft",
        WindowEvent::MouseInput { .. } => "MouseInput",
        WindowEvent::MouseWheel { .. } => "MouseWheel",
        WindowEvent::Touch(_) => "Touch",
        WindowEvent::KeyboardInput { .. } => "KeyboardInput",
        WindowEvent::Ime(_) => "Ime",
        WindowEvent::Resized(_) => "Resized",
        WindowEvent::ScaleFactorChanged { .. } => "ScaleFactorChanged",
        WindowEvent::Focused(_) => "Focused",
        WindowEvent::Occluded(_) => "Occluded",
        _ => "other",
    }
}

/// `winit`'s mouse button as ours, or `None` for one we have no name for.
const fn pointer_button(button: MouseButton) -> Option<PointerButton> {
    match button {
        MouseButton::Left => Some(PointerButton::Primary),
        MouseButton::Right => Some(PointerButton::Secondary),
        MouseButton::Middle => Some(PointerButton::Middle),
        MouseButton::Back => Some(PointerButton::Back),
        MouseButton::Forward => Some(PointerButton::Forward),
        // `MouseButton::Other(n)` — whatever a gaming mouse invents, numbered
        // differently by every driver on every platform. Still dropped, and now
        // for a narrower reason than before: back and forward have a meaning
        // that survives being carried across platforms, and button 9 does not.
        MouseButton::Other(_) => None,
    }
}
