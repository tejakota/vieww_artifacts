//! `Konva.Tween`, `node.to()` and `Konva.Animation` for the stage.
//!
//! A [`NodeTween`] moves any subset of a node's attributes (position,
//! rotation, scale, offset, opacity, fill and stroke colour, stroke width)
//! from wherever they are when it starts to target values, over a duration
//! with a [`Curve`], with a delay, repeats and yoyo — Konva's `Tween`
//! options. A [`Timeline`] owns tweens and per-frame [`Timeline::animate`]
//! callbacks (Konva's `Animation`, which hands you `frame.time` and
//! `frame.timeDiff`), and is advanced by absolute time like everything else
//! in vieww: `update(stage, now)` applies every tween's value at `now`, so a
//! dropped frame skips ahead rather than stretching.

use std::time::Duration;

use vieww_animation::Curve;
use vieww_foundation::{Color, Offset};

use crate::{NodeId, Stage};

/// Target values; `None` leaves an attribute alone.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct To {
    pub x: Option<f32>,
    pub y: Option<f32>,
    pub rotation: Option<f32>,
    pub scale_x: Option<f32>,
    pub scale_y: Option<f32>,
    pub offset: Option<Offset>,
    pub opacity: Option<f32>,
    pub fill: Option<Color>,
    pub stroke: Option<Color>,
    pub stroke_width: Option<f32>,
}

