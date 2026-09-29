//! An audio-reactive bar chart — the `vieww-audio::analysis` FFT,
//! photographed.
//!
//! A chord is synthesised (four tones across three octaves), rendered, and
//! analysed: the bar chart is the spectrum, banded logarithmically, so the
//! four notes stand out as four towers — synthesis and analysis agreeing,
//! in one picture, with no fixture file and no trust.

use vieww_audio::analysis::{Spectrum, Window};
use vieww_audio::{Envelope, Mixer, Tone, Waveform};
use vieww_foundation::{Color, Offset, Rect, Size};
use vieww_widget::prelude::*;

/// The chord: A3, A4, E5, A5 — 220, 440, 659, 880 Hz.
fn chord() -> vieww_audio::Samples {
    let shape = Envelope::attack_release(
        std::time::Duration::from_millis(20),
        std::time::Duration::from_millis(400),
    );
    Mixer::new(44_100)
        .add(Tone::new(220.0, Waveform::Sine).envelope(shape), 0.6)
        .add(Tone::new(440.0, Waveform::Sine).envelope(shape), 0.6)
        .add(Tone::new(659.3, Waveform::Sine).envelope(shape), 0.6)
        .add(Tone::new(880.0, Waveform::Sine).envelope(shape), 0.6)
        .render()
}

/// The log-spaced band edges for the chart: 12 bands from 40 Hz to
/// Nyquist, each 2^(1/2)× the one before — the spacing every spectrum
/// analyser uses, because hearing itself is logarithmic.
fn bands() -> Vec<(f32, f32)> {
    let mut out = Vec::new();
    let mut lo = 40.0_f32;
    while lo < 20_000.0 {
        let hi = (lo * 1.55).min(21_000.0);
        out.push((lo, hi));
        lo = hi;
    }
    out
}

#[derive(Debug)]
struct SpectrumBars;

impl Widget for SpectrumBars {
    fn debug_name(&self) -> &'static str {
        "SpectrumBars"
    }

    fn kind(&self) -> WidgetKind<'_> {
        WidgetKind::Composed
    }

    fn build(&self, _ctx: &BuildContext) -> WidgetNode {
        let spectrum = Spectrum::analyze(&chord(), Window::Hann);
        let bars: Vec<f32> = bands()
            .iter()
            .map(|&(lo, hi)| spectrum.band_energy(lo, hi))
            .collect();
        CustomPaint::sized(
            Size::new(560.0, 240.0),
            SpectrumPainter {
                bars,
                peak_bin_hz: spectrum.bin_hz(spectrum.peak_bin()),
            },
        )
        .into()
    }
}

widget_node_from!(SpectrumBars);

struct SpectrumPainter {
    bars: Vec<f32>,
    peak_bin_hz: f32,
}

impl CustomPainter for SpectrumPainter {
    fn paint(&self, size: Size) -> Vec<DrawInstruction> {
        let max = self.bars.iter().copied().fold(f32::MIN, f32::max).max(1e-4);
        let n = self.bars.len().max(1);
        let gap = 6.0;
        let bar_w = (size.width - gap * (n as f32 - 1.0)) / n as f32;
        let floor = size.height - 24.0;
        let mut out = Vec::new();
        for (i, energy) in self.bars.iter().enumerate() {
            let height = (energy / max) * (floor - 16.0);
            let x = i as f32 * (bar_w + gap);
            out.push(DrawInstruction::FillRoundedRect {
                rect: Rect::new(x, floor - height, x + bar_w, floor),
                radius: 2.0,
                color: Color::rgb(58, 122, 246),
            });
        }
        // The baseline.
        out.push(DrawInstruction::DrawLine {
            from: Offset::new(0.0, floor),
            to: Offset::new(size.width, floor),
            color: Color::rgb(210, 216, 228),
            width: 1.0,
        });
        out
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn should_repaint(&self, previous: &dyn CustomPainter) -> bool {
        match previous.as_any().downcast_ref::<Self>() {
            Some(prev) => {
                self.bars != prev.bars || self.peak_bin_hz != prev.peak_bin_hz
            }
            None => true,
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    feature_harness::launch("65 — audio reactive", Size::new(620.0, 320.0), |d| {
        feature_harness::set_page(
            d,
            Container::new()
                .color(Color::WHITE)
                .padding(EdgeInsets::all(20.0))
                .child(
                    Flex::column()
                        .spacing(16.0)
                        .children(children![
                            Text::new("A3 + A4 + E5 + A5, as a spectrum").style(TextStyle {
                                size: 14.0,
                                color: Color::rgb(90, 100, 120),
                                ..TextStyle::default()
                            }),
                            SpectrumBars,
                            Text::new(
                                "the four notes are the four towers — synthesis and analysis agree",
                            )
                            .style(TextStyle {
                                size: 12.0,
                                color: Color::rgb(130, 140, 160),
                                ..TextStyle::default()
                            }),
                        ]),
                ),
        );
    })
}
