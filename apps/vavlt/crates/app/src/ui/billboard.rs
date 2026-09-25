//! The full-bleed header a screen opens with.
//!
//! Art, a wash, a fade, and the words at the bottom. The fade is the whole
//! idea: a band that *ends at an edge* reads as a header, and a band that
//! disappears into the page reads as the page starting with a photograph.
//!
//! # Height is a function of the screen's job
//!
//! See [`Height`]. Every screen using the same 430 is what made five screens
//! with four different jobs look like the same screen — the browse screens earn
//! half the surface for a photograph and the progress screen does not.
//!
//! # Why the art is chosen rather than taken
//!
//! [`brightest`] picks which photograph goes here. Not the first one: a night
//! shot or an indoor frame makes the billboard three hundred points of
//! near-black with a number on it, and that is a large share of a real camera
//! roll. The app already decodes every thumbnail for the grid, so the mean is a
//! sum over pixels it is holding anyway.

use vieww::foundation::Image as ImageData;
use vieww::prelude::*;

use crate::model::Photo;
use crate::theme::VavltTheme;

/// How tall a billboard is, by what its screen is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Height {
    /// The landing screen. Earns the most, because it has the least else to say.
    Home,
    /// A decision screen: enough photograph to know what the number is about.
    Plan,
    /// A result screen.
    Outcome,
    /// A progress screen. Small — here the *grid* is the content, and a tall
    /// header would push the thing you are watching off the fold.
    Working,
    /// One photograph's own screen, where the picture is the point.
    Proof,
}

impl Height {
    #[must_use]
    pub const fn points(self) -> f32 {
        match self {
            Self::Home => 440.0,
            Self::Plan => 344.0,
            Self::Outcome => 330.0,
            Self::Working => 210.0,
            Self::Proof => 430.0,
        }
    }

    /// Scaled for a desktop window, where 440 of a 768-tall window is most of
    /// it. The phone numbers are the design; this is the same proportion of a
    /// surface that is shorter and much wider.
    #[must_use]
    pub fn for_form(self, form: crate::theme::FormFactor) -> f32 {
        match form {
            crate::theme::FormFactor::Phone => self.points(),
            crate::theme::FormFactor::Desktop => self.points() * 0.72,
        }
    }
}

/// Which photograph should carry the billboard.
///
/// The brightest thumbnail, by mean luminance over a subsample. Returns `None`
/// when nothing has decoded yet, which is the state the ghost billboard is for.
///
/// # Call this when a thumbnail arrives, not when a screen builds
///
/// It reads pixels. Three screens were calling it from inside their `build`,
/// which is every frame: a hundred thumbnails at 1,296 samples each is 130,000
/// iterations per frame to answer a question whose answer only changes when a
/// *new* thumbnail lands. `VavltState::billboard_pick` caches it; this stays
/// public because that cache is the only caller and it should be readable.
#[must_use]
pub fn brightest(photos: &[Photo]) -> Option<usize> {
    photos
        .iter()
        .enumerate()
        .filter_map(|(index, photo)| photo.thumb.as_ref().map(|t| (index, t)))
        .map(|(index, thumb)| {
            // Every 64th pixel. A thumbnail is 288² = 82,944 pixels and the
            // answer only has to be roughly right — sampling 1,296 of them is
            // the difference between a few microseconds and a few milliseconds
            // per photograph, over a hundred photographs, on a phone.
            let mut sum = 0u64;
            let mut count = 0u64;
            for chunk in thumb.pixels.chunks_exact(4).step_by(64) {
                // Rec. 601 luma, integer. Good enough to rank by, and it avoids
                // a float per pixel.
                sum += u64::from(chunk[0]) * 299
                    + u64::from(chunk[1]) * 587
                    + u64::from(chunk[2]) * 114;
                count += 1;
            }
            (index, if count == 0 { 0 } else { sum / count })
        })
        .max_by_key(|(_, luma)| *luma)
        .map(|(index, _)| index)
}

