//! scenes — the shot list, one module per scene.
//!
//! Each module exposes exactly one function, `frame(&Ctx) -> WidgetNode`: the
//! scene's whole picture at one instant, a pure function of film-time. No
//! scene reads the wall clock, keeps state between frames, or knows what
//! comes before or after it — continuity arrives through `ctx.spine`, and
//! every printed number through `ctx.probe`.

pub mod buffer;
pub mod s01_wait;
pub mod s02_cost;
pub mod s03_question;
pub mod s04_mark;
pub mod s05_opens;
pub mod s06_first_paint;
pub mod s07_compose;
pub mod s08_descent;
pub mod s09_machine;
pub mod s10_damage;
pub mod s11_mirror;
pub mod s12_world_tour;
pub mod s13_foundation;
pub mod s14_unfold;
pub mod s15_ledger;
pub mod s16_one_click;
pub mod s17_endcard;
pub mod s18_loop;
