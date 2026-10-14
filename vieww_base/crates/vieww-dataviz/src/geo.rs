//! `d3-geo`: map projections, graticules and great circles.
//!
//! A [`Projection`] maps `(longitude, latitude)` in degrees to the plane and
//! back ([`Projection::invert`]), with D3's `scale` and `translate`
//! semantics. Implemented: equirectangular, Mercator, orthographic (with a
//! rotation and back-hemisphere clipping), stereographic, azimuthal
//! equal-area, and Albers equal-area conic (with standard parallels).
//! [`geo_path`] turns rings of coordinates into a [`Path`], breaking lines
//! that cross clipped regions or jump the antimeridian; [`graticule`] gives
//! the meridian/parallel grid; [`distance`] and [`interpolate`] are the
//! haversine and great-circle (slerp) functions.

use vieww_foundation::{Offset, Path};

const R: f64 = std::f64::consts::PI / 180.0;

/// A projection kind.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Kind {
    Equirectangular,
    Mercator,
    /// Centre of view `(λ₀, φ₀)` in degrees.
    Orthographic {
        center: (f64, f64),
    },
    Stereographic {
        center: (f64, f64),
    },
    AzimuthalEqualArea {
        center: (f64, f64),
    },
    /// Standard parallels φ₁, φ₂ and origin (λ₀, φ₀), degrees.
    Albers {
        parallels: (f64, f64),
        origin: (f64, f64),
    },
}

/// A configured projection.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Projection {
    pub kind: Kind,
    pub scale: f64,
    pub translate: (f64, f64),
}

fn rotate(lon: f64, lat: f64, center: (f64, f64)) -> (f64, f64) {
    // Rotate so `center` sits at (0, 0): λ by −λ₀, then φ by −φ₀ about y.
    let (l, p) = ((lon - center.0) * R, lat * R);
    let (x, y, z) = (p.cos() * l.cos(), p.cos() * l.sin(), p.sin());
    let p0 = center.1 * R;
    let (xr, zr) = (x * p0.cos() + z * p0.sin(), -x * p0.sin() + z * p0.cos());
    (y.atan2(xr), zr.clamp(-1.0, 1.0).asin())
}

fn unrotate(l: f64, p: f64, center: (f64, f64)) -> (f64, f64) {
    let (xr, y, zr) = (p.cos() * l.cos(), p.cos() * l.sin(), p.sin());
    let p0 = center.1 * R;
    let (x, z) = (xr * p0.cos() - zr * p0.sin(), xr * p0.sin() + zr * p0.cos());
    let lon = y.atan2(x) / R + center.0;
    (
        ((lon + 180.0).rem_euclid(360.0)) - 180.0,
        z.clamp(-1.0, 1.0).asin() / R,
    )
}

impl Projection {
    #[must_use]
    pub const fn new(kind: Kind, scale: f64, translate: (f64, f64)) -> Self {
        Self {
            kind,
            scale,
            translate,
        }
    }

    /// `(lon, lat)` → pixels, or `None` where the projection clips the point
    /// (the far side of an orthographic globe, Mercator's poles).
    #[must_use]
    pub fn project(&self, lon: f64, lat: f64) -> Option<(f64, f64)> {
        let (x, y) = match self.kind {
            Kind::Equirectangular => (lon * R, lat * R),
            Kind::Mercator => {
                if lat.abs() > 89.5 {
                    return None;
                }
                (
                    lon * R,
                    ((std::f64::consts::FRAC_PI_4 + lat * R * 0.5).tan()).ln(),
                )
            }
            Kind::Orthographic { center } => {
                let (l, p) = rotate(lon, lat, center);
                if p.cos() * l.cos() < 0.0 {
                    return None;
                }
                (p.cos() * l.sin(), p.sin())
            }
            Kind::Stereographic { center } => {
                let (l, p) = rotate(lon, lat, center);
                let k = 1.0 + p.cos() * l.cos();
                if k < 1e-6 {
                    return None;
                }
                (p.cos() * l.sin() / k, p.sin() / k)
            }
            Kind::AzimuthalEqualArea { center } => {
                let (l, p) = rotate(lon, lat, center);
                let k = 1.0 + p.cos() * l.cos();
                if k < 1e-9 {
                    return None;
                }
                let s = (2.0 / k).sqrt();
                (s * p.cos() * l.sin(), s * p.sin())
            }
            Kind::Albers { parallels, origin } => {
                let (p1, p2) = (parallels.0 * R, parallels.1 * R);
                let n = (p1.sin() + p2.sin()) * 0.5;
                let c = p1.cos().powi(2) + 2.0 * n * p1.sin();
                let rho0 = (c - 2.0 * n * (origin.1 * R).sin()).sqrt() / n;
                let rho = (c - 2.0 * n * (lat * R).sin()).max(0.0).sqrt() / n;
                let th = n * (lon - origin.0) * R;
                (rho * th.sin(), rho0 - rho * th.cos())
            }
        };
        Some((
            self.translate.0 + x * self.scale,
            self.translate.1 - y * self.scale,
        ))
    }

