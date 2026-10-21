//! The demo tree, shared by the Android and iOS entry points.
//!
//! Not an example of its own: it lives in a subdirectory, which Cargo does not
//! auto-discover, and each entry point pulls it in with `#[path]`. One copy
//! rather than two, because the whole claim being tested is that the *same*
//! tree renders on both — two copies would drift and the demo would quietly
//! stop proving it.
//!
//! # What it is for, beyond a first frame
//!
//! Three things a phone can be asked that a desktop cannot: does a frame reach
//! the display, does a finger reach a handler, and **what does a frame cost**.
//! The third is why [`Demo`] holds a repeating animation and a performance
//! overlay: a screen that is allowed to go idle measures the idle path, and the
//! first version of this demo correctly idled after one frame and therefore
//! measured nothing.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use vieww_animation::Tween;
use vieww_element::{Animation, Signal};
use vieww_foundation::{Color, EdgeInsets, TargetPlatform, TextEditingValue};
use vieww_platform_winit::FrameLog;
use vieww_render::FrameDriver;
use vieww_widget::prelude::*;
// Not in the prelude, and rightly so: it is a diagnostic rather than something
// an application's tree is built out of.
use vieww_widget::PerformanceOverlay;

/// One 60Hz frame.
///
/// The budget the *log* uses, not the display's: `App` defaults to 60Hz and the
/// demo does not change it, so this is the same number `FrameLog::budget`
/// reports. On a 90 or 120Hz phone both are wrong together — which is the
/// honest state to be in, and better than a graph drawn against one budget by a
/// log keeping another.
const BUDGET: Duration = Duration::from_micros(16_667);

/// How long the band takes to cross its colour range. Long enough that the
/// motion reads as smooth rather than as strobing, short enough to see.
const PULSE: Duration = Duration::from_secs(2);

/// What the probe button says, and therefore how anything looking for it finds it.
///
/// Shared with `device_tests::tap_targets`, which locates this button in the
/// semantics tree to report where a script should tap. Defined once here, next to
/// the button, so the two cannot drift — a second copy of this string is a tap that
/// silently stops landing, which is the failure this whole mechanism replaced.
pub(crate) const TAP_LABEL: &str = "Tap me";

/// What the drawer's trigger says, and how `ci/mobile/a11y-android.sh` finds it.
///
/// Same reasoning as [`TAP_LABEL`]: one definition, next to the button, because
/// a second copy in a shell script is a check that silently stops locating
/// anything the moment somebody rewords the control.
pub(crate) const MENU_LABEL: &str = "Open the menu";

/// What the panel says, so a dump can tell "the drawer is up" from "the drawer
/// is up **and** the screen behind it is still readable".
///
/// The check is not that these appear — a panel that failed to open would fail
/// more loudly than that. It is that these appear **and [`TAP_LABEL`] does
/// not**.
pub(crate) const MENU_ITEMS: [&str; 3] = ["Inbox", "Archive", "Settings"];

/// What the last frames cost, as the platform layer measured them.
///
/// Carries the budget alongside the samples so both come from the same source —
/// the alternative is a constant in the tree that silently disagrees with the
/// log the moment somebody calls `App::refresh_rate`.
#[derive(Debug, Clone)]
pub(crate) struct Perf {
    samples: Vec<Duration>,
    budget: Duration,
}

impl Perf {
    /// Before any frame has been timed.
    fn empty() -> Self {
        Self {
            samples: Vec::new(),
            budget: BUDGET,
        }
    }

    /// What `App::on_frame` hands over, taken from the log.
    pub(crate) fn from_samples(samples: Vec<Duration>, budget: Duration) -> Self {
        Self { samples, budget }
    }
}

/// Joins `App::on_frame` to the tree it writes into.
///
/// The two are configured at different times and neither can wait for the
/// other: the hook is installed before the window exists, and the signal it
/// writes cannot be created until `build` runs, because a signal belongs to the
/// driver's runtime and the driver is only handed out then. So the hook is given
/// a slot, and `build` fills it in.
///
/// It lives here rather than in each entry point because there are two of them
/// and the whole point of this module is that they are the same demo.
#[derive(Debug, Default)]
pub(crate) struct PerfLink {
    /// `None` until the tree exists — at most one frame, and the hook simply
    /// does nothing until then.
    signal: Rc<RefCell<Option<Signal<Perf>>>>,
}

impl PerfLink {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// The closure to hand to `App::on_frame`.
    pub(crate) fn hook(&self) -> impl FnMut(&FrameLog) + 'static {
        let slot = Rc::clone(&self.signal);
        move |log| {
            if let Some(signal) = slot.borrow().as_ref() {
                signal.set(Perf::from_samples(log.work_samples(), log.budget()));
            }
        }
    }

    /// Point the hook at a built demo. Call from inside `build`.
    pub(crate) fn attach(&self, demo: &Demo) {
        *self.signal.borrow_mut() = Some(demo.perf());
    }
}

