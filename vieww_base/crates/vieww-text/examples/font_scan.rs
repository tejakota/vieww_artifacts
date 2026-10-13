//! Time the system font scan `FrameDriver::use_system_fonts` runs before a
//! window's first frame.
//!
//! ```console
//! cargo run --release -p vieww-text --example font_scan
//! ```
//!
//! Run it twice: the first run after the disk cache was emptied (a reboot, a
//! long build, or `sync; echo 3 | sudo tee /proc/sys/vm/drop_caches`) is the
//! cold-start cost a freshly launched application pays; the second is the warm
//! one. `tests/wait_loop.rs`' idle scenario in `vieww-platform-winit` timed out
//! inside this scan on a laptop right after an 11-minute build.

use std::time::Instant;

use cosmic_text::fontdb;

fn main() {
    let start = Instant::now();
    let mut db = fontdb::Database::new();
    db.load_system_fonts();
    let scan = start.elapsed();
    let faces = db.len();
    let files: std::collections::HashSet<_> = db
        .faces()
        .filter_map(|f| match &f.source {
            fontdb::Source::File(p) | fontdb::Source::SharedFile(p, _) => Some(p.clone()),
            fontdb::Source::Binary(_) => None,
        })
        .collect();
    println!(
        "fontdb scan:            {scan:>10.2?}  ({faces} faces in {} files)",
        files.len()
    );

    let start = Instant::now();
    let _store = vieww_text::FontStore::with_system_fallback();
    println!(
        "FontStore (what vieww does before the first frame): {:.2?}",
        start.elapsed()
    );
}
