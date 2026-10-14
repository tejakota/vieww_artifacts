//! A video, as a filmstrip — the gap-closure pass's `vieww-video` crate,
//! photographed.
//!
//! A `GeneratedVideo` (the deterministic test pattern) played by a
//! `VideoPlayer`, sampled at eight instants across its duration: seek,
//! read the frame, place it. The strip is the player's whole state machine
//! — play, seek, frame-accurate end — as one row of thumbnails, and the
//! generated sweep pattern makes the frame index visible *in* each
//! thumbnail.

use std::time::Duration;

use vieww_foundation::{Color, Size};
use vieww_video::{GeneratedVideo, Pattern, VideoPlayer, VideoSource};
use vieww_widget::prelude::*;

const THUMBS: usize = 8;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    feature_harness::launch("66 — video filmstrip", Size::new(620.0, 240.0), |d| {
        let video = GeneratedVideo::new(96, 64, 12, Pattern::Sweep).frames(24);
        let mut player = VideoPlayer::over(&video);
        player.play();

        let duration = video.duration();
        let mut thumbs = Flex::row().spacing(6.0);
        for i in 0..THUMBS {
            let at = Duration::from_secs_f32(
                duration.as_secs_f32() * f32::from(i as u16) / THUMBS as f32,
            );
            player.seek(at);
            player.advance(Duration::ZERO);
            if let Some(frame) = player.frame(&video) {
                thumbs = thumbs.push(
                    Container::new()
                        .radius(3.0)
                        .child(vieww_widget::Image::new(frame)),
                );
            }
        }
        let _ = &video;

        feature_harness::set_page(
            d,
            Container::new()
                .color(Color::WHITE)
                .padding(EdgeInsets::all(20.0))
                .child(Flex::column().spacing(14.0).children(children![
                            thumbs,
                            Text::new(format!(
                                "24 frames at 12 fps — {:.1} s, sampled at 8 instants",
                                duration.as_secs_f32()
                            ))
                            .style(TextStyle {
                                size: 12.0,
                                color: Color::rgb(130, 140, 160),
                                ..TextStyle::default()
                            }),
                        ])),
        );
    })
}