/// The demo's state: what a tap changes, what a frame costs, and the one thing
/// that never settles.
///
/// Built from the driver at startup, then read by the tree and written by the
/// per-frame hook. Both entry points construct one and neither knows anything
/// else about the demo.
#[derive(Debug)]
pub(crate) struct Demo {
    taps: Signal<u32>,
    perf: Signal<Perf>,
    /// What the text field holds.
    ///
    /// The demo needs somewhere to type for one reason that has nothing to do
    /// with text: **focusing it is the only way to raise the soft keyboard**,
    /// and the keyboard is the only way to make `view_insets` non-zero. Without
    /// a field on this screen, `the_keyboard_inset_appears_when_the_keyboard_does`
    /// has nothing it can ask the device.
    field: Signal<TextEditingValue>,
    /// Cloned into [`Pulse`], which is the handle that keeps it running.
    ///
    /// `Tickers` holds only a `Weak` to what it drives, so an `Animation` whose
    /// last strong handle drops stops silently — a window that renders one frame
    /// and then freezes, with no panic to explain it. The copy here is not that
    /// owner: this struct is dropped with the build closure. The widget's copy
    /// is, which is why the animation lives in the tree that displays it.
    pulse: Animation<f32>,
    /// Which of three things is chosen, shown five different ways.
    ///
    /// One signal behind the whole of [`Gallery`], because the interesting claim
    /// about a `TabBar`, a `SegmentedControl`, a row of `Radio`s and a
    /// `BottomNavigation` is that they are **four skins over one piece of
    /// state** — tap any of them and the other three follow. Four separate
    /// signals would demonstrate four controls; one demonstrates the layer they
    /// are built on.
    choice: Signal<usize>,
    /// Whether the navigation drawer is up.
    ///
    /// # This screen exists to be listened to, and had nothing modal on it
    ///
    /// `BlockSemantics` is applied **unconditionally** by every `ModalBarrier`
    /// in the framework, which is a decision made in code on 2026-08-14 and
    /// never once verified by ear. `ci/mobile/a11y-android.sh --activate` is the check,
    /// and until now it had nothing to point at: this screen carried no
    /// `Dialog`, no `Drawer` and no `ModalBarrier`, and `examples/drawer.rs` is
    /// a winit example that does not build for Android. The item was not
    /// "untested", it was **untestable**.
    ///
    /// A `Drawer` rather than a `Dialog`, on `docs/HANDOFF.md`'s reasoning: a
    /// dialog covers the middle of the screen, while a drawer leaves a live
    /// strip of scrim beside the panel — so "what can a screen reader still
    /// reach" has an answer that could be wrong in an interesting way rather
    /// than only in an obvious one.
    drawer: Signal<bool>,
    /// The stress screen's list position, driven a step every frame — see
    /// [`Demo::stress_screen`].
    stress_scroll: vieww_element::ScrollController,
    /// The stress screen's thumbnails: four generated pictures, shared by
    /// every row (an `Image` clone is a reference, not a copy).
    art: Rc<Vec<vieww_foundation::Image>>,
}

impl Demo {
    /// Create the signals and start the animation.
    pub(crate) fn new(driver: &mut FrameDriver) -> Self {
        let runtime = driver.elements().runtime().clone();
        // Never settles, so the framework never stops asking for frames — which
        // is the only way a frame rate on a device is a measurement rather than
        // a report on how quickly the loop went to sleep.
        let pulse = driver.animation(Tween::new(0.0_f32, 255.0), PULSE);
        pulse.repeat(true, Duration::ZERO);

        Self {
            taps: runtime.signal(0_u32),
            perf: runtime.signal(Perf::empty()),
            field: runtime.signal(TextEditingValue::new("tap to type")),
            pulse,
            choice: runtime.signal(0_usize),
            drawer: runtime.signal(false),
            stress_scroll: vieww_element::ScrollController::new(
                &runtime,
                vieww_gestures::ScrollPhysics::android(),
            ),
            art: Rc::new((0..4).map(stress_art).collect()),
        }
    }

    /// The stress screen's scroll position, for the hook that moves it.
    pub(crate) fn stress_scroll(&self) -> vieww_element::ScrollController {
        self.stress_scroll.clone()
    }

