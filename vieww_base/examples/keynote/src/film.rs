//! film — the assembly: the act table, the scene registry, the session
//! spine, and the census probe every on-screen number comes from.
//!
//! **Durations are the only numbers anyone writes here.** Frame counts, the
//! runtime, the act sums and every mount frame are *computed* from them —
//! the film's length is a consequence of its shot list, never a constant
//! somebody typed. `verify` asserts the sums close.
//!
//! **The session spine lives outside every scene tree.** The witness
//! counter, the elapsed chip and the session line are derived from absolute
//! film-time by [`ladder_at`] and read by scenes that mount and unmount
//! around them — which is why the counter survives every cut in Act II and
//! why S14 can pull the signal apart from the three trees and have it be
//! literally true of the film's own construction.

use std::path::{Path as FsPath, PathBuf};

use vieww_widget::WidgetNode;

use crate::kit::{FPS, H, W};
use crate::scenes;

// ── The scene definition ────────────────────────────────────────────────────

pub struct SceneDef {
    /// The scene's stable id — `S07`. Sheets and previews are named by it.
    pub id: &'static str,
    /// The scene's short name, used in file names and the chapter list.
    pub name: &'static str,
    /// The act it belongs to.
    pub act: Act,
    /// Film-seconds this scene spans. **The only number typed by a human.**
    pub seconds: f32,
    /// One frame of the scene, as a pure function of its context.
    pub build: fn(&Ctx) -> WidgetNode,
}

impl SceneDef {
    /// Frames this scene contributes — computed from its duration.
    #[must_use]
    pub fn frames(&self) -> usize {
        (self.seconds * FPS).round() as usize
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Act {
    /// 起 — the wait: the world the film argues against.
    Ki,
    /// 承 — the studio: one session, the hero at work.
    Sho,
    /// 転 — the twists: what the session was actually made of.
    Ten,
    /// 結 — the ledger: the receipts, the foundation, the close.
    Ketsu,
}

impl Act {
    #[must_use]
    pub const fn roman(self) -> &'static str {
        match self {
            Self::Ki => "ACT I",
            Self::Sho => "ACT II",
            Self::Ten => "ACT III",
            Self::Ketsu => "ACT IV",
        }
    }
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Ki => "THE WAIT",
            Self::Sho => "THE STUDIO",
            Self::Ten => "THE TWISTS",
            Self::Ketsu => "THE LEDGER",
        }
    }
    #[must_use]
    pub const fn kanji(self) -> &'static str {
        match self {
            Self::Ki => "起",
            Self::Sho => "承",
            Self::Ten => "転",
            Self::Ketsu => "結",
        }
    }
}

// ── The context handed to every frame ───────────────────────────────────────

pub struct Ctx<'a> {
    /// Progress through the scene, `0..=1`.
    pub t: f32,
    /// Seconds since the cut into this scene.
    pub sec: f32,
    /// Absolute film-time in seconds — the global clock.
    pub abs: f32,
    /// Absolute frame index — the only thing that advances, and the seed
    /// source for anything that shimmers.
    pub frame: u32,
    /// The session spine at `abs`: the witness counter, the elapsed chip,
    /// the nodes the session line has visited.
    pub spine: Spine,
    /// The census — every number the film prints comes from here.
    pub probe: &'a Probe,
}

impl Ctx<'_> {
    /// Progress across a window given in *scene seconds* — the spelling
    /// scene code uses, so a beat can be moved without recomputing `t`.
    #[must_use]
    pub fn beat(&self, from: f32, to: f32) -> f32 {
        crate::kit::seg(self.sec, from, to)
    }
}

// ── The session spine ───────────────────────────────────────────────────────

/// The continuity that outlives every cut.
#[derive(Clone, Copy, Default)]
pub struct Spine {
    /// The witness counter — one increment per human touch, 0..=7, never
    /// reset. The film's proof that the session never restarted.
    pub witness: u32,
    /// Seconds into the session (it starts when the studio opens, not when
    /// the film does).
    pub elapsed: f32,
    /// How far the session line has been drawn, `0..=1`.
    pub line: f32,
}

/// Where the session starts, in absolute film-time: the cut into S05.
#[must_use]
pub fn session_start() -> f32 {
    let all = scenes();
    all.iter()
        .take_while(|s| s.id != "S05")
        .map(|s| s.seconds)
        .sum()
}

/// Where the session's last touch lands: the end of S12's world tour.
#[must_use]
pub fn session_end() -> f32 {
    let all = scenes();
    let mut acc = 0.0;
    for s in &all {
        acc += s.seconds;
        if s.id == "S12" {
            break;
        }
    }
    acc
}

