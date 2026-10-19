//! **The viewwstudio release keynote film.**
//!
//! Every frame of this film is rendered by **vieww itself** — headless,
//! through `FrameDriver` + `vieww-paint`'s `native` rasterizer, at
//! 1920×1080 · 60 fps, streamed raw into ffmpeg. There is no browser, no
//! compositor, no after-effects pass. The closing sting is a contract:
//! *the graphics in this film were rendered with vieww.*
//!
//! The film is vieww's first customer and viewwstudio's release: the hero is
//! the studio, and the studio is a vieww app, so the film and its subject
//! are the same technology twice over.
//!
//! ```console
//! cargo run --release -p keynote -- chapters   # the act table, derived
//! cargo run --release -p keynote -- verify     # the gates
//! cargo run --release -p keynote -- census     # pass 1 → manifest.txt
//! cargo run --release -p keynote -- master     # pass 2 → keynote.mp4 + sheets
//! cargo run --release -p keynote -- S09        # one scene, 16 frames, a sheet
//! ```

// `film`, `kit`, `solid` and `studio` are the film's toolkit: palettes,
// easings and drawing helpers written ahead of the scenes that use them, so
// some are unused at any given cut. They were `pub` inside a binary, which
// hid that from `dead_code` and tripped `unreachable_pub` instead (300
// errors in `checks.sh`'s clippy stage, Framework checks run 39254022842).
// Now `pub(crate)`, with the kit's spare parts allowed here rather than
// deleted from a film that is still being cut.
#[allow(dead_code)]
mod film;
#[allow(dead_code)]
mod kit;
mod master;
mod scenes;
#[allow(dead_code)]
mod solid;
#[allow(dead_code)]
mod studio;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mode = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "chapters".to_string());
    if mode == "publish" {
        let dest = std::env::args()
            .nth(2)
            .unwrap_or_else(|| "../keynote".to_string());
        return master::publish(std::path::Path::new(&dest));
    }
    master::run(&mode)
}
