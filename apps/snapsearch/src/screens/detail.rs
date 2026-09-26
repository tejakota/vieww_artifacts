//! One photo, full screen.
//!
//! The other half of a photo-search app: finding a photo is only useful if
//! you can then look at it.
//!
//! # Two images, deliberately
//!
//! The grid holds a 256px thumbnail — that is what keeps a large library in
//! tens of megabytes rather than gigabytes. Scaled up to fill a phone screen
//! it is visibly soft, so opening a photo kicks off a full-resolution decode
//! on a worker and swaps it in when it lands. Until then the thumbnail is
//! shown, scaled: something immediately, sharpened a moment later, rather
//! than a spinner over an empty screen.

use vieww::prelude::*;

use crate::photo::Photo;
use crate::state::AppState;
use crate::theme::FluidTokens;
use crate::widgets::icons as app_icons;

pub struct DetailView {
    pub state: AppState,
    pub photo: Photo,
    /// The full-resolution decode, once it has arrived.
    pub full: Option<crate::photo::Image_>,
    /// 0→1 open progress.
    pub progress: f32,
}

impl std::fmt::Debug for DetailView {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DetailView").field("photo", &self.photo.id).finish()
    }
}

impl Widget for DetailView {
    fn debug_name(&self) -> &'static str {
        "DetailView"
    }

    fn kind(&self) -> WidgetKind<'_> {
        WidgetKind::Composed
    }

    fn build(&self, ctx: &BuildContext) -> WidgetNode {
        let theme = ThemeData::of(ctx);
        let tokens = FluidTokens::new();
        let t = self.progress.clamp(0.0, 1.0);

        let close = self.state.clone();
        let shown = self.full.clone().unwrap_or_else(|| self.photo.image.clone());
        let sharp = self.full.is_some();

        // Rises and settles as it opens. Spatial, so it may overshoot; the
        // scrim's opacity below is an effect, and may not.
        let rise = (1.0 - t) * 40.0;

        let stage = Transformed::translate(Offset::new(0.0, rise)).child(
            Padding::new(EdgeInsets::symmetric(theme.metrics.gap * 2.0, 0.0)).child(
                // The lift rides *outside* the clip: a shadow drawn inside
                // its own `Clip` is clipped to the very corner it is meant
                // to soften, and a shadow on a square box would not follow
                // the rounded corner at all. Same shape as the sheet and
                // the FAB — the one photograph on screen, over the deepest
                // scrim in the app, should read as floating, not pasted.
                Container::new()
                    .color(theme.colors.surface_variant)
                    .radius(tokens.tile_curve)
                    .shadow(Shadow::new(
                        Color::rgba(0, 0, 0, 0x99),
                        Offset::new(0.0, 18.0),
                        44.0,
                    ))
                    .child(
                        Clip::new(ClipShape::RRect {
                            radius: tokens.tile_curve,
                        })
                        .child(
                            Container::new()
                                .color(theme.colors.surface_variant)
                                .child(Image::new(shown).fit(BoxFit::Contain).label(self.photo.label.clone())),
                        ),
                    ),
            ),
        );

        // `Stretch` plus `TextAlign::Center`, not `CrossAxisAlignment::Center`:
        // centring the cross axis sizes each line to its own text, which
        // leaves `TextAlign` with no width to centre within — the caption
        // came out hugging the right edge.
        let caption = Flex::column()
            .main_axis_size(MainAxisSize::Min)
            .cross_axis_alignment(CrossAxisAlignment::Stretch)
            .spacing(6.0)
            .children(children![
                Text::new(self.photo.label.clone())
                    .color(theme.colors.on_surface)
                    .size(theme.text.title.size)
                    .bold()
                    .align(TextAlign::Center),
                // What this photo *is*, as chips rather than three stacked
                // centred lines: the provenance (path or sample), and the
                // resolution state. A chip row survives being read
                // half-attentively; three identical grey lines do not.
                Flex::row()
                    .main_axis_alignment(MainAxisAlignment::Center)
                    .main_axis_size(MainAxisSize::Min)
                    .cross_axis_alignment(CrossAxisAlignment::Center)
                    .spacing(8.0)
                    .children(children![
                        chip(
                            tokens.surface_raised,
                            app_icons::image(),
                            match self.photo.source.path() {
                                // The path is the one thing a person can act on
                                // outside the app, so it is shown rather than
                                // hidden — filename only; the full path is
                                // wider than any phone.
                                Some(path) => path
                                    .file_name()
                                    .map(|n| n.to_string_lossy().to_string())
                                    .unwrap_or_else(|| "file".to_string()),
                                None => "generated sample".to_string(),
                            },
                            theme.colors.on_surface_variant,
                        ),
                        chip(
                            tokens.surface_raised,
                            app_icons::sparkle(),
                            if sharp {
                                "full resolution".to_string()
                            } else {
                                "loading full size…".to_string()
                            },
                            if sharp {
                                theme.colors.success
                            } else {
                                theme.colors.on_surface_variant
                            },
                        ),
                    ]),
            ]);

        Positioned::fill()
            .child(
                Opacity::new(t).child(
                    Stack::new().fit(StackFit::Expand).children(children![
                        // Tapping anywhere outside closes. A full-screen
                        // viewer whose only exit is a small ✕ is a viewer
                        // people get stuck in.
                        Positioned::fill().child(
                            ModalBarrier::new()
                                .color(Color::rgba(0x06, 0x05, 0x0c, 0xf2))
                                .on_dismiss(move || close.close_detail()),
                        ),
                        Positioned::fill().child(
                            SafeArea::new().child(
                                Flex::column()
                                    .main_axis_alignment(MainAxisAlignment::Center)
                                    .cross_axis_alignment(CrossAxisAlignment::Stretch)
                                    .spacing(theme.metrics.gap * 2.0)
                                    .children(children![
                                        Flexible::expanded(1).child(stage),
                                        Padding::new(EdgeInsets::symmetric(
                                            theme.metrics.gap * 2.0,
                                            theme.metrics.gap
                                        ))
                                        .child(caption),
                                        Padding::new(EdgeInsets::only(
                                            0.0,
                                            0.0,
                                            0.0,
                                            theme.metrics.gap * 3.0
                                        ))
                                        .child(
                                            Align::new(Alignment::CENTER).child(close_button(
                                                &self.state,
                                                &tokens,
                                            ))
                                        ),
                                    ]),
                            ),
                        ),
                    ]),
                ),
            )
            .into()
    }
}