    /// Pixels → `(lon, lat)`.
    #[must_use]
    pub fn invert(&self, px: f64, py: f64) -> Option<(f64, f64)> {
        let (x, y) = (
            (px - self.translate.0) / self.scale,
            (self.translate.1 - py) / self.scale,
        );
        match self.kind {
            Kind::Equirectangular => Some((x / R, y / R)),
            Kind::Mercator => Some((
                x / R,
                (2.0 * y.exp().atan() - std::f64::consts::FRAC_PI_2) / R,
            )),
            Kind::Orthographic { center } => {
                let rho = (x * x + y * y).sqrt();
                if rho > 1.0 {
                    return None;
                }
                let c = rho.asin();
                let (p, l) = if rho < 1e-12 {
                    (0.0, 0.0)
                } else {
                    (
                        (y * c.sin() / rho).asin(),
                        (x * c.sin()).atan2(rho * c.cos()),
                    )
                };
                Some(unrotate(l, p, center))
            }
            Kind::Stereographic { center } => {
                let rho = (x * x + y * y).sqrt();
                let c = 2.0 * rho.atan();
                let (p, l) = if rho < 1e-12 {
                    (0.0, 0.0)
                } else {
                    (
                        (y * c.sin() / rho).asin(),
                        (x * c.sin()).atan2(rho * c.cos()),
                    )
                };
                Some(unrotate(l, p, center))
            }
            Kind::AzimuthalEqualArea { center } => {
                let rho = (x * x + y * y).sqrt();
                if rho > 2.0 {
                    return None;
                }
                let c = 2.0 * (rho * 0.5).asin();
                let (p, l) = if rho < 1e-12 {
                    (0.0, 0.0)
                } else {
                    (
                        (y * c.sin() / rho).asin(),
                        (x * c.sin()).atan2(rho * c.cos()),
                    )
                };
                Some(unrotate(l, p, center))
            }
            Kind::Albers { parallels, origin } => {
                let (p1, p2) = (parallels.0 * R, parallels.1 * R);
                let n = (p1.sin() + p2.sin()) * 0.5;
                let c = p1.cos().powi(2) + 2.0 * n * p1.sin();
                let rho0 = (c - 2.0 * n * (origin.1 * R).sin()).sqrt() / n;
                let yy = rho0 - y;
                let rho = (x * x + yy * yy).sqrt() * n.signum();
                let th = (x * n.signum()).atan2(yy * n.signum());
                let lat = ((c - (rho * n).powi(2)) / (2.0 * n))
                    .clamp(-1.0, 1.0)
                    .asin();
                Some((origin.0 + th / n / R, lat / R))
            }
        }
    }
}

/// Great-circle distance in radians (haversine).
#[must_use]
pub fn distance(a: (f64, f64), b: (f64, f64)) -> f64 {
    let (l1, p1, l2, p2) = (a.0 * R, a.1 * R, b.0 * R, b.1 * R);
    let h = ((p2 - p1) * 0.5).sin().powi(2) + p1.cos() * p2.cos() * ((l2 - l1) * 0.5).sin().powi(2);
    2.0 * h.sqrt().min(1.0).asin()
}

/// The point `t` of the way along the great circle from `a` to `b`.
#[must_use]
pub fn interpolate(a: (f64, f64), b: (f64, f64), t: f64) -> (f64, f64) {
    let d = distance(a, b);
    if d < 1e-12 {
        return a;
    }
    let (l1, p1, l2, p2) = (a.0 * R, a.1 * R, b.0 * R, b.1 * R);
    let (ka, kb) = (((1.0 - t) * d).sin() / d.sin(), (t * d).sin() / d.sin());
    let x = ka * p1.cos() * l1.cos() + kb * p2.cos() * l2.cos();
    let y = ka * p1.cos() * l1.sin() + kb * p2.cos() * l2.sin();
    let z = ka * p1.sin() + kb * p2.sin();
    (y.atan2(x) / R, z.atan2((x * x + y * y).sqrt()) / R)
}

/// Lines of longitude every `step` degrees and latitude every `step`
/// degrees (±80° for parallels), densely sampled.
#[must_use]
pub fn graticule(step: f64) -> Vec<Vec<(f64, f64)>> {
    let mut out = Vec::new();
    let mut lon = -180.0;
    while lon <= 180.0 {
        out.push((0..=180).map(|i| (lon, -90.0 + f64::from(i))).collect());
        lon += step;
    }
    let mut lat = -80.0;
    while lat <= 80.0 {
        out.push((0..=360).map(|i| (-180.0 + f64::from(i), lat)).collect());
        lat += step;
    }
    out
}