/// The seven touches, as absolute film-times, each derived from its own
/// scene's position in the table — move a scene and the ladder moves with
/// it. Nothing here is a wall-clock constant.
#[must_use]
pub fn touches() -> Vec<(u32, f32, &'static str)> {
    let at = |id: &str, frac: f32| -> f32 {
        let all = scenes();
        let mut acc = 0.0;
        for s in &all {
            if s.id == id {
                return acc + s.seconds * frac;
            }
            acc += s.seconds;
        }
        acc
    };
    vec![
        (1, at("S06", 0.62), "born"),
        (2, at("S07", 0.34), "composed"),
        (3, at("S07", 0.86), "repaired"),
        (4, at("S08", 0.74), "carried"),
        (5, at("S10", 0.46), "damaged"),
        (6, at("S12", 0.40), "desktop"),
        (7, at("S12", 0.74), "the phone"),
    ]
}

/// The spine at an absolute film-time.
#[must_use]
pub fn ladder_at(abs: f32) -> Spine {
    let t = touches();
    let witness = t.iter().filter(|(_, when, _)| abs >= *when).count() as u32;
    let start = session_start();
    let end = session_end();
    Spine {
        witness,
        elapsed: (abs - start).max(0.0),
        line: crate::kit::clamp01((abs - start) / (end - start).max(1e-3)),
    }
}

/// The label of the most recent touch — the counter's caption.
#[must_use]
pub fn touch_label(witness: u32) -> &'static str {
    touches()
        .iter()
        .find(|(n, _, _)| *n == witness)
        .map_or("", |(_, _, l)| *l)
}

// ── The act table ───────────────────────────────────────────────────────────

/// **The film.** Sixteen scenes, four movements. Durations are the score;
/// everything else is computed from them.
#[must_use]
pub fn scenes() -> Vec<SceneDef> {
    use Act::{Ketsu, Ki, Sho, Ten};
    vec![
        // 起 · the wait — 60s
        SceneDef { id: "S01", name: "wait",      act: Ki,    seconds: 15.0, build: scenes::s01_wait::frame },
        SceneDef { id: "S02", name: "cost",      act: Ki,    seconds: 15.0, build: scenes::s02_cost::frame },
        SceneDef { id: "S03", name: "question",  act: Ki,    seconds: 12.0, build: scenes::s03_question::frame },
        SceneDef { id: "S04", name: "mark",      act: Ki,    seconds: 18.0, build: scenes::s04_mark::frame },
        // 承 · the studio — 120s
        SceneDef { id: "S05", name: "opens",     act: Sho,   seconds: 18.0, build: scenes::s05_opens::frame },
        SceneDef { id: "S06", name: "firstpaint",act: Sho,   seconds: 24.0, build: scenes::s06_first_paint::frame },
        SceneDef { id: "S07", name: "compose",   act: Sho,   seconds: 24.0, build: scenes::s07_compose::frame },
        SceneDef { id: "S08", name: "descent",   act: Sho,   seconds: 20.0, build: scenes::s08_descent::frame },
        SceneDef { id: "S09", name: "machine",   act: Sho,   seconds: 24.0, build: scenes::s09_machine::frame },
        SceneDef { id: "S10", name: "damage",    act: Sho,   seconds: 10.0, build: scenes::s10_damage::frame },
        // 転 · the twists — 80s
        SceneDef { id: "S11", name: "mirror",    act: Ten,   seconds: 16.0, build: scenes::s11_mirror::frame },
        SceneDef { id: "S12", name: "worldtour", act: Ten,   seconds: 24.0, build: scenes::s12_world_tour::frame },
        SceneDef { id: "S13", name: "foundation",act: Ten,   seconds: 22.0, build: scenes::s13_foundation::frame },
        SceneDef { id: "S14", name: "unfold",    act: Ten,   seconds: 18.0, build: scenes::s14_unfold::frame },
        // 結 · the ledger — 70s
        SceneDef { id: "S15", name: "ledger",    act: Ketsu, seconds: 20.0, build: scenes::s15_ledger::frame },
        SceneDef { id: "S16", name: "oneclick",  act: Ketsu, seconds: 16.0, build: scenes::s16_one_click::frame },
        SceneDef { id: "S17", name: "endcard",   act: Ketsu, seconds: 24.0, build: scenes::s17_endcard::frame },
        SceneDef { id: "S18", name: "loop",      act: Ketsu, seconds: 10.0, build: scenes::s18_loop::frame },
    ]
}

/// Total frames — derived, never typed.
#[must_use]
pub fn total_frames() -> usize {
    scenes().iter().map(SceneDef::frames).sum()
}

/// Total runtime in seconds — derived.
#[must_use]
pub fn total_seconds() -> f32 {
    scenes().iter().map(|s| s.seconds).sum()
}

