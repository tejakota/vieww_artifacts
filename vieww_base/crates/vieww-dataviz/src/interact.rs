//! `d3-zoom` and `d3-brush`: the two interactions every exploratory chart
//! needs, as state machines a gesture layer drives.
//!
//! * [`ZoomTransform`] is d3's `{k, x, y}`: `screen = k·data + (x, y)`.
//!   [`ZoomTransform::scale_about`] zooms keeping a screen point fixed (the
//!   wheel/pinch behaviour), [`ZoomTransform::constrain`] keeps the content
//!   covering the viewport (`translateExtent`), and `rescale` maps a
//!   [`Continuous`](crate::scale::Continuous) scale's domain through it.
//! * [`Brush`] is a 1-D or 2-D selection: start a drag on empty space to
//!   draw one, drag inside to move it, drag an edge handle to resize it,
//!   clamped to the extent; [`Brush::select`] filters points.

use crate::scale::Continuous;

/// `screen = k · data + (x, y)`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ZoomTransform {
    pub k: f64,
    pub x: f64,
    pub y: f64,
}

impl Default for ZoomTransform {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl ZoomTransform {
    pub const IDENTITY: Self = Self { k: 1.0, x: 0.0, y: 0.0 };

    #[must_use]
    pub fn apply(&self, p: (f64, f64)) -> (f64, f64) {
        (p.0 * self.k + self.x, p.1 * self.k + self.y)
    }

    #[must_use]
    pub fn invert(&self, p: (f64, f64)) -> (f64, f64) {
        ((p.0 - self.x) / self.k, (p.1 - self.y) / self.k)
    }

    /// Multiply the scale by `factor` keeping screen point `at` fixed,
    /// with `k` clamped to `scale_extent`.
    #[must_use]
    pub fn scale_about(&self, factor: f64, at: (f64, f64), scale_extent: (f64, f64)) -> Self {
        let k = (self.k * factor).clamp(scale_extent.0, scale_extent.1);
        let d = self.invert(at);
        Self {
            k,
            x: at.0 - d.0 * k,
            y: at.1 - d.1 * k,
        }
    }

    #[must_use]
    pub fn translate(&self, dx: f64, dy: f64) -> Self {
        Self {
            x: self.x + dx,
            y: self.y + dy,
            ..*self
        }
    }

    /// Keep the data rectangle `content` covering the `viewport` (both
    /// `(x0, y0, x1, y1)`), like d3's default constrain.
    #[must_use]
    pub fn constrain(&self, viewport: (f64, f64, f64, f64), content: (f64, f64, f64, f64)) -> Self {
        let (c0, c1) = (self.apply((content.0, content.1)), self.apply((content.2, content.3)));
        let fix = |lo: f64, hi: f64, v0: f64, v1: f64| -> f64 {
            if hi - lo < v1 - v0 {
                // Content smaller than the viewport: centre it.
                (v0 + v1) * 0.5 - (lo + hi) * 0.5
            } else if lo > v0 {
                v0 - lo
            } else if hi < v1 {
                v1 - hi
            } else {
                0.0
            }
        };
        let dx = fix(c0.0, c1.0, viewport.0, viewport.2);
        let dy = fix(c0.1, c1.1, viewport.1, viewport.3);
        self.translate(dx, dy)
    }

    /// The domain a scale shows after this transform on its axis
    /// (`x == true` uses `k, x`; otherwise `k, y`).
    #[must_use]
    pub fn rescale(&self, s: &Continuous, x: bool) -> Continuous {
        let off = if x { self.x } else { self.y };
        let inv = |r: f64| s.invert((r - off) / self.k);
        Continuous {
            domain: (inv(s.range.0), inv(s.range.1)),
            ..*s
        }
    }
}

/// A rectangular selection.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Selection {
    pub x0: f64,
    pub y0: f64,
    pub x1: f64,
    pub y1: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Drag {
    None,
    Draw((f64, f64)),
    Move((f64, f64), Selection),
    /// Which edges move: (left, top, right, bottom).
    Resize((bool, bool, bool, bool)),
}

/// A brush over `extent`, 1-D (x only, `y` spans the extent) or 2-D.
#[derive(Debug, Clone, PartialEq)]
pub struct Brush {
    pub extent: Selection,
    pub selection: Option<Selection>,
    pub two_d: bool,
    /// Edge grab tolerance in pixels.
    pub handle: f64,
    drag: Drag,
}

impl Brush {
    #[must_use]
    pub fn x(extent: Selection) -> Self {
        Self {
            extent,
            selection: None,
            two_d: false,
            handle: 6.0,
            drag: Drag::None,
        }
    }

