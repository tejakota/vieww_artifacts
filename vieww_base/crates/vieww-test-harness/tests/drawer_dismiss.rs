//! A drawer's scrim, tapped where a finger (or Maestro) would tap it.
//!
//! Found by the Android emulator job on 2026-10-14: the Maestro flow opened the
//! demo's drawer, checked its items, tapped at 95% of the width — the scrim, by
//! `Drawer::width_within`'s own arithmetic — and the drawer stayed open.
//!
//! The cause was in `Drawer::build`: its `FocusTrap` wrapped the
//! `PositionedDirectional` instead of sitting inside it, so the `Stack` treated
//! the panel as an ordinary child, expanded it to the full surface, and the
//! panel (opaque to hits) sat over the whole scrim.

use std::cell::Cell;
use std::rc::Rc;

use vieww_foundation::{Offset, Size};
use vieww_test_harness::TestHarness;
use vieww_widget::{children, Drawer, Stack, StackFit, Text};

fn drawer_on(surface: Size) -> (TestHarness, Rc<Cell<u32>>) {
    let dismissed = Rc::new(Cell::new(0));
    let count = Rc::clone(&dismissed);
    let mut harness = TestHarness::new(surface);
    harness.mount(
        Stack::new()
            .fit(StackFit::Expand)
            .children(children![Drawer::new()
                .on_dismiss(move || count.set(count.get() + 1))
                .child(Text::new("Inbox"))]),
    );
    harness.tick_and_settle(16);
    (harness, dismissed)
}

#[test]
fn a_tap_on_the_scrim_dismisses_the_drawer() {
    // Pixel 6: 411 x 914 logical. The panel is 304 wide, so 95% across (390) is
    // scrim.
    let (mut harness, dismissed) = drawer_on(Size::new(411.0, 914.0));
    harness.tap(Offset::new(390.0, 457.0));
    harness.tick_and_settle(16);
    assert_eq!(dismissed.get(), 1, "a tap on the scrim must dismiss");
}

#[test]
fn a_tap_on_the_panel_does_not_dismiss() {
    let (mut harness, dismissed) = drawer_on(Size::new(411.0, 914.0));
    harness.tap(Offset::new(100.0, 457.0));
    harness.tick_and_settle(16);
    assert_eq!(
        dismissed.get(),
        0,
        "a tap inside the panel is not a dismiss"
    );
}
