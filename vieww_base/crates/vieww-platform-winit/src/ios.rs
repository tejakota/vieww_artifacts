//! iOS's answer to "where is the system furniture", through UIKit.
//!
//! The counterpart to [`crate::insets`]'s JNI, and deliberately the same shape:
//! one function that asks the platform for a [`SystemInsets`], and nothing else.
//! What is done with the numbers is shared, tested on every machine, and lives
//! next door.
//!
//! # Status: written, not run
//!
//! **No frame has reached an iPhone**, so nothing in this file has executed
//! against a real UIKit. It cross-compiles for `aarch64-apple-ios` and links for
//! `aarch64-apple-ios-sim` in CI, which establishes that the selectors resolve
//! and the types line up — and establishes nothing at all about the numbers
//! coming back. Every design decision below is written down with its reasoning
//! precisely because the reasoning is currently all there is to check.
//!
//! # Why `objc2` rather than hand-rolled `objc_msgSend`
//!
//! `-[UIView safeAreaInsets]` returns a 32-byte struct, and how a struct comes
//! back from a message send is ABI-dependent: four doubles are returned in
//! registers on arm64 and through a hidden pointer on x86_64, where the symbol
//! is `objc_msgSend_stret` instead. Declaring it wrong does not fail to compile.
//! It silently reads garbage on the simulator architecture nobody tested on.
//!
//! `objc2` already encodes that rule, `winit` already pulls it in for this
//! target so it costs nothing new in the tree, and DESIGN §9's "do not attempt
//! from scratch" list is exactly about this kind of thing.

use objc2::rc::Retained;
use objc2::runtime::NSObjectProtocol;
use objc2::sel;
use objc2_ui_kit::{UIKeyboardLayoutGuide, UIView};
use vieww_foundation::EdgeInsets;
use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};

use crate::insets::SystemInsets;

/// Where the notch, the home indicator and the keyboard are, in points.
///
/// `None` when the window is not a UIKit window or has no view yet — the same
/// contract as the Android side, so a caller can tell "there is no furniture"
/// from "we could not ask".
///
/// # No scale conversion
///
/// UIKit measures in points and vieww measures in logical pixels, and on iOS
/// those are the same unit. The `/ scale` the Android path needs is *not* a
/// missing step here; adding it would divide the insets by three on a modern
/// phone.
pub(crate) fn window_insets(window: &winit::window::Window) -> Option<SystemInsets> {
    let view = ui_view(window)?;

    let safe = view.safeAreaInsets();
    Some(SystemInsets {
        safe_area: EdgeInsets {
            left: safe.left as f32,
            top: safe.top as f32,
            right: safe.right as f32,
            bottom: safe.bottom as f32,
        },
        keyboard: keyboard_inset(view),
    })
}

/// Give the window's `CAMetalLayer` the screen's scale before the swapchain
/// is (re)built.
///
/// # Why
///
/// The Vulkan surface is a `CAMetalLayer` that `ash-window` (through
/// `raw-window-metal`) adds as a sublayer of the view, and a Vulkan swapchain on
/// MoltenVK takes its size from the layer: `currentExtent` is the layer's
/// bounds times its `contentsScale`. The first iOS frame came out with
/// `contents scale 1.0` on a 3x screen — rendered at a third of the screen's
/// resolution and magnified, visibly soft. The layer was created while the
/// window was still invisible and has not followed the screen since; setting it
/// here, before every swapchain rebuild, makes the extent the physical size.
pub(crate) fn match_layer_scale(window: &winit::window::Window) {
    use objc2::msg_send;
    use objc2::runtime::{AnyClass, AnyObject};

    let Some(view) = ui_view(window) else {
        return;
    };
    let Some(metal) = AnyClass::get("CAMetalLayer") else {
        return;
    };
    let scale: f64 = window.scale_factor();
    // SAFETY: plain Objective-C message sends with the documented signatures —
    // `-[UIView layer]` returns a `CALayer *`, `-[CALayer sublayers]` an
    // `NSArray *` (or nil), `-count`/`-objectAtIndex:` take and return
    // `NSUInteger`, `-isKindOfClass:` returns `BOOL` and `-setContentsScale:`
    // takes a `CGFloat` (`f64` on every 64-bit Apple target). Nil results are
    // checked before use. Main thread, as for `ui_view`.
    unsafe {
        let layer: *mut AnyObject = msg_send![view, layer];
        let Some(layer) = layer.as_ref() else {
            return;
        };
        let sublayers: *mut AnyObject = msg_send![layer, sublayers];
        let Some(sublayers) = sublayers.as_ref() else {
            return;
        };
        let count: usize = msg_send![sublayers, count];
        for index in 0..count {
            let sub: *mut AnyObject = msg_send![sublayers, objectAtIndex: index];
            let Some(sub) = sub.as_ref() else {
                continue;
            };
            let is_metal: bool = msg_send![sub, isKindOfClass: metal];
            if is_metal {
                let _: () = msg_send![sub, setContentsScale: scale];
            }
        }
    }
}

