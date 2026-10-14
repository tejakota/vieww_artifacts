//! Compositing, keying, tracking, export and pixel effects — After
//! Effects' composition model, Keylight, the point tracker / Warp
//! Stabilizer, a render queue, and TouchDesigner/Processing-style pixel
//! filters — photographed.
//!
//! Top row:
//! 1. A `Composition`: a generated background, a parented title card with
//!    eased keyframes, a star-shaped track matte cutting a gradient, a
//!    precomp with time remap (it runs backwards), a Screen-blended glow,
//!    and a posterize adjustment layer over the lower layers.
//! 2. A synthetic green-screen plate keyed with `chroma_key` (the subject's
//!    shadow on the screen keys too; spill suppressed) over a checkerboard.
//! 3. A shaky shot: `PointTracker` follows the feature (the red trail is its
//!    sub-pixel track); `stabilize` + `apply_transform` hold it still.
//! 4. The comp rendered through the export sinks — Y4M (read back with
//!    `read_y4m`) and animated GIF — with byte counts and SMPTE timecode.
//!
//! Bottom row: `vieww-effects::cpu::pixel` — pixelate, posterize, Sobel,
//! halftone, kaleidoscope, chromatic aberration, displacement map, and a
//! feedback loop.

use std::f32::consts::TAU;
use std::sync::Arc;

use feature_harness::draw::{grid, page, paint, panel, polyline, DIM, HUES};
use vieww_effects::cpu::pixel;
use vieww_foundation::{Color, Image as Pixels, Offset, Size};
use vieww_video::comp::{Blend, Composition, Layer, Prop, Source};
use vieww_video::export::{read_y4m, FrameSink, GifWriter, Timecode, Y4mWriter};
use vieww_video::matte::{chroma_key, MatteMode};
use vieww_video::track::{apply_transform, stabilize, PointTracker};
use vieww_video::VideoSource;
use vieww_widget::prelude::*;

const W: u32 = 200;
const H: u32 = 140;
const SPAN: f32 = 4.0;

fn gen(w: u32, h: u32, f: impl Fn(f32, f32) -> [u8; 4]) -> Pixels {
    let mut px = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        for x in 0..w {
            px.extend(f(x as f32, y as f32));
        }
    }
    Pixels::from_rgba8(px, w, h)
}

fn star_matte() -> Pixels {
    gen(80, 80, |x, y| {
        let (dx, dy) = (x - 40.0, y - 40.0);
        let a = dy.atan2(dx);
        let r = (dx * dx + dy * dy).sqrt();
        let edge = 22.0 + 14.0 * (a * 5.0).cos();
        [255, 255, 255, if r < edge { 255 } else { 0 }]
    })
}

