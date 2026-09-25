//! A list row, and the one row that carries a switch.
//!
//! Built as a struct with a builder rather than a nine-argument function: a row
//! has one required field and five optional ones, and the call sites that only
//! want a title should look like they only want a title.

use vieww::prelude::*;

use crate::theme::VavltTheme;

/// One line in a group.
#[derive(Debug, Clone, Default)]
pub struct Row {
    title: String,
    subtitle: String,
    trailing: String,
    icon: Option<IconData>,
    icon_tint: Option<Color>,
}

impl Row {
    #[must_use]
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            ..Self::default()
        }
    }

    /// The second line. Only worth adding when the title can be misunderstood
    /// without it — a caption under every row is what made the first phone pass
    /// unreadable.
    #[must_use]
    pub fn subtitle(mut self, subtitle: impl Into<String>) -> Self {
        self.subtitle = subtitle.into();
        self
    }

    /// A value on the right — a version, a size.
    #[must_use]
    pub fn trailing(mut self, trailing: impl Into<String>) -> Self {
        self.trailing = trailing.into();
        self
    }

    #[must_use]
    pub fn icon(mut self, icon: IconData, tint: Color) -> Self {
        self.icon = Some(icon);
        self.icon_tint = Some(tint);
        self
    }

    fn content(&self, theme: &VavltTheme) -> WidgetNode {
        let m = theme.metrics;
        let mut lead = children![];

        if let Some(icon) = self.icon.clone() {
            lead.push(
                // Nudged down by two, so a 20px glyph optically aligns with the
                // cap height of the line beside it rather than with its box.
                Padding::new(EdgeInsets::only(0.0, 2.0, 0.0, 0.0))
                    .child(
                        Icon::new(icon)
                            .size(20.0)
                            .color(self.icon_tint.unwrap_or(theme.colors.dim)),
                    )
                    .into(),
            );
        }

        let mut lines = children![Text::new(self.title.clone()).style(theme.body())];
        if !self.subtitle.is_empty() {
            lines.push(
                Text::new(self.subtitle.clone())
                    .style(theme.caption())
                    .into(),
            );
        }

        lead.push(
            Flexible::expanded(1)
                .child(
                    Flex::column()
                        .cross_axis_alignment(CrossAxisAlignment::Start)
                        .main_axis_size(MainAxisSize::Min)
                        .spacing(3.0)
                        .children(lines),
                )
                .into(),
        );

        if !self.trailing.is_empty() {
            lead.push(
                Text::new(self.trailing.clone())
                    .style(theme.caption())
                    .into(),
            );
        }

        Padding::new(EdgeInsets::symmetric(0.0, m.sp_2))
            .child(
                Flex::row()
                    .cross_axis_alignment(CrossAxisAlignment::Center)
                    .spacing(m.sp_2)
                    .children(lead),
            )
            .into()
    }
}

/// Render a row. `on_tap` makes it pressable and gives it a highlight; without
/// one it is static text and takes no touches.
pub fn row(theme: &VavltTheme, row: Row, on_tap: Option<Box<dyn Fn()>>) -> WidgetNode {
    let Some(on_tap) = on_tap else {
        return row.content(theme);
    };

    let theme = *theme;
    let radius = theme.metrics.r_row;
    Pressable::new(move |press| {
        Container::new()
            .color(theme.colors.hover.with_alpha((press * 46.0) as u8))
            .radius(radius)
            .child(row.content(&theme))
            .into()
    })
    .on_tap(on_tap)
    .into()
}

/// A row whose trailing element is a switch.
///
/// Separate from [`row`] because a switch is a *decision* and a trailing string
/// is a fact, and the app has exactly one switch that grants anything. Keeping
/// them apart means the grant cannot be added to a row by accident.
pub fn switch_row(
    theme: &VavltTheme,
    title: &str,
    subtitle: &str,
    checked: bool,
    destructive: bool,
    on_changed: impl Fn(bool) + 'static,
) -> WidgetNode {
    let m = theme.metrics;

    // A grant that authorises destruction reads red when on, not blue. vieww's
    // `Switch` fills from `primary`, so the swap is a one-role theme override
    // around this control only.
    let mut scheme = theme.colors.scheme();
    if destructive {
        scheme.primary = theme.colors.destructive_bg;
        scheme.on_primary = theme.colors.destructive_fg;
    }
    let data = ThemeData {
        colors: scheme,
        ..theme.framework()
    };

    Padding::new(EdgeInsets::symmetric(0.0, m.sp_2))
        .child(
            Flex::row()
                .cross_axis_alignment(CrossAxisAlignment::Center)
                .spacing(m.sp_2)
                .children(children![
                    Flexible::expanded(1).child(
                        Flex::column()
                            .cross_axis_alignment(CrossAxisAlignment::Start)
                            .main_axis_size(MainAxisSize::Min)
                            .spacing(3.0)
                            .children(children![
                                Text::new(title.to_string()).style(theme.body()),
                                Text::new(subtitle.to_string()).style(theme.caption()),
                            ])
                    ),
                    Theme::new(data)
                        .child(Switch::new(checked).on_changed(std::rc::Rc::new(on_changed))),
                ]),
        )
        .into()
}