/// The `UIView` behind `window`, if there is one.
///
/// Taken through `raw-window-handle` rather than `winit::platform::ios`, which
/// has no accessor for it — and the handle is the portable seam every GPU
/// backend already goes through, so it is a route that is exercised whether or
/// not this function is.
fn ui_view(window: &winit::window::Window) -> Option<&UIView> {
    let handle = window.window_handle().ok()?;
    let RawWindowHandle::UiKit(uikit) = handle.as_raw() else {
        return None;
    };

    // SAFETY: the pointer comes from `raw-window-handle`, which documents it as
    // a live `UIView` owned by the window — and the window outlives the borrow,
    // because `handle` borrows it. `UIView` is `MainThreadOnly` and this is
    // called from the event loop, which `winit` runs on the main thread on iOS;
    // that is the same guarantee every other UIKit call in this crate rests on.
    Some(unsafe { uikit.ui_view.cast::<UIView>().as_ref() })
}

/// How much of the bottom of `view` the soft keyboard is covering.
///
/// # Why `keyboardLayoutGuide` rather than the keyboard notifications
///
/// The canonical way to track the keyboard is to observe
/// `UIKeyboardWillChangeFrameNotification` and read
/// `UIKeyboardFrameEndUserInfoKey`. It is also a *push* model: it needs an
/// observer object, a block, a retain cycle to think about, and somewhere to
/// put the value until the next frame reads it.
///
/// Everything else in this bridge is *pull* — [`crate::app`] asks for the
/// metrics it needs on the frames it needs them, which is what
/// [`crate::insets::InsetWatch`] is for. `UIKeyboardLayoutGuide` is the pull
/// version of the same fact: a guide, automatically attached to every view,
/// whose frame is where the keyboard is. Reading it costs two message sends and
/// fits the existing shape exactly.
///
/// If it turns out not to resolve without an Auto Layout constraint referencing
/// it, the notification observer is the fallback, and it is a bigger change than
/// it looks — see the module header on what is and is not established here.
///
/// # The guard, and the failure it exists to stop
///
/// A guide that has never been laid out reports `CGRectZero`. Subtracting that
/// from the bottom of the view yields *the entire view height* as a keyboard
/// inset — a screen that is suddenly zero pixels tall, from a keyboard that is
/// not up. So the frame is only believed when it actually reaches the bottom
/// edge of the view, which a real keyboard always does and `CGRectZero` never
/// does on a view taller than nothing.
fn keyboard_inset(view: &UIView) -> EdgeInsets {
    // iOS 15. Below it the selector does not exist, and sending it anyway is an
    // unrecognised-selector exception, which on iOS is a crash rather than an
    // error — this is not a place to find out at runtime.
    if !view.respondsToSelector(sel!(keyboardLayoutGuide)) {
        return EdgeInsets::ZERO;
    }

    // SAFETY: `respondsToSelector:` has just confirmed the selector, and the
    // guide is a `UILayoutGuide` whose `layoutFrame` is a plain property read.
    let guide: Retained<UIKeyboardLayoutGuide> = unsafe { view.keyboardLayoutGuide() };
    let keyboard = unsafe { guide.layoutFrame() };
    let bounds = view.bounds();

    let view_bottom = bounds.origin.y + bounds.size.height;
    let keyboard_bottom = keyboard.origin.y + keyboard.size.height;

    // A point of slack, because these are two independently computed floats and
    // an exact comparison between them is a coin toss.
    let reaches_the_bottom = keyboard.size.height > 0.0 && keyboard_bottom >= view_bottom - 1.0;
    if !reaches_the_bottom {
        return EdgeInsets::ZERO;
    }

    let covered = (view_bottom - keyboard.origin.y).clamp(0.0, bounds.size.height);
    EdgeInsets::only(0.0, 0.0, 0.0, covered as f32)
}

