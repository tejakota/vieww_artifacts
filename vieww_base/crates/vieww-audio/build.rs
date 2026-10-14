//! Declares the `vieww_check_all` cfg: `RUSTFLAGS="--cfg vieww_check_all"
//! cargo check -p vieww-audio` type-checks every platform's audio backend
//! (Core Audio, WASAPI, AAudio, Web Audio) on any host. `check` does not
//! link, so the platform libraries need not exist.
fn main() {
    println!("cargo::rustc-check-cfg=cfg(vieww_check_all)");
}
