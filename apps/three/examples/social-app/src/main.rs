//! Desktop entry point for the `3` social application shell.
//!
//! The app logic lives in [`three_social_app::run_desktop`]; this binary is a
//! thin wrapper around it. The Android entry point (`android_main`) lives in
//! `src/lib.rs` because cargo-apk builds the crate's `cdylib` target and the
//! `#[no_mangle]` symbol must be exported from the library.

fn main() -> Result<(), Box<dyn std::error::Error>> {
    three_social_app::run_desktop()
}
