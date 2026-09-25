//! Chrome: the top bar, the tab bar, the pinned action bar, the empty state.

use std::rc::Rc;

use vieww::prelude::*;

use crate::model::Tab;
use crate::theme::VavltTheme;

/// Title, an optional back chevron, and whatever the screen wants on the right.
pub fn top_bar(
    theme: &VavltTheme,
    title: impl Into<String>,
    back: Option<Box<dyn Fn()>>,
    trailing: Option<WidgetNode>,
) -> WidgetNode {
    let m = theme.metrics;
    let mut children = children![];

    // The leading inset depends on whether there is a chevron: a round touch
    // target already carries its own optical padding, and adding the gutter to
    // it pushes the title a thumb's width off the grid every other screen.
    let leading = if back.is_some() { 6.0 } else { m.gutter };

    if let Some(on_back) = back {
        children.push(super::icon_button(
            theme,
            super::icons::back(),
            theme.colors.fg,
            "Back",
            on_back,
        ));
    }

    children.push(
        Flexible::expanded(1)
            .child(Text::new(title).style(theme.group()))
            .into(),
    );

    if let Some(trailing) = trailing {
        children.push(trailing);
    }

    // The top chrome deliberately mirrors the bottom navigation treatment:
    // a full-width fading band rather than a hard header strip. This keeps the
    // title readable when the scrolling body passes underneath it, without
    // pretending vieww has a true backdrop blur.
    let bg = theme.colors.window;
    Container::new()
        .height(top_bar_height(theme))
        .gradient(Gradient::vertical().with_stops(&[
            (0.0, bg),
            (0.42, bg.with_alpha(246)),
            (0.72, bg.with_alpha(190)),
            (1.0, bg.with_alpha(0)),
        ]))
        .padding(EdgeInsets::only(leading, 0.0, m.gutter, 0.0))
        .child(super::readable(
            theme,
            Flex::row()
                .cross_axis_alignment(CrossAxisAlignment::Center)
                .spacing(m.sp_1)
                .children(children),
        ))
        .into()
}

/// How tall the floating top bar is, band and all.
///
/// One function rather than a number in two files. The bar is a `Stack`
/// overlay, so every scrolling screen has to inset its content by exactly this
/// much — and when the two were written separately they drifted, which is how
/// Settings' first heading came to sit under the title.
pub fn top_bar_height(theme: &VavltTheme) -> f32 {
    theme.metrics.topbar + 28.0
}

/// The root's three tabs.
///
/// Built rather than taken from vieww's `BottomNavigation` for one reason: the
/// selected tab here is Adwaita's accent *text* colour, not a filled pill, and
/// the built-in control fills from `primary`. Everything else about it is the
/// same, and if that changes this function should go.
pub fn tab_bar(
    theme: &VavltTheme,
    current: Tab,
    on_selected: impl Fn(Tab) + 'static,
) -> WidgetNode {
    let theme = *theme;
    let on_selected = Rc::new(on_selected);

    let cells: Vec<WidgetNode> = Tab::ALL
        .iter()
        .map(|tab| {
            let tab = *tab;
            let selected = tab == current;
            let tint = if selected {
                theme.colors.accent_txt
            } else {
                theme.colors.dim
            };
            let pick = on_selected.clone();

            Flexible::expanded(1)
                .child(
                    Semantics::button(tab.title()).child(
                        Pressable::new(move |press| {
                            Container::new()
                                .color(theme.colors.hover.with_alpha((press * 46.0) as u8))
                                .alignment(Alignment::CENTER)
                                .child(
                                    Flex::column()
                                        .main_axis_size(MainAxisSize::Min)
                                        .cross_axis_alignment(CrossAxisAlignment::Center)
                                        .spacing(3.0)
                                        .children(children![
                                            Icon::new(tab_icon(tab)).size(21.0).color(tint),
                                            Text::new(tab.title().to_string()).style(if selected {
                                                theme.meta().color(tint)
                                            } else {
                                                theme.meta().color(tint).weight(FontWeight::Regular)
                                            }),
                                        ]),
                                )
                                .into()
                        })
                        .on_tap(move || pick(tab)),
                    ),
                )
                .into()
        })
        .collect();

    // A fade, not a slab: the mosaic scrolls *under* it and dissolves rather
    // than stopping at a hard edge.
    let bg = theme.colors.window;
    Container::new()
        .height(theme.metrics.tabbar + 10.0)
        .gradient(Gradient::vertical().with_stops(&[
            (0.0, bg.with_alpha(0)),
            (0.34, bg.with_alpha(235)),
            (0.62, bg),
            (1.0, bg),
        ]))
        .child(Flex::row().children(cells))
        .into()
}

fn tab_icon(tab: Tab) -> IconData {
    match tab {
        Tab::Vavlt => super::icons::vault(),
        Tab::Activity => super::icons::clock(),
        Tab::Settings => super::icons::sliders(),
    }
}

/// The commit control, pinned under the scrolling body.
///
/// It never scrolls: a button that can be scrolled past is a button that gets
/// missed, and every one of these is a step the user is stuck without.
pub fn action_bar(theme: &VavltTheme, children: Vec<WidgetNode>) -> WidgetNode {
    let m = theme.metrics;
    // Tall enough that the fade finishes before the button. A short one slices
    // a heading in half across the middle of it, which reads as clipping rather
    // than as depth.
    let bg = theme.colors.window;
    Container::new()
        .gradient(Gradient::vertical().with_stops(&[
            (0.0, bg.with_alpha(0)),
            (0.24, bg.with_alpha(184)),
            (0.46, bg),
            (1.0, bg),
        ]))
        .padding(EdgeInsets::only(m.gutter, 44.0, m.gutter, m.sp_2 + 4.0))
        // Capped and centred like the body above it, or the commit button
        // floats a hundred pixels away from the text it belongs to.
        .child(super::readable(
            theme,
            Flex::column()
                .cross_axis_alignment(CrossAxisAlignment::Stretch)
                .main_axis_size(MainAxisSize::Min)
                .spacing(m.sp_1)
                .children(children),
        ))
        .into()
}

/// Nothing here yet, and why that is fine.
pub fn status_page(
    theme: &VavltTheme,
    icon: IconData,
    title: &str,
    description: &str,
) -> WidgetNode {
    let m = theme.metrics;
    // The column *stretches* rather than centring its children. A `Text` with
    // `TextAlign::Center` centres inside the width it is offered, so being
    // handed the full width and centring itself is one centring; being shrunk
    // to its ink and then centred by a parent as well is two, and the second
    // one pushes it off the edge. See the note in `ui::button`.
    Center::new()
        .child(
            Padding::new(EdgeInsets::all(m.sp_4)).child(
                Constrained::new(Constraints::loose(Size::new(320.0, f32::INFINITY))).child(
                    Flex::column()
                        .main_axis_size(MainAxisSize::Min)
                        .cross_axis_alignment(CrossAxisAlignment::Stretch)
                        .spacing(m.sp_2)
                        .children(children![
                            Center::new().child(Icon::new(icon).size(40.0).color(theme.colors.dim)),
                            Text::new(title.to_string())
                                .style(theme.title())
                                .align(TextAlign::Center),
                            Text::new(description.to_string())
                                .style(theme.caption())
                                .align(TextAlign::Center),
                        ]),
                ),
            ),
        )
        .into()
}
