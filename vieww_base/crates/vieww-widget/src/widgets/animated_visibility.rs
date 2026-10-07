//! `AnimatedVisibility` — Jetpack Compose's enter/exit transition widget
//! (§2.11), SwiftUI's `.transition()` and Flutter's `AnimatedSwitcher` for
//! the show/hide case.
//!
//! Toggle `visible` and the child **enters** (fades, slides from an edge,
//! scales up — any combination) or **exits** the reverse way; once an exit
//! has finished the child is gone from the tree entirely (not merely
//! transparent), so it neither paints, lays out nor receives input. Built on
//! [`Animated`], so an interrupted transition reverses from where it is.

use std::time::Duration;

use vieww_animation::Curve;
use vieww_foundation::{Offset, Transform};

use crate::{Animated, Opacity, SizedBox, Transformed, WidgetNode};

/// One ingredient of an enter/exit transition.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Transition {
    /// Opacity 0 → 1.
    Fade,
    /// Translate from `offset` (logical pixels) to rest.
    Slide(Offset),
    /// Scale from `from` to 1.
    Scale(f32),
}

/// See the [module docs](self).
#[derive(Clone)]
pub struct AnimatedVisibility {
    visible: bool,
    transitions: Vec<Transition>,
    duration: Duration,
    curve: Curve,
    child: Option<WidgetNode>,
}

impl std::fmt::Debug for AnimatedVisibility {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AnimatedVisibility")
            .field("visible", &self.visible)
            .field("transitions", &self.transitions)
            .finish_non_exhaustive()
    }
}

impl AnimatedVisibility {
    /// Fade by default (Compose's `fadeIn() + expandIn()` is
    /// `.with(Fade).with(Scale(..))`).
    #[must_use]
    pub fn new(visible: bool) -> Self {
        Self {
            visible,
            transitions: Vec::new(),
            duration: Duration::from_millis(220),
            curve: Curve::FAST_OUT_SLOW_IN,
            child: None,
        }
    }

    #[must_use]
    pub fn with(mut self, t: Transition) -> Self {
        self.transitions.push(t);
        self
    }

    #[must_use]
    pub const fn duration(mut self, d: Duration) -> Self {
        self.duration = d;
        self
    }

    #[must_use]
    pub const fn curve(mut self, c: Curve) -> Self {
        self.curve = c;
        self
    }

    #[must_use]
    pub fn child(mut self, child: impl Into<WidgetNode>) -> Self {
        self.child = Some(child.into());
        self
    }

    /// The widget for progress `t` (0 = hidden, 1 = shown) — what the
    /// builder renders each frame, public so a painter or test can ask.
    #[must_use]
    pub fn at(transitions: &[Transition], child: &WidgetNode, t: f32) -> WidgetNode {
        if t <= 0.0005 {
            return SizedBox::shrink().into();
        }
        let ts: &[Transition] = if transitions.is_empty() { &[Transition::Fade] } else { transitions };
        let mut node = child.clone();
        let mut transform = Transform::IDENTITY;
        for tr in ts {
            match *tr {
                Transition::Fade => node = Opacity::new(t.clamp(0.0, 1.0)).child(node).into(),
                Transition::Slide(o) => {
                    transform = transform.then(Transform::translate(Offset::new(o.dx * (1.0 - t), o.dy * (1.0 - t))));
                }
                Transition::Scale(from) => {
                    let s = from + (1.0 - from) * t;
                    transform = transform.then(Transform::scale(s, s));
                }
            }
        }
        if transform.is_identity() {
            node
        } else {
            Transformed::new(transform).child(node).into()
        }
    }

    /// Into the [`Animated`] that runs it.
    #[must_use]
    pub fn into_animated(self) -> Animated {
        let child = self.child.unwrap_or_else(|| SizedBox::shrink().into());
        let ts = self.transitions;
        Animated::new(if self.visible { 1.0 } else { 0.0 })
            .duration(self.duration)
            .curve(self.curve)
            .build(move |t| Self::at(&ts, &child, t))
    }
}

impl From<AnimatedVisibility> for WidgetNode {
    fn from(v: AnimatedVisibility) -> Self {
        v.into_animated().into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ColoredBox;
    use std::any::TypeId;
    use vieww_foundation::Color;

    fn child() -> WidgetNode {
        ColoredBox::new(Color::RED).child(SizedBox::square(10.0)).into()
    }

    #[test]
    fn hidden_means_absent_not_transparent() {
        let n = AnimatedVisibility::at(&[Transition::Fade], &child(), 0.0);
        assert_eq!(n.widget_type_id(), TypeId::of::<SizedBox>());
    }

    #[test]
    fn mid_transition_composes_fade_slide_scale() {
        let ts = [Transition::Fade, Transition::Slide(Offset::new(0.0, 40.0)), Transition::Scale(0.8)];
        let n = AnimatedVisibility::at(&ts, &child(), 0.5);
        assert_eq!(n.widget_type_id(), TypeId::of::<Transformed>());
        let full = AnimatedVisibility::at(&[Transition::Slide(Offset::new(0.0, 40.0))], &child(), 1.0);
        assert_eq!(full.widget_type_id(), TypeId::of::<ColoredBox>(), "at rest there is no wrapper");
    }

    #[test]
    fn it_builds_an_animated() {
        let a = AnimatedVisibility::new(true).with(Transition::Fade).child(child()).into_animated();
        assert_eq!(a.target(), 1.0);
    }
}