    /// **A screen as heavy as a real app's**, for measuring what the demo
    /// screen cannot.
    ///
    /// The demo is light on purpose: its steady frames redraw a moving band
    /// and a graph, and Device Farm run 39594824376 measured exactly zero GPU
    /// time on layers, filters and shadows on all three phones. That answered
    /// "is reusing layer pixels worth it?" only for a screen with no layers.
    /// This one has what feeds and media apps have, and keeps all of it moving:
    ///
    /// - a list of [`STRESS_ROWS`] cards, each with a drop shadow, a rounded
    ///   thumbnail and three lines of wrapped text, **scrolling every frame**
    ///   ([`STRESS_SPEED`] points per frame, a steady fling) so the whole
    ///   list area is damaged every frame;
    /// - every fifth card inside three nested layers (opacity, saturation,
    ///   opacity), every seventh with a blurred thumbnail — layers and filters
    ///   over content that moves;
    /// - a **frosted header** — a backdrop blur over the scrolling list, the
    ///   most expensive thing a GPU compositor is asked for — holding the
    ///   suite's own button, counter, field and menu, so every check the demo
    ///   screen answers is still answered here;
    /// - a floating card whose shadow and opacity animate every frame.
    ///
    /// Taps find the button and the field by label and role, as on the demo
    /// screen (`DeviceSuite::tap_targets`), so nothing in `device-suite.sh`
    /// knows which screen it is driving.
    pub(crate) fn stress_screen(
        &self,
        platform: TargetPlatform,
        probe: impl Into<WidgetNode>,
    ) -> WidgetNode {
        let theme = ThemeData::adaptive(platform, true);
        let colors = theme.colors;
        let bump = self.taps.clone();
        let open_drawer = self.drawer.clone();
        let header = Filtered::blur(14.0).with_backdrop().child(
            Container::new()
                .color(colors.surface.with_alpha(150))
                .child(
                    SafeArea::new().child(
                        Container::new()
                            .padding(EdgeInsets::symmetric(20.0, 12.0))
                            .child(
                                Flex::column()
                                    .cross_axis_alignment(CrossAxisAlignment::Start)
                                    .children(children![
                                        Text::new("vieww · stress")
                                            .size(28.0)
                                            .bold()
                                            .color(colors.on_surface),
                                        SizedBox::height(10.0),
                                        Flex::row().children(children![
                                            Button::new(TAP_LABEL).on_pressed(move || {
                                                bump.set(bump.peek() + 1);
                                            }),
                                            SizedBox::width(12.0),
                                            Button::new(MENU_LABEL).on_pressed(move || {
                                                open_drawer.set(true);
                                            }),
                                        ]),
                                        SizedBox::height(8.0),
                                        Counter {
                                            taps: self.taps.clone()
                                        },
                                        SizedBox::height(8.0),
                                        Field {
                                            value: self.field.clone()
                                        },
                                        probe.into(),
                                    ]),
                            ),
                    ),
                ),
        );
        Theme::new(theme)
            .child(Stack::new().fit(StackFit::Expand).children(children![
                Container::new().gradient(
                    vieww_foundation::Gradient::linear(
                        vieww_foundation::Offset::new(0.0, 0.0),
                        vieww_foundation::Offset::new(1.0, 1.0),
                    )
                    .with_stops(&[
                        (0.0, Color::rgba(18, 22, 40, 255)),
                        (1.0, Color::rgba(40, 16, 48, 255)),
                    ]),
                ),
                StressList {
                    scroll: self.stress_scroll.clone(),
                    art: Rc::clone(&self.art),
                },
                Positioned::new().left(0.0).right(0.0).top(0.0).child(header),
                Positioned::new()
                    .left(24.0)
                    .right(24.0)
                    .bottom(28.0)
                    .child(Floater {
                        pulse: self.pulse.clone()
                    }),
                NavMenu {
                    open: self.drawer.clone(),
                },
            ]))
            .into()
    }

    /// Whether the drawer is up, so a device check can open it without a finger.
    ///
    /// `ci/mobile/a11y-android.sh` drives TalkBack, and TalkBack's own gestures are not
    /// something a script can synthesise — but the *state* the check needs is one
    /// boolean, and writing it from a frame hook puts the panel up in the same
    /// frame the dump is taken from. Without this the run needs two people: one
    /// holding the drawer open and one reading the dump.
    pub(crate) fn drawer(&self) -> &Signal<bool> {
        &self.drawer
    }

    /// The handle `App::on_frame` writes into, once per frame.
    pub(crate) fn perf(&self) -> Signal<Perf> {
        self.perf.clone()
    }

    /// What a tap increments, so something outside can watch it.
    pub(crate) fn taps(&self) -> &Signal<u32> {
        &self.taps
    }

