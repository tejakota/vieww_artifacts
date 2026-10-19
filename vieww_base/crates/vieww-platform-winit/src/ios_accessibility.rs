//! The iOS half of the screen-reader contract: the semantics tree as
//! VoiceOver sees it, through UIKit.
//!
//! # Status: written, not run
//!
//! The same standing as [`crate::ios`]'s insets work, and for the same
//! reason: **no frame has reached an iPhone**, so nothing here has executed
//! against a real UIKit. It type-checks for `aarch64-apple-ios` — the
//! selectors, the feature-gated classes and the method shapes were verified
//! against the fetched `objc2-ui-kit` 0.2.2 source rather than against
//! memory — and that establishes the types line up, nothing more. The
//! XCUITest that would hear it (`ci/mobile/a11y-ios.sh`,
//! `ci/mobile/xcuitest/ViewwA11yTests.swift`) is checked in beside this
//! file and runs on macOS, which is the machine this container is not.
//!
//! # Why this exists beside AccessKit
//!
//! The semantics tree already has a consumer: [`crate::a11y`] converts it to
//! an AccessKit `TreeUpdate`, and AccessKit's platform adapters carry it to
//! UI Automation, AT-SPI, NSAccessibility and the Android accessibility API —
//! exactly the part `docs/DESIGN.md` §9 says not to attempt from scratch.
//! **AccessKit ships no iOS adapter**, which is why PENDING §3.6 records
//! exactly this hole: Android's TalkBack has been heard (2026-10-14), iOS's
//! VoiceOver never. This module is the iOS arrow AccessKit does not draw,
//! built on the same tree [`crate::a11y`] consumes, so the two platforms
//! cannot drift apart on *what* is exposed — only on how it is handed over.
//!
//! # The shape
//!
//! One call per semantics rebuild, [`publish_semantics`]:
//!
//! * **VoiceOver-gated.** `UIAccessibilityIsVoiceOverRunning` first, for the
//!   reason the AccessKit closure is lazy: no screen reader, no cost.
//!   Building an element per node every frame for nobody is the per-frame
//!   tax the AccessKit path exists to avoid.
//! * **One `UIAccessibilityElement` per labelled or interactive node**, in
//!   reading order, each carrying the label, the button/static-text trait,
//!   the enabled state (the `NotEnabled` trait — the one piece of a dimmed
//!   control's state otherwise carried only by colour), the frame in screen
//!   points (vieww's global logical pixels *are* UIKit's points — see
//!   [`crate::ios`]'s no-scale-conversion note), and the packed render id as
//!   the accessibility identifier, so the XCUITest can assert against a
//!   stable name rather than a position.
//! * **The host view becomes a container, not an element** —
//!   `isAccessibilityElement = false`, `accessibilityElements = [...]` —
//!   which is the standard shape for a custom-drawn surface: VoiceOver walks
//!   the children we publish instead of reading the view as one blob. These
//!   two selectors go through `msg_send!` rather than through bound methods,
//!   the way [`crate::ios`]'s layer-scale code does for the same class of
//!   selector: the `UIAccessibility` category on `NSObject` is not generated
//!   as inherent methods on `UIView`.
//! * **`screenChanged` with the focused node's element** when the tree has
//!   one — the announcement that makes focus changes heard rather than
//!   visible, and the half of the contract PENDING §3.6 says is missing.
//!
//! # What this deliberately does not do
//!
//! Activation. A VoiceOver double-tap is an `accessibilityActivate` on an
//! element, and routing it back into `SemanticAction` is a block-with-state
//! problem the first XCUITest run will inform — writing it now would be a
//! guess in the one place a guess cannot be fixed by reading code. The
//! elements promise no actions; announcing what is on screen, hearing it,
//! and focus tracking are the contract's first half, and the test asserts
//! exactly that half.

use std::cell::RefCell;

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2_foundation::{CGPoint, CGRect, CGSize, MainThreadMarker, NSMutableArray, NSString};
use objc2_ui_kit::{
    UIAccessibilityElement, UIAccessibilityIdentification, UIAccessibilityIsVoiceOverRunning,
    UIAccessibilityPostNotification, UIAccessibilityScreenChangedNotification,
    UIAccessibilityTraitButton, UIAccessibilityTraitNotEnabled, UIAccessibilityTraitStaticText,
    UIAccessibilityTraits, UIView,
};
use vieww_foundation::Rect;
use vieww_render::{RenderId, Role, SemanticsTree};