    #[must_use]
    pub fn xy(extent: Selection) -> Self {
        Self {
            two_d: true,
            ..Self::x(extent)
        }
    }

    fn norm(&self, a: (f64, f64), b: (f64, f64)) -> Selection {
        let e = self.extent;
        let cx = |v: f64| v.clamp(e.x0, e.x1);
        let cy = |v: f64| v.clamp(e.y0, e.y1);
        let (y0, y1) = if self.two_d { (cy(a.1.min(b.1)), cy(a.1.max(b.1))) } else { (e.y0, e.y1) };
        Selection {
            x0: cx(a.0.min(b.0)),
            x1: cx(a.0.max(b.0)),
            y0,
            y1,
        }
    }

    /// Pointer down.
    pub fn start(&mut self, p: (f64, f64)) {
        self.drag = match self.selection {
            Some(s) => {
                let h = self.handle;
                let inside_y = !self.two_d || (p.1 >= s.y0 - h && p.1 <= s.y1 + h);
                let near = |a: f64, b: f64| (a - b).abs() <= h;
                let edges = (
                    near(p.0, s.x0) && inside_y,
                    self.two_d && near(p.1, s.y0) && p.0 >= s.x0 - h && p.0 <= s.x1 + h,
                    near(p.0, s.x1) && inside_y,
                    self.two_d && near(p.1, s.y1) && p.0 >= s.x0 - h && p.0 <= s.x1 + h,
                );
                if edges.0 || edges.1 || edges.2 || edges.3 {
                    Drag::Resize(edges)
                } else if p.0 > s.x0 && p.0 < s.x1 && (!self.two_d || (p.1 > s.y0 && p.1 < s.y1)) {
                    Drag::Move(p, s)
                } else {
                    Drag::Draw(p)
                }
            }
            None => Drag::Draw(p),
        };
        if let Drag::Draw(a) = self.drag {
            // A new draw replaces the old selection immediately, as d3's does.
            self.selection = Some(self.norm(a, a));
        }
    }

    /// Pointer move.
    pub fn update(&mut self, p: (f64, f64)) {
        match self.drag {
            Drag::None => {}
            Drag::Draw(a) => self.selection = Some(self.norm(a, p)),
            Drag::Move(a, s) => {
                let e = self.extent;
                let dx = (p.0 - a.0).clamp(e.x0 - s.x0, e.x1 - s.x1);
                let dy = if self.two_d { (p.1 - a.1).clamp(e.y0 - s.y0, e.y1 - s.y1) } else { 0.0 };
                self.selection = Some(Selection {
                    x0: s.x0 + dx,
                    x1: s.x1 + dx,
                    y0: s.y0 + dy,
                    y1: s.y1 + dy,
                });
            }
            Drag::Resize((l, t, r, b)) => {
                if let Some(mut s) = self.selection {
                    if l {
                        s.x0 = p.0;
                    }
                    if r {
                        s.x1 = p.0;
                    }
                    if t {
                        s.y0 = p.1;
                    }
                    if b {
                        s.y1 = p.1;
                    }
                    self.selection = Some(self.norm((s.x0, s.y0), (s.x1, s.y1)));
                }
            }
        }
    }