    /// The demo screen: a title, a tap counter, a button, a band that moves, and
    /// a graph of what the last frames cost.
    ///
    /// Deliberately small — a first screen on a new platform should fail for one
    /// reason at a time — and deliberately *interactive*, because a static tree
    /// proves a frame can be drawn and nothing more.
    ///
    /// `SafeArea` wraps the content and **not** the background. The other order
    /// reads as correct and is not: the insets then fall outside the fill, so the
    /// status bar and gesture bar show the window's bare background instead of
    /// the app's. Full-bleed colour, inset content, which is what every phone
    /// OS's own apps do.
    ///
    /// `probe` is mounted but draws nothing — it is how `device_tests` reads the
    /// view metrics, which are published to the *tree* and are not visible from
    /// the build closure. Pass `SizedBox::shrink()` to leave it out.
    ///
    /// # Nothing above the field may move
    ///
    /// `ci/mobile/device-suite.sh` taps the button at a fixed physical coordinate and
    /// the text field below it at another, because a script driving a phone from
    /// outside cannot see the screen. Those numbers are derived from the order
    /// and the spacing here, so **anything added goes below the frame graph**.
    /// Insert a widget above the button and the suite's tap lands on nothing,
    /// which shows up as `a_tap_reached_a_handler` being *absent* from the
    /// report rather than failing — the hardest kind of breakage to read.
    ///
    /// # Why the whole screen is themed dark, and by which platform's rules
    ///
    /// The background here is near-black and there was no [`Theme`] above it, so
    /// every control on this screen was being handed `ColorScheme::light()` —
    /// `ThemeData::of`'s documented fallback. A filled button survives that; the
    /// gallery below does not, because `on_surface_variant` in the light scheme
    /// is a dark grey that is invisible on this background. The theme is the fix
    /// and it moves nothing.
    ///
    /// `platform` decides which dark scheme: Apple's system blue and 44pt
    /// targets, or the standard Android scheme. See [`ThemeData::adaptive`] for what that does
    /// and — more importantly — does not amount to.
    pub(crate) fn screen(
        &self,
        subtitle: &str,
        platform: TargetPlatform,
        probe: impl Into<WidgetNode>,
    ) -> WidgetNode {
        let bump = self.taps.clone();
        // **The platform is passed in, not read.** Each entry point states the
        // one it is *for*, so the desktop run of `examples/ios.rs` is a genuine
        // preview of the Apple skin rather than the Linux harness's Android
        // one — which is the only way to look at that skin at all without a Mac.
        //
        // Exactly what `ThemeData::adaptive` takes a parameter for.
        let theme = ThemeData::adaptive(platform, true);
        // **The background is the theme's surface, not a constant.** It was
        // `0x121212` — a stock dark grey — which happens to sit close to
        // the standard dark surface and is 1.12:1 away from Apple's, which is
        // pure black. So on the Apple skin every token designed to sit *on* the
        // surface was being drawn against the wrong ground: dividers at 1.6:1,
        // the navigation pill at 1.1:1, both effectively invisible.
        //
        // The same mistake as the screen having had no `Theme` at all, one
        // level down: asking for a palette and then not using it.
        let colors = theme.colors;
        let open_drawer = self.drawer.clone();
        Theme::new(theme)
            .child(
                // **The `Stack` is what makes the modal claim audible.**
                // `BlockSemantics` clears what *preceding siblings* contributed,
                // so the drawer has to be a sibling of the screen rather than a
                // wrapper around it — the panel is second, the screen is first,
                // and the barrier between them is the thing under test.
                //
                // `NavMenu` collapses to `SizedBox::shrink()` while closed, so a
                // run that never opens it is the screen exactly as it was.
                //
                // **`Expand`, not the default `Loose`.** The background
                // `Container` used to be the root and was laid out under the
                // window's tight constraints, so it filled the screen. A loose
                // stack would hand it the window *loosened*, it would shrink-wrap
                // to the column inside it, and the full-bleed surface colour this
                // screen's doc comment argues for would stop at the last control
                // — leaving the window's bare background under the gesture bar.
                Stack::new().fit(StackFit::Expand).children(children![
                    Container::new().color(colors.surface).child(
                        SafeArea::new().child(
                            Container::new().padding(EdgeInsets::all(24.0)).child(
                                Flex::column()
                                    .cross_axis_alignment(CrossAxisAlignment::Start)
                                    .children(children![
                                        Text::new("vieww")
                                            .size(48.0)
                                            .bold()
                                            .color(colors.on_surface),
                                        SizedBox::height(12.0),
                                        Text::new(subtitle).color(colors.on_surface_variant),
                                        SizedBox::height(32.0),
                                        Button::new(TAP_LABEL).on_pressed(move || {
                                            // A handler runs during input dispatch, not
                                            // during a build, so writing a signal here is
                                            // allowed and is the whole point — DESIGN §1.
                                            bump.set(bump.peek() + 1);
                                        }),
                                        SizedBox::height(16.0),
                                        Counter {
                                            taps: self.taps.clone()
                                        },
                                        SizedBox::height(16.0),
                                        Field {
                                            value: self.field.clone()
                                        },
                                        SizedBox::height(16.0),
                                        Pulse {
                                            pulse: self.pulse.clone()
                                        },
                                        SizedBox::height(16.0),
                                        // The one thing on this screen whose exact
                                        // height does not matter, so it is the one
                                        // that gives way when there is not enough
                                        // room. On a window too short for
                                        // everything it shrinks — to nothing, if it
                                        // has to — and the controls still fit.
                                        //
                                        // `Flexible::new`, *not* `expanded`. Loose
                                        // means "up to this share", so the graph
                                        // takes its preferred 72 and leaves the
                                        // rest. `expanded` is tight and was the
                                        // first thing tried: the graph then filled
                                        // every spare pixel and repainted all of it
                                        // sixty times a second, which took the worst
                                        // frame's work from 8.3ms to 19.6ms and the
                                        // repaint count from 29 to 45. An instrument
                                        // that costs more the more room it is given
                                        // is measuring itself.
                                        //
                                        // The alternative to both is a fixed height
                                        // plus arithmetic about the window, which is
                                        // what was here first and it cut the
                                        // navigation bar off a 701-tall window.
                                        Flexible::new(1).child(FrameGraph {
                                            perf: self.perf.clone()
                                        }),
                                        SizedBox::height(16.0),
                                        Gallery {
                                            choice: self.choice.clone()
                                        },
                                        SizedBox::height(16.0),
                                        // **Below everything**, on the rule this
                                        // function's own doc states: `ci/mobile/device-suite.sh`
                                        // derives its tap coordinates from the order
                                        // above, and a widget inserted higher moves
                                        // the button out from under a tap that then
                                        // lands on nothing and reports nothing.
                                        //
                                        // Write-only, so it is a plain `Button` and
                                        // not a widget of its own: nothing here reads
                                        // the signal, so there is nothing to rebuild.
                                        Button::new(MENU_LABEL).on_pressed(move || {
                                            open_drawer.set(true);
                                        }),
                                        probe.into(),
                                    ]),
                            ),
                        ),
                    ),
                    NavMenu {
                        open: self.drawer.clone(),
                    },
                ]),
            )
            .into()
    }
}

/// Somewhere to type, so the soft keyboard can be raised.
///
/// Its own widget for the reason [`Counter`] is: the signal is read here, so a
/// keystroke rebuilds this and nothing else. It is also the only control on the
/// screen that asks the platform for an IME, which makes it the only way to
/// move `ViewMetrics::view_insets` off zero on a phone.
#[derive(Debug)]
struct Field {
    value: Signal<TextEditingValue>,
}

