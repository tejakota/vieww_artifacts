//! A rendered waveform — the gap-closure pass's `vieww-audio` crate,
//! photographed.
//!
//! A synthesised chime (three tones, one attack-and-release envelope)
//! rendered to samples and drawn as its waveform: the attack's ramp, the
//! hold, the release's decay — the *shape of the sound*, which is the
//! thing the crate's types are for.

use std::time::Duration;

use vieww_audio::{Envelope, Mixer, Tone, Waveform};
use vieww_foundation::{Color, Offset, Size};
use vieww_widget::prelude::*;

/// The chime: A4, E5 and C#6 under one 6 ms / 300 ms envelope.
fn chime() -> vieww_audio::Samples {
    let shape = Envelope::attack_release(Duration::from_millis(6), Duration::from_millis(300));
    Mixer::new(44_100)
        .add(Tone::new(440.0, Waveform::Sine).envelope(shape), 0.5)
        .add(
            Tone::new(659.3, Waveform::Sine)
                .envelope(shape)
                .hold(Duration::from_millis(60)),
            0.35,
        )
        .add(
            Tone::new(1108.7, Waveform::Sine)
                .envelope(shape)
                .hold(Duration::from_millis(120)),
            0.25,
        )
        .render()
}

#[derive(Debug)]
struct WaveformView;

impl Widget for WaveformView {
    fn debug_name(&self) -> &'static str {
        "WaveformView"
    }

    fn kind(&self) -> WidgetKind<'_> {
        WidgetKind::Composed
    }

    fn build(&self, _ctx: &BuildContext) -> WidgetNode {
        let samples = chime();
        let peak = samples.peak();
        let frames = samples.frames();
        CustomPaint::sized(
            Size::new(560.0, 180.0),
            WaveformPainter {
                samples: samples.data,
                peak,
                frames,
            },
        )
        .into()
    }
}

widget_node_from!(WaveformView);

struct WaveformPainter {
    samples: Vec<f32>,
    peak: f32,
    frames: usize,
}

impl CustomPainter for WaveformPainter {
    fn paint(&self, size: Size) -> Vec<DrawInstruction> {
        let mid = size.height / 2.0;
        let mut out = Vec::new();
        // The zero line: the wave oscillates around it.
        out.push(DrawInstruction::DrawLine {
            from: Offset::new(0.0, mid),
            to: Offset::new(size.width, mid),
            color: Color::rgb(228, 232, 240),
            width: 1.0,
        });
        if self.samples.is_empty() {
            return out;
        }
        // Min/max per column — the waveform's honest shape at any zoom,
        // rather than a decimated polyline that hides its own clipping.
        let columns = size.width as usize;
        let per_column = (self.frames / columns).max(1);
        for column in 0..columns {
            let start = column * per_column;
            let end = (start + per_column).min(self.samples.len());
            if start >= end {
                break;
            }
            let mut lo = f32::INFINITY;
            let mut hi = f32::NEG_INFINITY;
            for sample in &self.samples[start..end] {
                lo = lo.min(*sample);
                hi = hi.max(*sample);
            }
            let scale = if self.peak > f32::EPSILON {
                (size.height / 2.0 - 8.0) / self.peak
            } else {
                1.0
            };
            let x = column as f32;
            out.push(DrawInstruction::DrawLine {
                from: Offset::new(x, mid - hi * scale),
                to: Offset::new(x, mid - lo * scale),
                color: Color::rgb(58, 122, 246),
                width: 1.0,
            });
        }
        out
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn should_repaint(&self, previous: &dyn CustomPainter) -> bool {
        match previous.as_any().downcast_ref::<Self>() {
            Some(prev) => {
                self.samples.len() != prev.samples.len()
                    || self.peak != prev.peak
                    || self.frames != prev.frames
            }
            None => true,
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    feature_harness::launch("64 — audio waveform", Size::new(620.0, 300.0), |d| {
        let samples = chime();
        let duration = samples.duration();
        feature_harness::set_page(
            d,
            Container::new()
                .color(Color::WHITE)
                .padding(EdgeInsets::all(20.0))
                .child(Flex::column().spacing(16.0).children(children![
                            Text::new("a chime, rendered and drawn back").style(TextStyle {
                                size: 14.0,
                                color: Color::rgb(90, 100, 120),
                                ..TextStyle::default()
                            }),
                            WaveformView,
                            Text::new(format!(
                                "{:.2} s, peak {:.2}, {} frames",
                                duration.as_secs_f32(),
                                samples.peak(),
                                samples.frames()
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