    /// Pointer up; a zero-area draw clears the selection (d3's click-to-clear).
    pub fn end(&mut self) {
        if let Some(s) = self.selection {
            if (s.x1 - s.x0).abs() < 1e-9 || (self.two_d && (s.y1 - s.y0).abs() < 1e-9) {
                self.selection = None;
            }
        }
        self.drag = Drag::None;
    }

    /// Indices of `points` inside the selection.
    #[must_use]
    pub fn select(&self, points: &[(f64, f64)]) -> Vec<usize> {
        let Some(s) = self.selection else {
            return Vec::new();
        };
        points
            .iter()
            .enumerate()
            .filter(|(_, p)| p.0 >= s.x0 && p.0 <= s.x1 && (!self.two_d || (p.1 >= s.y0 && p.1 <= s.y1)))
            .map(|(i, _)| i)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zoom_keeps_the_pointer_fixed() {
        let t = ZoomTransform::IDENTITY.scale_about(2.0, (100.0, 50.0), (0.5, 8.0));
        let data_under = ZoomTransform::IDENTITY.invert((100.0, 50.0));
        let back = t.apply(data_under);
        assert!((back.0 - 100.0).abs() < 1e-9 && (back.1 - 50.0).abs() < 1e-9);
        assert_eq!(t.scale_about(100.0, (0.0, 0.0), (0.5, 8.0)).k, 8.0, "clamped");
    }

    #[test]
    fn constrain_keeps_content_on_screen() {
        let t = ZoomTransform { k: 2.0, x: 500.0, y: 0.0 }.constrain((0.0, 0.0, 400.0, 300.0), (0.0, 0.0, 400.0, 300.0));
        assert!(t.apply((0.0, 0.0)).0 <= 0.0 + 1e-9);
        let small = ZoomTransform { k: 0.5, x: 0.0, y: 0.0 }.constrain((0.0, 0.0, 400.0, 300.0), (0.0, 0.0, 400.0, 300.0));
        assert!((small.apply((200.0, 150.0)).0 - 200.0).abs() < 1e-9, "centred");
    }

    #[test]
    fn rescale_maps_the_domain() {
        let s = Continuous::linear((0.0, 10.0), (0.0, 100.0));
        let t = ZoomTransform { k: 2.0, x: -50.0, y: 0.0 };
        let r = t.rescale(&s, true);
        assert!((r.domain.0 - 2.5).abs() < 1e-9 && (r.domain.1 - 7.5).abs() < 1e-9);
    }

    #[test]
    fn brush_draw_move_resize_clear() {
        let mut b = Brush::xy(Selection { x0: 0.0, y0: 0.0, x1: 100.0, y1: 100.0 });
        b.start((10.0, 10.0));
        b.update((40.0, 30.0));
        b.end();
        assert_eq!(b.selection, Some(Selection { x0: 10.0, y0: 10.0, x1: 40.0, y1: 30.0 }));
        b.start((25.0, 20.0));
        b.update((95.0, 20.0)); // move, clamped to the extent
        b.end();
        assert_eq!(b.selection.unwrap().x1, 100.0);
        assert_eq!(b.selection.unwrap().x0, 70.0);
        b.start((70.0, 20.0)); // left edge
        b.update((50.0, 20.0));
        b.end();
        assert_eq!(b.selection.unwrap().x0, 50.0);
        assert_eq!(b.select(&[(60.0, 15.0), (10.0, 15.0), (60.0, 90.0)]), vec![0]);
        b.start((5.0, 90.0));
        b.end();
        assert_eq!(b.selection, None, "click outside clears");
    }

    #[test]
    fn a_1d_brush_spans_the_full_height() {
        let mut b = Brush::x(Selection { x0: 0.0, y0: 0.0, x1: 200.0, y1: 40.0 });
        b.start((20.0, 5.0));
        b.update((80.0, 7.0));
        b.end();
        let s = b.selection.unwrap();
        assert_eq!((s.y0, s.y1), (0.0, 40.0));
        assert_eq!(b.select(&[(50.0, 39.0), (90.0, 1.0)]), vec![0]);
    }
}
