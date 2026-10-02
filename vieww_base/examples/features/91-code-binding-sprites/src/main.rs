//! Code on screen, two-way bindings and sprite sheets — Motion Canvas'
//! `Code` node / Manim's code transforms, WPF/SwiftUI/Rive data binding,
//! and Unity/Godot/Phaser sprite animation — photographed.
//!
//! 1. `CodeBlock`: highlighted Rust with line numbers and a highlighted
//!    range that walks down the listing.
//! 2. `CodeMorph`: the same function edited — kept lines slide, deleted
//!    lines fade and collapse, inserted lines fade in.
//! 3. Bindings: one `ViewModel` (number, text, bool, enum, trigger, nested
//!    model) driving several views through `Binding`s; a `TextBinding`
//!    receives typed input and rejects the invalid keystroke; a `Command`
//!    enables itself when its predicate holds; the model's JSON is live.
//! 4. Sprites: a generated 8-frame sheet (`SpriteSheet::grid`) played by
//!    two `AnimatedSprite`s (loop and ping-pong) with a footstep event, and
//!    loose frames packed into a new atlas (`SpriteSheet::pack`).

use std::cell::Cell;
use std::rc::Rc;

use feature_harness::draw::{grid, page, panel, DIM, HUES, INK};
use vieww_element::binding::{Binding, Command, TextBinding, Value, ViewModel};
use vieww_element::Runtime;
use vieww_foundation::{Color, EdgeInsets, Image as Pixels, Size};
use vieww_image::sprite::{AnimatedSprite, Clip, PlayMode, SpriteSheet};
use vieww_widget::prelude::*;
use vieww_widget::{CodeBlock, CodeMorph, CodeTheme, Language};

const SPAN: f32 = 6.0;

const BEFORE: &str = r#"fn area(w: f32, h: f32) -> f32 {
    // rectangles only
    let a = w * h;
    println!("area {a}");
    a
}"#;

const AFTER: &str = r#"fn area(shape: &Shape) -> f32 {
    let a = match shape {
        Shape::Rect { w, h } => w * h,
        Shape::Circle { r } => PI * r * r,
    };
    println!("area {a}");
    a
}"#;

fn sheet_image(frames: u32, cell: u32) -> Pixels {
    let (w, h) = (frames * cell, cell);
    let mut px = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        for x in 0..w {
            let f = (x / cell) as f32;
            let (lx, ly) = ((x % cell) as f32, y as f32);
            let phase = f / frames as f32 * std::f32::consts::TAU;
            let (cx, cy) = (
                cell as f32 / 2.0,
                cell as f32 * 0.45 + phase.sin().abs() * -4.0,
            );
            let body =
                ((lx - cx) / 11.0).powi(2) + ((ly - cy) / (13.0 - phase.cos() * 2.0)).powi(2) < 1.0;
            let leg_l = (lx - cx + 5.0 + phase.sin() * 5.0).abs() < 2.5
                && ly > cy + 10.0
                && ly < cell as f32 - 3.0;
            let leg_r = (lx - cx - 5.0 - phase.sin() * 5.0).abs() < 2.5
                && ly > cy + 10.0
                && ly < cell as f32 - 3.0;
            let eye = ((lx - cx - 4.0).powi(2) + (ly - cy + 3.0).powi(2)) < 5.0;
            let c = if eye {
                [20, 20, 30, 255]
            } else if body {
                [120, 200, 140, 255]
            } else if leg_l || leg_r {
                [80, 150, 100, 255]
            } else {
                [0, 0, 0, 0]
            };
            px.extend(c);
        }
    }
    Pixels::from_rgba8(px, w, h)
}

