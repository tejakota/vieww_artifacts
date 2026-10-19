//! scenes — the shot list, one module per scene.
//!
//! Each module exposes exactly one function, `frame(&Ctx) -> WidgetNode`: the
//! scene's whole picture at one instant, a pure function of film-time. No
//! scene reads the wall clock, keeps state between frames, or knows what
//! comes before or after it — continuity arrives through `ctx.spine`, and
//! every printed number through `ctx.probe`.

pub(crate) mod buffer;
pub(crate) mod s01_wait;
pub(crate) mod s02_cost;
pub(crate) mod s03_question;
pub(crate) mod s04_mark;
pub(crate) mod s05_opens;
pub(crate) mod s06_first_paint;
pub(crate) mod s07_compose;
pub(crate) mod s08_descent;
pub(crate) mod s09_machine;
pub(crate) mod s10_damage;
pub(crate) mod s11_mirror;
pub(crate) mod s12_world_tour;
pub(crate) mod s13_foundation;
pub(crate) mod s14_unfold;
pub(crate) mod s15_ledger;
pub(crate) mod s16_one_click;
pub(crate) mod s17_endcard;
pub(crate) mod s18_loop;