/// Seconds an act spans — derived.
#[must_use]
pub fn act_seconds(act: Act) -> f32 {
    scenes()
        .iter()
        .filter(|s| s.act == act)
        .map(|s| s.seconds)
        .sum()
}

/// Where a scene starts, in absolute film-seconds.
#[must_use]
pub fn scene_start(id: &str) -> f32 {
    let mut acc = 0.0;
    for s in scenes() {
        if s.id == id {
            return acc;
        }
        acc += s.seconds;
    }
    acc
}

// ── The census probe ────────────────────────────────────────────────────────

/// Everything the film measured about itself in pass 1. The end card's bars
/// and the "alive in N seconds" caption read this and nothing else; a
/// missing manifest is a hard failure of the master render, because the
/// alternative is a number somebody typed.
#[derive(Clone, Debug, Default)]
pub struct Probe {
    pub frames: u64,
    pub shapes: u64,
    pub glyph_runs: u64,
    pub glyphs: u64,
    pub layers: u64,
    pub filtered: u64,
    pub strokes: u64,
    pub shadows: u64,
    pub images: u64,
    /// The measured edit→visible latency this bench pays for S06's first
    /// paint, at master resolution — the caption's N.
    pub alive_seconds: f32,
    /// Median whole-frame raster time at 1920×1080, this bench.
    pub frame_ms: f32,
    /// Worst sampled frame — the honest tail beside the median.
    pub worst_ms: f32,
    /// The bench's identity: determinism is per-bench, so the bench is
    /// named wherever the numbers are.
    pub bench: String,
    /// Crates in the vieww workspace — counted from `Cargo.toml`, not
    /// remembered.
    pub crates: u32,
}

impl Probe {
    /// The bench line. Glyph determinism follows the host's fonts, so the
    /// font count is part of the identity.
    #[must_use]
    pub fn bench_identity(fonts: u32) -> String {
        let arch = std::env::consts::ARCH;
        let os = std::env::consts::OS;
        format!("{os} · {arch} · rustc {} · {fonts} fonts", rustc_line())
    }

    pub fn save(&self, path: &FsPath) -> std::io::Result<()> {
        let body = format!(
            "frames={}\nshapes={}\nglyph_runs={}\nglyphs={}\nlayers={}\nfiltered_layers={}\nstrokes={}\nshadows={}\nimages={}\nalive_seconds={:.4}\nframe_ms={:.2}\nworst_ms={:.2}\ncrates={}\nbench={}\n",
            self.frames,
            self.shapes,
            self.glyph_runs,
            self.glyphs,
            self.layers,
            self.filtered,
            self.strokes,
            self.shadows,
            self.images,
            self.alive_seconds,
            self.frame_ms,
            self.worst_ms,
            self.crates,
            self.bench,
        );
        std::fs::write(path, body)
    }

    #[must_use]
    pub fn load(path: &FsPath) -> Option<Self> {
        let body = std::fs::read_to_string(path).ok()?;
        let mut p = Self::default();
        for line in body.lines() {
            let (k, v) = line.split_once('=')?;
            match k {
                "frames" => p.frames = v.parse().unwrap_or(0),
                "shapes" => p.shapes = v.parse().unwrap_or(0),
                "glyph_runs" => p.glyph_runs = v.parse().unwrap_or(0),
                "glyphs" => p.glyphs = v.parse().unwrap_or(0),
                "layers" => p.layers = v.parse().unwrap_or(0),
                "filtered_layers" => p.filtered = v.parse().unwrap_or(0),
                "strokes" => p.strokes = v.parse().unwrap_or(0),
                "shadows" => p.shadows = v.parse().unwrap_or(0),
                "images" => p.images = v.parse().unwrap_or(0),
                "alive_seconds" => p.alive_seconds = v.parse().unwrap_or(0.0),
                "frame_ms" => p.frame_ms = v.parse().unwrap_or(0.0),
                "worst_ms" => p.worst_ms = v.parse().unwrap_or(0.0),
                "crates" => p.crates = v.parse().unwrap_or(0),
                "bench" => p.bench = v.to_string(),
                _ => {}
            }
        }
        Some(p)
    }

    /// Has the census actually run? A zeroed probe must never reach a
    /// caption — rule 5 of the ledger, enforced at the one place it can be.
    #[must_use]
    pub const fn is_measured(&self) -> bool {
        self.frames > 0 && self.shapes > 0
    }

    /// `alive in N seconds`, formatted the way S06 prints it.
    #[must_use]
    pub fn alive_text(&self) -> String {
        if self.alive_seconds <= 0.0 {
            return "alive in — s".to_string();
        }
        format!("{:.3}", self.alive_seconds)
    }
}