/// Project lines of `(lon, lat)` into a path, breaking where a point is
/// clipped or consecutive points jump more than `max_jump` pixels.
#[must_use]
pub fn geo_path(proj: &Projection, lines: &[Vec<(f64, f64)>], closed: bool, max_jump: f64) -> Path {
    let mut path = Path::new();
    for line in lines {
        let mut prev: Option<(f64, f64)> = None;
        let mut started = 0;
        for &(lon, lat) in line {
            match proj.project(lon, lat) {
                None => prev = None,
                Some(p) => {
                    #[allow(clippy::cast_possible_truncation)]
                    let o = Offset::new(p.0 as f32, p.1 as f32);
                    match prev {
                        Some(q)
                            if ((p.0 - q.0).powi(2) + (p.1 - q.1).powi(2)).sqrt() <= max_jump =>
                        {
                            path.line_to(o);
                        }
                        _ => {
                            path.move_to(o);
                            started += 1;
                        }
                    }
                    prev = Some(p);
                }
            }
        }
        if closed && started == 1 && prev.is_some() {
            path.close();
        }
    }
    path
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds() -> Vec<Kind> {
        vec![
            Kind::Equirectangular,
            Kind::Mercator,
            Kind::Orthographic {
                center: (20.0, 30.0),
            },
            Kind::Stereographic {
                center: (-40.0, 10.0),
            },
            Kind::AzimuthalEqualArea {
                center: (100.0, -20.0),
            },
            Kind::Albers {
                parallels: (29.5, 45.5),
                origin: (-96.0, 37.5),
            },
        ]
    }

    #[test]
    fn every_projection_round_trips() {
        for k in kinds() {
            let p = Projection::new(k, 150.0, (480.0, 250.0));
            for &(lon, lat) in &[(0.0, 0.0), (25.0, 35.0), (-70.0, 10.0), (12.5, 55.0)] {
                let Some((x, y)) = p.project(lon, lat) else {
                    continue;
                };
                let (l2, p2) = p.invert(x, y).unwrap();
                assert!(
                    (l2 - lon).abs() < 1e-6 && (p2 - lat).abs() < 1e-6,
                    "{k:?} {lon},{lat} → {l2},{p2}"
                );
            }
        }
    }

    #[test]
    fn orthographic_hides_the_far_side() {
        let p = Projection::new(Kind::Orthographic { center: (0.0, 0.0) }, 100.0, (0.0, 0.0));
        assert!(p.project(0.0, 0.0).is_some());
        assert!(p.project(180.0, 0.0).is_none());
        let (x, _) = p.project(90.0, 0.0).unwrap();
        assert!((x - 100.0).abs() < 1e-9, "the limb is at the scale radius");
    }

    #[test]
    fn mercator_is_conformal_at_the_equator_and_stretches_north() {
        let p = Projection::new(Kind::Mercator, 1.0 / R, (0.0, 0.0));
        let (_, y60) = p.project(0.0, 60.0).unwrap();
        let (_, y30) = p.project(0.0, 30.0).unwrap();
        assert!(-y60 > 2.0 * -y30);
    }

    #[test]
    fn albers_preserves_area_ratios() {
        // Two cells of equal area on the sphere (same Δλ, sin φ bands equal).
        let p = Projection::new(
            Kind::Albers {
                parallels: (20.0, 50.0),
                origin: (0.0, 35.0),
            },
            1000.0,
            (0.0, 0.0),
        );
        let area = |lat0: f64, lat1: f64| {
            let c: Vec<(f64, f64)> = [(0.0, lat0), (10.0, lat0), (10.0, lat1), (0.0, lat1)]
                .iter()
                .map(|&(a, b)| p.project(a, b).unwrap())
                .collect();
            let mut s = 0.0;
            for i in 0..4 {
                s += c[i].0 * c[(i + 1) % 4].1 - c[(i + 1) % 4].0 * c[i].1;
            }
            (s * 0.5).abs()
        };
        // sin(30)−sin(20) vs a band with the same sin difference near 50°.
        let d = (30f64 * R).sin() - (20f64 * R).sin();
        let lat1 = ((50f64 * R).sin() + d).asin() / R;
        let (a, b) = (area(20.0, 30.0), area(50.0, lat1));
        assert!((a / b - 1.0).abs() < 0.02, "{a} vs {b}");
    }

    #[test]
    fn great_circles() {
        let d = distance((0.0, 0.0), (90.0, 0.0));
        assert!((d - std::f64::consts::FRAC_PI_2).abs() < 1e-12);
        let m = interpolate((0.0, 0.0), (90.0, 0.0), 0.5);
        assert!((m.0 - 45.0).abs() < 1e-9 && m.1.abs() < 1e-9);
        // London→NYC midpoint lies north of both (the great circle bows to the pole).
        let mid = interpolate((-0.13, 51.5), (-74.0, 40.7), 0.5);
        assert!(mid.1 > 51.5);
    }

    #[test]
    fn graticule_paths_break_at_the_antimeridian() {
        let p = Projection::new(Kind::Equirectangular, 100.0, (0.0, 0.0));
        let g = graticule(30.0);
        assert!(g.len() > 15);
        let path = geo_path(
            &p,
            &[vec![
                (160.0, 0.0),
                (170.0, 0.0),
                (-170.0, 0.0),
                (-160.0, 0.0),
            ]],
            false,
            50.0,
        );
        assert_eq!(path.open_subpaths(), 2, "the jump is not drawn");
        let globe = Projection::new(Kind::Orthographic { center: (0.0, 0.0) }, 100.0, (0.0, 0.0));
        assert!(!geo_path(&globe, &g, false, 40.0).is_empty());
    }
}
