//! Stereo rendering — the WebXR / A-Frame row (§1 Embedded 3D), a VR
//! headset's two eyes, and the red–cyan anaglyph print.
//!
//! A [`StereoRig`] turns one [`Camera`] into a left and right eye separated
//! by the interpupillary distance along the camera's right vector. Both eyes
//! look at a point on the **convergence plane** (the zero-parallax distance)
//! shifted with them, so objects at that distance sit on the screen plane,
//! nearer ones pop out, farther ones recede — the parallel-axis rig with
//! shifted targets rather than toe-in rotation, which avoids keystone
//! vertical parallax.
//!
//! [`side_by_side`] packs the eyes for a headset or 3D TV;
//! [`anaglyph`] combines them with Dubois' least-squares red–cyan matrices
//! for glasses.

use vieww_foundation::Image;

use crate::render::Renderer;
use crate::scene::{Camera, Scene};

/// Two eyes from one camera.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StereoRig {
    /// Eye separation in world units (≈ 0.064 m for people).
    pub ipd: f32,
    /// Distance of the zero-parallax plane.
    pub convergence: f32,
}

impl Default for StereoRig {
    fn default() -> Self {
        Self {
            ipd: 0.064,
            convergence: 2.0,
        }
    }
}

impl StereoRig {
    /// `(left, right)` cameras.
    #[must_use]
    pub fn eyes(&self, cam: &Camera) -> (Camera, Camera) {
        let fwd = (cam.target - cam.position).normalize();
        let right = fwd.cross(cam.up).normalize();
        let focus = cam.position + fwd * self.convergence;
        let half = right * (self.ipd * 0.5);
        let eye = |sign: f32| Camera {
            position: cam.position + half * sign,
            target: focus + half * sign,
            ..*cam
        };
        (eye(-1.0), eye(1.0))
    }

    /// Disparity, in pixels of a `width`-wide image, of a point at `depth`
    /// for parallel eyes (`ipd · f / depth`).
    #[must_use]
    pub fn disparity_px(&self, cam: &Camera, width: u32, height: u32, depth: f32) -> f32 {
        let fov = match cam.projection {
            crate::scene::Projection::Perspective { fov_y, .. } => fov_y,
            crate::scene::Projection::Orthographic { .. } => return 0.0,
        };
        #[allow(clippy::cast_precision_loss)]
        let focal_px = height as f32 / (2.0 * (fov * 0.5).tan());
        let _ = width;
        self.ipd * focal_px / depth.max(1e-6)
    }

    /// Render both eyes, then apply the horizontal image translation that
    /// puts the convergence plane at zero parallax.
    pub fn render(&self, r: &Renderer, scene: &mut Scene, cam: &Camera) -> (Image, Image) {
        let (l, rr) = self.eyes(cam);
        let d = self.disparity_px(cam, r.width, r.height, self.convergence);
        #[allow(clippy::cast_possible_truncation)]
        let half = (d * 0.5).round() as i64;
        (
            shift(&r.render(scene, &l).0, -half),
            shift(&r.render(scene, &rr).0, half),
        )
    }
}

/// Translate an image horizontally by `dx` pixels (vacated columns black).
#[must_use]
pub fn shift(img: &Image, dx: i64) -> Image {
    let (w, h) = (img.width() as usize, img.height() as usize);
    let mut px = vec![0u8; w * h * 4];
    for y in 0..h {
        for x in 0..w {
            #[allow(clippy::cast_possible_wrap)]
            let sx = x as i64 - dx;
            if sx < 0 || sx >= w as i64 {
                continue;
            }
            #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
            let s = (y * w + sx as usize) * 4;
            let d = (y * w + x) * 4;
            px[d..d + 4].copy_from_slice(&img.pixels()[s..s + 4]);
        }
    }
    #[allow(clippy::cast_possible_truncation)]
    Image::from_rgba8(px, w as u32, h as u32)
}