/// The band, with `lower` sitting at the bottom of it.
///
/// `art` is the photograph. `None` renders the ghost state — outlined tiles
/// rather than filled ones, because the app declares no media permission and
/// has nothing to show before a pick, and a grid of filled grey rectangles is
/// the skeleton-loader idiom rather than "this is where they go".
///
/// # The words go *over* a photograph and *under* the ghost wall
///
/// With art behind it this is a billboard, and words over a photograph under a
/// fade is the whole design. With the ghost wall behind it, the same overlay
/// reads as a mistake: the eyebrow lands on a tile outline and a headline word
/// sits inside a lit tile, so the reader's first impression is a rendering
/// fault rather than an empty state. `target/screenshots/01-vavlt-empty-phone-*`
/// before 2026-08-18 is what that looked like.
///
/// So the ghost case stacks instead of overlaying: the wall takes whatever the
/// copy leaves, in a column, and the fade lands exactly where the words begin.
/// Expressed as `Flexible::expanded` over the wall rather than as a measured
/// height, because the copy's height depends on how many lines the headline
/// wraps to — which is a thing the layout already knows and a constant would
/// have to be kept in agreement with.
pub fn billboard(
    theme: &VavltTheme,
    height: f32,
    art: Option<&Photo>,
    deep: bool,
    lower: Vec<WidgetNode>,
) -> WidgetNode {
    let colors = theme.colors;
    let mut layers = children![];
    let ghost = art.and_then(|photo| photo.thumb.as_ref()).is_none();

    match art.and_then(|p| p.thumb.as_ref()) {
        Some(thumb) => layers.push(
            // `from_shared_rgba8`, not `from_rgba8`. The latter takes an owned
            // `Vec`, so `as_ref().clone()` was deep-copying 331 KB of billboard
            // art on **every frame** — and, worse, handing the tree a fresh
            // allocation each time, which made vieww's identity-based damage
            // comparison report the image as changed and repaint the screen.
            Image::new(ImageData::from_shared_rgba8(
                thumb.pixels.clone(),
                thumb.edge,
                thumb.edge,
            ))
            .fit(BoxFit::Cover)
            .into(),
        ),
        None => layers.push(ghost_tiles(theme).into()),
    }

    // The wash: the tier's colour bled up from the bottom. The one purely
    // atmospheric thing in the app, and it is what stops a dark screen with a
    // photograph on it reading as a photo viewer.
    let wash = if deep {
        colors.destructive_tint
    } else {
        colors.accent_tint
    };
    layers.push(
        Container::new()
            .gradient(
                Gradient::radial(Offset::new(0.5, 1.0), 0.95)
                    .with_stops(&[(0.0, wash), (1.0, wash.with_alpha(0))]),
            )
            .into(),
    );

    // The fade. Six stops rather than three: a three-stop ramp to the page
    // colour has a visible knee about two thirds down, and the knee is exactly
    // where the eyebrow sits.
    let bg = colors.window;
    layers.push(
        Container::new()
            .gradient(Gradient::vertical().with_stops(&[
                (0.0, bg.with_alpha(0)),
                (0.34, bg.with_alpha(26)),
                (0.56, bg.with_alpha(107)),
                (0.74, bg.with_alpha(199)),
                (0.89, bg.with_alpha(245)),
                (1.0, bg),
            ]))
            .into(),
    );

    let copy = Padding::new(EdgeInsets::only(
        theme.metrics.gutter,
        0.0,
        theme.metrics.gutter,
        6.0,
    ))
    .child(
        Flex::column()
            .cross_axis_alignment(CrossAxisAlignment::Start)
            .main_axis_size(MainAxisSize::Min)
            .spacing(2.0)
            .children(lower),
    );

    let band: WidgetNode = if ghost {
        // Wall above, words below, and the wall takes what is left. The copy is
        // the last child at its natural height, so a headline that wraps to two
        // lines shrinks the wall rather than climbing into it.
        Flex::column()
            .cross_axis_alignment(CrossAxisAlignment::Stretch)
            .children(children![
                Flexible::expanded(1)
                    .child(Clip::rect().child(Stack::new().fit(StackFit::Expand).children(layers))),
                copy,
            ])
            .into()
    } else {
        layers.push(
            Positioned::new()
                .left(0.0)
                .right(0.0)
                .bottom(0.0)
                .child(copy)
                .into(),
        );
        Clip::rect()
            .child(Stack::new().fit(StackFit::Expand).children(layers))
            .into()
    };

    Container::new().height(height).child(band).into()
}