fn scaled(img: &Pixels, k: u32) -> Pixels {
    let (w, h) = (img.width() * k, img.height() * k);
    let src = img.pixels();
    let mut px = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        for x in 0..w {
            let i = (((y / k) * img.width() + x / k) * 4) as usize;
            px.extend_from_slice(&src[i..i + 4]);
        }
    }
    Pixels::from_rgba8(px, w, h)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    feature_harness::launch(
        "91 — code, binding, sprites",
        Size::new(900.0, 590.0),
        |d| {
            let view = feature_harness::clocked(d, SPAN, |t| {
                // 1 — code block with a walking highlight.
                let line = ((t / SPAN) * 6.0) as usize % 6;
                let block = CodeBlock::new(AFTER)
                    .language(Language::Rust)
                    .font_size(12.0)
                    .highlight_lines(line, (line + 1).min(7))
                    .width(400.0);
                let p1 = panel(
                    "CodeBlock",
                    &format!("highlighted Rust · lines {}–{} marked", line + 1, line + 2),
                    block,
                );

                // 2 — morph.
                let m = ((t / SPAN) * 1.6 - 0.2).clamp(0.0, 1.0);
                let morph = CodeMorph::new(BEFORE, AFTER, m)
                    .language(Language::Rust)
                    .font_size(12.0)
                    .theme(CodeTheme::DARK)
                    .width(400.0);
                let p2 = panel("CodeMorph", &format!("LCS line diff · t = {m:.2}"), morph);

                // 3 — bindings, driven by a script of edits.
                let rt = Runtime::new();
                let vm = ViewModel::new(&rt, "Player")
                    .with("health", Value::Number(100.0))
                    .with("name", Value::Text("Ada".into()))
                    .with("shield", Value::Bool(false))
                    .with(
                        "mode",
                        Value::Enum {
                            options: vec!["idle".into(), "walk".into(), "run".into()],
                            selected: 0,
                        },
                    )
                    .with("hit", Value::Trigger(0))
                    .nest(
                        "weapon",
                        ViewModel::new(&rt, "Weapon").with("ammo", Value::Number(12.0)),
                    );
                let health = vm.number("health");
                let ammo = vm.number("weapon.ammo");
                let shield = vm.flag("shield");
                let pct = health.map(
                    |h| format!("{h:.0}%"),
                    |s: String| s.trim_end_matches('%').parse().unwrap_or(0.0),
                );
                let age = rt.signal(30u32);
                let field = TextBinding::new(
                    &rt,
                    Binding::signal(&age),
                    |s| {
                        s.parse::<u32>()
                            .map_err(|_| format!("'{s}' is not a number"))
                    },
                    u32::to_string,
                );
                let fired = Rc::new(Cell::new(0));
                let f2 = fired.clone();
                let ammo2 = ammo.clone();
                let reload =
                    Command::new(move || f2.set(f2.get() + 1)).when(&rt, move || ammo2.get() < 7.0);
                let steps = (t / SPAN * 12.0) as usize;
                let typed = ["3", "35", "41", "4x"];
                for k in 0..steps {
                    health.update(|h| *h -= 7.0);
                    ammo.update(|a| *a -= 1.0);
                    vm.fire("hit");
                    if k == 4 {
                        shield.set(true);
                        vm.set(
                            "mode",
                            Value::Enum {
                                options: vec!["idle".into(), "walk".into(), "run".into()],
                                selected: 2,
                            },
                        );
                    }
                    if let Some(s) = typed.get(k) {
                        field.input(s);
                    }
                    rt.settle_memos();
                    reload.execute();
                }
                if steps >= 6 {
                    pct.set("80%".into()); // writing through the converter updates the model
                }
                rt.settle_memos();
                let hv = health.get();
                let json = vm.to_json().to_string();
                let rows = Flex::column().spacing(6.0).children(children![
                    Text::new(format!("health {} (converter view: {})", hv, pct.get()))
                        .size(12.0)
                        .color(INK),
                    Container::new()
                        .width(320.0)
                        .height(8.0)
                        .color(DIM.with_alpha(60))
                        .child(
                            Container::new()
                                .width(320.0 * (hv / 100.0).clamp(0.0, 1.0))
                                .height(8.0)
                                .color(HUES[2])
                        ),
                    Text::new(format!(
                        "ammo {} · reload enabled {} · reloads {}",
                        ammo.get(),
                        reload.can_execute(),
                        fired.get()
                    ))
                    .size(12.0)
                    .color(INK),
                    Text::new(format!(
                        "shield {} · mode {:?}",
                        shield.get(),
                        vm.get("mode")
                            .map(|v| if let Value::Enum { options, selected } = v {
                                options[selected].clone()
                            } else {
                                String::new()
                            })
                            .unwrap_or_default()
                    ))
                    .size(12.0)
                    .color(INK),
                    Text::new(format!(
                        "age field \"{}\" → model {} {}",
                        field.text(),
                        age.peek(),
                        field
                            .error()
                            .map_or(String::new(), |e| format!("· error: {e}"))
                    ))
                    .size(12.0)
                    .color(if field.error().is_some() {
                        HUES[1]
                    } else {
                        INK
                    }),
                    Text::new(json).size(9.0).color(DIM),
                ]);
                let p3 = panel(
                    "ViewModel + two-way bindings",
                    "lens · converter · validation · command · JSON",
                    Container::new()
                        .width(330.0)
                        .height(210.0)
                        .padding(EdgeInsets::all(4.0))
                        .child(rows),
                );

                // 4 — sprites.
                let mut sheet = SpriteSheet::grid(sheet_image(8, 40), 8, 1);
                sheet.add_clip(
                    "walk",
                    Clip::new(0..8, 10.0).event(2, "step").event(6, "step"),
                );
                sheet.add_clip("bob", Clip::new(0..4, 6.0).mode(PlayMode::PingPong));
                let mut walk = AnimatedSprite::new("walk");
                let mut bob = AnimatedSprite::new("bob");
                let mut steps_heard = 0;
                let mut tt = 0.0;
                while tt < t {
                    steps_heard += walk
                        .advance(&sheet, 1.0 / 60.0)
                        .iter()
                        .filter(|e| *e == "step")
                        .count();
                    bob.advance(&sheet, 1.0 / 60.0);
                    tt += 1.0 / 60.0;
                }
                let wf = walk.frame(&sheet).unwrap_or(0);
                let bf = bob.frame(&sheet).unwrap_or(0);
                let a = sheet.frame_image(wf).expect("frame");
                let b = sheet.frame_image(bf).expect("frame");
                let loose: Vec<Pixels> =
                    (0..5).map(|i| sheet_image(1 + i % 3, 16 + i * 4)).collect();
                let named: Vec<(String, &Pixels)> = loose
                    .iter()
                    .enumerate()
                    .map(|(i, p)| (format!("f{i}"), p))
                    .collect();
                let refs: Vec<(&str, &Pixels)> =
                    named.iter().map(|(n, p)| (n.as_str(), *p)).collect();
                let packed = SpriteSheet::pack(&refs, 128, 64)
                    .map(|s| s.image)
                    .unwrap_or_else(|| sheet_image(1, 16));
                let sprites = Flex::column().spacing(8.0).children(children![
                    Image::new(scaled(&sheet.image, 1)),
                    Flex::row().spacing(16.0).children(children![
                        Image::new(scaled(&a, 3)),
                        Image::new(scaled(&b, 3)),
                        Image::new(scaled(&packed, 1))
                    ]),
                ]);
                let p4 = panel("sprite sheet + animated sprites", &format!("walk f{wf} · bob f{bf} (ping-pong) · {steps_heard} step events · packed atlas"), Container::new().width(340.0).height(210.0).child(sprites));
                let _ = Color::WHITE;
                page("91 · code, bindings, sprites", "vieww-widget (CodeBlock, CodeMorph) · vieww-element (binding) · vieww-image (sprite)", grid(2, vec![p1, p2, p3, p4]))
            });
            feature_harness::set_page(d, view);
        },
    )
}
