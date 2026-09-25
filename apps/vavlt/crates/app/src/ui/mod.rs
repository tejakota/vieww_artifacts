//! Touch-sized Adwaita patterns, as functions that return a subtree.
//!
//! This is `android/ui/widgets.slint`. Functions rather than `Widget` impls
//! because none of them holds state — a `Card` is a `Container` with two
//! numbers on it, and wrapping that in a type would add a debug name and
//! nothing else. The three that *do* hold state — a press highlight, a
//! selection that slides, a tile — say so by taking a builder or by living in
//! their own module.
//!
//! The rules this vocabulary follows, which are the rules the phone screens
//! were rebuilt under after the first pass turned out to be a desktop app with
//! bigger text:
//!
//! * A border is a last resort. Separation comes from space, then from a
//!   surface change, and only then from a line.
//! * A row has a title. It gets a caption only if the title can be
//!   misunderstood without one.
//! * The photographs are the interface. Text explains what the pictures cannot
//!   show, and stops there.

mod bar;
mod billboard;
mod button;
mod card;
pub mod icons;
mod mosaic;
mod rail;
mod row;
mod segmented;
mod tile;

pub use bar::{action_bar, status_page, tab_bar, top_bar, top_bar_height};
pub use billboard::{billboard, brightest, eyebrow, figure, lede, meter, Height};
pub use button::{button, button_with, icon_button, ButtonKind};
pub use card::{card, chip, divider, tinted_card, ChipTone};
pub use mosaic::mosaic;
pub use rail::{rail, section};
pub use row::{row, switch_row, Row};
pub use segmented::tier_control;
pub use tile::tile;

use vieww::element::ScrollController;
use vieww::prelude::*;

use crate::theme::VavltTheme;

/// A screen's scrolling body, gutter and all.
///
/// Every screen in the app is this: a column of sections inside the gutter,
/// scrolling under a fixed top bar. Written once because the padding was
/// getting retyped on seven screens and had already diverged on two of them.
///
/// The controller is passed in rather than owned, because a scroll position
/// that lived here would reset every time the screen rebuilt — and it is the
/// controller, not the offset, that flings and clamps.
pub fn screen_body(
    theme: &VavltTheme,
    scroll: &ScrollController,
    children: Vec<WidgetNode>,
) -> WidgetNode {
    let m = theme.metrics;
    let column = Flex::column()
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .main_axis_size(MainAxisSize::Min)
        .spacing(m.sp_3)
        .children(children);

    scrolling(
        scroll,
        // The top inset clears the floating top bar, and it is not optional.
        //
        // The chrome is a `Stack` overlay rather than a column — which is what
        // stopped the tab bar resizing the page mid-navigation — so it occupies
        // no layout space. A screen that opens with a billboard has the
        // billboard holding its first line clear; Activity and Settings do not,
        // and their first entry and first heading were rendering *underneath*
        // the title.
        //
        // Taken from `top_bar_height` rather than restated, because the two
        // numbers were written separately and drifted by four points — and a
        // screen that then added an inset *of its own* on top of this one is
        // how Settings ended up with a hundred and fifty points of nothing
        // under its title.
        Padding::new(EdgeInsets::only(
            m.gutter,
            top_bar_height(theme) - m.sp_2,
            m.gutter,
            m.sp_4,
        ))
        .child(readable(theme, column)),
    )
}

/// A scrolling viewport that subscribes to its own offset.
///
/// # Why this is a widget and not three lines inline
///
/// `Scrollable` takes an offset, and reading that offset out of a
/// [`ScrollController`] is a **signal read**. A signal read is attributed to
/// whichever element is building at the time — so calling
/// `Scrollable::vertical(scroll.offset())` from inside a screen's own `build`
/// subscribes *the whole screen* to the scroll position, and every frame of a
/// scroll rebuilds every widget on it.
///
/// Measured, before this existed: twenty scroll steps on the plan screen
/// rebuilt **6,720 elements** — a hundred tiles, each cloning a `Photo` and
/// allocating six `String`s, sixty times a second, to draw them exactly where
/// they already were. That is the lag a phone reports.
///
/// Putting the read inside a widget of its own moves the subscription down to
/// this element. Its child arrives as an already-built `WidgetNode`, so a
/// rebuild here hands the same `Rc` back down and the element tree skips the
/// entire subtree — see `ElementTree::update`'s `ptr_eq` early-out.
///
/// `crates/app/tests/idle_cost.rs` is the guard.
#[derive(Debug)]
struct Scrolling {
    scroll: ScrollController,
    child: WidgetNode,
}