impl Widget for Field {
    fn debug_name(&self) -> &'static str {
        "Field"
    }

    fn kind(&self) -> WidgetKind<'_> {
        WidgetKind::Composed
    }

    fn build(&self, ctx: &BuildContext) -> WidgetNode {
        let write = self.value.clone();
        // `ThemeData::of`, not a constant: this is the field an application
        // would write, and one that hard-coded its own grey would look wrong the
        // moment the platform's did not match it.
        let colors = ThemeData::of(ctx).colors;
        Container::new()
            .color(colors.surface_variant)
            .padding(EdgeInsets::all(12.0))
            .child(
                TextField::new(self.value.get())
                    .size(18.0)
                    .color(colors.on_surface)
                    .cursor(colors.primary, 2.0)
                    .on_changed(Rc::new(move |next| write.set(next))),
            )
            .into()
    }
}

widget_node_from!(Field);

/// Shows how many times the button has been tapped.
///
/// Its own widget so that the *read* of the signal happens here: a tap then
/// marks this element and nothing else, and the rest of the screen does not
/// rebuild. That is the property the whole element layer exists for, and a demo
/// that rebuilt the world on every tap would not be demonstrating it.
#[derive(Debug)]
struct Counter {
    taps: Signal<u32>,
}

impl Widget for Counter {
    fn debug_name(&self) -> &'static str {
        "Counter"
    }

    fn kind(&self) -> WidgetKind<'_> {
        WidgetKind::Composed
    }

    fn build(&self, ctx: &BuildContext) -> WidgetNode {
        let colors = ThemeData::of(ctx).colors;
        let taps = self.taps.get();
        let label = match taps {
            0 => String::from("Not tapped yet"),
            1 => String::from("1 tap"),
            more => format!("{more} taps"),
        };
        Text::new(label).color(colors.on_surface_variant).into()
    }
}

widget_node_from!(Counter);

/// The navigation drawer, and the only modal thing on this screen.
///
/// # What this is for
///
/// Every `ModalBarrier` in the framework wraps itself in `BlockSemantics`
/// **unconditionally**, so the screen behind a modal is unreachable to a screen
/// reader. That is a decision taken in code on 2026-08-14, tested on the host in
/// `crates/vieww/tests/settings_screen.rs`, and **never once heard**. It could
/// not be: no entry point that builds for Android had anything modal on it.
///
/// So this widget is the subject rather than a feature of the demo. With the
/// panel up, `ci/mobile/a11y-android.sh --activate` should find [`MENU_ITEMS`] in the
/// tree and should **not** find [`TAP_LABEL`], and a finger on the strip of
/// scrim beside the panel should reach nothing at all.
///
/// # Why it reads the signal here rather than in `screen`
///
/// `Demo::screen` runs once, in the build closure, and is not a `build` — a
/// signal read there subscribes nothing and the panel would never appear. The
/// read has to happen inside a `Widget::build`, which is the same reason
/// [`Counter`] and [`Gallery`] exist, and it has the same payoff: opening the
/// drawer marks this element and leaves the rest of the screen alone.
#[derive(Debug)]
struct NavMenu {
    open: Signal<bool>,
}

impl Widget for NavMenu {
    fn debug_name(&self) -> &'static str {
        "NavMenu"
    }

    fn kind(&self) -> WidgetKind<'_> {
        WidgetKind::Composed
    }

    fn build(&self, ctx: &BuildContext) -> WidgetNode {
        // The subscription. Closed is the overwhelmingly common state, and it
        // has to cost nothing: `SizedBox::shrink()` in a `Stack` contributes no
        // geometry, no paint and no semantics, so a run that never opens the
        // drawer is the screen exactly as `ci/mobile/device-suite.sh` has always
        // measured it.
        if !self.open.get() {
            return SizedBox::shrink().into();
        }

        let colors = ThemeData::of(ctx).colors;
        let close = self.open.clone();
        Drawer::new()
            // Tapping the scrim closes it. Without this the panel can only be
            // dismissed by the back gesture, and a device check that cannot get
            // back to the screen it started on can only be run once per launch.
            .on_dismiss(move || close.set(false))
            .child(
                Container::new().padding(EdgeInsets::all(24.0)).child(
                    Flex::column()
                        .cross_axis_alignment(CrossAxisAlignment::Start)
                        .children(MENU_ITEMS.iter().map(|item| {
                            Container::new()
                                .padding(EdgeInsets::symmetric(0.0, 12.0))
                                .child(Text::new(*item).color(colors.on_surface))
                                .into()
                        })),
                ),
            )
            .into()
    }
}

widget_node_from!(NavMenu);

/// A band whose colour changes every frame, so the screen is never still.
///
/// Driven by a repeating animation rather than by a counter somebody
/// increments: that is the only thing in the framework which guarantees a frame
/// on every vsync. Without it the demo idles — correctly, and uselessly, since
/// `dumpsys SurfaceFlinger --latency` then reports on a screen that is not
/// being drawn.
///
/// Behind a `RepaintBoundary` so that a per-frame rebuild costs the band rather
/// than the screen: the read of the animation's value below is the subscription,
/// and it marks this element alone.
#[derive(Debug)]
struct Pulse {
    pulse: Animation<f32>,
}

