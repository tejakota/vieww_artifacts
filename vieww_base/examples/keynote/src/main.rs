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

mod film;
mod kit;
mod master;
mod scenes;
mod solid;
mod studio;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mode = std::env::args().nth(1).unwrap_or_else(|| "chapters".to_string());
    if mode == "publish" {
        let dest = std::env::args()
            .nth(2)
            .unwrap_or_else(|| "../keynote".to_string());
        return master::publish(std::path::Path::new(&dest));
    }
    master::run(&mode)
}
