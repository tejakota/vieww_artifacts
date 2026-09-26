//! Tab 1 — the audit log.
//!
//! Append-only and hash-chained: each entry commits to the one before it, so a
//! removed entry is detectable rather than merely missing. Skips and failures
//! are logged as prominently as successes — a log that only records what went
//! well is marketing.
//!
//! A 3px rule beside each entry rather than a bordered card: the entries are a
//! sequence, and boxing each one made them read as unrelated notices stacked by
//! accident.

use std::rc::Rc;

use vieww::element::ScrollController;
use vieww::prelude::*;

use crate::model::{AuditKind, AuditRow};
use crate::state::VavltState;
use crate::theme::VavltTheme;
use crate::ui;

pub fn activity(
    theme: &VavltTheme,
    state: &Rc<VavltState>,
    scroll: &ScrollController,
) -> WidgetNode {
    let rows = state.audit_rows.get();

    if rows.is_empty() {
        return ui::status_page(
            theme,
            icons::chevron_down(),
            "Nothing logged yet",
            "Every grant, run, skip and failure lands here, chained to the entry before it. The \
             log never leaves this device on its own.",
        );
    }

    let m = theme.metrics;
    let mut body: Vec<WidgetNode> = rows
        .iter()
        .enumerate()
        .map(|(index, row)| entry(theme, row, index))
        .collect();

    body.push(ui::gap(m.sp_1));

    let export = state.clone();
    body.push(ui::button_with(
        theme,
        "Export the log",
        Some(ui::icons::export()),
        ui::ButtonKind::Regular,
        true,
        move || export.export_audit(),
    ));

    body.push(ui::caption(
        theme,
        "The log does not survive uninstalling the app. That is deliberate, and it is why \
         export exists.",
    ));

    ui::screen_body(theme, scroll, body)
}

fn entry(theme: &VavltTheme, row: &AuditRow, index: usize) -> WidgetNode {
    let m = theme.metrics;
    let rule = match row.kind {
        AuditKind::Fail => theme.colors.destructive_bg,
        AuditKind::Skip => theme.colors.warning_txt,
        AuditKind::Grant => theme.colors.accent_bg,
        _ => theme.colors.line_strong,
    };
    let title_ink = if row.kind == AuditKind::Fail {
        theme.colors.destructive_txt
    } else {
        theme.colors.fg
    };

    let mut lines = children![Flex::row()
        .spacing(m.sp_1)
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .children(children![
            Flexible::expanded(1)
                .child(Text::new(row.title.clone()).style(theme.body().color(title_ink).bold())),
            Text::new(row.time.clone()).style(theme.meta()),
        ]),];

    // Only when there is one. An empty detail used to render as a blank line,
    // which read as a missing sentence rather than as an entry that needs none.
    if !row.detail.is_empty() {
        lines.push(
            Text::new(row.detail.clone())
                .style(theme.secondary())
                .into(),
        );
    }

    // The chain link. Shown rather than tucked behind a developer setting — it
    // is what makes the log worth having.
    lines.push(
        Text::new(row.hash.clone())
            .style(theme.meta().weight(FontWeight::Regular))
            .into(),
    );

    let content = Padding::new(EdgeInsets::only(m.sp_2 + 3.0, m.sp_1, 0.0, m.sp_1)).child(
        Flex::column()
            .cross_axis_alignment(CrossAxisAlignment::Start)
            .main_axis_size(MainAxisSize::Min)
            .spacing(3.0)
            .children(lines),
    );

    // The rule is a `Positioned` in a `Stack`, not a fixed-width child of a
    // `Row` with `CrossAxisAlignment::Stretch`. Stretch needs a bounded cross
    // axis to stretch *to*, and this row sits in a `MainAxisSize::Min` column
    // where the height is whatever the content turns out to be — so the row
    // version laid out a 3x0 rectangle and drew nothing at all. Anchoring top
    // *and* bottom in a stack sized by its content is the arrangement that
    // actually takes the entry's height.
    let stack = Stack::new()
        .children(children![
            content,
            Positioned::new()
                .left(0.0)
                .top(m.sp_1)
                .bottom(m.sp_1)
                .width(3.0)
                .child(Container::new().color(rule).radius(2.0)),
        ]);

    // Zebra striping: the audit's verdict on this screen was "hard to scan —
    // the timestamps and tags run together", and a hash-chained log is the
    // one screen where the eye needs to walk rows without losing its place.
    // Even rows sit on `card`, odd rows on the bare window — a full surface
    // step rather than a wash, because the washes this theme carries land
    // within a couple of points of `card` in dark and are invisible in light
    // (where card and view are both white). The rule rides inside the plate,
    // clear of its corner.
    let plate = if index % 2 == 0 {
        theme.colors.card
    } else {
        Color::rgba(0, 0, 0, 0)
    };
    Padding::new(EdgeInsets::only(m.sp_1, 0.0, 0.0, 0.0)).child(
        Container::new()
            .color(plate)
            .radius(10.0)
            .padding(EdgeInsets::only(m.sp_1, m.sp_1, m.sp_1, m.sp_1 + 2.0))
            .child(stack),
    )
    .into()
}