impl Widget for Pulse {
    fn debug_name(&self) -> &'static str {
        "Pulse"
    }

    fn kind(&self) -> WidgetKind<'_> {
        WidgetKind::Composed
    }

    fn build(&self, _ctx: &BuildContext) -> WidgetNode {
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a tween bounded to 0..=255"
        )]
        let step = self.pulse.value().clamp(0.0, 255.0) as u8;
        RepaintBoundary::new()
            .child(ColoredBox::new(Color::rgb(step, 40, 255 - step)).child(
                // Infinite width, which the column's constraints clamp to
                // the screen. `SizedBox::height` alone leaves the width
                // unconstrained, and a `ColoredBox` sized by a child that
                // asked for nothing is a band nobody can see.
                SizedBox::from_size(Size::new(f32::INFINITY, 24.0)),
            ))
            .into()
    }
}

widget_node_from!(Pulse);

/// The frame graph, fed by `App::on_frame`.
///
/// One bar per recent frame against the budget line, so a stutter on a device
/// has a *shape* rather than a number after the fact. It reads the signal here
/// and nowhere else, so the per-frame write rebuilds this element and leaves the
/// rest of the screen alone — the same property the counter demonstrates, under
/// the load of a write on every single frame.
///
/// The height below is a *preference*, and a loose `Flexible` in
/// [`Demo::screen`] is what makes it one: the graph gets 72 when there is room
/// for 72 and less when there is not. It is the number that matters rather than
/// a fallback, which is why it stays here rather than moving to the flex.
#[derive(Debug)]
struct FrameGraph {
    perf: Signal<Perf>,
}

impl Widget for FrameGraph {
    fn debug_name(&self) -> &'static str {
        "FrameGraph"
    }

    fn kind(&self) -> WidgetKind<'_> {
        WidgetKind::Composed
    }

    fn build(&self, _ctx: &BuildContext) -> WidgetNode {
        let perf = self.perf.get();
        PerformanceOverlay::new(perf.samples, perf.budget)
            .height(72.0)
            .into()
    }
}

widget_node_from!(FrameGraph);

/// What the gallery lets you choose between. Three, because two would not show
/// that a `SegmentedControl` divides its width evenly.
const CHOICES: [&str; 3] = ["Day", "Week", "Month"];

/// Four controls over one signal, and a fifth reporting it.
///
/// Every control in the widget library that was not on this screen: `TabBar`,
/// `SegmentedControl`, a row of `Radio`s, `LinearProgress` and
/// `BottomNavigation`. They were written with tests and never looked at, which
/// is a poor state for anything whose whole output is pixels.
///
/// # One signal, not five
///
/// Tapping any of the four selectors moves all four, because they read the same
/// `choice`. That is worth more than five independent demos: it shows the
/// element layer doing the thing it exists for, and it makes a wrong answer
/// obvious — if the tab bar and the segmented control ever disagree, one of them
/// is reading state it should not have.
///
/// It is also the only way to *see* the rule both new bars are built on. The
/// underline under an unselected tab and the pill behind an unselected icon are
/// both drawn, in a colour that says "not this one", so the bars do not change
/// height when the selection moves. Tap along the row and nothing below should
/// shift by a pixel; if it does, that rule has been broken.
///
/// # It goes at the bottom, and that is not an aesthetic choice
///
/// `ci/mobile/device-suite.sh` taps the button and the field at fixed coordinates
/// derived from this column's order. Everything here is appended after the frame
/// graph so those two stay where the script expects them.
///
/// Which leaves the bottom of the column as the only place that can run out of
/// room — and it did, on the first run: the navigation bar fell off a window
/// 180 logical pixels shorter than the one that had been asked for. The frame
/// graph absorbing the slack is what fixed it, and the reason that is a better
/// fix than smaller gaps is that smaller gaps are another guess about a height
/// nobody controls.
#[derive(Debug)]
struct Gallery {
    choice: Signal<usize>,
}

impl Gallery {
    /// A radio and the word next to it.
    ///
    /// The label has to be drawn here: `Radio::label` is what a *screen reader*
    /// says, and the control paints nothing but its ring — deliberately, since a
    /// radio in a list row and a radio beside a word want different layouts. It
    /// is a trap all the same, because the builder reads as though it were text.
    fn option(index: usize, chosen: usize, ink: Color, choose: &Rc<dyn Fn(usize)>) -> WidgetNode {
        let pick = Rc::clone(choose);
        Flex::row()
            .main_axis_size(MainAxisSize::Min)
            .cross_axis_alignment(CrossAxisAlignment::Center)
            .children(children![
                Radio::new(chosen == index)
                    .label(CHOICES[index])
                    .on_selected(Rc::new(move |()| pick(index))),
                Text::new(CHOICES[index]).color(ink),
                SizedBox::width(12.0),
            ])
            .into()
    }
}

