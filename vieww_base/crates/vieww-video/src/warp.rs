//! Puppet pins — After Effects' Puppet tool, Photoshop's Puppet Warp,
//! Rive/Spine mesh deformation driven by handles.
//!
//! **Moving least squares**, rigid variant (Schaefer, McPhail & Warren,
//! 2006): every point `v` gets its own best rigid transform of the pins,
//! weighted by `1 / |pᵢ − v|^{2α}`, so the image bends smoothly between
//! handles while staying locally as rigid as possible — no shearing, no
//! scaling of local detail.
//!
//! Images are warped by **inverse** mapping: the deformation from the
//! *deformed* pins back to the *rest* pins is evaluated at each output
//! pixel and the source is sampled there (bilinear), on a coarse grid with
//! per-cell bilinear interpolation of the mapping for speed (`grid` px).

use vieww_foundation::{Image, Offset};

/// The rigid MLS deformation of `v` taking `from` pins to `to` pins.
#[must_use]
pub fn mls_rigid(v: Offset, from: &[Offset], to: &[Offset], alpha: f32) -> Offset {
    let n = from.len().min(to.len());
    if n == 0 {
        return v;
    }
    let mut w = Vec::with_capacity(n);
    for p in &from[..n] {
        let d2 = (p.dx - v.dx).powi(2) + (p.dy - v.dy).powi(2);
        if d2 < 1e-10 {
            // On a pin: exactly where the pin went.
            let i = w.len();
            return to[i];
        }
        w.push(1.0 / d2.powf(alpha));
    }
    let ws: f32 = w.iter().sum();
    let (mut ps, mut qs) = ((0.0f32, 0.0f32), (0.0f32, 0.0f32));
    for i in 0..n {
        ps.0 += w[i] * from[i].dx;
        ps.1 += w[i] * from[i].dy;
        qs.0 += w[i] * to[i].dx;
        qs.1 += w[i] * to[i].dy;
    }
    let (ps, qs) = ((ps.0 / ws, ps.1 / ws), (qs.0 / ws, qs.1 / ws));
    let vp = (v.dx - ps.0, v.dy - ps.1);
    // f_r(v) = Σ q̂ᵢ Aᵢ with Aᵢ = wᵢ [p̂ᵢ; −p̂ᵢ⊥] [v−p*; −(v−p*)⊥]ᵀ
    let (mut fx, mut fy) = (0.0f32, 0.0f32);
    for i in 0..n {
        let ph = (from[i].dx - ps.0, from[i].dy - ps.1);
        let qh = (to[i].dx - qs.0, to[i].dy - qs.1);
        // a = p̂·(v−p*), b = p̂⊥·(v−p*) with p̂⊥ = (−p̂y, p̂x)
        let a = ph.0 * vp.0 + ph.1 * vp.1;
        let b = -ph.1 * vp.0 + ph.0 * vp.1;
        fx += w[i] * (qh.0 * a - qh.1 * b);
        fy += w[i] * (qh.0 * b + qh.1 * a);
    }
    let len_v = (vp.0 * vp.0 + vp.1 * vp.1).sqrt();
    let len_f = (fx * fx + fy * fy).sqrt();
    if len_f < 1e-12 {
        return Offset::new(qs.0, qs.1);
    }
    let s = len_v / len_f;
    Offset::new(fx * s + qs.0, fy * s + qs.1)
}