/// The compiler that built this binary, read from the environment the build
/// script sees — no shelling out at render time.
fn rustc_line() -> String {
    option_env!("CARGO_PKG_RUST_VERSION")
        .map_or_else(|| "unknown".to_string(), |v| format!("≥{v}"))
}

/// Count the workspace's crates by reading its manifest — the end card's
/// "36 crates" is counted by the code that draws it.
#[must_use]
pub fn count_crates() -> u32 {
    let roots = [
        PathBuf::from("Cargo.toml"),
        PathBuf::from("../Cargo.toml"),
        PathBuf::from("../../Cargo.toml"),
    ];
    for root in roots {
        if let Ok(body) = std::fs::read_to_string(&root) {
            if body.contains("[workspace]") {
                let n = body
                    .lines()
                    .filter(|l| {
                        let l = l.trim();
                        l.starts_with("\"crates/") && l.ends_with("\",")
                    })
                    .count();
                if n > 0 {
                    return n as u32;
                }
            }
        }
    }
    0
}

// ── verify — the gates ──────────────────────────────────────────────────────

/// The gates the ledger asks for, run as a command. Failure prints the
/// finding; the film is not allowed to render around it.
pub fn verify() -> Result<(), Box<dyn std::error::Error>> {
    let all = scenes();
    let mut findings: Vec<String> = Vec::new();

    println!("keynote · verify");
    println!("  {} scenes · {} frames · {:.0}s runtime (all derived)", all.len(), total_frames(), total_seconds());

    // 1 — the act sums close against the runtime.
    let sums: Vec<(Act, f32)> = [Act::Ki, Act::Sho, Act::Ten, Act::Ketsu]
        .into_iter()
        .map(|a| (a, act_seconds(a)))
        .collect();
    let total: f32 = sums.iter().map(|(_, s)| *s).sum();
    for (a, s) in &sums {
        println!("    {} {} · {:.0}s", a.roman(), a.name(), s);
    }
    if (total - total_seconds()).abs() > 0.01 {
        findings.push(format!("act sums {total} ≠ runtime {}", total_seconds()));
    }

    // 2 — frames are consistent with the runtime at the master's rate.
    let expect = (total_seconds() * FPS).round() as usize;
    if total_frames() != expect {
        findings.push(format!("frames {} ≠ {:.0}s × {FPS}", total_frames(), total_seconds()));
    }

    // 3 — the mount-frame rule, stated once so it cannot surprise anyone:
    // a scene of n frames is sampled at i = 0..n, so its last sampled
    // film-time is (n-1)/FPS, not n/FPS. Scene starts must tile exactly.
    let mut acc = 0.0f32;
    for s in &all {
        let start = scene_start(s.id);
        if (start - acc).abs() > 1e-3 {
            findings.push(format!("{} starts at {start} but the table tiles to {acc}", s.id));
        }
        acc += s.seconds;
    }

    // 4 — the ladder is monotone, lands inside its scenes, and reaches 7.
    let t = touches();
    for w in t.windows(2) {
        if w[1].1 <= w[0].1 {
            findings.push(format!("witness {} lands at or before {}", w[1].0, w[0].0));
        }
    }
    if t.len() != 7 || t.last().map(|x| x.0) != Some(7) {
        findings.push("the witness ladder does not reach 7".to_string());
    }
    if ladder_at(total_seconds()).witness != 7 {
        findings.push("the counter does not hold 7 to the end".to_string());
    }

    // 5 — every scene has a positive duration and a unique id.
    let mut ids: Vec<&str> = all.iter().map(|s| s.id).collect();
    ids.sort_unstable();
    ids.dedup();
    if ids.len() != all.len() {
        findings.push("duplicate scene ids".to_string());
    }
    if let Some(s) = all.iter().find(|s| s.seconds <= 0.0) {
        findings.push(format!("{} has a non-positive duration", s.id));
    }

    // 6 — the master resolution is the one the sheets and cutdowns assume.
    if (W - 1920.0).abs() > 0.5 || (H - 1080.0).abs() > 0.5 {
        findings.push("the master is not 1920×1080".to_string());
    }

    // 7 — the crate count is countable. The end card would otherwise print
    // a number nobody measured.
    let crates = count_crates();
    if crates == 0 {
        findings.push("the workspace crate count could not be read — run from vieww_base".to_string());
    } else {
        println!("    workspace crates counted: {crates}");
    }

    if findings.is_empty() {
        println!("  all gates green");
        Ok(())
    } else {
        for f in &findings {
            println!("  FINDING: {f}");
        }
        Err(format!("{} finding(s)", findings.len()).into())
    }
}