fn comp() -> Composition {
    let mut c = Composition::new("main", W, H, 25, SPAN).background(Color::rgb(12, 14, 22));
    c.push(Layer::new(
        "bg",
        Source::Generator(Arc::new(|t| {
            gen(W, H, move |x, y| {
                let v = ((x * 0.05 + t * 2.0).sin() * (y * 0.07 - t).cos() * 0.5 + 0.5) * 90.0;
                [30 + v as u8, 40, 90 + v as u8, 255]
            })
        })),
    ));
    // Posterize everything above this line in the stack order below it.
    c.push(Layer::adjustment(
        "posterize",
        vec![Arc::new(|img: &Pixels, _t| {
            let mut p = img.pixels().to_vec();
            pixel::posterize(&mut p, 4);
            Pixels::from_rgba8(p, img.width(), img.height())
        })],
    ));
    // A gradient cut by the star above it (track matte).
    c.push(
        Layer::new(
            "gradient",
            Source::Still(gen(80, 80, |x, y| {
                [(x * 3.0) as u8, 200, (y * 3.0) as u8, 255]
            })),
        )
        .anchor(40.0, 40.0)
        .at(Prop::keys(&[(0.0, 50.0), (2.0, 150.0), (4.0, 50.0)]), 70.0)
        .rotation(Prop::expr(|t| t * 45.0))
        .matte(MatteMode::Alpha),
    );
    c.push(
        Layer::new("star", Source::Still(star_matte()))
            .anchor(40.0, 40.0)
            .at(Prop::keys(&[(0.0, 50.0), (2.0, 150.0), (4.0, 50.0)]), 70.0)
            .rotation(Prop::expr(|t| t * 45.0)),
    );
    // A precomp played backwards via time remap.
    let mut inner = Composition::new("inner", 60, 30, 25, 4.0);
    inner.push(
        Layer::new(
            "bar",
            Source::Solid {
                color: Color::rgb(240, 113, 120),
                width: 12,
                height: 30,
            },
        )
        .at(Prop::Linear(vec![(0.0, 0.0), (4.0, 48.0)]), 0.0),
    );
    c.push(
        Layer::new("reverse", Source::Precomp(Arc::new(inner)))
            .at(130.0, 100.0)
            .remap(Prop::Linear(vec![(0.0, 4.0), (4.0, 0.0)])),
    );
    // A parented title: a null drives it.
    let null = c.push(Layer::new("null", Source::Null).at(
        Prop::keys(&[(0.0, 20.0), (1.0, 100.0), (3.0, 100.0), (4.0, 20.0)]),
        20.0,
    ));
    c.push(
        Layer::new(
            "title",
            Source::Solid {
                color: Color::rgb(229, 192, 123),
                width: 70,
                height: 16,
            },
        )
        .parent(null)
        .opacity(Prop::keys(&[(0.0, 0.0), (0.8, 1.0)])),
    );
    c.push(
        Layer::new(
            "glow",
            Source::Generator(Arc::new(|_| {
                gen(60, 60, |x, y| {
                    [
                        255,
                        220,
                        120,
                        (255.0
                            * (1.0 - (((x - 30.0).powi(2) + (y - 30.0).powi(2)).sqrt() / 30.0))
                                .clamp(0.0, 1.0)) as u8,
                    ]
                })
            })),
        )
        .anchor(30.0, 30.0)
        .at(
            Prop::expr(|t| 100.0 + 70.0 * (t * 1.6).cos()),
            Prop::expr(|t| 70.0 + 40.0 * (t * 1.6).sin()),
        )
        .blend(Blend::Screen),
    );
    c
}

fn green_plate(t: f32) -> Pixels {
    let (cx, cy) = (100.0 + 30.0 * (t * 1.5).sin(), 70.0);
    gen(W, H, move |x, y| {
        let d = ((x - cx).powi(2) + (y - cy).powi(2)).sqrt();
        let shadow = ((x - cx - 18.0).powi(2) / 900.0 + (y - 118.0).powi(2) / 60.0) < 1.0;
        if d < 28.0 {
            let rim = if d > 24.0 { 40.0 } else { 0.0 };
            [
                (220.0 - d) as u8,
                (170.0 + rim) as u8,
                (150.0 - d) as u8,
                255,
            ]
        } else if shadow {
            [20, 95, 30, 255]
        } else {
            [40, 190, 60, 255]
        }
    })
}

fn over_checker(img: &Pixels) -> Pixels {
    let p = img.pixels();
    gen(img.width(), img.height(), |x, y| {
        let i = ((y as u32 * img.width() + x as u32) * 4) as usize;
        let c = if ((x as u32 / 10) + (y as u32 / 10)).is_multiple_of(2) {
            70.0
        } else {
            110.0
        };
        let a = f32::from(p[i + 3]) / 255.0;
        let mix = |v: u8| (f32::from(v) * a + c * (1.0 - a)) as u8;
        [mix(p[i]), mix(p[i + 1]), mix(p[i + 2]), 255]
    })
}

