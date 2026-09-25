//! Buttons, hand-built rather than vieww's `Button`.
//!
//! vieww's own `Button` is a good control and the wrong one here: Adwaita has
//! four button kinds and the difference between *suggested* and *destructive*
//! is the whole consent design — a run that cannot be undone must not look like
//! a run that can. `ButtonStyle` has two variants, and restyling one from the
//! outside is not possible because the fill comes from `primary`.
//!
//! So this is a `Pressable` with an Adwaita chip drawn under it. It is thirty
//! lines, and it is the one place in the app where the tier's colour becomes a
//! button's colour.

use vieww::foundation::IconData;
use vieww::prelude::*;

use crate::theme::VavltTheme;

/// Adwaita's four.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonKind {
    /// The default: a flat chip level with the controls beside it.
    Regular,
    /// The action the screen is for. Filled in the accent.
    Suggested,
    /// Filled in red, because the colour is the consequence rather than the
    /// brand. Deep Move's commit button is this.
    Destructive,
    /// No fill and no border. A secondary escape — "Stop", "Restore".
    Flat,
    /// Filled in the foreground colour: white ink on a dark page, dark ink on a
    /// light one. Netflix's "Play" button, and the right weight for the one
    /// action a billboard is asking for — an accent fill next to a photograph
    /// competes with it, and this does not.
    Solid,
    /// Frosted. vieww has no backdrop blur, so this is a flat translucency —
    /// which over a photograph still reads as a panel you can see through, and
    /// that is most of what the effect was for.
    Glass,
}

impl ButtonKind {
    const fn tinted(self) -> bool {
        matches!(
            self,
            Self::Suggested | Self::Destructive | Self::Solid | Self::Glass
        )
    }
}

/// A full-width action.
pub fn button(
    theme: &VavltTheme,
    label: impl Into<String>,
    kind: ButtonKind,
    enabled: bool,
    on_tap: impl Fn() + 'static,
) -> WidgetNode {
    button_with(theme, label, None, kind, enabled, on_tap)
}

