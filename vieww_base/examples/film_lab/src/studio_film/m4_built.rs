//! Movement IV · THE PROOF — **built with vieww, already.**
//!
//! Four real applications in this repository's `apps/` are built on the
//! same engine: **3** (temporal 3D capture and its social product),
//! **vavlt** (a consent-first photo vault), **upxcale** (a photo
//! upscaler) and **SnapSearch** (find a photo by describing it). Each one
//! ships an audit sheet — every screen of its story, rendered headless
//! through vieww's native rasteriser by the app's own screenshot suite.
//!
//! This scene shows those sheets, decoded from the PNGs on disk at render
//! time (`apps/<app>/renders/sheet.png`), each card slowly panning down
//! its sheet. The one-line descriptions are the apps' own `what=` lines
//! from their `metrics.txt`. If a sheet is missing the card says so; the
//! film never stands in a picture of its own for an app's.

use std::sync::OnceLock;

use vieww_foundation::{Alignment, BoxFit, Color, Offset, Rect, Shadow, Size, Sketchbook, TextAlign};
use vieww_widget::prelude::*;
use vieww_widget::Clip;

use crate::film_lib::{clamp01, ease_in_out, ease_out_expo};
use crate::product_film as pf;
use super::{ACCENT, BG_DEEP, BRAND_FAR, CANVAS, INK, MUTED, SYN_TYPE};

/// The four apps: folder, display name, where they live. (No platform
/// claims here — the apps' own READMEs carry those, with their caveats.)
const APPS: [(&str, &str, &str); 4] = [
    ("three", "3", "apps/three"),
    ("vavlt", "vavlt", "apps/vavlt"),
    ("upxcale", "upxcale", "apps/upxcale"),
    ("snapsearch", "SnapSearch", "apps/snapsearch"),
];

struct Sheet {
    image: Option<vieww_foundation::Image>,
    what: String,
}

fn sheets() -> &'static [Sheet] {
    static S: OnceLock<Vec<Sheet>> = OnceLock::new();
    S.get_or_init(|| {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../apps");
        APPS.iter()
            .map(|(dir, _, _)| {
                let renders = root.join(dir).join("renders");
                let image = std::fs::read(renders.join("sheet.png"))
                    .ok()
                    .and_then(|b| image::load_from_memory(&b).ok())
                    .map(|img| {
                        let rgba = img.to_rgba8();
                        let (w, h) = rgba.dimensions();
                        vieww_foundation::Image::from_rgba8(rgba.into_raw(), w, h)
                    });
                let what = std::fs::read_to_string(renders.join("metrics.txt"))
                    .unwrap_or_default()
                    .lines()
                    .find_map(|l| l.strip_prefix("what=").map(str::to_string))
                    .unwrap_or_default();
                Sheet { image, what }
            })
            .collect()
    })
}

const CARD_W: f32 = 860.0;
const CARD_H: f32 = 352.0;
const GAP: f32 = 30.0;
const GRID_X: f32 = 85.0;
const GRID_Y: f32 = 262.0;

fn card(i: usize) -> Rect {
    let (c, r) = (i % 2, i / 2);
    let x = GRID_X + c as f32 * (CARD_W + GAP);
    let y = GRID_Y + r as f32 * (CARD_H + GAP);
    Rect::new(x, y, x + CARD_W, y + CARD_H)
}

pub fn built_with_vieww(ctx: &pf::Ctx) -> WidgetNode {
    let sec = ctx.sec;
    let mut stack = Stack::new();

    super::frame::ground(Positioned::fill().child(Painting::sized(
        CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, s: Size| {
            book.rect(Rect::new(0.0, 0.0, s.width, s.height), BG_DEEP);
            pf::vignette(book, s.width, s.height, 0.5);
        }),
    )));

    let list = sheets();
    for (i, (_, name, platforms)) in APPS.iter().enumerate() {
        let r = card(i);
        let p = ease_out_expo(clamp01((sec - 0.5 - i as f32 * 0.35) / 0.8));
        if p <= 0.01 {
            continue;
        }
        let dy = (1.0 - p) * 26.0;
        let r = Rect::new(r.left, r.top + dy, r.right, r.bottom + dy);
        // The card.
        stack = stack.push(Positioned::fill().child(Opacity::new(p).child(Painting::sized(CANVAS, PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
            book.shadow(r, 16.0, Shadow::new(pf::alpha(Color::BLACK, 0.55), Offset::new(0.0, 14.0), 34.0));
            book.rrect(r, 16.0, Color::rgb(16, 15, 21));
            book.stroke_rrect(r, 16.0, pf::alpha(Color::WHITE, 0.08), 1.0);
        })))));
        // The sheet, panning down its own length — slow, so it reads.
        let shot = Rect::new(r.left + 300.0, r.top + 14.0, r.right - 14.0, r.bottom - 14.0);
        let pan = ease_in_out(clamp01((sec - 1.2 - i as f32 * 0.35) / 10.5));
        let node: WidgetNode = match &list[i].image {
            Some(img) => vieww_widget::Image::new(img.clone())
                .fit(BoxFit::Cover)
                .alignment(Alignment::new(0.0, -1.0 + 2.0 * pan))
                .label(format!("{name} — its own screens, rendered by vieww"))
                .into(),
            None => Text::new("sheet not found in this checkout".to_string())
                .style(pf::geist_mono(16.0).color(pf::alpha(MUTED, 0.9)))
                .into(),
        };
        stack = stack.push(
            Positioned::new().left(shot.left).top(shot.top).width(shot.width()).height(shot.height())
                .child(Opacity::new(p).child(Clip::rounded(10.0).child(node))),
        );
        // The words.
        stack = stack.push(super::frame::label(r.left + 28.0, r.top + 26.0, 268.0, 64.0, name.to_string(),
            pf::geist(40.0).bold().color(pf::alpha(INK, 0.97)), TextAlign::Left, p));
        stack = stack.push(super::frame::label(r.left + 28.0, r.top + 96.0, 250.0, 26.0, platforms.to_string(),
            pf::geist_mono(16.0).letter_spacing(0.6).color(pf::alpha(SYN_TYPE, 0.95)), TextAlign::Left, p));
        let what = list[i].what.clone();
        stack = stack.push(
            Positioned::new().left(r.left + 28.0).top(r.top + 140.0).width(250.0).height(170.0).child(
                Opacity::new(p * clamp01((sec - 1.6 - i as f32 * 0.35) / 0.6)).child(
                    Text::new(what).style(pf::geist(19.0).color(pf::alpha(MUTED, 0.95))),
                ),
            ),
        );
    }

    let tag_a = clamp01((sec - 3.4) / 0.6);
    stack = stack.push(super::frame::label(GRID_X, GRID_Y - 52.0, 1750.0, 34.0,
        "apps/ in this repository — their own screens, rendered headless by vieww".to_string(),
        pf::geist_mono(20.0).letter_spacing(0.8).color(pf::alpha(BRAND_FAR, 0.95)), TextAlign::Left, tag_a));

    stack = stack.push(super::frame::caption("Built with vieww. Already.", 1002.0, clamp01((sec - 0.2) / 0.5)));
    stack = stack.push(super::frame::caption("Four real apps in this repository, on the engine you just saw.", 966.0, clamp01((sec - 4.2) / 0.6)));
    let _ = ACCENT;
    stack.into()
}