use crate::scale::Scale;

/// The published elements of the last [`publish_semantics`], keyed by the
/// packed render id — the lookup [`announce_focus`] needs when the tree's
/// focused node changes. A thread-local rather than app state because the
/// elements are already reachable from the view that holds them, and this
/// map only exists to hand the focused one back; it dies with the thread
/// that published them, which is the main thread, which is the only thread
/// UIKit accessibility exists on — `objc2` 0.5 has no `MainThreadMarker`
/// to enforce it, so the safety notes below carry the requirement in
/// words instead.
thread_local! {
    static PUBLISHED: RefCell<Vec<(u64, Retained<UIAccessibilityElement>)>> =
        const { RefCell::new(Vec::new()) };
}

/// A render id packed the way [`crate::a11y::node_id`] packs it for
/// AccessKit — generation above index — so both platforms hand a screen
/// reader the same identity for the same node.
fn packed(id: RenderId) -> u64 {
    (u64::from(id.generation()) << 32) | u64::from(id.index())
}

/// Publish `semantics` to VoiceOver on `window`'s view, announcing the
/// focused node if the tree has one. Returns how many elements were
/// published — the number the device-suite log line prints — or `None` when
/// there was nothing to do: no VoiceOver, no main thread, no view.
///
/// **The walk is a full read of `semantics`, not a diff.** Replacing the
/// whole `accessibilityElements` array per update is the shape UIKit's own
/// container examples use for small trees, and vieww's semantics trees are
/// small (the gallery's editor screen is a few dozen nodes). Per-node
/// diffing is the change worth making only when a device run says this one
/// is measurable, which is the same measure-first rule the rest of this
/// framework's caches were held to.
pub(crate) fn publish_semantics(
    window: &winit::window::Window,
    semantics: &SemanticsTree,
    _scale: Scale,
) -> Option<usize> {
    // SAFETY: a plain C query over global VoiceOver state; no object, no
    // memory it does not own.
    if !(unsafe { UIAccessibilityIsVoiceOverRunning() }).as_bool() {
        return None;
    }
    // The frame/event path this runs on is the main thread by winit's own
    // contract; `MainThreadMarker::new()` is the checkable spelling of that
    // fact, and `None` here means "not on the main thread", which is not a
    // place UIKit accessibility can be touched from at all.
    let mtm = MainThreadMarker::new()?;
    let view = crate::ios::ui_view(window)?;
    // SAFETY (whole call): the frame/event path this runs on is the main
    // thread by winit's own contract, which is what every UIKit call below
    // additionally requires.
    unsafe { Some(publish_on_view(view, semantics, mtm)) }
}

/// Build and attach the elements to `view`, then announce focus.
///
/// # Safety
///
/// Must run on the main thread — every UIKit call here requires it, and
/// `objc2` 0.5 has no `MainThreadMarker` type to make it a check.
unsafe fn publish_on_view(
    view: &UIView,
    semantics: &SemanticsTree,
    mtm: MainThreadMarker,
) -> usize {
    let mut published: Vec<(u64, Retained<UIAccessibilityElement>)> = Vec::new();
    let mut focused_element: Option<Retained<UIAccessibilityElement>> = None;

    for id in semantics.reading_order() {
        let Some(node) = semantics.node(id) else {
            continue;
        };
        // A group with no label carries nothing a screen reader can speak —
        // the same rule `crate::a11y`'s conversion applies, and the reason
        // both bridges skip it rather than publishing a container the user
        // has to swipe through to reach its children.
        let speakable = node.label.is_some()
            || matches!(node.role, Role::Button | Role::Label | Role::TextField);
        if !speakable {
            continue;
        }
        // SAFETY: `initWithAccessibilityContainer:` is the class's designed
        // initialiser, this runs on the main thread (the caller's contract),
        // and `view` is the live UIView the elements are parented to — its
        // pointer is cast to `&AnyObject` because the header types the
        // container as `id`, and any object pointer is a valid `AnyObject`
        // pointer.
        let element: Retained<UIAccessibilityElement> = unsafe {
            let container: &AnyObject = &*std::ptr::from_ref::<UIView>(view).cast::<AnyObject>();
            UIAccessibilityElement::initWithAccessibilityContainer(mtm.alloc(), container)
        };
        // All inherent methods on the class, per the 0.2.2 bindings —
        // label/value/traits/frame in the `UIAccessibility` category, the
        // identifier through the `UIAccessibilityIdentification` trait
        // whose conformance the bindings declare.
        //
        // SAFETY: setters on a live element, main thread (the caller's
        // contract), and the strings are retained by the setters.
        unsafe {
            if let Some(label) = &node.label {
                element.setAccessibilityLabel(Some(&NSString::from_str(label)));
            }
            if let Some(value) = &node.value {
                element.setAccessibilityValue(Some(&NSString::from_str(value)));
            }
            element.setAccessibilityTraits(traits_for(node));
            element.setAccessibilityFrame(frame_for(node));
            element.setAccessibilityIdentifier(Some(&NSString::from_str(&format!(
                "vieww.{}",
                packed(node.id)
            ))));
        }
        if node.focused && focused_element.is_none() {
            focused_element = Some(element.clone());
        }
        published.push((packed(node.id), element));
    }

    let count = published.len();
    let elements: Vec<Retained<UIAccessibilityElement>> =
        published.iter().map(|(_, e)| e.clone()).collect();
    let array = NSMutableArray::from_vec(elements);
    PUBLISHED.with(|slot| *slot.borrow_mut() = published);

    // SAFETY: the two selectors are `UIView`'s own accessibility-container
    // API (declared in the `UIAccessibility` category on `NSObject`), the
    // view is live on the main thread, and the array is retained for the
    // selector's duration and by the view afterwards — UIKit copies it.
    unsafe {
        use objc2::msg_send;
        let _: () = msg_send![view, setIsAccessibilityElement: false];
        let _: () = msg_send![view, setAccessibilityElements: &*array];
        if let Some(focused) = &focused_element {
            UIAccessibilityPostNotification(
                UIAccessibilityScreenChangedNotification,
                Some(&*Retained::cast::<AnyObject>(focused.clone())),
            );
        }
    }
    count
}