/// A full-width action with a glyph in front of its label.
///
/// The icon is optional rather than a second function, and it is *leading*
/// rather than free-placed, because the two rules a button icon has to follow
/// are that it sits where the reading starts and that it takes the label's
/// colour. Both are decided here rather than at the call site: an icon that
/// picks its own colour is how a destructive button ends up with an accent
/// glyph on it.
///
/// Sized from the body text rather than fixed, so it scales with the type when
/// the OS text size changes — a 20pt glyph beside 24pt text reads as a mistake,
/// and a glyph that does not grow with the label is exactly that at the largest
/// accessibility sizes.
pub fn button_with(
    theme: &VavltTheme,
    label: impl Into<String>,
    icon: Option<IconData>,
    kind: ButtonKind,
    enabled: bool,
    on_tap: impl Fn() + 'static,
) -> WidgetNode {
    let label = label.into();
    let colors = theme.colors;
    let m = theme.metrics;

    let fill = match kind {
        _ if !enabled => colors.btn,
        ButtonKind::Suggested => colors.accent_bg,
        ButtonKind::Destructive => colors.destructive_bg,
        ButtonKind::Flat => Color::rgba(0, 0, 0, 0),
        ButtonKind::Solid => colors.fg,
        ButtonKind::Glass => colors.btn,
        ButtonKind::Regular => colors.btn,
    };

    let ink = match kind {
        _ if !enabled => colors.dim,
        ButtonKind::Suggested => colors.accent_fg,
        ButtonKind::Destructive => colors.destructive_fg,
        ButtonKind::Solid => colors.window,
        _ => colors.fg,
    };

    let body = move |press: f32| -> WidgetNode {
        // The press darkens rather than lightening: on a filled Adwaita button
        // a lighter pressed state reads as a hover, and a phone has no hover.
        let pressed = Color::rgba(0, 0, 0, (press * 46.0) as u8);
        // Half the height, so every button is a stadium and the family is
        // obvious at a glance. Derived rather than chosen: a fixed radius on a
        // control whose height changes with the form factor is what reads as a
        // rectangle with the corners taken off.
        let radius = m.tap_lg / 2.0;
        let mut container = Container::new()
            .height(m.tap_lg)
            // A flat button has no fill of its own, so its *only* visible
            // state is the press. Everything else darkens an existing fill.
            .color(if fill.is_transparent() {
                colors.btn_active.with_alpha((press * 54.0) as u8)
            } else {
                fill
            })
            .radius(radius)
            .alignment(Alignment::CENTER);

        // A flat fill at this radius reads as a sticker; a vertical ramp with a
        // coloured glow under it reads as lit. Only the two accent kinds get
        // it — `Solid` is the foreground colour and must not acquire a hue, and
        // `Glass` is meant to be flat translucency.
        //
        // `ramp` existed in this file with no caller, which is why the accent
        // buttons were rendering flat.
        if let Some((top, bottom)) = ramp(kind, colors) {
            container = container
                .gradient(Gradient::vertical().with_stops(&[
                    (0.0, top),
                    (0.6, fill),
                    (1.0, bottom),
                ]))
                .shadow(Shadow::new(
                    fill.with_alpha(87),
                    Offset::new(0.0, 6.0),
                    18.0,
                ));
        }

        let mut container = container.child(
            Container::new()
                .color(pressed)
                .radius(radius)
                .alignment(Alignment::CENTER)
                .child({
                    // No `TextAlign::Center` here, deliberately. A
                    // `Text` aligns inside the width it was *offered* while
                    // still sizing itself to its ink, so a centred label
                    // inside a centring parent is offset twice and runs off
                    // the right edge. The parent centres; the text does not.
                    let text: WidgetNode = Text::new(label.clone())
                        .style(theme_label(m.t_body, ink))
                        .into();
                    match &icon {
                        None => text,
                        Some(glyph) => Flex::row()
                            .main_axis_size(MainAxisSize::Min)
                            .cross_axis_alignment(CrossAxisAlignment::Center)
                            .spacing(m.sp_1)
                            .children(children![
                                // Boxed to an exact square. `Icon::size`
                                // sets the glyph's em, and inside a
                                // `MainAxisSize::Min` row the box it lands
                                // in is not square — the frame of the photo
                                // glyph came out clipped on its right-hand
                                // side, which reads as a smudge rather than
                                // as an icon.
                                SizedBox::square(m.t_body * 1.5).child(
                                    Icon::new(glyph.clone()).size(m.t_body * 1.5).color(ink),
                                ),
                                text,
                            ])
                            .into(),
                    }
                }),
        );

        // No border on the tinted or flat kinds: a filled button that also has
        // an outline reads as two controls stacked.
        if !kind.tinted() && kind != ButtonKind::Flat {
            container = container.border(Border::thin(colors.line));
        }
        container.into()
    };

    if enabled {
        Pressable::new(body).on_tap(on_tap).into()
    } else {
        // Not a disabled `Pressable`: an element that still enters the gesture
        // arena can win a contest against the list scrolling under it, which is
        // a dead button that also eats the scroll.
        body(0.0)
    }
}

/// A round icon target in the top bar.
pub fn icon_button(
    theme: &VavltTheme,
    icon: IconData,
    tint: Color,
    label: &'static str,
    on_tap: impl Fn() + 'static,
) -> WidgetNode {
    let colors = theme.colors;
    let side = theme.metrics.tap;

    Semantics::button(label)
        .child(
            Pressable::new(move |press| {
                Container::new()
                    .size(side, side)
                    .color(colors.btn_active.with_alpha((press * 54.0) as u8))
                    .radius(side / 2.0)
                    .alignment(Alignment::CENTER)
                    .child(Icon::new(icon.clone()).size(22.0).color(tint))
                    .into()
            })
            .on_tap(on_tap),
        )
        .into()
}

/// The lighter top and darker bottom of a filled button's ramp.
fn ramp(kind: ButtonKind, colors: crate::theme::VavltColors) -> Option<(Color, Color)> {
    let _ = colors;
    match kind {
        ButtonKind::Suggested => Some((Color::hex(0x4A_92E8), Color::hex(0x2F_76CF))),
        ButtonKind::Destructive => Some((Color::hex(0xEA_4A55), Color::hex(0xC4_252F))),
        _ => None,
    }
}

fn theme_label(size: f32, color: Color) -> TextStyle {
    TextStyle::new(size)
        .color(color)
        .weight(FontWeight::Bold)
        .line_height(1.2)
}
