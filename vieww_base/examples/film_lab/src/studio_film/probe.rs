//! probe — a development door into the real studio: mount it, apply a
//! list of actions, and save what it draws. Used to read the product's
//! own geometry (pane rects, device frames, tab positions) so the film's
//! plates quote the product rather than guess at it.
//!
//! `film_lab sfprobe` writes `probe_<k>.png` under the work root, and
//! prints the ink box of the preview pane for each step — the device
//! frame's rectangle, measured.

use std::time::Duration;

use vieww_paint::native::NativeRenderer;
use vieww_render::FrameDriver;
use viewwstudio::command::Command;
use viewwstudio::state::{Platform, RightTab};

use super::script::{apply_one, Sf};
use crate::product_film as pf;
use crate::product_film::script::{self, Action, TAB_LIVE};

pub(crate) fn run() -> Result<(), Box<dyn std::error::Error>> {
    let root = super::master::work_root().join("probe");
    std::fs::create_dir_all(&root)?;
    let mut driver = FrameDriver::new(pf::CANVAS);
    driver.set_fonts(pf::fonts());
    let studio = script::mount(&mut driver);
    let mut renderer = NativeRenderer::new();
    let bg = viewwstudio::StudioTheme::dark().window;

    let steps: Vec<(Vec<Sf>, &str)> = vec![
        (
            vec![
                Sf::Pf(Action::ActiveTab(TAB_LIVE)),
                Sf::Pf(Action::Run(Command::LivePreview)),
                Sf::Pf(Action::AcceptLive),
            ],
            "ios",
        ),
        (vec![Sf::Pf(Action::Platform(Platform::Android))], "android"),
        (vec![Sf::Pf(Action::Platform(Platform::Desktop))], "desktop"),
        (
            vec![
                Sf::Pf(Action::Platform(Platform::Ios)),
                Sf::PreviewDark(true),
            ],
            "ios_dark",
        ),
        (
            vec![
                Sf::PreviewDark(false),
                Sf::EditLive("title \"Inbox\"", "title \"My own inbox\""),
                Sf::Pf(Action::ShowDamage(true)),
            ],
            "edit_damage",
        ),
        (
            vec![
                Sf::Pf(Action::ShowDamage(false)),
                Sf::Pf(Action::OpenPath("counter.say".into())),
            ],
            "say",
        ),
        (vec![Sf::Tab("live.rs")], "back_live"),
        (
            vec![
                Sf::Pf(Action::RightTab(RightTab::Devices)),
                Sf::Device(Some(3)),
            ],
            "devices_tab",
        ),
        (vec![Sf::Pf(Action::RightTab(RightTab::Preview))], "tablet"),
        (vec![Sf::Device(Some(6))], "laptop"),
        (vec![Sf::Device(Some(0))], "phone_small"),
    ];
    let mut clock = 1.0f64;
    for (k, (actions, name)) in steps.into_iter().enumerate() {
        for a in &actions {
            apply_one(&mut driver, &studio, a, clock as f32);
        }
        for _ in 0..4 {
            driver.set_root(script::shell_root(
                &studio,
                vieww_widget::prelude::Stack::new().into(),
            ));
            driver.draw_frame_at(Duration::from_secs_f64(clock));
            clock += 0.5;
        }
        let pixels = renderer
            .render_to_pixels(driver.scene(), pf::W as u32, pf::H as u32, bg)?
            .0;
        let d = pixels.data();
        // The device's ink inside the preview pane's stage.
        let (mut x0, mut y0, mut x1, mut y1) = (u32::MAX, u32::MAX, 0, 0);
        let ref_px = {
            let o = ((700 * pf::W as u32 + 1460) * 4) as usize;
            [d[o], d[o + 1], d[o + 2]]
        };
        for y in 118..712u32 {
            for x in 1458..1908u32 {
                let o = ((y * pf::W as u32 + x) * 4) as usize;
                let diff = (d[o] as i32 - ref_px[0] as i32).abs()
                    + (d[o + 1] as i32 - ref_px[1] as i32).abs()
                    + (d[o + 2] as i32 - ref_px[2] as i32).abs();
                if diff > 30 {
                    x0 = x0.min(x);
                    y0 = y0.min(y);
                    x1 = x1.max(x);
                    y1 = y1.max(y);
                }
            }
        }
        let img = image::RgbaImage::from_raw(pf::W as u32, pf::H as u32, d.to_vec())
            .ok_or("invalid frame")?;
        img.save(root.join(format!("probe_{k:02}_{name}.png")))?;
        println!("probe {k:02} {name}: preview ink ({x0},{y0})-({x1},{y1})");
    }
    Ok(())
}