/// The focus announcement for a node that changed after its publish — the
/// other half of "a screen reader announces where focus went". Quiet when
/// the id is not in the last published set, which is the honest answer: an
/// element that was never announced as present should not be announced as
/// focused either.
pub(crate) fn announce_focus(id: RenderId) {
    let key = packed(id);
    PUBLISHED.with(|slot| {
        let map = slot.borrow();
        if let Some((_, element)) = map.iter().find(|(k, _)| *k == key) {
            // SAFETY: the notification posts on the main thread (the only
            // thread this map is populated on), and the argument is the
            // element UIKit already holds in the view's container array.
            unsafe {
                UIAccessibilityPostNotification(
                    UIAccessibilityScreenChangedNotification,
                    Some(&*Retained::cast::<AnyObject>(element.clone())),
                );
            }
        }
    });
}

/// The trait word for a role, in the vocabulary VoiceOver announces: a
/// button is "button", static text is read as text, anything else stays
/// traitless and is read by label alone. Disabled adds `NotEnabled` — the
/// trait that makes VoiceOver say "dimmed", which is the one piece of a
/// control's state otherwise carried only by colour (the semantics field's
/// own doc says exactly this).
fn traits_for(node: &vieww_render::SemanticsNode) -> UIAccessibilityTraits {
    // The trait constants are `extern static`s (UIKit exposes them as
    // `UIAccessibilityTraits` globals), so every read is an unsafe read —
    // SAFETY: each is a plain `u64` constant that UIKit documents as
    // immutable, and reading it has no aliasing or data-race surface.
    let (button, static_text, not_enabled) = unsafe {
        (
            UIAccessibilityTraitButton,
            UIAccessibilityTraitStaticText,
            UIAccessibilityTraitNotEnabled,
        )
    };
    let mut traits: UIAccessibilityTraits = 0;
    match node.role {
        Role::Button => traits |= button,
        Role::Label => traits |= static_text,
        _ => {}
    }
    if !node.enabled {
        traits |= not_enabled;
    }
    traits
}

/// A node's bounds as a UIKit screen rect. No scale conversion — vieww's
/// global logical pixels and UIKit's screen points are the same unit on iOS
/// (`crate::ios`'s insets doc has the full argument, and the same absence
/// of a `/ scale` here is the same correctness, not the same oversight).
fn frame_for(node: &vieww_render::SemanticsNode) -> CGRect {
    let Rect {
        left,
        top,
        right,
        bottom,
    } = node.bounds;
    CGRect::new(
        CGPoint::new(left as f64, top as f64),
        CGSize::new(
            (right - left).max(0.0) as f64,
            (bottom - top).max(0.0) as f64,
        ),
    )
}
