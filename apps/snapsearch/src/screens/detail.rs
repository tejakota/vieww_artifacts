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

use std::rc::Rc;

use vieww::prelude::*;

use crate::photo::Photo;
use crate::state::AppState;
use crate::theme::FluidTokens;

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
                Clip::new(ClipShape::RRect {
                    radius: tokens.tile_curve,
                })
                .child(
                    Container::new()
                        .color(theme.colors.surface_variant)
                        .child(Image::new(shown).fit(BoxFit::Contain).label(self.photo.label.clone())),
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
            .spacing(4.0)
            .children(children![
                Text::new(self.photo.label.clone())
                    .color(theme.colors.on_surface)
                    .size(theme.text.title.size)
                    .bold()
                    .align(TextAlign::Center),
                Text::new(match self.photo.source.path() {
                    // The path is the one thing a person can act on outside
                    // the app, so it is shown rather than hidden.
                    Some(path) => path.display().to_string(),
                    None => "generated sample".to_string(),
                })
                .color(theme.colors.on_surface_variant)
                .size(theme.text.label.size)
                .align(TextAlign::Center),
                Text::new(if sharp { "full resolution" } else { "loading full size…" })
                    .color(theme.colors.on_surface_variant)
                    .size(theme.text.label.size)
                    .align(TextAlign::Center),
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
                                                &theme
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

fn close_button(state: &AppState, theme: &Rc<ThemeData>) -> WidgetNode {
    let close = state.clone();
    Flex::row()
        .main_axis_size(MainAxisSize::Min)
        .children(children![Button::new("Done")
            .on_pressed(move || close.close_detail())])
        .into()
}

widget_node_from!(DetailView);
