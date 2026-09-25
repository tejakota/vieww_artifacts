//! The seven screens, one file each.
//!
//! Every one of them is a function of [`VavltState`](crate::VavltState) and
//! nothing else. None holds a value of its own — that is what makes the consent
//! flow checkable: there is exactly one place a grant can be set, and it logs.

mod activity;
mod outcome;
mod photo;
mod plan;
mod settings;
mod vault;
mod working;

pub use activity::activity;
pub use outcome::outcome;
pub use photo::photo;
pub use plan::plan;
pub use settings::settings;
pub use vault::vault;
pub use working::working;