impl Widget for Gallery {
    fn debug_name(&self) -> &'static str {
        "Gallery"
    }

    fn kind(&self) -> WidgetKind<'_> {
        WidgetKind::Composed
    }

    fn build(&self, ctx: &BuildContext) -> WidgetNode {
        let colors = ThemeData::of(ctx).colors;
        let chosen = self.choice.get();

        // One handler, cloned into each control. A signal write during input
        // dispatch marks this element pending and nothing else, so a tap on the
        // tab bar rebuilds the gallery and leaves the rest of the screen — and
        // the frame graph in particular — alone.
        let write = self.choice.clone();
        let choose: Rc<dyn Fn(usize)> = Rc::new(move |index| write.set(index));

        let radios: Vec<WidgetNode> = (0..CHOICES.len())
            .map(|index| Self::option(index, chosen, colors.on_surface_variant, &choose))
            .collect();

        // Fractions rather than a division: three cases, and no cast from a
        // length to a float to argue with.
        let progress = match chosen {
            0 => 1.0 / 3.0,
            1 => 2.0 / 3.0,
            _ => 1.0,
        };

        Flex::column()
            .cross_axis_alignment(CrossAxisAlignment::Start)
            .children(children![
                TabBar::new(CHOICES, chosen).on_selected(Rc::clone(&choose)),
                SizedBox::height(16.0),
                SegmentedControl::new(CHOICES, chosen).on_selected(Rc::clone(&choose)),
                SizedBox::height(8.0),
                Flex::row().children(radios),
                SizedBox::height(8.0),
                // All three progress indicators in one row, and the two rings
                // side by side on purpose: the determinate one is the only way
                // to check that an arc of a *known* size is the size it should
                // be, and the indeterminate one beside it is the only thing
                // that has ever drawn `Path::arc_ring` in motion. The arc
                // geometry was written, tested and never once looked at.
                Flex::row()
                    .cross_axis_alignment(CrossAxisAlignment::Center)
                    .children(children![
                        CircularProgress::new(progress).label("Chosen"),
                        SizedBox::width(12.0),
                        CircularProgress::indeterminate(),
                        SizedBox::width(12.0),
                        Flexible::expanded(1).child(
                            LinearProgress::new(progress).label("How far along the choice is")
                        ),
                    ]),
                SizedBox::height(16.0),
                BottomNavigation::new(
                    [
                        BottomNavItem::new(icons::check(), CHOICES[0]),
                        BottomNavItem::new(icons::add(), CHOICES[1]),
                        BottomNavItem::new(icons::close(), CHOICES[2]),
                    ],
                    chosen,
                )
                .on_selected(choose),
            ])
            .into()
    }
}

widget_node_from!(Gallery);

// ── the stress screen ──────────────────────────────────────────────

/// Rows in the stress screen's list: enough that it never runs out while the
/// suite watches (scrolling resets at the end).
pub(crate) const STRESS_ROWS: usize = 400;

/// Each row's height, in logical points.
pub(crate) const STRESS_ROW: f32 = 148.0;

/// How far the list moves each frame, in logical points: 360 points a second
/// at 60 Hz, a steady fling.
pub(crate) const STRESS_SPEED: f32 = 6.0;

/// The list starts below the frosted header, so the first rows are not
/// under it at rest; once it scrolls, every row passes beneath the blur.
const STRESS_HEADER: f32 = 230.0;

/// One of the stress screen's thumbnails: a generated 128x128 picture, so the
/// screen needs no assets and draws the same pixels everywhere.
fn stress_art(seed: usize) -> vieww_foundation::Image {
    const SIDE: u32 = 128;
    let mut pixels = Vec::with_capacity((SIDE * SIDE * 4) as usize);
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss,
        reason = "colour arithmetic on 0..=255"
    )]
    for y in 0..SIDE {
        for x in 0..SIDE {
            let (fx, fy) = (x as f32 / SIDE as f32, y as f32 / SIDE as f32);
            let ring = ((fx - 0.5).hypot(fy - 0.5) * (8.0 + seed as f32 * 3.0)).sin() * 0.5 + 0.5;
            let r = (255.0 * (0.25 + 0.75 * fx * ring)) as u8;
            let g = (255.0 * (0.2 + 0.6 * fy * (1.0 - ring))) as u8;
            let b = (255.0 * (0.4 + 0.6 * ((seed as f32 + fx + fy) * 2.1).sin().abs())) as u8;
            pixels.extend_from_slice(&[r, g, b, 255]);
        }
    }
    vieww_foundation::Image::from_rgba8(pixels, SIDE, SIDE)
}

/// What a row says.
const STRESS_TITLES: [&str; 6] = [
    "Morning ride along the river",
    "Quarterly numbers, explained",
    "A recipe that needs one pan",
    "What the tide tables say",
    "Field notes from the hills",
    "The night market, in pictures",
];

/// One card of the stress list. Every fifth sits inside three nested layers,
/// every seventh has a blurred thumbnail, every third a gradient ground.
fn stress_row(index: usize, art: &[vieww_foundation::Image]) -> WidgetNode {
    let thumb_image = art[index % art.len()].clone();
    let thumb: WidgetNode = Clip::rounded(14.0)
        .child(SizedBox::square(88.0).child(Image::new(thumb_image)))
        .into();
    let thumb: WidgetNode = if index.is_multiple_of(7) {
        Filtered::blur(2.5).child(thumb).into()
    } else {
        thumb
    };
    let title = STRESS_TITLES[index % STRESS_TITLES.len()];
    let body = format!(
        "Card {index}. A paragraph long enough to wrap onto a second and a third line, \
         so every row lays out and shapes real text rather than a single word."
    );
    let mut card = Container::new()
        .radius(18.0)
        .padding(EdgeInsets::all(14.0))
        .shadow(vieww_foundation::Shadow::new(
            Color::rgba(0, 0, 0, 110),
            vieww_foundation::Offset::new(0.0, 6.0),
            16.0,
        ));
    card = if index.is_multiple_of(3) {
        card.gradient(
            vieww_foundation::Gradient::linear(
                vieww_foundation::Offset::new(0.0, 0.0),
                vieww_foundation::Offset::new(1.0, 0.0),
            )
            .with_stops(&[
                (0.0, Color::rgba(44, 52, 92, 255)),
                (1.0, Color::rgba(70, 40, 96, 255)),
            ]),
        )
    } else {
        card.color(Color::rgba(36, 40, 64, 255))
    };
    let card = card.child(Flex::row().children(children![
        thumb,
        SizedBox::width(14.0),
        Flexible::new(1).child(
            Flex::column()
                .cross_axis_alignment(CrossAxisAlignment::Start)
                .children(children![
                    Text::new(title).size(16.0).bold().color(Color::WHITE),
                    SizedBox::height(4.0),
                    Text::new(body)
                        .size(12.5)
                        .max_lines(3)
                        .color(Color::rgba(200, 204, 224, 255)),
                ]),
        ),
    ]));
    let card: WidgetNode = if index.is_multiple_of(5) {
        Opacity::new(0.92)
            .child(
                Filtered::new()
                    .saturation(0.6)
                    .child(Opacity::new(0.97).child(card)),
            )
            .into()
    } else {
        card.into()
    };
    Padding::new(EdgeInsets::symmetric(16.0, 8.0))
        .child(card)
        .into()
}