fn close_button(state: &AppState, tokens: &FluidTokens) -> WidgetNode {
    let close = state.clone();
    // The ramp travels into the `'static` press builder as a value, not a
    // reference — `Gradient` is `Copy`, and the token struct it came from is
    // gone by the time a press rebuilds this.
    let gradient = tokens.fluid_gradient();
    // The viewer's exit, composed rather than a stock `Button`: the one
    // control sitting directly on the photo-black scrim gets the fluid ramp
    // and the FAB's shadow language — filled, lifted, unmistakably the
    // primary thing to press — where a flat filled button washed out against
    // the scrim and a text button worse.
    Pressable::new(move |press| {
        let scale = 1.0 - press * 0.06;
        Transformed::scale(scale, scale).child(
            Container::new()
                .gradient(gradient)
                .radius(f32::MAX)
                .shadow(Shadow {
                    color: Color::rgba(0x7c, 0x3a, 0xed, 0x99),
                    offset: Offset::new(0.0, 10.0),
                    blur: 26.0,
                    spread: 0.0,
                    is_inset: false,
                })
                .padding(EdgeInsets::symmetric(34.0, 14.0))
                .child(
                    Text::new("Done")
                        .color(Color::WHITE)
                        .size(16.0)
                        .bold(),
                ),
        )
        .into()
    })
    .on_tap(move || close.close_detail())
    .into()
}

/// A small pill of metadata: glyph + text on a raised surface.
fn chip(plate: Color, icon: IconData, text: String, ink: Color) -> WidgetNode {
    Container::new()
        .color(plate)
        .radius(f32::MAX)
        .padding(EdgeInsets::symmetric(10.0, 5.0))
        .child(
            Flex::row()
                .main_axis_size(MainAxisSize::Min)
                .cross_axis_alignment(CrossAxisAlignment::Center)
                .spacing(5.0)
                .children(children![
                    Icon::new(icon).size(11.0).color(ink),
                    Text::new(text).color(ink).size(12.0),
                ]),
        )
        .into()
}

widget_node_from!(DetailView);