/// A `CADisplayLink` that asks for the screen's full refresh rate while the
/// app is drawing, and is paused while it is idle.
///
/// # Why
///
/// The iPhone 18 Pro Max's GPU path measured 16.3 ms on every Device Farm run
/// (2026-10-16/17) against an 8.3 ms budget, and its slowest frames were two
/// missed vsyncs inside the GPU submit — a 60 Hz cadence on a 120 Hz screen.
/// `CADisableMinimumFrameDurationOnPhone` in the Info.plist only *permits*
/// more than 60 Hz on an iPhone; ProMotion stays at 60 until something running
/// asks for more, and a Vulkan swapchain (MoltenVK's `CAMetalLayer`) does not.
/// A display link with a preferred frame-rate range is the request UIKit
/// listens to.
///
/// Its callback is `-hash` on a plain `NSObject`: nothing has to happen per
/// vsync, the link only has to exist and be unpaused. Paused when the loop
/// sleeps, so an idle app lets the display drop back to its slowest rate.
///
/// Not run on an iPhone yet — the next Device Farm run is its first.
/// `VIEWW_IOS_DISPLAY_LINK=0` turns it off.
pub(crate) fn display_link_active(active: bool) {
    use std::cell::Cell;
    thread_local! {
        /// The link once made — never freed: the run loop retains it, and it
        /// lives as long as the app. Null until a screen above 60 Hz needs it,
        /// or forever when none does.
        static LINK: Cell<*mut objc2::runtime::AnyObject> = const { Cell::new(std::ptr::null_mut()) };
        static TRIED: Cell<bool> = const { Cell::new(false) };
        static PAUSED: Cell<Option<bool>> = const { Cell::new(None) };
    }
    if !TRIED.get() {
        TRIED.set(true);
        LINK.set(make_display_link().unwrap_or(std::ptr::null_mut()));
    }
    let link = LINK.get();
    if link.is_null() || PAUSED.get() == Some(!active) {
        return;
    }
    PAUSED.set(Some(!active));
    // SAFETY: `link` is the live `CADisplayLink` made below; `-setPaused:`
    // takes a `BOOL`. Main thread: called from the event loop.
    unsafe {
        let _: () = objc2::msg_send![link, setPaused: !active];
    }
}

/// `CAFrameRateRange` (iOS 15): three `float`s.
#[repr(C)]
#[derive(Clone, Copy)]
struct FrameRateRange {
    minimum: f32,
    maximum: f32,
    preferred: f32,
}

// SAFETY: the layout above is `CAFrameRateRange`'s, three C `float`s.
unsafe impl objc2::Encode for FrameRateRange {
    const ENCODING: objc2::Encoding = objc2::Encoding::Struct(
        "CAFrameRateRange",
        &[objc2::Encoding::Float, objc2::Encoding::Float, objc2::Encoding::Float],
    );
}

fn make_display_link() -> Option<*mut objc2::runtime::AnyObject> {
    use objc2::msg_send;
    use objc2::runtime::{AnyClass, AnyObject};

    if std::env::var_os("VIEWW_IOS_DISPLAY_LINK").is_some_and(|v| v == "0") {
        return None;
    }
    let screen_class = AnyClass::get("UIScreen")?;
    let link_class = AnyClass::get("CADisplayLink")?;
    let object_class = AnyClass::get("NSObject")?;
    let run_loop_class = AnyClass::get("NSRunLoop")?;
    // SAFETY: documented signatures — `+[UIScreen mainScreen]` returns a
    // `UIScreen *`, `-maximumFramesPerSecond` an `NSInteger`; `+[NSObject new]`
    // a retained object (kept for the life of the app, as the link's target);
    // `+[CADisplayLink displayLinkWithTarget:selector:]` a `CADisplayLink *`;
    // `-setPreferredFrameRateRange:` takes a `CAFrameRateRange` and
    // `-setPreferredFramesPerSecond:` an `NSInteger`; `+[NSRunLoop
    // mainRunLoop]` returns the main run loop and `-addToRunLoop:forMode:`
    // takes it and an `NSString *`. Nil results are checked. Main thread.
    unsafe {
        let screen: *mut AnyObject = msg_send![screen_class, mainScreen];
        let screen = screen.as_ref()?;
        let max: isize = msg_send![screen, maximumFramesPerSecond];
        if max <= 60 {
            return None;
        }
        let target: *mut AnyObject = msg_send![object_class, new];
        let target = target.as_ref()?;
        let link: *mut AnyObject =
            msg_send![link_class, displayLinkWithTarget: target, selector: objc2::sel!(hash)];
        let link_ref = link.as_ref()?;
        #[allow(clippy::cast_precision_loss)] // a refresh rate
        let rate = max as f32;
        let responds: bool = msg_send![
            link_ref,
            respondsToSelector: objc2::sel!(setPreferredFrameRateRange:)
        ];
        if responds {
            let range = FrameRateRange {
                minimum: 80.0_f32.min(rate),
                maximum: rate,
                preferred: rate,
            };
            let _: () = msg_send![link_ref, setPreferredFrameRateRange: range];
        } else {
            let _: () = msg_send![link_ref, setPreferredFramesPerSecond: max];
        }
        let main: *mut AnyObject = msg_send![run_loop_class, mainRunLoop];
        let main = main.as_ref()?;
        let mode = objc2_foundation::NSString::from_str("kCFRunLoopCommonModes");
        let _: () = msg_send![link_ref, addToRunLoop: main, forMode: &*mode];
        log::info!("vieww: asked for {max} Hz with a display link");
        Some(link)
    }
}
