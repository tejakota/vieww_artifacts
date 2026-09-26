//! The masonry grid — few, large cells, alive on their own clock.
//!
//! `vieww_widget::GridView` is uniform-pitch only (see the framework's own
//! `docs/PRODUCTION-GAPS.md`), so — same as the HTML prototype's "Option A"
//! — this hand-builds N `Flex::column()`s and distributes tiles into the
//! shortest one as it goes. It's a plain `Widget` (`WidgetKind::Composed`),
//! not a virtualised list, which is the right trade at "a couple of large
//! cells per screen": there's nothing here worth virtualising.

use std::rc::Rc;

use vieww::prelude::*;
use vieww::widget::Handler;

use crate::photo::{self, Photo};
use crate::theme::FluidTokens;

pub const COLUMN_COUNT: usize = 2;
pub const TILE_BASE: f32 = 208.0;

/// A cheap deterministic triangle wave in `[-1, 1]`, standing in for a sine
/// without pulling in `libm`/`std::f32::sin` formatting concerns — plenty
/// smooth at the tick rate this is sampled at.
fn triangle(phase: f32) -> f32 {
    let p = phase - phase.floor();
    if p < 0.5 {
        4.0 * p - 1.0
    } else {
        3.0 - 4.0 * p
    }
}

#[derive(Clone)]
pub struct Masonry {
    pub photos: Vec<(Photo, Option<f32>)>, // (photo, match score if this is a results grid)
    pub tick: u64,
    /// Per-tile entrance progress, 0→1, one entry per tile.
    ///
    /// Read out of the reveal `Signal`s by the *caller*, inside its own
    /// `Widget::build`, and passed down as plain numbers. That is deliberate:
    /// a `.get()` only subscribes while a build is on the tracking stack, so
    /// reading a signal inside one of the nested builder closures below would
    /// subscribe nothing at all — silently.
    pub reveal: Vec<f32>,
    /// Whether tiles turn themselves over to other photos.
    ///
    /// True while browsing, false while showing search results. A results
    /// grid that quietly reshuffles itself is not alive, it is broken: the
    /// screenshots caught a "sunset over water" search whose tiles had
    /// flipped to a desert and a night skyline while still wearing the
    /// original matches' 100% badges.
    pub live: bool,
    pub on_tap: Handler<Photo>,
    pub key: Option<Key>,
}

// `Handler<Photo>` is `Rc<dyn Fn(Photo)>`, which has no `Debug` impl — every
// `Widget` must be `Debug` (it's part of the trait bound), so this reports
// the shape without trying to print the closure.
impl std::fmt::Debug for Masonry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Masonry")
            .field("tiles", &self.photos.len())
            .field("tick", &self.tick)
            .finish()
    }
}

impl Masonry {
    #[must_use]
    pub fn new(photos: Vec<(Photo, Option<f32>)>, tick: u64, on_tap: Handler<Photo>) -> Self {
        Self {
            photos,
            tick,
            reveal: Vec::new(),
            live: true,
            on_tap,
            key: None,
        }
    }

    /// Give the tiles a staggered entrance — see [`Self::reveal`].
    #[must_use]
    pub fn reveal(mut self, reveal: Vec<f32>) -> Self {
        self.reveal = reveal;
        self
    }

    /// Hold every tile on its own photo — see [`Self::live`].
    #[must_use]
    pub fn still(mut self) -> Self {
        self.live = false;
        self
    }

    #[must_use]
    pub fn key(mut self, key: impl Into<Key>) -> Self {
        self.key = Some(key.into());
        self
    }
}

