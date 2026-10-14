//! The bench identity's compiler version, measured rather than typed.
//!
//! `Probe::bench_identity` used to carry a string literal — the
//! toolchain `rust-toolchain.toml` pins. That is the version the
//! repository *intends* to build with, not the one that rendered the
//! frames in front of you, and the two differ the moment anyone builds
//! with anything else. In a film whose whole argument is that every
//! number on screen is a receipt, the one number describing the render
//! itself cannot be a literal. So it is asked of the compiler that is
//! actually compiling this, at the moment it compiles it.

fn main() {
    let rustc = std::env::var("RUSTC").unwrap_or_else(|_| "rustc".to_string());
    let version = std::process::Command::new(rustc)
        .arg("--version")
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .and_then(|s| s.split_whitespace().nth(1).map(str::to_string))
        .unwrap_or_else(|| "unknown".to_string());
    println!("cargo:rustc-env=FILM_RUSTC_VERSION={version}");
    println!("cargo:rerun-if-changed=build.rs");
}