impl Widget for Scrolling {
    fn debug_name(&self) -> &'static str {
        "Scrolling"
    }

    fn kind(&self) -> WidgetKind<'_> {
        WidgetKind::Composed
    }

    fn build(&self, _ctx: &BuildContext) -> WidgetNode {
        Scrollable::vertical(self.scroll.offset())
            .on_drag(self.scroll.on_drag())
            .on_drag_end(self.scroll.on_drag_end())
            .on_extents(self.scroll.on_extents())
            .child(self.child.clone())
            .into()
    }
}

vieww::widget::widget_node_from!(Scrolling);

/// Wrap an already-built body in a scrolling viewport. See [`Scrolling`].
pub fn scrolling(scroll: &ScrollController, child: impl Into<WidgetNode>) -> WidgetNode {
    Scrolling {
        scroll: scroll.clone(),
        child: child.into(),
    }
    .into()
}

/// The longest a line of this app's prose is allowed to get.
///
/// Roughly 80 characters at the desktop body size. Above it a paragraph stops
/// being read and starts being scanned — and the settings screen, which is
/// almost entirely two-line rows, was running 990 pixels wide in a maximised
/// window. A phone never reaches this, so it costs that layout nothing.
pub const READING_WIDTH: f32 = 720.0;

/// Cap a column at [`READING_WIDTH`] and centre it.
///
/// Deliberately *not* applied to the photo grid, which is the one thing on
/// these screens that genuinely wants the whole window: more photographs is
/// better, longer sentences are not. The grid escapes by being a child of the
/// column rather than by opting out — a column capped at 720 inside a 1400
/// window still hands its grid 720, which is the compromise. Widening that is
/// a two-column desktop layout, and that is a design decision rather than a
/// number.
pub fn readable(theme: &VavltTheme, child: impl Into<WidgetNode>) -> WidgetNode {
    if theme.form == crate::theme::FormFactor::Phone {
        return child.into();
    }

    Align::new(Alignment::TOP_CENTER)
        .child(
            Constrained::new(Constraints::loose(vieww::foundation::Size::new(
                READING_WIDTH,
                f32::INFINITY,
            )))
            .child(child),
        )
        .into()
}

/// A screen whose content bleeds to the edges.
///
/// The counterpart to [`screen_body`]: that one puts a gutter on everything,
/// which is right for a column of prose and wrong for a billboard, a rail or a
/// mosaic — all three of which are supposed to touch the screen edge. Here the
/// children carry their own padding, so a section can choose.
pub fn bleed_body(
    theme: &VavltTheme,
    scroll: &ScrollController,
    children: Vec<WidgetNode>,
) -> WidgetNode {
    let _ = theme;
    scrolling(
        scroll,
        Flex::column()
            .cross_axis_alignment(CrossAxisAlignment::Stretch)
            .main_axis_size(MainAxisSize::Min)
            .children(children),
    )
}

/// Gutter-width padding, for a section that wants the column but sits in a
/// bleeding screen.
pub fn inset(theme: &VavltTheme, top: f32, child: impl Into<WidgetNode>) -> WidgetNode {
    let g = theme.metrics.gutter;
    Padding::new(EdgeInsets::only(g, top, g, 0.0))
        .child(child)
        .into()
}

/// A section heading. Sentence case, bold, no rule under it.
pub fn group_heading(theme: &VavltTheme, text: &str) -> WidgetNode {
    Text::new(text).style(theme.group()).into()
}

/// The dim explanatory line under something.
///
/// Dim by construction: this is the style that stops commentary competing with
/// what it is commenting on, and taking the colour as an argument is how that
/// stopped being true on the first pass.
pub fn caption(theme: &VavltTheme, text: impl Into<String>) -> WidgetNode {
    Text::new(text).style(theme.caption()).into()
}

/// Vertical air, where a gap is the separator rather than a line.
pub fn gap(height: f32) -> WidgetNode {
    SizedBox::height(height).into()
}