/// Left eye on the left half, right on the right, each squeezed to half width
/// (the "half side-by-side" format).
#[must_use]
pub fn side_by_side(left: &Image, right: &Image) -> Image {
    let (w, h) = (left.width() as usize, left.height() as usize);
    let half = (w / 2).max(1);
    let mut px = vec![0u8; w * h * 4];
    for y in 0..h {
        for x in 0..w {
            let (src, sx) = if x < half {
                (left, x * 2)
            } else {
                (right, (x - half) * 2)
            };
            let sx = sx.min(w - 1);
            let s = (y * w + sx) * 4;
            let d = (y * w + x) * 4;
            px[d..d + 4].copy_from_slice(&src.pixels()[s..s + 4]);
        }
    }
    #[allow(clippy::cast_possible_truncation)]
    Image::from_rgba8(px, w as u32, h as u32)
}

/// Red–cyan anaglyph with Dubois' matrices (applied in sRGB space, as the
/// published coefficients are).
#[must_use]
pub fn anaglyph(left: &Image, right: &Image) -> Image {
    const L: [[f32; 3]; 3] = [
        [0.456, 0.500, 0.176],
        [-0.040, -0.038, -0.016],
        [-0.015, -0.021, -0.005],
    ];
    const R: [[f32; 3]; 3] = [
        [-0.043, -0.088, -0.002],
        [0.378, 0.734, -0.018],
        [-0.072, -0.113, 1.226],
    ];
    let (w, h) = (left.width(), left.height());
    let (lp, rp) = (left.pixels(), right.pixels());
    let mut px = Vec::with_capacity(lp.len());
    for (a, b) in lp.as_chunks::<4>().0.iter().zip(rp.as_chunks::<4>().0) {
        let la = [a[0], a[1], a[2]].map(|v| f32::from(v) / 255.0);
        let ra = [b[0], b[1], b[2]].map(|v| f32::from(v) / 255.0);
        for k in 0..3 {
            let v = (0..3).map(|j| L[k][j] * la[j] + R[k][j] * ra[j]).sum::<f32>();
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            px.push((v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8);
        }
        px.push(255);
    }
    Image::from_rgba8(px, w, h)
}

/// Screen parallax (in units of the convergence plane width) of a point at
/// `depth` — positive behind the screen, negative in front, zero at convergence.
#[must_use]
pub fn parallax(rig: &StereoRig, depth: f32) -> f32 {
    rig.ipd * (1.0 - rig.convergence / depth.max(1e-6))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::box_mesh;
    use crate::math::Vec3;
    use crate::scene::{Content, Light, Material, Node, Rgb};
    use vieww_foundation::Color;

    fn cam() -> Camera {
        Camera::perspective(Vec3::new(0.0, 0.0, 4.0), Vec3::ZERO, 0.8)
    }

    #[test]
    fn eyes_are_ipd_apart_and_parallel() {
        let rig = StereoRig {
            ipd: 0.2,
            convergence: 4.0,
        };
        let (l, r) = rig.eyes(&cam());
        assert!(((r.position - l.position).length() - 0.2).abs() < 1e-5);
        assert!(l.position.x < r.position.x);
        let fl = (l.target - l.position).normalize();
        let fr = (r.target - r.position).normalize();
        assert!((fl - fr).length() < 1e-5, "parallel axes, no toe-in");
    }

    #[test]
    fn parallax_is_zero_at_convergence() {
        let rig = StereoRig::default();
        assert!(parallax(&rig, rig.convergence).abs() < 1e-7);
        assert!(parallax(&rig, 10.0) > 0.0);
        assert!(parallax(&rig, 1.0) < 0.0);
    }

    #[test]
    fn the_two_eyes_see_different_images_and_compose() {
        let mut s = Scene::new();
        s.background = Rgb::BLACK;
        s.add(
            Node::new(
                "amb",
                Content::Light(Light::Ambient {
                    color: Rgb::WHITE,
                    intensity: 1.0,
                }),
            ),
            None,
        );
        s.add(
            Node::new("b", Content::mesh(box_mesh(1.0, 1.0, 1.0), Material::basic(Color::WHITE))),
            None,
        );
        let rig = StereoRig {
            ipd: 0.4,
            convergence: 4.0,
        };
        let (l, r) = rig.render(&Renderer::new(64, 48).samples(1), &mut s, &cam());
        assert_ne!(l.pixels(), r.pixels());
        let sbs = side_by_side(&l, &r);
        assert_eq!((sbs.width(), sbs.height()), (64, 48));
        let ana = anaglyph(&l, &r);
        assert_eq!(ana.pixels().len(), l.pixels().len());
    }
}