fn shaky(i: usize) -> (Pixels, Offset) {
    let s = i as f32;
    let shake = Offset::new(
        10.0 * (s * 0.31).sin() + 3.0 * (s * 0.77).cos(),
        7.0 * (s * 0.43).cos(),
    );
    let feat = Offset::new(100.0, 70.0) + shake;
    let img = gen(W, H, move |x, y| {
        let (dx, dy) = (x - feat.dx, y - feat.dy);
        let blob = (-(dx * dx + dy * dy) / 40.0).exp();
        let tex = (((x - shake.dx) * 0.2).sin() * ((y - shake.dy) * 0.25).cos() * 0.5 + 0.5) * 60.0;
        let v = (tex + blob * 190.0).min(255.0);
        [v as u8, (v * 0.9) as u8, (v * 0.7 + 30.0) as u8, 255]
    });
    (img, feat)
}

fn source_art() -> Pixels {
    gen(W, H, |x, y| {
        let (u, v) = (x / W as f32, y / H as f32);
        let d = ((u - 0.5).powi(2) + (v - 0.5).powi(2)).sqrt();
        let ring = ((d * 30.0).sin() * 0.5 + 0.5) * 255.0;
        [(u * 255.0) as u8, ring as u8, (v * 255.0) as u8, 255]
    })
}

