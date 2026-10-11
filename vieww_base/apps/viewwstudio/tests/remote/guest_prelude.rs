use vieww::prelude::*;
use std::any::Any;

/// Set by a tap, read by the next build: `take_pending` is what makes the
/// tree rebuild after a handler wrote it.
#[derive(Debug, Default)]
struct Tapped { on: bool, pending: bool }
impl ElementState for Tapped {
    fn as_any(&self) -> &dyn Any { self }
    fn as_any_mut(&mut self) -> &mut dyn Any { self }
    fn take_pending(&mut self) -> bool { std::mem::take(&mut self.pending) }
}

fn tapped(ctx: &BuildContext) -> bool { ctx.state::<Tapped, _>(|s| s.on).unwrap_or(false) }

fn on_tap(ctx: &BuildContext) -> impl Fn(vieww::foundation::TapDetails) + 'static {
    let handle = ctx.state_handle();
    move |_| {
        if let Some(handle) = &handle {
            if let Some(t) = handle.borrow_mut().as_any_mut().downcast_mut::<Tapped>() {
                t.on = true;
                t.pending = true;
            }
        }
    }
}