impl Widget for Masonry {
    fn debug_name(&self) -> &'static str {
        "Masonry"
    }

    fn kind(&self) -> WidgetKind<'_> {
        WidgetKind::Composed
    }

    fn key(&self) -> Option<&Key> {
        self.key.as_ref()
    }

    fn build(&self, ctx: &BuildContext) -> WidgetNode {
        let theme = ThemeData::of(ctx);

        let mut heights = [0.0_f32; COLUMN_COUNT];
        let mut columns: Vec<Vec<WidgetNode>> = vec![Vec::new(); COLUMN_COUNT];

        for (index, (photo, score)) in self.photos.iter().enumerate() {
            let shortest = heights
                .iter()
                .enumerate()
                .min_by(|a, b| a.1.total_cmp(b.1))
                .map(|(i, _)| i)
                .unwrap_or(0);
            let tile_h = TILE_BASE * photo.aspect;
            heights[shortest] += tile_h + theme.metrics.gap * 1.5;

            columns[shortest].push(tile(
                photo.clone(),
                *score,
                tile_h,
                index as u64,
                self.tick,
                self.live,
                // Anything past the pool is already in place rather than
                // permanently invisible: a missing reveal value must never
                // be the reason a photo cannot be seen.
                self.reveal.get(index).copied().unwrap_or(1.0),
                &self.on_tap,
                &theme,
            ));
        }

        // Each column must be `Flexible::expanded` — a plain `Flex::row`
        // child takes only its own preferred width, and a `Flex::column`
        // with nothing of its own explicit width collapses to whatever its
        // (also width-less) tiles report, which is next to nothing. This is
        // what actually splits the row's width evenly between columns.
        let column_widgets: Vec<WidgetNode> = columns
            .into_iter()
            .map(|tiles| {
                Flexible::expanded(1)
                    .child(
                        Flex::column()
                            .cross_axis_alignment(CrossAxisAlignment::Stretch)
                            .spacing(theme.metrics.gap * 1.5)
                            .children(tiles),
                    )
                    .into()
            })
            .collect();

        Flex::row()
            .cross_axis_alignment(CrossAxisAlignment::Start)
            .spacing(theme.metrics.gap * 1.5)
            .children(column_widgets)
            .into()
    }
}