/// A card floating over the list whose shadow and opacity follow the pulse,
/// so a layer and a shadow change every frame even where the list does not.
#[derive(Debug)]
struct Floater {
    pulse: Animation<f32>,
}

impl Widget for Floater {
    fn debug_name(&self) -> &'static str {
        "Floater"
    }

    fn kind(&self) -> WidgetKind<'_> {
        WidgetKind::Composed
    }

    fn build(&self, _ctx: &BuildContext) -> WidgetNode {
        let t = self.pulse.value().clamp(0.0, 255.0) / 255.0;
        let breathe = 0.5 - 0.5 * (t * std::f32::consts::TAU).cos();
        Opacity::new(0.75 + 0.25 * breathe)
            .child(
                Container::new()
                    .height(64.0)
                    .radius(32.0)
                    .color(Color::rgba(60, 70, 120, 255))
                    .shadow(vieww_foundation::Shadow::new(
                        #[expect(
                            clippy::cast_possible_truncation,
                            clippy::cast_sign_loss,
                            reason = "80..=200"
                        )]
                        Color::rgba(90, 120, 255, (80.0 + 120.0 * breathe) as u8),
                        vieww_foundation::Offset::new(0.0, 4.0 + 8.0 * breathe),
                        10.0 + 20.0 * breathe,
                    ))
                    .padding(EdgeInsets::symmetric(24.0, 0.0))
                    .alignment(Alignment::CENTER_LEFT)
                    .child(Text::new("Now playing · stress").color(Color::WHITE)),
            )
            .into()
    }
}

widget_node_from!(Floater);

/// The stress list, as its own widget so that reading the scroll offset
/// happens inside a build — and so a step of the scroll rebuilds this and
/// nothing above it.
#[derive(Debug)]
struct StressList {
    scroll: vieww_element::ScrollController,
    art: Rc<Vec<vieww_foundation::Image>>,
}

impl Widget for StressList {
    fn debug_name(&self) -> &'static str {
        "StressList"
    }

    fn kind(&self) -> WidgetKind<'_> {
        WidgetKind::Composed
    }

    fn build(&self, _ctx: &BuildContext) -> WidgetNode {
        let art = Rc::clone(&self.art);
        let rows: vieww_widget::ItemBuilder = Rc::new(move |index| stress_row(index, &art));
        let scroll = &self.scroll;
        Scrollable::vertical(scroll.offset())
            .on_drag(scroll.on_drag())
            .on_drag_end(scroll.on_drag_end())
            .on_extents(scroll.on_extents())
            .child(
                Padding::new(EdgeInsets::only(0.0, STRESS_HEADER, 0.0, 120.0))
                    .child(ListView::new(STRESS_ROWS, STRESS_ROW, rows)),
            )
            .into()
    }
}

widget_node_from!(StressList);

/// Whether this run asked for the stress screen: `VIEWW_SCREEN=stress`
/// (Android sets it from `debug.vieww.screen`).
pub(crate) fn wants_stress() -> bool {
    std::env::var("VIEWW_SCREEN").is_ok_and(|screen| screen == "stress")
}

/// Joins `App::on_frame` to the stress list, as [`PerfLink`] joins it to the
/// graph: the hook exists before the tree does, so it is handed a slot.
///
/// Each frame it moves the list [`STRESS_SPEED`] points on, and back to the
/// top at the end — a fling that never stops, so every frame the suite
/// measures redraws the whole list under the frosted header.
#[derive(Debug, Default)]
pub(crate) struct StressLink {
    scroll: Rc<RefCell<Option<vieww_element::ScrollController>>>,
}

impl StressLink {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// The closure to hand to `App::on_frame`.
    pub(crate) fn hook(&self) -> impl FnMut(&FrameLog) + 'static {
        let slot = Rc::clone(&self.scroll);
        move |_log| {
            if let Some(scroll) = slot.borrow().as_ref() {
                let max = scroll.max_offset();
                let next = scroll.peek() + STRESS_SPEED;
                // Before the first layout the extent is unknown (zero): keep
                // still rather than jump to the end of nothing.
                if max > 0.0 {
                    scroll.jump_to(if next > max { 0.0 } else { next });
                }
            }
        }
    }

    /// Fill the slot, once `build` has made the demo.
    pub(crate) fn attach(&self, demo: &Demo) {
        *self.scroll.borrow_mut() = Some(demo.stress_scroll());
    }
}