/// Warp `img` so the points at `rest` move to `moved`.
#[must_use]
pub fn puppet_warp(img: &Image, rest: &[Offset], moved: &[Offset], grid: usize) -> Image {
    let (w, h) = (img.width() as usize, img.height() as usize);
    let g = grid.max(1);
    let (gw, gh) = (w.div_ceil(g) + 1, h.div_ceil(g) + 1);
    // Inverse map at grid nodes: output position → source position.
    let mut map = vec![Offset::ZERO; gw * gh];
    for gy in 0..gh {
        for gx in 0..gw {
            #[allow(clippy::cast_precision_loss)]
            let v = Offset::new((gx * g) as f32, (gy * g) as f32);
            map[gy * gw + gx] = mls_rigid(v, moved, rest, 1.0);
        }
    }
    let src = img.pixels();
    let mut out = vec![0u8; w * h * 4];
    for y in 0..h {
        for x in 0..w {
            let (cx, cy) = (x / g, y / g);
            #[allow(clippy::cast_precision_loss)]
            let (fx, fy) = ((x % g) as f32 / g as f32, (y % g) as f32 / g as f32);
            let m = |a: usize, b: usize| map[(cy + b).min(gh - 1) * gw + (cx + a).min(gw - 1)];
            let lerp = |a: Offset, b: Offset, t: f32| Offset::new(a.dx + (b.dx - a.dx) * t, a.dy + (b.dy - a.dy) * t);
            let s = lerp(lerp(m(0, 0), m(1, 0), fx), lerp(m(0, 1), m(1, 1), fx), fy);
            #[allow(clippy::cast_precision_loss)]
            if s.dx < 0.0 || s.dy < 0.0 || s.dx > (w - 1) as f32 || s.dy > (h - 1) as f32 {
                continue;
            }
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let (x0, y0) = (s.dx as usize, s.dy as usize);
            let (x1, y1) = ((x0 + 1).min(w - 1), (y0 + 1).min(h - 1));
            #[allow(clippy::cast_precision_loss)]
            let (ax, ay) = (s.dx - x0 as f32, s.dy - y0 as f32);
            for c in 0..4 {
                let p = |xx: usize, yy: usize| f32::from(src[(yy * w + xx) * 4 + c]);
                let v = (p(x0, y0) * (1.0 - ax) + p(x1, y0) * ax) * (1.0 - ay)
                    + (p(x0, y1) * (1.0 - ax) + p(x1, y1) * ax) * ay;
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                {
                    out[(y * w + x) * 4 + c] = (v + 0.5) as u8;
                }
            }
        }
    }
    #[allow(clippy::cast_possible_truncation)]
    Image::from_rgba8(out, w as u32, h as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn o(x: f32, y: f32) -> Offset {
        Offset::new(x, y)
    }

    #[test]
    fn pins_land_exactly_and_identity_is_identity() {
        let p = [o(0.0, 0.0), o(10.0, 0.0), o(5.0, 8.0)];
        let q = [o(1.0, 1.0), o(12.0, -2.0), o(4.0, 9.0)];
        for i in 0..3 {
            assert_eq!(mls_rigid(p[i], &p, &q, 1.0), q[i]);
        }
        let v = mls_rigid(o(3.3, 2.2), &p, &p, 1.0);
        assert!((v.dx - 3.3).abs() < 1e-4 && (v.dy - 2.2).abs() < 1e-4);
    }

    #[test]
    fn a_global_rigid_motion_is_reproduced_everywhere() {
        let (s, c) = (0.5f32.sin(), 0.5f32.cos());
        let rot = |p: Offset| o(c * p.dx - s * p.dy + 7.0, s * p.dx + c * p.dy - 3.0);
        let p = [o(0.0, 0.0), o(20.0, 0.0), o(20.0, 20.0), o(0.0, 20.0)];
        let q = p.map(rot);
        for v in [o(5.0, 5.0), o(13.0, 2.0), o(-4.0, 30.0)] {
            let r = mls_rigid(v, &p, &q, 1.0);
            let t = rot(v);
            assert!((r.dx - t.dx).abs() < 1e-3 && (r.dy - t.dy).abs() < 1e-3);
        }
    }

    #[test]
    fn dragging_one_pin_bends_locally() {
        let p = [o(0.0, 10.0), o(50.0, 10.0), o(100.0, 10.0)];
        let mut q = p;
        q[2] = o(100.0, 40.0);
        let near_fixed = mls_rigid(o(5.0, 10.0), &p, &q, 1.0);
        let near_moved = mls_rigid(o(95.0, 10.0), &p, &q, 1.0);
        assert!((near_fixed.dy - 10.0).abs() < 2.0);
        assert!(near_moved.dy > 30.0);
    }

    #[test]
    fn image_warp_moves_content() {
        let (w, h) = (40usize, 20usize);
        let px: Vec<u8> = (0..w * h).flat_map(|i| if i % w < 5 { [255, 0, 0, 255] } else { [0, 0, 255, 255] }).collect();
        #[allow(clippy::cast_possible_truncation)]
        let img = Image::from_rgba8(px, w as u32, h as u32);
        let rest = [o(2.0, 10.0), o(20.0, 10.0), o(38.0, 10.0)];
        let shift = rest.map(|p| o(p.dx + 6.0, p.dy));
        let out = puppet_warp(&img, &rest, &shift, 4);
        let p = |x: usize| out.pixels()[(10 * w + x) * 4];
        assert_eq!(p(8), 255, "the red strip moved right");
        assert_eq!(p(20), 0);
    }
}
