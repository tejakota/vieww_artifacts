//! The Wright app's library: the CLI commands and — behind the `ui`
//! feature — the vieww shell, as a mountable tree.
//!
//! The binary in `main.rs` is a thin dispatch over this library. The
//! library exists for the one consumer that needs the shell *as a
//! tree*: `examples/wright_film`, the product film, which mounts the
//! real window on its own `FrameDriver` and drives it through a
//! frame-indexed script — the same pattern `viewwstudio` set for
//! `film_lab`'s studio act. Nothing here is a second implementation;
//! it is the same modules the binary runs, addressable by path.
//!
//! Headless builds (no `ui` feature) link the library too, and it
//! stays exactly as light: `cmd` is std-only like the rest.

pub mod cmd;

#[cfg(feature = "ui")]
pub mod ui;

/// One line about the UI for `wright doctor`.
pub fn ui_status() -> String {
    #[cfg(feature = "ui")]
    {
        "vieww shell built in (`wright ui`)".into()
    }
    #[cfg(not(feature = "ui"))]
    {
        "not built in (build with --features ui)".into()
    }
}