fn img_panel(title: &str, detail: &str, img: Pixels, overlay: Option<WidgetNode>) -> WidgetNode {
    let body: WidgetNode = match overlay {
        Some(o) => Stack::new()
            .children(children![
                Image::new(img),
                Positioned::new().left(0.0).top(0.0).child(o)
            ])
            .into(),
        None => Image::new(img).into(),
    };
    panel(
        title,
        detail,
        SizedBox::from_size(Size::new(W as f32, H as f32)).child(body),
    )
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Render queue: the whole comp out through two sinks, once.
    let c = comp();
    let mut y4m = Y4mWriter::new(Vec::new(), W, H, (c.fps, 1));
    let mut gif_bytes = Vec::new();
    {
        let mut gif = GifWriter::new(&mut gif_bytes, W, H, c.fps)?;
        for i in (0..c.frame_count()).step_by(2) {
            let f = c.frame_at(i).expect("in range");
            y4m.push(&f)?;
            gif.push(&f)?;
        }
        gif.finish()?;
    }
    y4m.finish()?;
    let y4m_bytes = y4m.into_inner();
    let back = read_y4m(&y4m_bytes).map_err(|e| e.to_string())?;
    let export_note = format!(
        "Y4M {}K · {} fr back · GIF {}K",
        y4m_bytes.len() / 1024,
        back.frame_count(),
        gif_bytes.len() / 1024
    );
    let track: Vec<Offset> = {
        let (f0, p0) = shaky(0);
        let mut tr = PointTracker::new(&f0, p0, 7, 12);
        (0..48)
            .map(|i| if i == 0 { p0 } else { tr.track(&shaky(i).0) })
            .collect()
    };
    let fixes = stabilize(std::slice::from_ref(&track));

    feature_harness::launch(
        "89 — video & effects",
        Size::new(1000.0, 680.0),
        move |d| {
            let c = comp();
            let export_note = export_note.clone();
            let track = track.clone();
            let fixes = fixes.clone();
            let art = source_art();
            let view = feature_harness::clocked(d, SPAN, move |t| {
                let frame = c.render(t);
                let p1 = img_panel(
                    "composition",
                    "matte · remap · parent · screen · adjust",
                    frame,
                    None,
                );
                let keyed = chroma_key(&green_plate(t), Color::rgb(40, 190, 60), 22.0, 12.0, true);
                let p2 = img_panel(
                    "chroma key",
                    "shadow keys too · spill suppressed",
                    over_checker(&keyed),
                    None,
                );
                let i = ((t / SPAN) * 47.0) as usize;
                let (shot, _) = shaky(i);
                let steady = apply_transform(&shot, fixes[i]);
                let upto: Vec<Offset> = track[..=i].to_vec();
                let overlay = paint(Size::new(W as f32, H as f32), move |g, _| {
                    g.stroke(
                        polyline(
                            &upto
                                .iter()
                                .map(|p| *p + Offset::new(0.0, 0.0))
                                .collect::<Vec<_>>(),
                        ),
                        HUES[1],
                        1.5,
                    );
                });
                let p3 = img_panel(
                    "point tracker",
                    &format!(
                        "NCC, sub-pixel · frame {i} · fix {:.1},{:.1}",
                        fixes[i].tx, fixes[i].ty
                    ),
                    shot.clone(),
                    Some(overlay),
                );
                let p3b = img_panel(
                    "stabilised",
                    "stabilize() + apply_transform()",
                    steady,
                    None,
                );
                let tc = Timecode::from_frame((t * c.fps as f32) as u64, c.fps, false);
                let p4 = img_panel(
                    "export",
                    &format!("{export_note} · TC {tc}"),
                    c.render((t + 0.5) % SPAN),
                    None,
                );

                let (w, h) = (W as usize, H as usize);
                let mut pix = art.pixels().to_vec();
                pixel::pixelate(&mut pix, w, h, 4 + (t * 3.0) as usize);
                let e1 = Pixels::from_rgba8(pix, W, H);
                let mut post = art.pixels().to_vec();
                pixel::posterize(&mut post, 3);
                let e2 = Pixels::from_rgba8(pixel::sobel(&post, w, h), W, H);
                let e3 = Pixels::from_rgba8(
                    pixel::halftone(art.pixels(), w, h, 7.0, 0.4 + t * 0.2, [30, 30, 60]),
                    W,
                    H,
                );
                let e4 =
                    Pixels::from_rgba8(pixel::kaleidoscope(art.pixels(), w, h, 6, t * 0.5), W, H);
                let e5 = Pixels::from_rgba8(
                    pixel::chromatic_aberration(
                        art.pixels(),
                        w,
                        h,
                        3.0 + 5.0 * (t * 2.0).sin().abs(),
                    ),
                    W,
                    H,
                );
                let map: Vec<u8> = (0..h)
                    .flat_map(|y| {
                        (0..w).flat_map(move |x| {
                            let v = ((x as f32 * 0.12 + t * 3.0).sin() * 0.5 + 0.5) * 255.0;
                            let u = ((y as f32 * 0.1).cos() * 0.5 + 0.5) * 255.0;
                            [v as u8, u as u8, 0, 255]
                        })
                    })
                    .collect();
                let e6 = Pixels::from_rgba8(pixel::displace(art.pixels(), &map, w, h, 8.0), W, H);
                let mut fb = pixel::Feedback::new(w, h, 0.93, 1.03, 0.04);
                for k in 0..((t * 15.0) as usize + 5) {
                    let a = k as f32 * 0.35;
                    let (cx, cy) = (
                        w as f32 / 2.0 + 40.0 * a.cos(),
                        h as f32 / 2.0 + 30.0 * (a * 1.3).sin(),
                    );
                    let dot = gen(W, H, |x, y| {
                        if (x - cx).powi(2) + (y - cy).powi(2) < 36.0 {
                            [255, (120.0 + 100.0 * a.sin()) as u8, 200, 255]
                        } else {
                            [0, 0, 0, 0]
                        }
                    });
                    fb.step(dot.pixels());
                }
                let e7 = Pixels::from_rgba8(fb.frame().to_vec(), W, H);
                let fx = [
                    ("pixelate", e1),
                    ("posterize → sobel", e2),
                    ("halftone", e3),
                    ("kaleidoscope", e4),
                    ("chromatic aberration", e5),
                    ("displacement map", e6),
                    ("feedback", e7),
                ];
                let mut items = vec![p1, p2, p3, p3b, p4];
                items.extend(
                    fx.into_iter()
                        .map(|(n, im)| img_panel(n, "vieww-effects::cpu::pixel", im, None)),
                );
                let _ = (DIM, TAU);
                page(
                    "89 · compositing, keying, tracking, export, pixel effects",
                    "vieww-video (comp, matte, track, export) · vieww-effects",
                    grid(4, items),
                )
            });
            feature_harness::set_page(d, view);
        },
    )
}
