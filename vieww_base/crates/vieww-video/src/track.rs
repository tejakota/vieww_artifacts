//! Motion tracking and stabilisation — After Effects' Tracker and Warp
//! Stabilizer layer (§2.15 L7: "Analyze footage → generate masks/tracking
//! data → apply to layers").
//!
//! [`PointTracker`] follows a patch through a sequence by **normalised
//! cross-correlation** over a search window, refined to sub-pixel accuracy
//! by fitting a parabola through the correlation peak — the classic
//! template tracker AE's point tracker is. [`stabilize`] turns one or two
//! tracks into per-frame corrective transforms (translation from one point;
//! translation, rotation and scale from two), and [`apply_transform`]
//! resamples a frame through one. Roto-brush-style segmentation is a
//! learned model and is not claimed; keying is in [`matte`](crate::matte).

use vieww_foundation::{Image, Offset, Transform};

fn luma(img: &Image, x: i64, y: i64) -> f32 {
    let (w, h) = (i64::from(img.width()), i64::from(img.height()));
    let (x, y) = (x.clamp(0, w - 1), y.clamp(0, h - 1));
    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    let i = ((y * w + x) * 4) as usize;
    let p = img.pixels();
    0.299 * f32::from(p[i]) + 0.587 * f32::from(p[i + 1]) + 0.114 * f32::from(p[i + 2])
}

/// A single-feature tracker.
#[derive(Debug, Clone, PartialEq)]
pub struct PointTracker {
    /// Half-size of the feature patch (a patch is `2r+1` square).
    pub patch: i64,
    /// How far (pixels) from the last position to search.
    pub search: i64,
    template: Vec<f32>,
    position: Offset,
    /// Correlation of the last match (1.0 = identical).
    pub confidence: f32,
}

impl PointTracker {
    /// Start tracking the feature at `at` in `frame`.
    #[must_use]
    pub fn new(frame: &Image, at: Offset, patch: i64, search: i64) -> Self {
        let mut t = Self {
            patch,
            search,
            template: Vec::new(),
            position: at,
            confidence: 1.0,
        };
        #[allow(clippy::cast_possible_truncation)]
        let (cx, cy) = (at.dx.round() as i64, at.dy.round() as i64);
        t.template = t.sample(frame, cx, cy);
        t
    }

    fn sample(&self, img: &Image, cx: i64, cy: i64) -> Vec<f32> {
        let r = self.patch;
        let mut v = Vec::with_capacity(((2 * r + 1) * (2 * r + 1)) as usize);
        for y in -r..=r {
            for x in -r..=r {
                v.push(luma(img, cx + x, cy + y));
            }
        }
        v
    }

    fn ncc(a: &[f32], b: &[f32]) -> f32 {
        #[allow(clippy::cast_precision_loss)]
        let n = a.len() as f32;
        let (ma, mb) = (a.iter().sum::<f32>() / n, b.iter().sum::<f32>() / n);
        let (mut num, mut da, mut db) = (0.0, 0.0, 0.0);
        for (x, y) in a.iter().zip(b) {
            num += (x - ma) * (y - mb);
            da += (x - ma) * (x - ma);
            db += (y - mb) * (y - mb);
        }
        if da < 1e-6 || db < 1e-6 {
            return 0.0;
        }
        num / (da * db).sqrt()
    }

    /// Find the feature in the next frame; returns its (sub-pixel)
    /// position.
    pub fn track(&mut self, frame: &Image) -> Offset {
        #[allow(clippy::cast_possible_truncation)]
        let (px, py) = (
            self.position.dx.round() as i64,
            self.position.dy.round() as i64,
        );
        let s = self.search;
        let size = (2 * s + 1) as usize;
        let mut scores = vec![f32::MIN; size * size];
        let (mut best, mut bx, mut by) = (f32::MIN, 0i64, 0i64);
        for dy in -s..=s {
            for dx in -s..=s {
                let score = Self::ncc(&self.template, &self.sample(frame, px + dx, py + dy));
                #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
                {
                    scores[((dy + s) as usize) * size + (dx + s) as usize] = score;
                }
                if score > best {
                    best = score;
                    bx = dx;
                    by = dy;
                }
            }
        }
        // Parabolic sub-pixel refinement along each axis.
        let at = |dx: i64, dy: i64| {
            if dx.abs() > s || dy.abs() > s {
                return best;
            }
            #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
            scores[((dy + s) as usize) * size + (dx + s) as usize]
        };
        let refine = |l: f32, c: f32, r: f32| {
            let d = l - 2.0 * c + r;
            if d.abs() < 1e-9 {
                0.0
            } else {
                (0.5 * (l - r) / d).clamp(-0.5, 0.5)
            }
        };
        let sx = refine(at(bx - 1, by), best, at(bx + 1, by));
        let sy = refine(at(bx, by - 1), best, at(bx, by + 1));
        #[allow(clippy::cast_precision_loss)]
        let p = Offset::new((px + bx) as f32 + sx, (py + by) as f32 + sy);
        self.position = p;
        self.confidence = best;
        p
    }

    #[must_use]
    pub const fn position(&self) -> Offset {
        self.position
    }
}

