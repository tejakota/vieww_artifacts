//! The tier control: two options, each carrying its own price.
//!
//! vieww's `SegmentedControl` takes labels and nothing else, which is the one
//! thing this control cannot do. The first version of the app made the tier
//! choice from prose on a screen of its own, so you picked a tier before being
//! told what it was worth. **The number belongs on the control** — that is the
//! whole design, and a label-only segmented control cannot express it.
//!
//! The selection slides rather than teleporting. A choice that jumps reads as a
//! redraw; one that travels reads as a thing you moved.

use std::rc::Rc;
use std::time::Duration;

use vavlt_core::Tier;
use vieww::prelude::*;

use crate::theme::{motion, VavltTheme};

const HEIGHT: f32 = 76.0;
/// The inset of the sliding lozenge inside its track.
const INSET: f32 = 5.0;

/// Adwaita's deep out-ease: fast departure, long settle. Reads as inertia,
/// which is the point — the thing being chosen is a decision with weight.
const SETTLE: Curve = Curve::cubic(0.16, 1.0, 0.3, 1.0);

/// `Move` on the left with its figure, `Deep Move` on the right with its own.
pub fn tier_control(
    theme: &VavltTheme,
    current: Tier,
    move_figure: &str,
    deep_figure: &str,
    on_selected: impl Fn(Tier) + 'static,
) -> WidgetNode {
    let theme = *theme;
    let deep = current != Tier::Move;
    let labels = [
        ("Move", move_figure.to_string()),
        ("Deep Move", deep_figure.to_string()),
    ];
    let on_selected = Rc::new(on_selected);

    LayoutBuilder::new(move |constraints| {
        let width = constraints.max_width;
        let half = width / 2.0;
        let labels = labels.clone();
        let pick = on_selected.clone();

        Animated::new(if deep { 1.0 } else { 0.0 })
            .duration(motion::BASE)
            .curve(SETTLE)
            .build(move |t| {
                let colors = theme.colors;
                // The fill travels with the thumb: at t=0.5 the selection is
                // between the two tiers and so is its colour, which is what
                // makes blue-becoming-red read as one movement.
                let fill = lerp_color(colors.accent_bg, colors.destructive_bg, t);

                // A stadium, and the radius is *derived*: half the inner
                // height. A fixed 14 on a 66-tall lozenge is what reads as a
                // rectangle with its corners taken off — the curve has to be a
                // function of the height or it fights it.
                //
                // It also lifts. A vertical ramp plus a coloured glow plus a
                // one-pixel top highlight, so the selection reads as an object
                // resting in a channel rather than as a filled region — which
                // is the difference between a segment that is on and one you
                // slid there.
                let inner = HEIGHT - INSET * 2.0;
                let lighter = lerp_color(Color::hex(0x4A_92E8), Color::hex(0xEA_4A55), t);
                let darker = lerp_color(Color::hex(0x2F_76CF), Color::hex(0xC4_252F), t);
                let thumb = Positioned::new()
                    .left(INSET + t * half)
                    .top(INSET)
                    .width(half - INSET * 2.0)
                    .height(inner)
                    .child(
                        Container::new()
                            .radius(inner / 2.0)
                            .gradient(Gradient::vertical().with_stops(&[
                                (0.0, lighter),
                                (0.62, fill),
                                (1.0, darker),
                            ]))
                            .shadow(Shadow::new(
                                fill.with_alpha(107),
                                Offset::new(0.0, 4.0),
                                14.0,
                            )),
                    );

                let cells: Vec<WidgetNode> = labels
                    .iter()
                    .enumerate()
                    .map(|(index, (label, figure))| {
                        // How selected this cell is, right now. Both cells read
                        // the same `t`, so during the slide the outgoing label
                        // fades out exactly as the incoming one fades in.
                        let selected = if index == 0 { 1.0 - t } else { t };
                        let ink = lerp_color(colors.fg_2, colors.accent_fg, selected);
                        let sub = lerp_color(colors.dim, colors.accent_fg, selected);
                        let pick = pick.clone();
                        let tier = if index == 0 {
                            Tier::Move
                        } else {
                            Tier::DeepMove
                        };

                        Flexible::expanded(1)
                            .child(
                                Semantics::button(format!("{label}, {figure}")).child(
                                    GestureDetector::new().on_tap(move |_| pick(tier)).child(
                                        Container::new()
                                            .height(HEIGHT)
                                            .alignment(Alignment::CENTER)
                                            .child(
                                                Flex::column()
                                                    .main_axis_size(MainAxisSize::Min)
                                                    .cross_axis_alignment(
                                                        CrossAxisAlignment::Center,
                                                    )
                                                    .spacing(1.0)
                                                    .children(children![
                                                        Text::new((*label).to_string())
                                                            .style(theme.body().color(ink))
                                                            .bold(),
                                                        Text::new(figure.clone())
                                                            .style(theme.caption().color(sub)),
                                                    ]),
                                            ),
                                    ),
                                ),
                            )
                            .into()
                    })
                    .collect();

                // The channel. vieww has no inset shadow, so the recess is a
                // gradient on the track itself — dark at the top, clearing by a
                // third — which is what an inner shadow would have drawn anyway.
                let track = theme.colors.btn;
                Container::new()
                    .height(HEIGHT)
                    .radius(HEIGHT / 2.0)
                    .gradient(Gradient::vertical().with_stops(&[
                        (0.0, Color::rgba(0, 0, 0, 56).over(track)),
                        (0.34, track),
                        (1.0, track),
                    ]))
                    .border(Border::new(theme.colors.line_strong, 1.0))
                    .child(Stack::new().children(children![thumb, Flex::row().children(cells),]))
                    .into()
            })
            .into()
    })
    // **No `breakpoints` here.** They quantise the width the callback is given,
    // and this callback uses that width as *geometry*: `half` is how far the
    // lozenge travels. Quantised up to the next threshold, `half` came out
    // wider than the track, so choosing Deep Move slid the lozenge past the
    // right-hand edge and off the screen — which reads as "the toggle did not
    // move", because the part of it still visible is over the wrong label.
    //
    // Breakpoints are for a callback that picks a *layout*. A callback that
    // computes a position needs the real number. This is the second place in
    // this app to learn that, after the photo grid.
    .into()
}

/// Straight-line blend in sRGB.
///
/// Not perceptually uniform, and deliberately not: the two endpoints here are
/// Adwaita blue and Adwaita red at the same lightness, so an OKLab path would
/// bow through a purple that neither colour is, over 260ms, in the middle of a
/// control the user is looking straight at.
fn lerp_color(a: Color, b: Color, t: f32) -> Color {
    let t = t.clamp(0.0, 1.0);
    let mix = |x: u8, y: u8| (f32::from(x) + (f32::from(y) - f32::from(x)) * t) as u8;
    Color::rgba(mix(a.r, b.r), mix(a.g, b.g), mix(a.b, b.b), mix(a.a, b.a))
}

#[allow(dead_code)]
const _: Duration = motion::FAST;