impl To {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            x: None,
            y: None,
            rotation: None,
            scale_x: None,
            scale_y: None,
            offset: None,
            opacity: None,
            fill: None,
            stroke: None,
            stroke_width: None,
        }
    }
    #[must_use]
    pub const fn xy(mut self, x: f32, y: f32) -> Self {
        self.x = Some(x);
        self.y = Some(y);
        self
    }
    #[must_use]
    pub const fn rotation(mut self, r: f32) -> Self {
        self.rotation = Some(r);
        self
    }
    #[must_use]
    pub const fn scale(mut self, s: f32) -> Self {
        self.scale_x = Some(s);
        self.scale_y = Some(s);
        self
    }
    #[must_use]
    pub const fn opacity(mut self, o: f32) -> Self {
        self.opacity = Some(o);
        self
    }
    #[must_use]
    pub const fn fill(mut self, c: Color) -> Self {
        self.fill = Some(c);
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct From {
    x: f32,
    y: f32,
    rotation: f32,
    scale_x: f32,
    scale_y: f32,
    offset: Offset,
    opacity: f32,
    fill: Option<Color>,
    stroke: Option<Color>,
    stroke_width: f32,
}

/// One tween on one node.
#[derive(Debug, Clone)]
pub struct NodeTween {
    pub node: NodeId,
    pub to: To,
    pub duration: Duration,
    pub delay: Duration,
    pub curve: Curve,
    /// Extra plays after the first (`u32::MAX` = forever).
    pub repeat: u32,
    pub yoyo: bool,
    start: Option<Duration>,
    from: Option<From>,
    done: bool,
}

impl NodeTween {
    #[must_use]
    pub fn new(node: NodeId, to: To, duration: Duration) -> Self {
        Self {
            node,
            to,
            duration,
            delay: Duration::ZERO,
            curve: Curve::EASE_IN_OUT,
            repeat: 0,
            yoyo: false,
            start: None,
            from: None,
            done: false,
        }
    }
    #[must_use]
    pub fn curve(mut self, c: Curve) -> Self {
        self.curve = c;
        self
    }
    #[must_use]
    pub fn delay(mut self, d: Duration) -> Self {
        self.delay = d;
        self
    }
    #[must_use]
    pub fn repeat(mut self, n: u32, yoyo: bool) -> Self {
        self.repeat = n;
        self.yoyo = yoyo;
        self
    }
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

fn lerp_c(a: Color, b: Color, t: f32) -> Color {
    let l = |x: u8, y: u8| {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let v = lerp(f32::from(x), f32::from(y), t).round().clamp(0.0, 255.0) as u8;
        v
    };
    Color::rgba(l(a.r, b.r), l(a.g, b.g), l(a.b, b.b), l(a.a, b.a))
}

/// A per-frame callback: `(stage, time since start, time since last frame)`.
pub type FrameFn = Box<dyn FnMut(&mut Stage, Duration, Duration)>;

/// Tweens and animations on one clock.
#[derive(Default)]
pub struct Timeline {
    tweens: Vec<NodeTween>,
    animations: Vec<(Option<Duration>, FrameFn)>,
    last: Option<Duration>,
}

impl std::fmt::Debug for Timeline {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Timeline")
            .field("tweens", &self.tweens.len())
            .field("animations", &self.animations.len())
            .finish()
    }
}

impl Timeline {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Start a tween (it captures its start values on its first update).
    pub fn to(&mut self, t: NodeTween) {
        self.tweens.push(t);
    }

    /// Run `f` every frame (Konva's `new Konva.Animation(f, layer).start()`).
    pub fn animate(&mut self, f: impl FnMut(&mut Stage, Duration, Duration) + 'static) {
        self.animations.push((None, Box::new(f)));
    }

    /// Whether any tween is still running.
    #[must_use]
    pub fn is_active(&self) -> bool {
        !self.animations.is_empty() || self.tweens.iter().any(|t| !t.done)
    }

    /// Apply everything at absolute time `now`.
    pub fn update(&mut self, stage: &mut Stage, now: Duration) {
        let dt = self.last.map_or(Duration::ZERO, |l| now.saturating_sub(l));
        self.last = Some(now);
        for t in &mut self.tweens {
            if t.done {
                continue;
            }
            let start = *t.start.get_or_insert(now);
            let a = stage.attrs_mut(t.node);
            let from = *t.from.get_or_insert(From {
                x: a.x,
                y: a.y,
                rotation: a.rotation,
                scale_x: a.scale_x,
                scale_y: a.scale_y,
                offset: a.offset,
                opacity: a.opacity,
                fill: a.fill,
                stroke: a.stroke,
                stroke_width: a.stroke_width,
            });
            let local = now.saturating_sub(start);
            if local < t.delay {
                continue;
            }
            let run = (local - t.delay).as_secs_f32();
            let d = t.duration.as_secs_f32().max(1e-6);
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let play = (run / d).floor() as u64;
            let finished = t.repeat != u32::MAX && play > u64::from(t.repeat);
            let mut u = if finished { 1.0 } else { (run / d).fract() };
            if finished {
                t.done = true;
                // The final play's direction decides where it rests.
                if t.yoyo && t.repeat % 2 == 1 {
                    u = 0.0;
                }
            } else if t.yoyo && play % 2 == 1 {
                u = 1.0 - u;
            }
            let k = t.curve.transform(u);
            let to = t.to;
            if let Some(v) = to.x {
                a.x = lerp(from.x, v, k);
            }
            if let Some(v) = to.y {
                a.y = lerp(from.y, v, k);
            }
            if let Some(v) = to.rotation {
                a.rotation = lerp(from.rotation, v, k);
            }
            if let Some(v) = to.scale_x {
                a.scale_x = lerp(from.scale_x, v, k);
            }
            if let Some(v) = to.scale_y {
                a.scale_y = lerp(from.scale_y, v, k);
            }
            if let Some(v) = to.offset {
                a.offset = Offset::new(lerp(from.offset.dx, v.dx, k), lerp(from.offset.dy, v.dy, k));
            }
            if let Some(v) = to.opacity {
                a.opacity = lerp(from.opacity, v, k);
            }
            if let Some(v) = to.fill {
                a.fill = Some(lerp_c(from.fill.unwrap_or(Color::TRANSPARENT), v, k));
            }
            if let Some(v) = to.stroke {
                a.stroke = Some(lerp_c(from.stroke.unwrap_or(Color::TRANSPARENT), v, k));
            }
            if let Some(v) = to.stroke_width {
                a.stroke_width = lerp(from.stroke_width, v, k);
            }
        }
        for (start, f) in &mut self.animations {
            let s = *start.get_or_insert(now);
            f(stage, now.saturating_sub(s), dt);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Attrs, Shape};

    fn stage() -> (Stage, NodeId) {
        let mut s = Stage::new(200.0, 200.0);
        let l = s.add_layer();
        let id = s.add_shape(
            l,
            Attrs { fill: Some(Color::rgb(0, 0, 0)), ..Attrs::default() },
            Shape::Rect { width: 10.0, height: 10.0, corner: 0.0 },
        );
        (s, id)
    }

    fn ms(v: u64) -> Duration {
        Duration::from_millis(v)
    }

    #[test]
    fn a_tween_moves_from_where_it_is_to_where_it_goes() {
        let (mut s, id) = stage();
        let mut tl = Timeline::new();
        tl.to(NodeTween::new(id, To::new().xy(100.0, 50.0).fill(Color::rgb(200, 100, 0)), ms(1000)).curve(Curve::Linear));
        tl.update(&mut s, ms(0));
        tl.update(&mut s, ms(500));
        let a = s.node(id).attrs.clone();
        assert!((a.x - 50.0).abs() < 1e-3 && (a.y - 25.0).abs() < 1e-3);
        assert_eq!(a.fill, Some(Color::rgb(100, 50, 0)));
        tl.update(&mut s, ms(5000));
        assert_eq!(s.node(id).attrs.x, 100.0);
        assert!(!tl.is_active());
    }

    #[test]
    fn delay_repeat_and_yoyo() {
        let (mut s, id) = stage();
        let mut tl = Timeline::new();
        tl.to(NodeTween::new(id, To::new().opacity(0.0), ms(100)).curve(Curve::Linear).delay(ms(50)).repeat(1, true));
        tl.update(&mut s, ms(0));
        tl.update(&mut s, ms(40));
        assert_eq!(s.node(id).attrs.opacity, 1.0, "still in the delay");
        tl.update(&mut s, ms(100));
        assert!((s.node(id).attrs.opacity - 0.5).abs() < 1e-3);
        tl.update(&mut s, ms(175));
        assert!((s.node(id).attrs.opacity - 0.25).abs() < 1e-3, "on the way back");
        tl.update(&mut s, ms(1000));
        assert_eq!(s.node(id).attrs.opacity, 1.0, "a yoyo repeat ends where it began");
    }

    #[test]
    fn animations_get_time_and_delta() {
        let (mut s, id) = stage();
        let mut tl = Timeline::new();
        tl.animate(move |st, t, dt| {
            st.attrs_mut(id).rotation = t.as_secs_f32() * 90.0 + dt.as_secs_f32() * 0.0;
        });
        tl.update(&mut s, ms(1000));
        tl.update(&mut s, ms(1500));
        assert!((s.node(id).attrs.rotation - 45.0).abs() < 1e-3);
    }
}