/// Per-frame corrective transforms that hold the tracked feature(s) where
/// they were in frame 0. One track: translation. Two tracks: translation,
/// rotation and uniform scale (a similarity), as Warp Stabilizer's
/// "position, scale, rotation" mode.
#[must_use]
pub fn stabilize(tracks: &[Vec<Offset>]) -> Vec<Transform> {
    let n = tracks.first().map_or(0, Vec::len);
    (0..n)
        .map(|f| match tracks {
            [a] => {
                let d = a[0] - a[f];
                Transform::translate(d)
            }
            [a, b, ..] => {
                let (a0, b0, af, bf) = (a[0], b[0], a[f], b[f]);
                let v0 = b0 - a0;
                let vf = bf - af;
                let scale = v0.distance() / vf.distance().max(1e-6);
                let rot = v0.dy.atan2(v0.dx) - vf.dy.atan2(vf.dx);
                // Map af → a0 after rotating/scaling about af.
                Transform::translate(Offset::new(-af.dx, -af.dy))
                    .then(Transform::scale(scale, scale))
                    .then(Transform::rotate(rot))
                    .then(Transform::translate(a0))
            }
            [] => Transform::IDENTITY,
        })
        .collect()
}

/// Resample `frame` through `t` (bilinear; outside is transparent).
#[must_use]
pub fn apply_transform(frame: &Image, t: Transform) -> Image {
    let (w, h) = (frame.width(), frame.height());
    let inv = t.invert().unwrap_or(Transform::IDENTITY);
    let src = frame.pixels();
    let mut out = vec![0u8; (w * h * 4) as usize];
    let (wi, hi) = (i64::from(w), i64::from(h));
    for y in 0..h {
        for x in 0..w {
            #[allow(clippy::cast_precision_loss)]
            let p = inv.apply(Offset::new(x as f32 + 0.5, y as f32 + 0.5));
            let (fx, fy) = (p.dx - 0.5, p.dy - 0.5);
            let (x0, y0) = (fx.floor(), fy.floor());
            let (tx, ty) = (fx - x0, fy - y0);
            #[allow(clippy::cast_possible_truncation)]
            let (x0, y0) = (x0 as i64, y0 as i64);
            let mut acc = [0.0f32; 4];
            for (dx, dy, wgt) in [
                (0, 0, (1.0 - tx) * (1.0 - ty)),
                (1, 0, tx * (1.0 - ty)),
                (0, 1, (1.0 - tx) * ty),
                (1, 1, tx * ty),
            ] {
                let (sx, sy) = (x0 + dx, y0 + dy);
                if sx < 0 || sy < 0 || sx >= wi || sy >= hi {
                    continue;
                }
                #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
                let i = ((sy * wi + sx) * 4) as usize;
                for k in 0..4 {
                    acc[k] += f32::from(src[i + k]) * wgt;
                }
            }
            let o = ((y * w + x) * 4) as usize;
            for k in 0..4 {
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                {
                    out[o + k] = acc[k].round().clamp(0.0, 255.0) as u8;
                }
            }
        }
    }
    Image::from_rgba8(out, w, h)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A frame with a bright textured blob at `c`.
    fn blob(c: Offset) -> Image {
        let (w, h) = (80u32, 60u32);
        let mut px = Vec::new();
        for y in 0..h {
            for x in 0..w {
                #[allow(clippy::cast_precision_loss)]
                let (dx, dy) = (x as f32 - c.dx, y as f32 - c.dy);
                let d2 = dx * dx + dy * dy;
                let v = (255.0 * (-d2 / 18.0).exp()
                    + 40.0 * ((dx * 0.9).sin() * (dy * 0.7).cos()).max(0.0) * (-d2 / 60.0).exp())
                .min(255.0);
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let b = v as u8;
                px.extend_from_slice(&[b, b, b, 255]);
            }
        }
        Image::from_rgba8(px, w, h)
    }

    #[test]
    fn tracks_a_moving_feature_with_subpixel_accuracy() {
        let path: Vec<Offset> = (0..10)
            .map(|i| Offset::new(20.0 + i as f32 * 3.3, 25.0 + i as f32 * 1.7))
            .collect();
        let mut t = PointTracker::new(&blob(path[0]), path[0], 6, 6);
        for p in &path[1..] {
            let got = t.track(&blob(*p));
            assert!((got - *p).distance() < 0.5, "{got:?} vs {p:?}");
        }
        assert!(t.confidence > 0.9);
    }

    #[test]
    fn one_track_stabilises_translation() {
        let track = vec![Offset::new(10.0, 10.0), Offset::new(13.0, 8.0)];
        let t = stabilize(&[track]);
        assert_eq!(t[1].apply(Offset::new(13.0, 8.0)), Offset::new(10.0, 10.0));
    }

    #[test]
    fn two_tracks_undo_rotation_and_scale() {
        let a = vec![Offset::new(0.0, 0.0), Offset::new(5.0, 5.0)];
        let b = vec![Offset::new(10.0, 0.0), Offset::new(5.0, 25.0)];
        let t = stabilize(&[a.clone(), b.clone()]);
        let pa = t[1].apply(a[1]);
        let pb = t[1].apply(b[1]);
        assert!(
            (pa - a[0]).distance() < 1e-3 && (pb - b[0]).distance() < 1e-3,
            "{pa:?} {pb:?}"
        );
    }

    #[test]
    fn apply_transform_moves_pixels() {
        let img = blob(Offset::new(30.0, 30.0));
        let moved = apply_transform(&img, Transform::translate(Offset::new(10.0, 0.0)));
        let v = |i: &Image, x: u32, y: u32| i.pixels()[((y * i.width() + x) * 4) as usize];
        assert_eq!(v(&moved, 40, 30), v(&img, 30, 30));
        assert_eq!(moved.pixels()[3], 0, "revealed edge is transparent");
    }
}