fn tile(
    photo: Photo,
    score: Option<f32>,
    height: f32,
    index: u64,
    tick: u64,
    live: bool,
    reveal: f32,
    on_tap: &Handler<Photo>,
    theme: &Rc<ThemeData>,
) -> WidgetNode {
    // Ambient drift: a slow, per-tile-offset wander in x and y, entirely
    // derived from the shared `tick` clock — no per-tile state needed.
    let drift_period = 90.0 + (index % 5) as f32 * 18.0;
    let phase_offset = index as f32 * 0.37;
    let (dx, dy) = if live {
        (
            triangle(tick as f32 / drift_period + phase_offset) * 6.0,
            triangle(tick as f32 / (drift_period * 0.8) + phase_offset + 0.25) * 8.0,
        )
    } else {
        (0.0, 0.0)
    };

    // Live-replace: every `cycle` ticks this tile flips to a freshly
    // generated "photo" and back — a page-turn (scale-collapse) rather than
    // a true 3D rotateY, since `Transformed` only exposes a 2D matrix.
    let cycle = 220 + (index % 7) * 40;
    let showing_alt = live && (tick / cycle as u64 + index) % 2 == 1;
    let flip_target = if showing_alt { 1.0 } else { 0.0 };
    let alt_seed = 10_000 + index * 977 + tick / cycle as u64;
    let alt_photo = photo::photo_for_seed(alt_seed);

    let on_tap_photo = photo.clone();
    let on_tap_alt = alt_photo.clone();
    let handler = on_tap.clone();

    let theme_for_build = theme.clone();
    let card = Animated::new(flip_target)
        .duration(std::time::Duration::from_millis(380))
        .curve(Curve::EASE_IN_OUT)
        .build(move |t| {
            let (shown, shown_label) = if t < 0.5 {
                (on_tap_photo.image.clone(), on_tap_photo.label.clone())
            } else {
                (on_tap_alt.image.clone(), on_tap_alt.label.clone())
            };
            // A page-turn stand-in for a true 3D card flip: `Transformed`
            // only exposes a flat 2D matrix, so the "flip" is a horizontal
            // squash-to-nothing-and-back, with content swapped at the
            // collapsed midpoint (t == 0.5) where nothing is visible.
            // Full width (1.0) at both rest states t=0 and t=1; collapsed
            // (0.0) exactly at the swap point t=0.5 — a "V", not a tent.
            let squash = (t * 2.0 - 1.0).abs().max(0.02);

            // The picture itself, clipped to the tile's rounded corners.
            let picture: WidgetNode = Clip::new(ClipShape::RRect {
                // The tile's own curve token, not `metrics.corner`: that one
                // is tuned so *controls* come out as stadiums, and a 200px
                // card at that radius would be a capsule.
                radius: crate::theme::FluidTokens::new().tile_curve,
            })
            .child(
                Container::new()
                    .color(theme_for_build.colors.surface_variant)
                    .child(Image::new(shown).fit(BoxFit::Cover).label(shown_label)),
            )
            .into();

            let mut layers = vec![Positioned::fill().child(picture).into()];

            if let Some(score) = score {
                // The relevance badge as the fluid ramp itself: gradient pill,
                // sparkle glyph, score — the result grid's one mark of “how
                // well this matched”, wearing the same ramp the search began
                // from. A dark plate said “an overlay”; the ramp says “this
                // came out of the search”.
                //
                // Deliberately a *sibling* of the clip rather than a child of
                // it. Inside the `Clip` the badge's pill painted but its
                // glyphs did not — the known CPU-backend glyph placement bug
                // the framework records in `HANDOFF-2026-08-21.md`. The badge
                // is inset well clear of the rounded corners, so it never
                // needed clipping in the first place; this both dodges the
                // bug and is the more honest tree.
                layers.push(
                    Positioned::new()
                        .left(10.0)
                        .bottom(10.0)
                        .child(
                            Container::new()
                                .gradient(FluidTokens::new().fluid_gradient())
                                .radius(f32::MAX)
                                .padding(EdgeInsets::symmetric(11.0, 5.0))
                                .shadow(Shadow {
                                    color: Color::rgba(0x7c, 0x3a, 0xed, 0x8c),
                                    offset: Offset::new(0.0, 4.0),
                                    blur: 14.0,
                                    spread: 0.0,
                                    is_inset: false,
                                })
                                .child(
                                    Flex::row()
                                        .main_axis_size(MainAxisSize::Min)
                                        .cross_axis_alignment(CrossAxisAlignment::Center)
                                        .spacing(5.0)
                                        .children(children![
                                            Icon::new(crate::widgets::icons::sparkle())
                                                .size(11.0)
                                                .color(Color::WHITE)
                                                .label("match"),
                                            Text::new(format!(
                                                "{}%",
                                                (score * 100.0).round() as i32
                                            ))
                                            .color(Color::WHITE)
                                            .size(theme_for_build.text.label.size)
                                            .bold(),
                                        ]),
                                ),
                        )
                        .into(),
                );
            }

            Transformed::scale(squash, 1.0)
                .child(
                    Container::new()
                        .height(height)
                        .child(Stack::new().fit(StackFit::Expand).children(layers)),
                )
                .into()
        });

    let handler_for_press = handler.clone();
    let alt_for_press = alt_photo;
    let orig_for_press = photo;

    // The entrance: rise into place and fade in together. `reveal` comes from
    // a spring in the *spatial* family, so it overshoots slightly and the
    // tile settles — but the opacity is clamped rather than following it,
    // because an alpha that overshoots asks for 1.06, clamps, and stalls
    // visibly at full opacity. Spatial motion may overshoot; effects may not.
    let rise = (1.0 - reveal) * 26.0;
    let fade = reveal.clamp(0.0, 1.0);

    let placed = Transformed::translate(Offset::new(dx, dy + rise))
        .child(
            Pressable::new(move |_press| card.clone().into()).on_tap(move || {
                let current = if showing_alt {
                    alt_for_press.clone()
                } else {
                    orig_for_press.clone()
                };
                handler_for_press(current);
            }),
        );

    if fade >= 0.999 {
        // No layer when nothing is being faded — an `Opacity` at 1.0 still
        // costs a composited group.
        placed.into()
    } else {
        Opacity::new(fade).child(placed).into()
    }
}

widget_node_from!(Masonry);