/// The band with nothing in it: outlined slots, three of them lit.
fn ghost_tiles(theme: &VavltTheme) -> WidgetNode {
    let colors = theme.colors;
    let rows: Vec<WidgetNode> = (0..3)
        .map(|row| {
            let cells: Vec<WidgetNode> = (0..5)
                .map(|col| {
                    let index = row * 5 + col;
                    let lit = matches!(index, 0 | 7 | 11);
                    Flexible::expanded(1)
                        .child(
                            Container::new()
                                .height(66.0)
                                .radius(22.0)
                                .color(if lit {
                                    colors.accent_tint
                                } else {
                                    Color::rgba(0, 0, 0, 0)
                                })
                                .border(Border::new(
                                    if lit {
                                        colors.accent_bg
                                    } else {
                                        colors.line_strong
                                    },
                                    1.5,
                                )),
                        )
                        .into()
                })
                .collect();
            Flex::row().spacing(7.0).children(cells).into()
        })
        .collect();

    // Below the top bar, so the first row is not half under a chevron.
    Padding::new(EdgeInsets::only(20.0, 78.0, 20.0, 0.0))
        .child(
            Flex::column()
                .main_axis_size(MainAxisSize::Min)
                .spacing(7.0)
                .children(rows),
        )
        .into()
}

/// The label above a billboard figure. Uppercase, tracked wide, dim.
pub fn eyebrow(theme: &VavltTheme, text: &str, tint: Option<Color>) -> WidgetNode {
    Text::new(text.to_uppercase())
        .style(
            TextStyle::new(12.0)
                .color(tint.unwrap_or(theme.colors.dim))
                .weight(FontWeight::Bold)
                .letter_spacing(1.4),
        )
        .into()
}

/// The figure itself: large, tight, and negatively tracked.
///
/// Negative letter spacing at this size and positive at small ones — the
/// opposite of the old `Numeric` rule, and correct: tracking that opens up a
/// 12px label closes up a 62px number, which otherwise reads as spaced-out.
pub fn figure(theme: &VavltTheme, text: impl Into<String>, size: f32, tint: Color) -> WidgetNode {
    let _ = theme;
    Text::new(text)
        .style(
            TextStyle::new(size)
                .color(tint)
                .weight(FontWeight::Medium)
                .line_height(1.0)
                .letter_spacing(-size * 0.016),
        )
        .into()
}

/// The bar under a figure that gives it a scale.
///
/// A number alone is a number; a number against what it is a fraction of is a
/// claim. `fraction` is clamped rather than trusted — a run that saved more
/// than it started with is a bug, and a bar wider than its track is how it
/// would show up.
pub fn meter(theme: &VavltTheme, fraction: f32, deep: bool) -> WidgetNode {
    let colors = theme.colors;
    let fill = if deep {
        colors.destructive_bg
    } else {
        colors.accent_bg
    };
    let lift = if deep {
        Color::hex(0xFF_7A68)
    } else {
        Color::hex(0x6A_A6F2)
    };
    let fraction = fraction.clamp(0.0, 1.0);

    Padding::new(EdgeInsets::only(0.0, 12.0, 0.0, 10.0))
        .child(
            Container::new()
                .height(5.0)
                .radius(3.0)
                .color(colors.btn)
                .child(LayoutBuilder::new(move |constraints| {
                    Align::new(Alignment::CENTER_LEFT)
                        .child(
                            Container::new()
                                .width((constraints.max_width * fraction).max(0.0))
                                .height(5.0)
                                .radius(3.0)
                                .gradient(
                                    Gradient::horizontal().with_stops(&[(0.0, fill), (1.0, lift)]),
                                ),
                        )
                        .into()
                })),
        )
        .into()
}

/// A line of supporting text under a figure.
pub fn lede(theme: &VavltTheme, text: impl Into<String>) -> WidgetNode {
    Text::new(text)
        .style(
            TextStyle::new(15.0)
                .color(theme.colors.fg_2)
                .line_height(1.35),
        )
        .into()
}
