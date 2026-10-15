//! Bake the facts a benchmark receipt needs about *this binary's* build into
//! it, because they cannot be recovered at run time.
//!
//! `rustc --version` at run time reports whatever compiler is installed when
//! the benchmark is run, which is not necessarily the one that built it — and
//! the flags (`target-cpu`, `prefer-dynamic`) are not visible to a running
//! program at all. A receipt that guesses either is a receipt nobody can
//! reproduce, which is the one thing a receipt is for.

use std::process::Command;

fn main() {
    let rustc = std::env::var("RUSTC").unwrap_or_else(|_| "rustc".to_owned());
    let version = Command::new(&rustc)
        .arg("--version")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map_or_else(|| "unknown".to_owned(), |s| s.trim().to_owned());
    println!("cargo:rustc-env=FIXTURES_RUSTC_VERSION={version}");

    for (key, name) in [
        ("PROFILE", "FIXTURES_PROFILE"),
        ("OPT_LEVEL", "FIXTURES_OPT_LEVEL"),
        ("TARGET", "FIXTURES_TARGET"),
    ] {
        let value = std::env::var(key).unwrap_or_else(|_| "unknown".to_owned());
        println!("cargo:rustc-env={name}={value}");
    }

    // `CARGO_ENCODED_RUSTFLAGS` separates flags with 0x1f; a space is
    // readable and unambiguous enough for a receipt.
    let flags = std::env::var("CARGO_ENCODED_RUSTFLAGS")
        .unwrap_or_default()
        .replace('\u{1f}', " ");
    println!("cargo:rustc-env=FIXTURES_RUSTFLAGS={flags}");
    println!("cargo:rerun-if-env-changed=CARGO_ENCODED_RUSTFLAGS");
    println!("cargo:rerun-if-changed=build.rs");
}
