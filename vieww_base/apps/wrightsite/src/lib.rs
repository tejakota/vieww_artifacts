//! Wright's product page — a vieww widget tree.
//!
//! # What this is
//!
//! The page that sells Wright, built the way `apps/viewwsite` builds the
//! studio's page: as a vieww widget tree — the same widget catalogue, the
//! same layout algorithm, the same rasteriser — with a DOM half
//! (`src/dom.rs`) that serves in the browser and a canvas half (this file)
//! that rasterises on the host through `examples/render.rs`, so a layout
//! change is reviewed in a second rather than in a two-minute wasm build.
//!
//! # The argument it makes
//!
//! Wright's window *is* a vieww application — the app has exactly one
//! external dependency, vieww, by path — so the page selling it is written
//! in the same thing. The live demo below is a real widget tree with real
//! signals and real animations, mounted on a `<canvas>`: the product, doing
//! what the product does, in the frame of the page that says it does.
//!
//! # The palette
//!
//! Wright's own, from `apps/wright/app/src/ui/theme.rs`: VS Code's "Dark
//! Modern", by role. Restated here rather than imported because the app is
//! a binary crate this page must not depend on — and because a page that
//! drifted from the product it sells would be advertising a theme nobody
//! ships. The mark is the app's too: one stroke, a W (`ui/logo.rs`), drawn
//! as a path rather than loaded as an image, on both halves of the page.

pub mod dom;

use std::time::Duration;

use vieww::animation::Tickers;
use vieww::element::{Animation, Signal};
use vieww::foundation::{Axis, Cursor, FontFamily, StrokeStyle};
use vieww::prelude::*;
use vieww::ImageData;
use vieww::{FrameDriver, ScrollController, ScrollPhysics};

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::wasm_bindgen;

// ─── the palette ──────────────────────────────────────────────────────────
//
// `apps/wright/app/src/ui/theme.rs`'s `DARK` pal, value for value. The names
// are the app's own: `ground` is the editor background, `surface` is a card,
// `accent_ink` is the accent used as text (VS Code's blue is too deep to read
// at body size on this ground — the app lifts it to `4DAAFC` for text, and so
// does this page), and `good`/`wait`/`bad` are the run states the window
// paints a firing's stages with.

/// `DARK.ground` — `1F1F1F`: what everything sits on.
const GROUND: Color = Color::rgb(0x1F, 0x1F, 0x1F);
/// `DARK.surface` — cards, menus, the composer.
const SURFACE: Color = Color::rgb(0x25, 0x25, 0x26);
/// `DARK.surface2` — inputs, code, quiet fills inside a card.
const SURFACE_2: Color = Color::rgb(0x2D, 0x2D, 0x2D);
/// `DARK.line` — a hairline inside the chrome, between two surfaces.
const LINE: Color = Color::rgb(0x2B, 0x2B, 0x2B);
/// `DARK.line_strong` — a hairline on the ground, where `LINE` would vanish.
const LINE_STRONG: Color = Color::rgb(0x3C, 0x3C, 0x3C);
/// `DARK.ink` — the window's own body text, which reads as page ink here.
const INK: Color = Color::rgb(0xE8, 0xE8, 0xE8);
/// `DARK.muted` — the second text colour.
const INK_2: Color = Color::rgb(0x9D, 0x9D, 0x9D);
/// Between muted and the line: captions, file names, small print.
const INK_3: Color = Color::rgb(0x6E, 0x6E, 0x6E);

/// `DARK.accent` — VS Code's blue, as a button is filled with.
const ACCENT: Color = Color::rgb(0x00, 0x78, 0xD4);
/// `DARK.accent_ink` — the accent as text on the dark ground.
const ACCENT_INK: Color = Color::rgb(0x4D, 0xAA, 0xFC);
/// `DARK.accent_soft` / `DARK.bubble` — an accent wash, and the user's bubble.
const WASH: Color = Color::rgb(0x04, 0x39, 0x5E);
/// `DARK.good` — a stage that finished; the live dot.
const GOOD: Color = Color::rgb(0x89, 0xD1, 0x85);
/// `DARK.wait` — a stage still working.
const WAIT: Color = Color::rgb(0xCC, 0xA7, 0x00);
/// `DARK.bad` — a refusal.
const BAD: Color = Color::rgb(0xF1, 0x4C, 0x4C);
/// `DARK.status` — the bar along the bottom of the window.
const STATUS: Color = Color::rgb(0x18, 0x18, 0x18);

/// The widest the *page* is allowed to get. `viewwsite`'s `COLUMN`: a product
/// page is not an essay, but a paragraph still gets [`PROSE`].
const COLUMN: f32 = 1280.0;
/// The widest a *paragraph* is allowed to get.
const PROSE: f32 = 672.0;
/// How wide a section heading may run.
const HEADING_MEASURE: f32 = 768.0;
/// `py-20 md:py-28` — the vertical padding every section carries.
const SECTION_Y: f32 = 112.0;
/// The same, narrow.
const SECTION_Y_NARROW: f32 = 80.0;
/// The narrowest a two-column band stays two columns.
const BREAKPOINT: f32 = 720.0;
/// The page's own gutter.
const GUTTER: f32 = 28.0;

// ─── the release assets ───────────────────────────────────────────────────

/// The repository the download links point at. Filled by `build-site.sh`
/// from `WRIGHT_REPO`; `__REPO__` until then, and a placeholder that 404s is
/// the intended failure — a plausible wrong repository is worse.
const REPO: &str = "__REPO__";

/// One downloadable build.
struct Asset {
    /// Which platform's row this is, in the words a person picking a download
    /// uses — not the target triple.
    label: &'static str,
    /// The file name a release publishes, verbatim — `packaging/RELEASE-NOTES.md`'s.
    file: &'static str,
    /// About how big it is. `Pending` until a release publishes real numbers;
    /// the chip beside it says the same thing and comes out with it.
    size: &'static str,
    /// What `detect` matches against.
    os: Os,
}

/// The three platforms the installers ship for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Os {
    Linux,
    MacOs,
    Windows,
}

impl Os {
    /// The name to put on a button.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Linux => "Linux",
            Self::MacOs => "macOS",
            Self::Windows => "Windows",
        }
    }
}

/// Every published build, in the order the table lists them. The names are
/// `packaging/RELEASE-NOTES.md`'s, which is what the release workflow
/// publishes — a link that names the wrong file is a link that never works.
const ASSETS: &[Asset] = &[
    Asset {
        label: "Windows 10/11 (x64)",
        file: "Wright-Setup-x64.exe",
        size: "Pending",
        os: Os::Windows,
    },
    Asset {
        label: "Windows portable",
        file: "wright-windows-x64.zip",
        size: "Pending",
        os: Os::Windows,
    },
    Asset {
        label: "macOS, Apple silicon and Intel",
        file: "Wright-macos-universal.dmg",
        size: "Pending",
        os: Os::MacOs,
    },
    Asset {
        label: "macOS portable",
        file: "Wright-macos-universal.app.zip",
        size: "Pending",
        os: Os::MacOs,
    },
    Asset {
        label: "Linux (deb, glibc 2.35+)",
        file: "wright_amd64.deb",
        size: "Pending",
        os: Os::Linux,
    },
    Asset {
        label: "Linux portable",
        file: "wright-linux-x64.tar.gz",
        size: "Pending",
        os: Os::Linux,
    },
];

/// The repository the links actually point at, this run.
///
/// Read from the page's `<meta name="wright-repo">` at start-up rather than
/// compiled in, so changing it is a `sed` over `index.html` and not a Rust
/// toolchain and a new wasm binary — the same trade `viewwsite` made.
#[cfg(target_arch = "wasm32")]
pub(crate) fn repo() -> String {
    use std::cell::RefCell;
    thread_local! {
        static CACHED: RefCell<Option<String>> = const { RefCell::new(None) };
    }
    CACHED.with(|cached| {
        cached
            .borrow_mut()
            .get_or_insert_with(|| {
                web_sys::window()
                    .and_then(|window| window.document())
                    .and_then(|document| {
                        document
                            .query_selector("meta[name=\"wright-repo\"]")
                            .ok()?
                    })
                    .and_then(|meta| meta.get_attribute("content"))
                    .filter(|value| !value.is_empty() && !value.starts_with("__"))
                    .unwrap_or_else(|| REPO.to_owned())
            })
            .clone()
    })
}

/// Off the browser there is no document to ask, so the compiled-in default is
/// the whole answer — which is what `examples/render.rs` draws.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn repo() -> String {
    REPO.to_owned()
}

/// The download URL for one asset.
fn url_for(file: &str) -> String {
    format!(
        "https://github.com/{}/releases/latest/download/{file}",
        repo()
    )
}

/// Send the browser somewhere. A canvas has no `<a>` in it, so every link on
/// this page is a tap handler and this function.
#[cfg(target_arch = "wasm32")]
fn navigate(url: &str) {
    if let Some(window) = web_sys::window() {
        let _ = window.location().set_href(url);
    }
}

/// Off the browser there is nowhere to navigate to. It exists so the tree
/// below is one tree rather than two.
#[cfg(not(target_arch = "wasm32"))]
fn navigate(_url: &str) {}

/// Which build to put under the big button. A guess, and always overridable —
/// the table below the button is the whole list.
#[cfg(target_arch = "wasm32")]
#[must_use]
pub fn detect() -> Option<Os> {
    let agent = web_sys::window()?.navigator().user_agent().ok()?;
    if agent.contains("Windows") {
        Some(Os::Windows)
    } else if agent.contains("Mac") || agent.contains("iPhone") || agent.contains("iPad") {
        Some(Os::MacOs)
    } else if agent.contains("Linux") || agent.contains("X11") || agent.contains("Android") {
        Some(Os::Linux)
    } else {
        None
    }
}

/// No user agent to read off the browser: the host renderer is told which
/// platform to draw for.
#[cfg(not(target_arch = "wasm32"))]
#[must_use]
pub const fn detect() -> Option<Os> {
    None
}

/// The page's theme: Wright's accent over its own dark palette, in the roles
/// the catalogue's controls read.
fn theme() -> ThemeData {
    let mut colors = ColorScheme::dark();
    colors.primary = ACCENT;
    colors.on_primary = Color::WHITE;
    colors.surface = SURFACE;
    colors.on_surface = INK;
    colors.surface_variant = SURFACE;
    colors.on_surface_variant = INK_2;
    colors.outline = LINE_STRONG;
    ThemeData::from_colors(colors)
}

// ─── the faces ────────────────────────────────────────────────────────────

/// The page ships **Geist** and **Geist Mono** — the same faces, subsets and
/// licence (`assets/fonts/Geist-OFL.txt`) as the studio's page — retagged for
/// vieww's four-step weight model, with the embedded faces behind them so
/// anything outside the subset still renders.
const GEIST: &[u8] = include_bytes!("../assets/fonts/vw-Geist-Regular.ttf");
const GEIST_MEDIUM: &[u8] = include_bytes!("../assets/fonts/vw-Geist-Medium.ttf");
const GEIST_BOLD: &[u8] = include_bytes!("../assets/fonts/vw-Geist-Bold.ttf");
const GEIST_MONO: &[u8] = include_bytes!("../assets/fonts/vw-GeistMono-Regular.ttf");
const GEIST_MONO_MEDIUM: &[u8] = include_bytes!("../assets/fonts/vw-GeistMono-Medium.ttf");

/// The font store the page is drawn with. Handed to [`FrameDriver::set_fonts`]
/// before the first frame, by both [`root`] and `examples/render.rs`, so the
/// browser and the host renders measure text identically.
#[must_use]
pub fn fonts() -> vieww::text::FontStore {
    vieww::text::FontStore::with_application_faces(
        [
            GEIST.to_vec(),
            GEIST_MEDIUM.to_vec(),
            GEIST_BOLD.to_vec(),
            GEIST_MONO.to_vec(),
            GEIST_MONO_MEDIUM.to_vec(),
        ],
        Some("Geist"),
        Some("Geist Mono"),
    )
}

/// The face the code blocks and the file names are set in. Named rather than
/// `monospace()`: the generic resolves differently on the two backends, and
/// naming the family is the only spelling both read the same way.
fn mono(size: f32, color: Color) -> TextStyle {
    TextStyle::new(size)
        .family(FontFamily::Named("Geist Mono"))
        .color(color)
}

// ─── the mark ─────────────────────────────────────────────────────────────

/// The W's corners on its 24-unit grid — `apps/wright/app/src/ui/logo.rs`'s
/// own constants, verbatim: one stroke, round caps and joins.
const W: [(f32, f32); 5] = [
    (3.0, 6.0),
    (7.0, 19.0),
    (12.0, 9.0),
    (17.0, 19.0),
    (21.0, 6.0),
];
/// Stroke width on the same grid.
const W_STROKE: f32 = 2.0;

/// The app icon's tile colours — not the theme's: an icon looks the same
/// everywhere. `ICON_GROUND` is the blue, `ICON_INK` the white stroke.
const ICON_GROUND: Color = ACCENT;
const ICON_INK: Color = Color::WHITE;

/// The mark, as a painter: the same one-stroke W the app's top bar, splash,
/// watermark and app icon draw, drawn here at `progress = 1` — the page's
/// version of the splash's self-drawing stroke is in `index.html`, where CSS
/// can animate an SVG dash without a ticker.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Mark {
    color: Color,
    /// 0..=1 — how much of the stroke is drawn.
    progress: f32,
}

impl Mark {
    /// The W at `side` points square.
    fn widget(color: Color, side: f32) -> WidgetNode {
        Painting::sized(Size::new(side, side), Self { color, progress: 1.0 }).into()
    }
}

impl Painter for Mark {
    fn paint(&self, book: &mut Sketchbook, size: Size) {
        let side = size.width.min(size.height);
        let scale = side / 24.0;
        let origin = Offset::new(
            (size.width - side) / 2.0 + 3.0 * scale,
            (size.height - side) / 2.0 + 6.0 * scale,
        );
        let pts: Vec<Offset> = W
            .iter()
            .map(|&(x, y)| Offset::new(origin.dx + x * scale, origin.dy + y * scale))
            .collect();
        // How much of the polyline is visible — the app's own `visible`,
        // because the page should draw the mark the way the app does.
        let lens: Vec<f32> = pts
            .windows(2)
            .map(|w| {
                ((w[1].dx - w[0].dx).powi(2) + (w[1].dy - w[0].dy).powi(2)).sqrt()
            })
            .collect();
        let mut left = lens.iter().sum::<f32>() * self.progress.clamp(0.0, 1.0);
        let mut visible = vec![pts[0]];
        for (i, len) in lens.iter().enumerate() {
            if left <= 0.0 {
                break;
            }
            let t = (left / len).min(1.0);
            visible.push(Offset::new(
                pts[i].dx + (pts[i + 1].dx - pts[i].dx) * t,
                pts[i].dy + (pts[i + 1].dy - pts[i].dy) * t,
            ));
            left -= len;
        }
        if visible.len() < 2 || self.color.a == 0 {
            return;
        }
        let mut path = Path::new();
        path.move_to(visible[0]);
        for point in &visible[1..] {
            path.line_to(*point);
        }
        book.stroke_styled(
            path,
            self.color,
            W_STROKE * scale,
            StrokeStyle::rounded(),
        );
    }
    fn should_repaint(&self, previous: &dyn Painter) -> bool {
        previous.as_any().downcast_ref::<Self>() != Some(self)
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

/// The mark on its tile, the way the installers' icon and the window's top
/// corner carry it: the blue squircle, radius 30% of the side, W at 62%.
fn mark(side: f32) -> WidgetNode {
    Container::new()
        .color(ICON_GROUND)
        .radius(side * 0.3)
        .alignment(Alignment::CENTER)
        .child(Mark::widget(ICON_INK, side * 0.62))
        .into()
}

// ─── the page ─────────────────────────────────────────────────────────────

/// The scrolling page — `viewwsite::Site`'s shape: the one widget that reads
/// the scroll signal, so a drag rebuilds the page's description while the
/// element tree underneath keeps every child it can.
#[derive(Debug)]
pub struct Site {
    /// Where the page is scrolled to.
    pub scroll: ScrollController,
    /// The surface, from the [`LayoutBuilder`] above this widget.
    pub surface: Size,
    /// What [`detect`] answered, resolved once at start-up.
    pub host: Option<Os>,
    /// 0 to 1, once, over the first second. Every entrance on the page is a
    /// slice of this one animation rather than an animation of its own.
    pub entrance: Animation<f32>,
    /// The demo's heartbeat: one firing, auto-played once on mount, held at
    /// the approval gate until the reader answers it. See [`session_screen`].
    pub fire: Animation<f32>,
    /// The second half of the firing: what [Allow] releases.
    pub deliver: Animation<f32>,
    /// The gate's answer: 0 pending, 1 allowed, 2 refused.
    pub gate: Signal<u8>,
    /// The decoded screenshots. Cloning one is an `Arc` bump; see [`Art`].
    pub art: Art,
    /// Where each navigable section ended up, measured. See [`Anchors`].
    pub anchors: Anchors,
}

impl Widget for Site {
    fn debug_name(&self) -> &'static str {
        "Site"
    }

    fn kind(&self) -> WidgetKind<'_> {
        WidgetKind::Composed
    }

    fn build(&self, _ctx: &BuildContext) -> WidgetNode {
        // Read here, in a build, which is what subscribes this element to it.
        let entrance = self.entrance.value();
        let width = self.surface.width;
        let wide = width >= BREAKPOINT;
        let gutter = if wide { GUTTER } else { 18.0 };
        let column = (width - gutter * 2.0).clamp(240.0, COLUMN);

        let page = Flex::column()
            .cross_axis_alignment(CrossAxisAlignment::Stretch)
            .spacing(0.0)
            .children(children![
                // The nav is drawn over the page, so the page owes it a gap.
                SizedBox::height(NAV_HEIGHT),
                hero(column, wide, self.host, entrance),
                divider(column),
                self.anchors.reveal(0, &self.scroll, flow(column, wide)),
                divider(column),
                self.anchors
                    .reveal(1, &self.scroll, window_section(column, wide, &self.art)),
                divider(column),
                self.anchors.mark(
                    0,
                    &self.scroll,
                    self.anchors.reveal(
                        2,
                        &self.scroll,
                        demo_band(column, wide, &self.fire, &self.deliver, &self.gate),
                    ),
                ),
                divider(column),
                self.anchors.reveal(3, &self.scroll, firing(column, wide)),
                divider(column),
                self.anchors.mark(
                    1,
                    &self.scroll,
                    self.anchors.reveal(4, &self.scroll, blueprint(wide)),
                ),
                divider(column),
                self.anchors.reveal(5, &self.scroll, cache(column, wide)),
                divider(column),
                self.anchors.reveal(6, &self.scroll, features(column, wide)),
                divider(column),
                self.anchors.reveal(7, &self.scroll, get_started(column, wide)),
                divider(column),
                self.anchors.mark(
                    2,
                    &self.scroll,
                    self.anchors
                        .reveal(8, &self.scroll, downloads(column, wide, self.host)),
                ),
                divider(column),
                self.anchors.reveal(9, &self.scroll, closing(wide)),
                divider(column),
                self.anchors.reveal(10, &self.scroll, footer(column, wide)),
            ]);

        let scrollable = Scrollable::vertical(self.scroll.offset())
            .viewport(self.surface.height)
            .on_drag(self.scroll.on_drag())
            .on_drag_end(self.scroll.on_drag_end())
            .on_extents(self.scroll.on_extents())
            .child(
                Container::new()
                    .color(GROUND)
                    .padding(EdgeInsets::symmetric(gutter, 0.0))
                    .child(
                        Center::new().child(
                            Constrained::new(Constraints::new(0.0, column, 0.0, f32::INFINITY))
                                .child(page),
                        ),
                    ),
            );

        // The bar is a **sibling** drawn over the scrollable, not a wrapper
        // around it: that is what lets it overlay the last few points of the
        // page without taking a column away from the content.
        Stack::new()
            .fit(StackFit::Expand)
            .children(children![
                scrollable,
                Positioned::new().left(0.0).right(0.0).top(0.0).child(nav(
                    &self.scroll,
                    &self.anchors,
                    column,
                    wide,
                    self.host
                ),),
                Positioned::new()
                    .right(2.0)
                    .top(NAV_HEIGHT)
                    .bottom(2.0)
                    .child(SizedBox::width(BAR_WIDTH).child(Scrollbar {
                        scroll: self.scroll.clone(),
                    }),),
            ])
            .into()
    }
}

widget_node_from!(Site);

// ─── syntax colours ───────────────────────────────────────────────────────
//
// The window's own editor palette — `theme.rs`'s roles, applied to the same
// token classes the studio's highlighter draws. Code on this page should look
// like code in the product it sells.

/// Keywords — KDL's node words, and the structural words of the samples.
const SYN_KEYWORD: Color = Color::rgb(0x56, 0x9C, 0xD6);
/// Types — an agent's name, a capitalised identifier.
const SYN_TYPE: Color = Color::rgb(0x4D, 0xC9, 0xC9);
/// Strings.
const SYN_STRING: Color = Color::rgb(0xCE, 0x91, 0x78);
/// Numbers, and the micro-USD budgets.
const SYN_NUMBER: Color = Color::rgb(0xB5, 0xCA, 0xE8);
/// Comments.
const SYN_COMMENT: Color = Color::rgb(0x6A, 0x99, 0x55);
/// Property names — `goal`, `model`, the word before `=`.
const SYN_PROP: Color = Color::rgb(0x9C, 0xDC, 0xFE);
/// Function calls — `plan`, `run`, the word before `(`.
const SYN_FUNCTION: Color = Color::rgb(0xDC, 0xDC, 0xAA);
/// Everything else.
const SYN_PUNCT: Color = Color::rgb(0xD4, 0xD4, 0xD4);

/// The words the highlighter colours as keywords: the blueprint's node and
/// property names, plus the CLI verbs the terminal samples type. It
/// highlights *these samples*, not the languages; a token it does not know
/// falls through to punctuation rather than to a wrong colour.
const KEYWORDS: &[&str] = &[
    // The blueprint's nodes.
    "meta", "intent", "project", "agent", "edge", "chips", "guardrails",
    "deliverable", "models", "scheduling", "secrets", "logging", "tests", "ci",
    // The CLI's verbs, for the terminal blocks.
    "wright", "cargo", "build", "sudo", "apt", "install",
    // The structural words of a plan's output.
    "true", "false", "on", "off", "in", "out",
];

/// One line of source, as coloured spans. Tokenised once per line, not once
/// per frame — the samples are `&'static str` constants that never change,
/// and a scroll rebuilds on every frame.
pub(crate) fn highlight(line: &str, size: f32) -> Vec<Span> {
    use std::cell::RefCell;
    use std::collections::HashMap;
    thread_local! {
        static CACHE: RefCell<HashMap<(String, u32), Vec<Span>>> =
            RefCell::new(HashMap::new());
    }
    let key = (line.to_owned(), size.to_bits());
    if let Some(cached) = CACHE.with(|cache| cache.borrow().get(&key).cloned()) {
        return cached;
    }
    let spans = highlight_uncached(line, size);
    CACHE.with(|cache| {
        cache.borrow_mut().insert(key, spans.clone());
    });
    spans
}

/// The tokeniser itself. Knows five things — comments, strings, numbers,
/// identifiers and everything else — and one rule the KDL samples need that
/// Rust does not have: an identifier followed by `=` is a property, and an
/// identifier followed by `(` is a call.
fn highlight_uncached(line: &str, size: f32) -> Vec<Span> {
    let base = mono(size, SYN_PUNCT);
    let mut spans: Vec<Span> = Vec::new();
    let bytes: Vec<char> = line.chars().collect();
    let mut index = 0;

    while index < bytes.len() {
        let character = bytes[index];

        if character == '/' && bytes.get(index + 1) == Some(&'/') {
            let rest: String = bytes[index..].iter().collect();
            spans.push(Span::new(rest).style(mono(size, SYN_COMMENT)));
            break;
        }

        if character == '"' {
            let mut end = index + 1;
            while end < bytes.len() && bytes[end] != '"' {
                if bytes[end] == '\\' {
                    end += 1;
                }
                end += 1;
            }
            let text: String = bytes[index..(end + 1).min(bytes.len())].iter().collect();
            spans.push(Span::new(text).style(mono(size, SYN_STRING)));
            index = (end + 1).min(bytes.len());
            continue;
        }

        if character.is_ascii_digit() {
            let mut end = index;
            while end < bytes.len()
                && (bytes[end].is_ascii_digit()
                    || bytes[end] == '.'
                    || (bytes[end] == '-' && end == index))
            {
                end += 1;
            }
            let text: String = bytes[index..end].iter().collect();
            spans.push(Span::new(text).style(mono(size, SYN_NUMBER)));
            index = end;
            continue;
        }

        if character.is_alphabetic() || character == '_' || character == '-' {
            let mut end = index;
            while end < bytes.len()
                && (bytes[end].is_alphanumeric() || bytes[end] == '_' || bytes[end] == '-')
            {
                end += 1;
            }
            let word: String = bytes[index..end].iter().collect();
            let next = bytes.get(end).copied();
            let colour = if next == Some('=') {
                SYN_PROP
            } else if next == Some('(') {
                SYN_FUNCTION
            } else if KEYWORDS.contains(&word.as_str()) {
                SYN_KEYWORD
            } else if word.starts_with(char::is_uppercase) {
                SYN_TYPE
            } else {
                SYN_PUNCT
            };
            spans.push(Span::new(word).style(mono(size, colour)));
            index = end;
            continue;
        }

        // Runs of everything else in one span, so a line of `{` and `=`
        // is not six spans and six glyph runs.
        let mut end = index;
        while end < bytes.len()
            && !bytes[end].is_alphanumeric()
            && bytes[end] != '_'
            && bytes[end] != '"'
            && bytes[end] != '-'
            && !(bytes[end] == '/' && bytes.get(end + 1) == Some(&'/'))
        {
            end += 1;
        }
        let text: String = bytes[index..end.max(index + 1)].iter().collect();
        spans.push(Span::new(text).style(base));
        index = end.max(index + 1);
    }

    if spans.is_empty() {
        spans.push(Span::new(" ").style(base));
    }
    spans
}

// ─── the pictures ─────────────────────────────────────────────────────────

/// The screenshots, decoded once — the app's own `wright ui --snapshot`
/// output, from `apps/wright/docs/screens`. Held rather than decoded per
/// build for the obvious reason: a PNG decode in a `build` would run on
/// every frame of a scroll. Every field is an `Option`: a picture that
/// failed to decode leaves its figure out rather than taking the page down.
#[derive(Debug, Clone, Default)]
pub struct Art {
    /// The Interview: describing the project, in your words.
    pub describe: Option<ImageData>,
    /// The Confirm Gate: the plan, in plain words generated from the blueprint.
    pub plan: Option<ImageData>,
    /// The firing: the event river, the gate card, the composer.
    pub run: Option<ImageData>,
    /// The result: the receipt, the export.
    pub result: Option<ImageData>,
}

impl Art {
    /// Decode the four pictures compiled into this binary.
    ///
    /// `include_bytes!` rather than a fetch: they are 470 KB together, which
    /// is less than the round trips would cost, and a page whose pictures
    /// arrive a second after the section is worse than one that ships with
    /// them.
    #[must_use]
    pub fn embedded() -> Self {
        fn decode(bytes: &[u8]) -> Option<ImageData> {
            vieww_asset::decode(bytes).ok().map(|(image, _)| image)
        }
        #[cfg(not(target_arch = "wasm32"))]
        if std::env::var("WRIGHTSITE_NO_ART").is_ok() {
            return Self::default();
        }
        Self {
            describe: decode(include_bytes!("../assets/1-describe.png")),
            plan: decode(include_bytes!("../assets/2-plan.png")),
            run: decode(include_bytes!("../assets/3-run.png")),
            result: decode(include_bytes!("../assets/4-result.png")),
        }
    }
}

// ─── where the sections actually are ──────────────────────────────────────

/// The sections the nav can jump to, in page order. The label is what the
/// nav shows and the index is the slot each section writes its measured
/// position into. One list, so a link and a section cannot drift apart.
pub const SECTIONS: &[&str] = &["The flow", "The window", "Live demo", "Blueprint", "Download"];

/// Where each section starts, in document coordinates, once it has been laid
/// out — `viewwsite::Anchors`' arrangement, which is the fix for a nav that
/// held guessed fractions: a widget cannot know its own position, `Measured`
/// is the way the answer gets back into the tree, and it reports only when
/// the rectangle changes.
#[derive(Debug, Clone)]
pub struct Anchors {
    tops: Vec<Signal<f32>>,
    spots: Vec<Signal<f32>>,
}

/// How many blocks on the page can be revealed on scroll. One more than the
/// page currently uses, so adding a band does not mean rewiring this.
const REVEALS: usize = 16;

/// Over how many points of scroll a block goes from invisible to arrived.
/// Scroll-linked rather than timed: the scrollable already eases a wheel
/// notch, so a band this wide reads as the same fade a CSS version runs —
/// without a second clock, a ticker per block, or a fade that finishes while
/// the block is still moving.
const REVEAL_BAND: f32 = 150.0;

/// The fraction of the window a block's top has to cross before it starts.
const REVEAL_LINE: f32 = 0.92;

/// One block of the page, faded and lifted by where the page is scrolled to.
#[derive(Debug, Clone)]
struct Reveal {
    scroll: ScrollController,
    top: Signal<f32>,
    child: WidgetNode,
}

impl Widget for Reveal {
    fn debug_name(&self) -> &'static str {
        "Reveal"
    }

    fn kind(&self) -> WidgetKind<'_> {
        WidgetKind::Composed
    }

    fn build(&self, _ctx: &BuildContext) -> WidgetNode {
        let top = self.top.get();
        let offset = self.scroll.offset();
        let viewport = self.scroll.viewport();

        let progress = if viewport <= 0.0 {
            1.0
        } else {
            ((offset + viewport * REVEAL_LINE - top) / REVEAL_BAND).clamp(0.0, 1.0)
        };
        if progress >= 1.0 {
            return self.child.clone();
        }

        let eased = 1.0 - (1.0 - progress).powi(3);
        Opacity::new(eased)
            .child(
                Transformed::translate(Offset::new(0.0, (1.0 - eased) * 18.0))
                    .child(self.child.clone()),
            )
            .into()
    }
}

widget_node_from!(Reveal);

impl Anchors {
    /// One signal per section, plus a pool of slots for the scroll reveals.
    #[must_use]
    pub fn new(runtime: &vieww::element::Runtime) -> Self {
        Self {
            tops: SECTIONS.iter().map(|_| runtime.signal(0.0_f32)).collect(),
            spots: (0..REVEALS).map(|_| runtime.signal(0.0_f32)).collect(),
        }
    }

    /// Wrap `node` so it fades and lifts in as it scrolls into view.
    fn reveal(&self, index: usize, scroll: &ScrollController, node: WidgetNode) -> WidgetNode {
        let Some(slot) = self.spots.get(index).cloned() else {
            return node;
        };
        let writer = slot.clone();
        let peeker = scroll.clone();
        Measured::new()
            .on_measured(std::rc::Rc::new(move |rect: Rect| {
                writer.set(rect.top + peeker.peek());
            }))
            .child(Reveal {
                scroll: scroll.clone(),
                top: slot,
                child: node,
            })
            .into()
    }

    /// Wrap a section so it reports where it ends up.
    fn mark(&self, index: usize, scroll: &ScrollController, node: WidgetNode) -> WidgetNode {
        let Some(slot) = self.tops.get(index).cloned() else {
            return node;
        };
        let scroll = scroll.clone();
        Measured::new()
            .on_measured(std::rc::Rc::new(move |rect: Rect| {
                // `peek`, not `offset`: this runs inside layout's report, and
                // reading a signal there would subscribe the *measuring*
                // element to the scroll offset — a rebuild on every frame of
                // every scroll, which is the loop this page cannot afford.
                slot.set(rect.top + scroll.peek());
            }))
            .child(node)
            .into()
    }

    /// Where the nav should scroll to for `index`, allowing for the bar.
    fn target(&self, index: usize) -> f32 {
        self.tops
            .get(index)
            .map_or(0.0, |slot| (slot.get() - NAV_HEIGHT - 12.0).max(0.0))
    }
}

// ─── the scrollbar ────────────────────────────────────────────────────────

/// A scrollbar, because a canvas does not get one — `viewwsite::Scrollbar`,
/// narrowed to one axis, in this page's colours. The geometry comes from the
/// same [`ScrollController`] the `Scrollable` reads, so the two cannot
/// disagree about where the content is.
const BAR_WIDTH: f32 = 12.0;
const THUMB_WIDTH: f32 = 6.0;
const MIN_THUMB: f32 = 32.0;

#[derive(Debug)]
struct Scrollbar {
    scroll: ScrollController,
}

impl Widget for Scrollbar {
    fn debug_name(&self) -> &'static str {
        "Scrollbar"
    }

    fn kind(&self) -> WidgetKind<'_> {
        WidgetKind::Composed
    }

    fn build(&self, _ctx: &BuildContext) -> WidgetNode {
        let scroll = self.scroll.clone();
        let offset = scroll.offset();
        let viewport = scroll.viewport();
        let max = scroll.max_offset();

        if max <= 0.5 || viewport <= 0.0 {
            return SizedBox::width(BAR_WIDTH).into();
        }
        let content = viewport + max;

        LayoutBuilder::new(move |constraints: Constraints| {
            let track = constraints.max_height;
            if !track.is_finite() || track <= 0.0 {
                return SizedBox::shrink().into();
            }

            let thumb = ((viewport / content) * track).clamp(MIN_THUMB.min(track), track);
            let travel = (track - thumb).max(0.0);
            let at = (offset / max).clamp(0.0, 1.0) * travel;
            let scale = if travel > 0.0 { max / travel } else { 0.0 };

            let dragging = scroll.clone();
            let paging = scroll.clone();

            let bar = Flex::column()
                .cross_axis_alignment(CrossAxisAlignment::Center)
                .children(children![
                    SizedBox::height(at),
                    Container::new()
                        .color(Color::rgb(0x4A, 0x4A, 0x55))
                        .radius(THUMB_WIDTH / 2.0)
                        .size(THUMB_WIDTH, thumb),
                    Flexible::expanded(1).child(SizedBox::shrink()),
                ]);

            GestureDetector::new()
                .touch_target(0.0)
                .drag_axis(Axis::Vertical)
                .on_drag_update(move |details: vieww::foundation::DragDetails| {
                    dragging.jump_to(dragging.peek() + details.delta.dy * scale);
                })
                .on_tap_down(move |details| {
                    let point = details.local.dy;
                    if point < at {
                        paging.jump_to(paging.peek() - viewport);
                    } else if point > at + thumb {
                        paging.jump_to(paging.peek() + viewport);
                    }
                })
                .child(
                    Container::new().color(Color::rgba(0, 0, 0, 1)).child(bar),
                )
                .into()
        })
        .into()
    }
}

widget_node_from!(Scrollbar);

// ─── the entrance ─────────────────────────────────────────────────────────

/// One element's slice of the page's single entrance animation.
fn stage(t: f32, delay: f32) -> f32 {
    ((t - delay) / 0.30).clamp(0.0, 1.0)
}

/// Fade and lift `node` in, at its own moment. The lift is a *transform*,
/// not padding: nothing above it is laid out again on any of the sixty frames
/// this runs for.
fn enter(t: f32, delay: f32, node: WidgetNode) -> WidgetNode {
    let progress = stage(t, delay);
    if progress >= 1.0 {
        return node;
    }
    Opacity::new(progress)
        .child(Transformed::translate(Offset::new(0.0, (1.0 - progress) * 14.0)).child(node))
        .into()
}

// ─── pieces ───────────────────────────────────────────────────────────────

/// A one-pixel rule the width of the column.
fn divider(column: f32) -> WidgetNode {
    Container::new()
        .color(LINE)
        .child(SizedBox::from_size(Size::new(column, 1.0)))
        .into()
}

/// The pill that opens each section.
fn eyebrow(text: &str) -> WidgetNode {
    Container::new()
        .color(WASH)
        .radius(999.0)
        .border(Border::thin(ACCENT))
        .padding(EdgeInsets::symmetric(12.0, 5.0))
        .child(
            Flex::row()
                .main_axis_size(MainAxisSize::Min)
                .cross_axis_alignment(CrossAxisAlignment::Center)
                .spacing(8.0)
                .children(children![
                    Container::new().color(ACCENT_INK).radius(999.0).size(6.0, 6.0),
                    Text::new(spaced(text)).style(mono(10.5, ACCENT_INK)),
                ]),
        )
        .into()
}

/// A string with a hair space between every character, so a label reads as a
/// label at eleven pixels rather than as small body text.
fn spaced(text: &str) -> String {
    let mut out = String::with_capacity(text.len() * 2);
    for (index, character) in text.chars().enumerate() {
        if index > 0 {
            out.push(' ');
        }
        out.push(character);
    }
    out
}

/// A small outlined label: a version, a status, a chip on a stage.
fn chip(text: &str, ink: Color) -> WidgetNode {
    Container::new()
        .color(SURFACE)
        .radius(999.0)
        .border(Border::thin(LINE_STRONG))
        .padding(EdgeInsets::symmetric(9.0, 3.0))
        .child(Text::new(text.to_string()).style(mono(11.0, ink)))
        .into()
}

// ─── the run states, drawn rather than typed ──────────────────────────────
//
// The window's stage glyphs — ✓, ◌, ○ — drawn as strokes rather than set as
// text, so they are the same pixels on every host and no font subset decides
// what the run looks like. The shapes are the window's own vocabulary: a
/// finished stage is a check, a working stage is a ring that breathes with
// the animation that is moving it, a pending stage is a thin ring, and a
/// refused stage is a cross.

/// A check, stroked — two lines, round caps.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Check {
    color: Color,
}

impl Painter for Check {
    fn paint(&self, book: &mut Sketchbook, size: Size) {
        let side = size.width.min(size.height);
        let scale = side / 10.0;
        let at = |fx: f32, fy: f32| Offset::new(fx * scale, fy * scale);
        let mut path = Path::new();
        path.move_to(at(1.5, 5.5));
        path.line_to(at(4.2, 8.2));
        path.line_to(at(8.8, 2.2));
        book.stroke_styled(path, self.color, 1.7 * scale, StrokeStyle::rounded());
    }
    fn should_repaint(&self, previous: &dyn Painter) -> bool {
        previous.as_any().downcast_ref::<Self>() != Some(self)
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

/// A cross, stroked — the refusal mark.
#[derive(Debug, Copy, Clone, PartialEq)]
struct Cross {
    color: Color,
}

impl Painter for Cross {
    fn paint(&self, book: &mut Sketchbook, size: Size) {
        let side = size.width.min(size.height);
        let scale = side / 10.0;
        let at = |fx: f32, fy: f32| Offset::new(fx * scale, fy * scale);
        let mut path = Path::new();
        path.move_to(at(2.2, 2.2));
        path.line_to(at(7.8, 7.8));
        path.move_to(at(7.8, 2.2));
        path.line_to(at(2.2, 7.8));
        book.stroke_styled(path, self.color, 1.7 * scale, StrokeStyle::rounded());
    }
    fn should_repaint(&self, previous: &dyn Painter) -> bool {
        previous.as_any().downcast_ref::<Self>() != Some(self)
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

/// A ring — pending, or the working ring when it is carrying a wash.
fn ring(color: Color, weight: f32, fill: Color) -> WidgetNode {
    Container::new()
        .color(fill)
        .radius(999.0)
        .border(Border {
            width: weight,
            color,
        })
        .child(SizedBox::from_size(Size::new(9.0, 9.0)))
        .into()
}

/// One stage's glyph, from the run's state machine.
fn glyph_done() -> WidgetNode {
    Painting::sized(Size::new(10.0, 10.0), Check { color: GOOD }).into()
}
fn glyph_working(pulse: f32) -> WidgetNode {
    Opacity::new(0.55 + 0.45 * pulse)
        .child(ring(ACCENT_INK, 1.6, Color::rgba(77, 170, 252, 30)))
        .into()
}
fn glyph_pending() -> WidgetNode {
    ring(INK_3, 1.2, Color::TRANSPARENT)
}
fn glyph_refused() -> WidgetNode {
    Painting::sized(Size::new(10.0, 10.0), Cross { color: BAD }).into()
}

// ─── the nav ──────────────────────────────────────────────────────────────

/// The bar across the top, which does not scroll away: `position: sticky` is
/// a CSS declaration, and here it is a sibling in a stack.
fn nav(
    scroll: &ScrollController,
    anchors: &Anchors,
    column: f32,
    wide: bool,
    host: Option<Os>,
) -> WidgetNode {
    let mut items: Vec<WidgetNode> = vec![Flex::row()
        .cross_axis_alignment(CrossAxisAlignment::Center)
        .spacing(9.0)
        .children(children![
            SizedBox::from_size(Size::new(22.0, 22.0)).child(mark(22.0)),
            Text::new("Wright").size(15.0).color(Color::WHITE).bold(),
            chip("0.5.1", INK_3),
        ])
        .into()];

    if wide {
        let mut links: Vec<WidgetNode> = Vec::new();
        for (index, label) in SECTIONS.iter().enumerate() {
            let target = scroll.clone();
            let anchors = anchors.clone();
            links.push(
                CursorArea::new(Cursor::Pointer)
                    .child(
                        Pressable::sensed(move |sense: Sense| {
                            Container::new()
                                .radius(6.0)
                                .padding(EdgeInsets::symmetric(9.0, 5.0))
                                .color(if sense.emphasis() > 0.0 {
                                    SURFACE
                                } else {
                                    GROUND
                                })
                                .child(
                                    Text::new((*label).to_string())
                                        .size(12.5)
                                        .color(if sense.hover > 0.0 { Color::WHITE } else { INK_2 }),
                                )
                                .into()
                        })
                        .on_tap(move || target.jump_to(anchors.target(index))),
                    )
                    .into(),
            );
        }
        items.push(
            Flexible::expanded(1)
                .child(
                    Flex::row()
                        .main_axis_alignment(MainAxisAlignment::Center)
                        .spacing(2.0)
                        .children(links),
                )
                .into(),
        );
    } else {
        items.push(Flexible::expanded(1).child(SizedBox::shrink()).into());
    }

    if let Some(os) = host {
        let asset = ASSETS
            .iter()
            .find(|asset| asset.os == os)
            .unwrap_or(&ASSETS[0]);
        let target = url_for(asset.file);
        items.push(
            CursorArea::new(Cursor::Pointer)
                .child(
                    Button::new(if wide { "Download" } else { "Get it" })
                        .style(ButtonStyle::Filled)
                        .on_pressed(move || navigate(&target)),
                )
                .into(),
        );
    }

    Container::new()
        .color(GROUND)
        .border(Border::thin(LINE))
        .padding(EdgeInsets::symmetric(0.0, 8.0))
        .child(
            Center::new().child(
                Constrained::new(Constraints::new(0.0, column, 0.0, f32::INFINITY)).child(
                    Flex::row()
                        .cross_axis_alignment(CrossAxisAlignment::Center)
                        .spacing(12.0)
                        .children(items),
                ),
            ),
        )
        .into()
}

/// The bar's height, which is also the top inset the page is given.
const NAV_HEIGHT: f32 = 56.0;

// ─── the hero ─────────────────────────────────────────────────────────────

/// The hero: centred, and built around the headline. The first screenful has
/// one job — say what this is, in the fewest words, at a size nobody can
/// miss.
fn hero(column: f32, wide: bool, host: Option<Os>, entrance: f32) -> WidgetNode {
    let lede = "You describe what should exist. Wright drafts a typed blueprint, shows \
                you — in plain words generated from that blueprint, never by a model — \
                what will run, what each step may touch, what it may cost and how done \
                is checked. You approve; a workforce of agents builds it in a sandbox, \
                tests it, and delivers it.";

    let headline = if wide { 68.0 } else { 36.0 };
    let mark_side = if wide { 84.0 } else { 64.0 };

    let pill = Container::new()
        .color(SURFACE)
        .radius(999.0)
        .border(Border::thin(LINE_STRONG))
        .padding(EdgeInsets::symmetric(14.0, 6.0))
        .child(
            Flex::row()
                .main_axis_size(MainAxisSize::Min)
                .cross_axis_alignment(CrossAxisAlignment::Center)
                .spacing(9.0)
                .children(children![
                    Container::new()
                        .color(GOOD)
                        .radius(999.0)
                        .size(7.0, 7.0),
                    Text::new("v0.5.1").style(mono(11.5, Color::WHITE)),
                    Text::new("·").style(mono(11.5, INK_3)),
                    Text::new(if wide {
                        "Built-in models. No key, no account."
                    } else {
                        "No key needed"
                    })
                    .style(mono(11.5, INK_2)),
                ]),
        );

    Container::new()
        .padding(EdgeInsets::only(
            0.0,
            if wide { 96.0 } else { 72.0 },
            0.0,
            if wide { 88.0 } else { 64.0 },
        ))
        .child(
            Flex::column()
                .cross_axis_alignment(CrossAxisAlignment::Center)
                .spacing(0.0)
                .children(children![
                    enter(
                        entrance,
                        0.02,
                        SizedBox::from_size(Size::new(mark_side, mark_side))
                            .child(mark(mark_side))
                            .into()
                    ),
                    SizedBox::height(22.0),
                    enter(entrance, 0.10, pill.into()),
                    SizedBox::height(if wide { 30.0 } else { 24.0 }),
                    enter(
                        entrance,
                        0.16,
                        Flex::column()
                            .cross_axis_alignment(CrossAxisAlignment::Center)
                            .spacing(2.0)
                            .children(children![
                                Text::new("Describe the outcome.")
                                    .size(headline)
                                    .color(Color::WHITE)
                                    .bold()
                                    .align(TextAlign::Center),
                                Text::new("Agents wright it.")
                                    .size(headline)
                                    .color(ACCENT_INK)
                                    .bold()
                                    .align(TextAlign::Center),
                            ])
                            .into()
                    ),
                    SizedBox::height(24.0),
                    enter(
                        entrance,
                        0.26,
                        Constrained::new(Constraints::new(
                            0.0,
                            column.min(660.0),
                            0.0,
                            f32::INFINITY
                        ))
                        .child(
                            Text::new(lede)
                                .size(if wide { 19.0 } else { 16.5 })
                                .color(INK_2)
                                .align(TextAlign::Center)
                        )
                        .into()
                    ),
                    SizedBox::height(30.0),
                    enter(entrance, 0.34, download_button(host, column)),
                    SizedBox::height(18.0),
                    enter(
                        entrance,
                        0.40,
                        Text::new("MIT · zero external crates, headless · one dependency for the window: vieww")
                            .style(mono(11.0, INK_3))
                            .align(TextAlign::Center)
                            .into()
                    ),
                    SizedBox::height(46.0),
                    enter(entrance, 0.46, stats(wide)),
                ]),
        )
        .into()
}

/// Four measured numbers a person deciding whether to download wants first.
/// Each is a claim the repository makes about itself, not a boast invented
/// for the page: the README's own census.
fn stats(wide: bool) -> WidgetNode {
    let entries = [
        ("236", "tests", "passing, clippy clean"),
        ("0", "", "external crates, headless"),
        ("1", "", "dependency for the window"),
        ("3", "", "platforms with installers"),
    ];
    let cells: Vec<WidgetNode> = entries
        .iter()
        .map(|(value, unit, note)| {
            Flexible::expanded(1)
                .child(
                    Flex::column()
                        .cross_axis_alignment(CrossAxisAlignment::Center)
                        .spacing(4.0)
                        .children(children![
                            Flex::row()
                                .main_axis_size(MainAxisSize::Min)
                                .cross_axis_alignment(CrossAxisAlignment::Baseline)
                                .spacing(3.0)
                                .children(children![
                                    Text::new((*value).to_string())
                                        .style(mono(23.0, Color::WHITE).bold()),
                                    Text::new((*unit).to_string()).style(mono(12.0, ACCENT_INK)),
                                ]),
                            Text::new((*note).to_string())
                                .size(if wide { 11.5 } else { 10.5 })
                                .color(INK_3)
                                .align(TextAlign::Center),
                        ]),
                )
                .into()
        })
        .collect();

    let row = if wide {
        Flex::row().cross_axis_alignment(CrossAxisAlignment::Start).spacing(16.0).children(cells)
    } else {
        Flex::column().cross_axis_alignment(CrossAxisAlignment::Start).spacing(12.0).children(cells)
    };

    Container::new()
        .color(SURFACE)
        .radius(14.0)
        .border(Border::thin(LINE_STRONG))
        .padding(EdgeInsets::symmetric(20.0, 16.0))
        .child(row)
        .into()
}

/// The download buttons: the detected platform filled, the other two outlined.
fn download_button(host: Option<Os>, available: f32) -> WidgetNode {
    /// Three buttons plus their gaps.
    const ROW_WIDTH: f32 = 560.0;
    let side_by_side = available >= ROW_WIDTH;
    let each = [Os::Linux, Os::MacOs, Os::Windows];
    let mut buttons: Vec<WidgetNode> = Vec::new();

    for os in each {
        let asset = ASSETS
            .iter()
            .find(|asset| asset.os == os)
            .unwrap_or(&ASSETS[0]);
        let target = url_for(asset.file);
        let mine = host == Some(os);
        buttons.push(
            CursorArea::new(Cursor::Pointer)
                .child(
                    Button::new(format!("Download for {}", os.name()))
                        .style(if mine {
                            ButtonStyle::Filled
                        } else {
                            ButtonStyle::Outlined
                        })
                        .on_pressed(move || navigate(&target)),
                )
                .into(),
        );
    }

    let note = match host {
        Some(Os::MacOs) => "One universal build — Apple silicon and Intel. The portable .app.zip is in the list below.".to_owned(),
        Some(Os::Windows) => "Per-user install, no admin prompt. The portable zip is in the list below.".to_owned(),
        Some(Os::Linux) => "glibc 2.35+. The .deb, and the tar.gz beside it, are in the list below.".to_owned(),
        None => "Pick a platform, or see every build in the list below.".to_owned(),
    };

    Flex::column()
        .cross_axis_alignment(CrossAxisAlignment::Center)
        .spacing(10.0)
        .children(children![
            if side_by_side {
                Flex::row()
                    .main_axis_size(MainAxisSize::Min)
                    .spacing(8.0)
                    .children(buttons)
            } else {
                Flex::column()
                    .cross_axis_alignment(CrossAxisAlignment::Stretch)
                    .spacing(8.0)
                    .children(buttons)
            },
            Text::new(note).style(mono(11.5, INK_3)),
        ])
        .into()
}

/// A section: a pill eyebrow, a heading, a lead, and whatever follows.
fn section(eyebrow_text: &str, heading: &str, body: Option<&str>, rest: WidgetNode) -> WidgetNode {
    section_at(eyebrow_text, heading, body, rest, true)
}

/// [`section`], with the width switch it needs for its own padding.
fn section_at(
    eyebrow_text: &str,
    heading: &str,
    body: Option<&str>,
    rest: WidgetNode,
    wide: bool,
) -> WidgetNode {
    let mut head: Vec<WidgetNode> = vec![
        eyebrow(eyebrow_text),
        SizedBox::height(6.0).into(),
        Constrained::new(Constraints::new(0.0, HEADING_MEASURE, 0.0, f32::INFINITY))
            .child(
                Text::new(heading.to_string())
                    .size(if wide { 42.0 } else { 30.0 })
                    .color(Color::WHITE)
                    .bold()
                    .align(TextAlign::Center),
            )
            .into(),
    ];
    if let Some(body) = body {
        head.push(SizedBox::height(14.0).into());
        head.push(
            Constrained::new(Constraints::new(0.0, PROSE, 0.0, f32::INFINITY))
                .child(
                    Text::new(body.to_string())
                        .size(if wide { 17.5 } else { 15.5 })
                        .color(INK_2)
                        .align(TextAlign::Center),
                )
                .into(),
        );
    }

    Container::new()
        .padding(EdgeInsets::symmetric(
            0.0,
            if wide { SECTION_Y } else { SECTION_Y_NARROW },
        ))
        .child(
            Flex::column()
                .cross_axis_alignment(CrossAxisAlignment::Center)
                .spacing(0.0)
                .children(children![
                    Flex::column()
                        .cross_axis_alignment(CrossAxisAlignment::Center)
                        .children(head),
                    SizedBox::height(44.0),
                    Flex::column()
                        .cross_axis_alignment(CrossAxisAlignment::Stretch)
                        .children(children![rest]),
                ]),
        )
        .into()
}

// ─── the live demo ────────────────────────────────────────────────────────
//
// This is the page's one thing a screenshot could not have been, and it is
// built the way the counter on the studio's page was: a real vieww widget
// tree, real signals, real animations — mounted in a canvas on the DOM page
// and rasterised here for the host review. It replays the run the README
// uses as its example, the HN Daily Digest, through the window's own
// vocabulary: a firing, a gate, a receipt.

/// The eight stages of the example firing, in order — the README's own
/// table, abridged to what fits beside a glyph. The note is the chip the
/// window would show for it.
const STAGES: &[(&str, &str)] = &[
    ("Shipwright", "skeleton · no model"),
    ("Logwright", "logging"),
    ("Fetcher", "live data · always fresh"),
    ("Summarizer", "cached if unchanged"),
    ("Testwright", "writes + runs checks"),
    ("Reviewer", "APPROVE or REJECT"),
    ("Mailer", "asks first · never retried"),
    ("Operator", "the receipt is written"),
];

/// How much of the demo's firing the first animation carries: everything up
/// to the gate. Six stages, eased so the early ones land quickly and the
/// last settles — the way a real event river reads.
const GATE_STAGE: usize = 6;
/// The demo's whole clock: one firing to the gate.
const FIRE_MS: u64 = 4200;
/// The second half: [Allow] releases Mailer and Operator.
const DELIVER_MS: u64 = 2400;

/// The session screen — the window in miniature.
///
/// A widget rather than a function because it **reads the animations and the
/// gate**, and a signal read is only a subscription inside a `build`. That is
/// also what keeps a tap cheap: the element that reads the run is this one,
/// so pressing [Allow] rebuilds a screen-sized subtree and leaves the rest of
/// the page untouched.
#[derive(Debug)]
pub struct Session {
    /// The firing's first half: 0 to 1 across the six sandboxed stages.
    pub fire: Animation<f32>,
    /// The firing's second half: what [Allow] releases.
    pub deliver: Animation<f32>,
    /// The gate's answer: 0 pending, 1 allowed, 2 refused.
    pub gate: Signal<u8>,
}

/// The run's state, resolved from the two animations and the gate. One
/// struct so the layout below and the status bar read one answer rather
/// than three.
struct RunView {
    /// How many stages have finished.
    done: usize,
    /// The index of the working stage, if one is working.
    working: Option<usize>,
    /// 0..1, the working stage's breathing pulse.
    pulse: f32,
    /// Whether the Mailer was refused at the gate.
    refused: bool,
    /// Whether everything finished and the receipt was written.
    delivered: bool,
}

impl RunView {
    /// What the run looks like right now.
    fn resolve(fire: f32, deliver: f32, gate: u8) -> Self {
        match gate {
            1 => {
                let done = 6 + (deliver * 2.0).floor().min(2.0) as usize;
                let working = if done < 8 && deliver > 0.0 {
                    Some(done)
                } else {
                    None
                };
                Self {
                    done,
                    working,
                    pulse: (deliver * 2.0).fract(),
                    refused: false,
                    delivered: done >= 8,
                }
            }
            2 => Self {
                done: 6,
                working: None,
                pulse: 0.0,
                refused: true,
                delivered: false,
            },
            _ => {
                let done = (fire * 6.0).floor().min(GATE_STAGE as f32) as usize;
                let working = if done < GATE_STAGE { Some(done) } else { None };
                Self {
                    done,
                    working,
                    pulse: (fire * 6.0).fract(),
                    refused: false,
                    delivered: false,
                }
            }
        }
    }

    /// The cost so far — the README's own arithmetic, $0.0005 a stage,
    /// $0.004 by the receipt.
    fn cost(&self) -> f32 {
        self.done as f32 * 0.0005
    }

    /// One line for the status bar.
    fn status(&self) -> String {
        if self.refused {
            "Refused at the gate · nothing was sent".to_owned()
        } else if self.delivered {
            "Delivered · receipt written".to_owned()
        } else if self.done == 0 {
            "Firing".to_owned()
        } else {
            let name = STAGES
                .get(self.done)
                .map_or("delivering", |(name, _)| *name);
            format!("Running · {name} working")
        }
    }

    /// The dot's colour on the status bar.
    fn dot(&self) -> Color {
        if self.refused {
            BAD
        } else if self.delivered {
            GOOD
        } else {
            WAIT
        }
    }
}

impl Widget for Session {
    fn debug_name(&self) -> &'static str {
        "Session"
    }

    fn kind(&self) -> WidgetKind<'_> {
        WidgetKind::Composed
    }

    fn build(&self, _ctx: &BuildContext) -> WidgetNode {
        // Both reads subscribe: this element, and nothing above it, rebuilds
        // as the firing advances. The page above is idle at zero frames a
        // second throughout.
        let fire = self.fire.value();
        let deliver = self.deliver.value();
        let gate = self.gate.get();
        let run = RunView::resolve(fire, deliver, gate);

        // The gate's answer, as tap handlers. `forward` from inside a tap is
        // the same steering the scroll's `jump_to` does: the animation is
        // already attached to this tree's tickers, so starting it is a call.
        let allow_gate = self.gate.clone();
        let allow_deliver = self.deliver.clone();
        let refuse = self.gate.clone();
        // Fire again: everything back to the start.
        let again_gate = self.gate.clone();
        let again_fire = self.fire.clone();
        let again_deliver = self.deliver.clone();

        // The stage rows. One per stage, each reading the one `RunView`.
        let rows: Vec<WidgetNode> = STAGES
            .iter()
            .enumerate()
            .map(|(index, (name, note))| {
                let (glyph, ink) = if index < run.done {
                    (glyph_done(), Color::WHITE)
                } else if run.working == Some(index) {
                    (glyph_working(run.pulse), ACCENT_INK)
                } else if run.refused && index == GATE_STAGE {
                    (glyph_refused(), BAD)
                } else {
                    (glyph_pending(), INK_2)
                };
                Flex::row()
                    .cross_axis_alignment(CrossAxisAlignment::Center)
                    .spacing(7.0)
                    .children(children![
                        SizedBox::from_size(Size::new(11.0, 11.0)).child(glyph),
                        Text::new((*name).to_string()).size(12.0).color(ink).bold(),
                        Flexible::expanded(1).child(SizedBox::shrink()),
                        Text::new((*note).to_string()).style(mono(9.5, INK_3)),
                    ])
                    .into()
            })
            .collect();

        // The gate card, only when the run is holding at it. The card is
        // the window's own shape: a wash, an accent border, the question in
        // plain words, and the two buttons that answer it.
        let gate_card: Option<WidgetNode> = (gate == 0 && run.done >= GATE_STAGE).then(|| {
            Container::new()
                .color(WASH)
                .radius(10.0)
                .border(Border::thin(ACCENT))
                .padding(EdgeInsets::all(10.0))
                .child(
                    Flex::column()
                        .cross_axis_alignment(CrossAxisAlignment::Start)
                        .spacing(8.0)
                        .children(children![
                            Text::new("Mailer is asking before it touches the world.")
                                .size(11.5)
                                .color(Color::WHITE)
                                .bold(),
                            Flex::row()
                                .main_axis_size(MainAxisSize::Min)
                                .spacing(8.0)
                                .children(children![
                                    CursorArea::new(Cursor::Pointer).child(
                                        Button::new("Allow")
                                            .style(ButtonStyle::Filled)
                                            .on_pressed(move || {
                                                allow_gate.set(1);
                                                allow_deliver.forward(Duration::ZERO);
                                            }),
                                    ),
                                    CursorArea::new(Cursor::Pointer).child(
                                        Button::new("Don't")
                                            .style(ButtonStyle::Outlined)
                                            .on_pressed(move || refuse.set(2)),
                                    ),
                                ]),
                        ]),
                )
                .into()
        });

        // The composer: the window's own bottom row, and — once the run has
        // answered — the way to fire it again.
        let composer: WidgetNode = if run.delivered || run.refused {
            CursorArea::new(Cursor::Pointer)
                .child(
                    Button::new("Fire the run again")
                        .style(ButtonStyle::Filled)
                        .on_pressed(move || {
                            again_gate.set(0);
                            again_deliver.set_progress(0.0);
                            again_deliver.stop();
                            again_fire.set_progress(0.0);
                            again_fire.forward(Duration::ZERO);
                        }),
                )
                .into()
        } else {
            Container::new()
                .color(SURFACE_2)
                .radius(999.0)
                .padding(EdgeInsets::symmetric(12.0, 7.0))
                .child(
                    Text::new("+  Ask for a change…")
                        .style(mono(10.5, INK_3)),
                )
                .into()
        };

        // The screen: top strip, the request, the run, the gate, the composer.
        let screen = Flex::column()
            .cross_axis_alignment(CrossAxisAlignment::Stretch)
            .spacing(0.0)
            .children(children![
                // The session strip — the window's own furniture.
                Container::new()
                    .color(SURFACE)
                    .padding(EdgeInsets::symmetric(12.0, 8.0))
                    .child(
                        Flex::row()
                            .cross_axis_alignment(CrossAxisAlignment::Center)
                            .spacing(7.0)
                            .children(children![
                                SizedBox::from_size(Size::new(14.0, 14.0)).child(mark(14.0)),
                                Text::new("HN Daily Digest").size(11.5).color(Color::WHITE).bold(),
                                Container::new()
                                    .color(run.dot())
                                    .radius(999.0)
                                    .size(6.0, 6.0),
                                Flexible::expanded(1).child(SizedBox::shrink()),
                                Text::new("Engine").style(mono(9.5, INK_3)),
                            ]),
                    ),
                Container::new().padding(EdgeInsets::all(10.0)).child(
                    Flex::column()
                        .cross_axis_alignment(CrossAxisAlignment::Stretch)
                        .spacing(9.0)
                        .children(children![
                            // Your request, in its bubble — the window puts
                            // the person on the right. The `Constrained` is
                            // load-bearing: a flex row hands its children
                            // unbounded width, and a Text there measures
                            // intrinsically — one long line that walks off
                            // the screen instead of a bubble that wraps.
                            Flex::row()
                                .main_axis_alignment(MainAxisAlignment::End)
                                .children(children![
                                    Constrained::new(Constraints::new(
                                        0.0,
                                        262.0,
                                        0.0,
                                        f32::INFINITY,
                                    ))
                                    .child(
                                        Container::new()
                                            .color(WASH)
                                            .radius(12.0)
                                            .padding(EdgeInsets::all(9.0))
                                            .child(
                                                Text::new(
                                                    "Daily digest of top HN stories, summarized, \
                                                     emailed at 8am."
                                                )
                                                .size(11.0)
                                                .color(Color::WHITE),
                                            ),
                                    ),
                                ]),
                            // The run card.
                            Container::new()
                                .color(SURFACE)
                                .radius(12.0)
                                .border(Border::thin(LINE_STRONG))
                                .padding(EdgeInsets::all(10.0))
                                .child(
                                    Flex::column()
                                        .cross_axis_alignment(CrossAxisAlignment::Stretch)
                                        .spacing(5.0)
                                        .children(children![
                                            Flex::row()
                                                .cross_axis_alignment(CrossAxisAlignment::Center)
                                                .children(children![
                                                    Text::new("Run 3").size(11.5)
                                                        .color(Color::WHITE).bold(),
                                                    Flexible::expanded(1).child(SizedBox::shrink()),
                                                    Text::new(format!("${:.4}", run.cost()))
                                                        .style(mono(9.5, ACCENT_INK)),
                                                ]),
                                            SizedBox::height(2.0),
                                            Flex::column()
                                                .cross_axis_alignment(CrossAxisAlignment::Stretch)
                                                .spacing(4.0)
                                                .children(rows),
                                            SizedBox::height(2.0),
                                            Container::new()
                                                .color(LINE)
                                                .child(SizedBox::height(1.0)),
                                            Text::new(
                                                "Fetcher is fresh every run anyway (live data)."
                                            )
                                            .style(mono(8.5, INK_3)),
                                        ]),
                                ),
                            // The gate, when the run is holding at it.
                            gate_card.map_or_else(
                                || SizedBox::shrink().into(),
                                |card| card,
                            ),
                            // The composer, or the way to fire again.
                            composer,
                        ]),
                ),
                Flexible::expanded(1).child(SizedBox::shrink()),
                // The status bar.
                Container::new()
                    .color(STATUS)
                    .border(Border::thin(LINE))
                    .padding(EdgeInsets::symmetric(12.0, 6.0))
                    .child(
                        Flex::row()
                            .cross_axis_alignment(CrossAxisAlignment::Center)
                            .spacing(6.0)
                            .children(children![
                                Container::new()
                                    .color(run.dot())
                                    .radius(999.0)
                                    .size(6.0, 6.0),
                                Text::new(run.status()).style(mono(9.0, INK_2)),
                                Flexible::expanded(1).child(SizedBox::shrink()),
                                Text::new(if run.refused {
                                    "stopped".to_owned()
                                } else {
                                    format!("±${:.4} budget", run.cost() * 2.0)
                                })
                                .style(mono(9.0, INK_3)),
                            ]),
                    ),
            ]);

        Container::new().color(GROUND).child(screen).into()
    }
}

widget_node_from!(Session);

/// The session screen on its own, with no window around it. What the DOM
/// page mounts into its `<canvas>` island: there the bezel is a
/// `border-radius` and a `box-shadow` on the element wrapping the canvas, so
/// the widget tree inside it is only the screen. Same tree, same signals,
/// same rasteriser as the private `Session` widget.
#[must_use]
pub fn session_screen(
    fire: Animation<f32>,
    deliver: Animation<f32>,
    gate: Signal<u8>,
) -> WidgetNode {
    // Wrapped in the page's own theme: without it the `Button` inside picks
    // up `ThemeData`'s default primary, and the one control in the demo comes
    // out a different colour from every control around it.
    Theme::new(theme())
        .child(Session {
            fire,
            deliver,
            gate,
        })
        .into()
}

/// Wires the demo's animations to a tree's tickers and fires the first run.
///
/// Called by [`root`] and by `dom.rs`'s island mount, so both halves of the
/// page start the same firing the same way: `fire` plays once to the gate
/// and holds; `deliver` waits, attached, for [Allow].
pub fn start_demo(
    runtime: &vieww::element::Runtime,
    tickers: &mut Tickers,
) -> (Animation<f32>, Animation<f32>, Signal<u8>) {
    let fire = Animation::new(
        runtime,
        Tween::new(0.0_f32, 1.0),
        Duration::from_millis(FIRE_MS),
    )
    .curve(Curve::EASE_OUT);
    let deliver = Animation::new(
        runtime,
        Tween::new(0.0_f32, 1.0),
        Duration::from_millis(DELIVER_MS),
    )
    .curve(Curve::EASE_OUT);
    let gate = runtime.signal(0_u8);

    // Both attached before either is started — an animation nobody attached
    // sits at zero, which here would mean a run that never leaves the pad.
    fire.attach(tickers);
    deliver.attach(tickers);
    fire.forward(Duration::ZERO);

    (fire, deliver, gate)
}

/// The live demo and its caption, as their own band under the hero.
fn demo_band(
    column: f32,
    wide: bool,
    fire: &Animation<f32>,
    deliver: &Animation<f32>,
    gate: &Signal<u8>,
) -> WidgetNode {
    let body: WidgetNode = if wide {
        Flex::row()
            .cross_axis_alignment(CrossAxisAlignment::Center)
            .spacing(40.0)
            .children(children![
                Flexible::expanded(1).child(
                    Flex::column()
                        .cross_axis_alignment(CrossAxisAlignment::Start)
                        .spacing(14.0)
                        .children(children![
                            eyebrow("LIVE DEMO"),
                            Text::new("This is not a screenshot.")
                                .size(24.0)
                                .color(Color::WHITE)
                                .bold(),
                            Constrained::new(Constraints::new(0.0, PROSE, 0.0, f32::INFINITY))
                                .child(
                                    Text::new(
                                        "The window below is a vieww widget tree, running in \
                                         your browser: a Signal for the gate, two Animations \
                                         for the run, and the same stage glyphs the real \
                                         window draws. It fired on its own when you arrived; \
                                         it is holding at the gate. Answer it. Allow, and \
                                         Mailer sends and the receipt is written. Refuse, and \
                                         nothing leaves the machine."
                                    )
                                    .size(14.5)
                                    .color(INK_2)
                                ),
                            Text::new(
                                "Nothing on this page replays a video. The firing is computed."
                            )
                            .style(mono(11.0, INK_3)),
                        ])
                ),
                demo(fire, deliver, gate),
            ])
            .into()
    } else {
        Flex::column()
            .cross_axis_alignment(CrossAxisAlignment::Start)
            .spacing(20.0)
            .children(children![
                eyebrow("LIVE DEMO"),
                Text::new("This is not a screenshot.")
                    .size(21.0)
                    .color(Color::WHITE)
                    .bold(),
                Text::new(
                    "The window below is a vieww widget tree, running in your browser. It is \
                     holding at the gate — answer it."
                )
                .size(14.0)
                .color(INK_2),
                demo(fire, deliver, gate),
            ])
            .into()
    };

    let _ = column;
    Container::new()
        .padding(EdgeInsets::symmetric(0.0, 44.0))
        .child(body)
        .into()
}

/// The window frame and the screen inside it. Two containers: the body of
/// the frame, and the screen inside its bezel. The radii differ by the bezel
/// width, which is what stops the inner corners looking loose inside the
/// outer ones.
fn demo(
    fire: &Animation<f32>,
    deliver: &Animation<f32>,
    gate: &Signal<u8>,
) -> WidgetNode {
    Flex::column()
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .spacing(10.0)
        .children(children![
            Container::new()
                .color(Color::rgb(0x0A, 0x0A, 0x0A))
                .radius(18.0)
                .border(Border::thin(LINE_STRONG))
                .padding(EdgeInsets::all(8.0))
                .child(
                    Container::new()
                        .color(GROUND)
                        .radius(12.0)
                        .child(SizedBox::from_size(Size::new(300.0, 520.0)).child(Session {
                            fire: fire.clone(),
                            deliver: deliver.clone(),
                            gate: gate.clone(),
                        })),
                ),
            Text::new("A real vieww screen, running here. Answer the gate.")
                .style(mono(11.0, INK_3)),
        ])
        .into()
}

// ─── the flow ─────────────────────────────────────────────────────────────

/// The three moves, which really are a sequence — which is why they are
/// numbered and nothing else on this page is. The words are the product's:
/// describe, confirm, fire.
fn flow(column: f32, wide: bool) -> WidgetNode {
    let steps = [
        (
            "01 · DESCRIBE",
            "Say what should exist",
            "The Interview condenses what you say — typed or spoken, transcribed \
             on this machine by the built-in Whisper — into a typed blueprint: \
             agents, edges, chips, guardrails, deliverables. One file is the \
             whole project, and you never have to write it.",
        ),
        (
            "02 · CONFIRM",
            "Read the plan in plain words",
            "The Confirm Gate shows exactly what will run — what each step may \
             touch, what it may cost, how done is checked — in words generated \
             from the blueprint by code, never by a model. Nothing executes \
             until you approve, and nothing touches the world before \
             Testwright and Reviewer pass.",
        ),
        (
            "03 · FIRE",
            "Agents build it in a sandbox",
            "A workforce of agents builds the project inside a Landlock and \
             network-namespace sandbox, tests it, has it reviewed, and delivers \
             it. Every run is fsynced to a durable event log with receipts, and \
             it resumes after a crash. Change one requirement later and only \
             the affected work runs again.",
        ),
    ];

    let cells: Vec<WidgetNode> = steps
        .iter()
        .map(|(number, title, body)| {
            Flexible::expanded(1)
                .child(
                    Container::new()
                        .padding(EdgeInsets::only(14.0, 2.0, 0.0, 2.0))
                        .child(
                            Flex::row()
                                .cross_axis_alignment(CrossAxisAlignment::Stretch)
                                .spacing(14.0)
                                .children(children![
                                    Container::new()
                                        .color(ACCENT)
                                        .child(SizedBox::width(2.0)),
                                    Flexible::expanded(1).child(
                                        Flex::column()
                                            .cross_axis_alignment(CrossAxisAlignment::Start)
                                            .spacing(7.0)
                                            .children(children![
                                                Text::new((*number).to_string())
                                                    .style(mono(10.5, ACCENT_INK).bold()),
                                                Text::new((*title).to_string())
                                                    .size(15.0)
                                                    .color(Color::WHITE)
                                                    .bold(),
                                                Text::new((*body).to_string())
                                                    .size(13.0)
                                                    .color(INK_2),
                                            ])
                                    ),
                                ]),
                        ),
                )
                .into()
        })
        .collect();

    let body = if wide {
        Flex::row()
            .cross_axis_alignment(CrossAxisAlignment::Start)
            .spacing(18.0)
            .children(cells)
    } else {
        Flex::column()
            .cross_axis_alignment(CrossAxisAlignment::Stretch)
            .spacing(20.0)
            .children(cells)
    };

    section(
        "THE FLOW",
        "Describe it. Confirm it in plain words. Fire it.",
        Some(
            "Wright turns a person's intent into a working, repeatable project — \
             and the one thing it will not do is run something you have not \
             read and approved first.",
        ),
        Flex::column()
            .cross_axis_alignment(CrossAxisAlignment::Stretch)
            .spacing(0.0)
            .children(children![SizedBox::height(6.0), body])
            .into(),
    )
    .pipe(|node| {
        let _ = column;
        node
    })
}

// ─── the window ───────────────────────────────────────────────────────────

/// The screenshots, in a frame that quotes the window's own chrome. Four
/// beats — describe, plan, run, result — because that is the product's
/// whole story in pictures.
fn window_section(column: f32, wide: bool, art: &Art) -> WidgetNode {
    let shots = [
        ("THE INTERVIEW", "Describe the outcome, in your words.", &art.describe),
        ("THE CONFIRM GATE", "The plan, generated from the blueprint.", &art.plan),
        ("THE FIRING", "The event river, and the gate.", &art.run),
        ("THE RECEIPT", "Delivered, with its history.", &art.result),
    ];

    let cells: Vec<WidgetNode> = shots
        .iter()
        .filter_map(|(tag, caption, image)| {
            let image = (*image).clone()?;
            // One ratio for every figure: the frames are the same shape, and a
            // row of pictures that disagree about their own aspect is a row
            // that looks broken rather than varied.
            let plate = Container::new()
                .color(SURFACE)
                .radius(10.0)
                .border(Border::thin(LINE_STRONG))
                .child(
                    Clip::new(ClipShape::RRect { radius: 10.0 }).child(
                        Flex::column()
                            .cross_axis_alignment(CrossAxisAlignment::Stretch)
                            .children(children![
                                Container::new()
                                    .color(Color::rgb(0x14, 0x14, 0x14))
                                    .padding(EdgeInsets::all(8.0))
                                    .child(
                                        AspectRatio::new(1.6).child(
                                            Image::new(image).fit(BoxFit::Cover),
                                        ),
                                    ),
                                Container::new().padding(EdgeInsets::all(12.0)).child(
                                    Flex::column()
                                        .cross_axis_alignment(CrossAxisAlignment::Start)
                                        .spacing(3.0)
                                        .children(children![
                                            Text::new(spaced(tag)).style(mono(9.5, ACCENT_INK)),
                                            Text::new((*caption).to_string())
                                                .size(12.5)
                                                .color(INK_2),
                                        ]),
                                ),
                            ]),
                    ),
                )
                .into();
            Some(plate)
        })
        .collect();

    if cells.is_empty() {
        return SizedBox::shrink().into();
    }

    let rows: Vec<WidgetNode> = cells
        .chunks(2)
        .map(|pair| {
            let mut row: Vec<WidgetNode> = pair
                .iter()
                .map(|plate| Flexible::expanded(1).child(plate.clone()).into())
                .collect();
            if pair.len() == 1 {
                row.push(Flexible::expanded(1).child(SizedBox::shrink()).into());
            }
            Flex::row()
                .cross_axis_alignment(CrossAxisAlignment::Stretch)
                .spacing(14.0)
                .children(row)
                .into()
        })
        .collect();

    let body: WidgetNode = if wide {
        Flex::column()
            .cross_axis_alignment(CrossAxisAlignment::Stretch)
            .spacing(14.0)
            .children(rows)
            .into()
    } else {
        Flex::column()
            .cross_axis_alignment(CrossAxisAlignment::Stretch)
            .spacing(12.0)
            .children(cells)
            .into()
    };

    section(
        "THE WINDOW",
        "Sessions, swipe, a live engine, and your voice.",
        Some(
            "The window is a vieww shell: every session in a row you swipe \
             through, the engine live under Space, approvals as cards you \
             answer with a tap or a drag, files you drop on the window, and a \
             mic that transcribes on this machine with the built-in Whisper.",
        ),
        Flex::column()
            .cross_axis_alignment(CrossAxisAlignment::Stretch)
            .children(children![SizedBox::height(6.0), body])
            .into(),
    )
    .pipe(|node| {
        let _ = column;
        node
    })
}

// ─── the firing ───────────────────────────────────────────────────────────

/// The eight stages of the example firing — the README's own table, as rows.
/// The glyphs are the window's own vocabulary, drawn as strokes: what is
/// done, what is working, what is waiting on you.
fn firing(_column: f32, wide: bool) -> WidgetNode {
    let notes = [
        ("built-in", "lays the universal skeleton — no model, milliseconds"),
        ("chip: logging", "logging, inside that skeleton"),
        ("your agent", "live data: always fresh"),
        ("your agent", "cached if its input did not change"),
        ("chip: tests", "writes + runs checks"),
        ("", "APPROVE or REJECT — rejection stops delivery"),
        ("", "touches the world: asks first, never cached, never retried"),
        ("built-in", "registers the schedule; the receipt is written"),
    ];

    let rows: Vec<WidgetNode> = STAGES
        .iter()
        .zip(notes)
        .enumerate()
        .map(|(index, ((name, _), (chip_text, note)))| {
            let number = format!("{:02}", index + 1);
            let row: WidgetNode = if wide {
                Flex::row()
                    .cross_axis_alignment(CrossAxisAlignment::Center)
                    .spacing(14.0)
                    .children(children![
                        Text::new(number).style(mono(11.0, INK_3)),
                        glyph_pending(),
                        Text::new((*name).to_string())
                            .size(14.0)
                            .color(Color::WHITE)
                            .bold(),
                        Flexible::expanded(1).child(SizedBox::shrink()),
                        Text::new((*note).to_string())
                            .size(12.5)
                            .color(INK_2),
                        SizedBox::width(12.0),
                        if chip_text.is_empty() {
                            SizedBox::width(90.0).into()
                        } else {
                            chip(chip_text, ACCENT_INK)
                        },
                    ])
                    .into()
            } else {
                Flex::column()
                    .cross_axis_alignment(CrossAxisAlignment::Start)
                    .spacing(4.0)
                    .children(children![
                        Flex::row()
                            .cross_axis_alignment(CrossAxisAlignment::Center)
                            .spacing(9.0)
                            .children(children![
                                Text::new(number).style(mono(11.0, INK_3)),
                                glyph_pending(),
                                Text::new((*name).to_string())
                                    .size(14.0)
                                    .color(Color::WHITE)
                                    .bold(),
                            ]),
                        Text::new((*note).to_string()).size(12.5).color(INK_2),
                    ])
                    .into()
            };

            // The two system agents the window draws a line under: the one
            // that can stop delivery, and the one that asks before it touches
            // the world.
            let emphasis = index == 5 || index == 6;
            let fill = if emphasis { WASH } else { SURFACE };
            let edge = if emphasis {
                Border::thin(ACCENT)
            } else {
                Border::thin(LINE_STRONG)
            };
            Container::new()
                .color(fill)
                .radius(8.0)
                .border(edge)
                .padding(EdgeInsets::symmetric(16.0, 11.0))
                .child(row)
                .into()
        })
        .collect();

    let stack = Flex::column()
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .spacing(8.0)
        .children(rows);

    section(
        "THE FIRING",
        "Eight stages. Nothing touches the world before the checks pass.",
        Some(
            "The stages of a real firing, in order. Your agents work inside the \
             sandbox; Testwright writes and runs the checks; Reviewer's \
             rejection stops delivery; and Mailer — the one step that touches \
             the outside — asks first, never runs from cache, and is never \
             retried.",
        ),
        Flex::column()
            .cross_axis_alignment(CrossAxisAlignment::Stretch)
            .spacing(12.0)
            .children(children![
                SizedBox::height(2.0),
                stack,
                Text::new(
                    "Stage glyphs from a live run: done, working, waiting — and the gate."
                )
                .style(mono(11.0, INK_3)),
            ])
            .into(),
    )
}

// ─── the blueprint ────────────────────────────────────────────────────────

/// A block of code, with the chrome an editor gives it: traffic lights, the
/// file name, a language chip, and a line-number gutter. The gutter is what
/// makes a code block read as *code* at a glance rather than as an indented
/// paragraph.
fn code_block(lines: &[&str], size: f32, filename: &str, lang: &str) -> WidgetNode {
    let dot = |color: Color| -> WidgetNode {
        Container::new()
            .color(color)
            .radius(999.0)
            .size(9.0, 9.0)
            .into()
    };

    let bar = Container::new()
        .color(SURFACE_2)
        .padding(EdgeInsets::symmetric(14.0, 9.0))
        .child(
            Flex::row()
                .cross_axis_alignment(CrossAxisAlignment::Center)
                .spacing(7.0)
                .children(children![
                    dot(Color::rgb(0xE0, 0x6C, 0x60)),
                    dot(Color::rgb(0xE0, 0xA8, 0x4E)),
                    dot(Color::rgb(0x5C, 0xB8, 0x60)),
                    SizedBox::width(6.0),
                    Text::new(filename.to_string()).style(mono(11.5, INK_2)),
                    Flexible::expanded(1).child(SizedBox::shrink()),
                    Container::new()
                        .color(SURFACE)
                        .radius(6.0)
                        .border(Border::thin(LINE_STRONG))
                        .padding(EdgeInsets::symmetric(7.0, 2.0))
                        .child(Text::new(spaced(lang)).style(mono(9.0, INK_3))),
                ]),
        );

    let body: Vec<WidgetNode> = lines
        .iter()
        .enumerate()
        .map(|(index, line)| {
            Flex::row()
                .cross_axis_alignment(CrossAxisAlignment::Start)
                .spacing(14.0)
                .children(children![
                    SizedBox::width(20.0).child(
                        Text::new(format!("{}", index + 1))
                            .style(mono(size - 1.0, Color::rgb(0x5A, 0x5A, 0x5A)))
                            .align(TextAlign::Right),
                    ),
                    Flexible::expanded(1).child(RichText::new(highlight(line, size))),
                ])
                .into()
        })
        .collect();

    Container::new()
        .color(Color::rgb(0x1E, 0x1E, 0x1E))
        .radius(12.0)
        .border(Border::thin(LINE_STRONG))
        .child(
            Clip::new(ClipShape::RRect { radius: 12.0 }).child(
                Flex::column()
                    .cross_axis_alignment(CrossAxisAlignment::Stretch)
                    .children(children![
                        bar,
                        Container::new().padding(EdgeInsets::all(16.0)).child(
                            Flex::column()
                                .cross_axis_alignment(CrossAxisAlignment::Stretch)
                                .spacing(3.0)
                                .children(body),
                        ),
                    ]),
            ),
        )
        .into()
}

/// A terminal block: the page's own `$` prompt in the accent, the command in
/// ink, the output in the muted tones a terminal actually uses.
fn terminal_block(lines: &[&str], size: f32, title: &str) -> WidgetNode {
    let rows: Vec<WidgetNode> = lines
        .iter()
        .map(|line| {
            if let Some(rest) = line.strip_prefix("$ ") {
                Flex::row()
                    .cross_axis_alignment(CrossAxisAlignment::Start)
                    .spacing(8.0)
                    .children(children![
                        Text::new("$").style(mono(size, ACCENT_INK).bold()),
                        // A row hands its children unbounded width and a Text
                        // there measures intrinsically — so the command goes
                        // in a `Flexible`, which bounds it, and a command too
                        // long for the box wraps instead of walking off it.
                        Flexible::expanded(1)
                            .child(Text::new(rest.to_string()).style(mono(size, Color::WHITE))),
                    ])
                    .into()
            } else {
                Text::new((*line).to_string())
                    .style(mono(size, if line.starts_with("  ") { INK_2 } else { INK_3 }))
                    .into()
            }
        })
        .collect();

    Container::new()
        .color(Color::rgb(0x1E, 0x1E, 0x1E))
        .radius(12.0)
        .border(Border::thin(LINE_STRONG))
        .child(
            Clip::new(ClipShape::RRect { radius: 12.0 }).child(
                Flex::column()
                    .cross_axis_alignment(CrossAxisAlignment::Stretch)
                    .children(children![
                        Container::new()
                            .color(SURFACE_2)
                            .padding(EdgeInsets::symmetric(14.0, 8.0))
                            .child(
                                Text::new(title.to_string()).style(mono(11.0, INK_3)),
                            ),
                        Container::new().padding(EdgeInsets::all(16.0)).child(
                            Flex::column()
                                .cross_axis_alignment(CrossAxisAlignment::Stretch)
                                .spacing(5.0)
                                .children(rows),
                        ),
                    ]),
            ),
        )
        .into()
}

/// The blueprint, and the words the Confirm Gate generates from it. Two faces
/// of one file: the typed definition, and the plain-words plan it compiles
/// to — which is the product's whole argument about trust.
fn blueprint(wide: bool) -> WidgetNode {
    // The HN Daily Digest blueprint, abridged to the lines that carry the
    // idea — the full file is `apps/wright/examples/hn-digest/blueprint.kdl`.
    let kdl = [
        "// blueprint.kdl — one file is the whole project.",
        "meta { name \"HN Daily Digest\" format 1 }",
        "",
        "intent {",
        "    summary \"Daily digest of top HN stories,",
        "             summarized, emailed at 8am.\"",
        "    confirmed \"v7\"",
        "}",
        "",
        "agent Fetcher {",
        "    goal \"Fetch top 30 HN stories: id, title, url, points\"",
        "    model tier=\"cheap\"   tools \"http-read\"",
        "    out \"stories.json\"   permissions network=true",
        "}",
        "agent Summarizer {",
        "    goal \"Write a punchy two-sentence summary per story\"",
        "    in \"stories.json\"   out \"summaries.json\"",
        "}",
        "agent Mailer {",
        "    goal \"Compose and send the digest email\"",
        "    in \"summaries.json\" out \"receipt.eml\"",
        "    approval before-send   retries 0",
        "}",
        "",
        "edge from=\"Fetcher\" to=\"Summarizer\"",
        "edge from=\"Summarizer\" to=\"Mailer\"",
        "",
        "chips {",
        "    scheduling cron=\"0 8 * * *\" tz=\"Asia/Kolkata\"",
        "    secrets smtp=\"keychain:wright/hn-digest/smtp\"",
        "    tests on   logging on",
        "}",
        "",
        "guardrails {",
        "    budget per-firing=500000 daily=3000000  // micro-USD",
        "    wall-clock 300   max-turns 40",
        "}",
        "",
        "deliverable kind=\"email\" {",
        "    check \"Mail received 08:00-08:05 local, every story",
        "           present, no dead links\"",
        "}",
    ];
    // The Confirm Gate's own words for it — `core/src/confirm.rs`'s output,
    // which is generated from the model by code, never by a model.
    let plan = [
        "$ wright plan .",
        "Wright will build \"HN Daily Digest\".",
        "In your words: Daily digest of top HN stories, summarized, emailed at 8am.",
        "",
        "Fetcher will: Fetch top 30 HN stories: id, title, url, points (writes stories.json).",
        "  Fetcher may use the network.",
        "  Fetcher reads live data, so it runs fresh every time.",
        "Summarizer will: Write a punchy two-sentence summary per story",
        "  (reads stories.json, writes summaries.json).",
        "  Summarizer is fully sandboxed: no files outside its outputs, no network, no programs.",
        "Mailer will: Compose and send the digest email (reads summaries.json, writes receipt.eml).",
        "  Mailer touches the outside world. It runs only after the checks pass, and is",
        "  never retried. It will ask you before each send.",
        "",
        "Before anything leaves your machine, Testwright writes and runs checks and",
        "Reviewer must approve the work.",
        "It runs every day at 8am (Asia/Kolkata), while Wright's background service is running.",
        "It spends at most $0.50 per run and $3.00 per day, and stops after 300 seconds",
        "or 40 model turns.",
        "Done means: Mail received 08:00-08:05 local, every story present, no dead links.",
    ];

    section(
        "THE BLUEPRINT",
        "One file is the whole project. The plan is read, not interpreted.",
        Some(
            "The blueprint is typed and validated at the gate — agents, \
             edges, chips, guardrails, deliverables — and diffable by \
             structure rather than text. The plan beside it is generated from \
             that model by code: the same file, in plain words, before \
             anything runs.",
        ),
        Flex::column()
            .cross_axis_alignment(CrossAxisAlignment::Stretch)
            .spacing(14.0)
            .children(children![
                SizedBox::height(6.0),
                code_block(
                    &kdl,
                    if wide { 12.5 } else { 11.0 },
                    "blueprint.kdl",
                    "kdl"
                ),
                terminal_block(
                    &plan,
                    if wide { 12.5 } else { 11.0 },
                    "the confirm gate — generated from the blueprint, never by a model"
                ),
            ])
            .into(),
    )
}

// ─── the cache ────────────────────────────────────────────────────────────

/// The refire story: what `wright diff` says before anything runs. This is
/// the feature that makes a repeatable project cheap to change — and the
/// words below are the command's own output, from the README.
fn cache(_column: f32, wide: bool) -> WidgetNode {
    let diff = [
        "$ wright diff",
        "This change refires Summarizer + Mailer + Testwright + Reviewer.",
        "Fetcher is fresh every run anyway (live data or delivery).",
    ];

    let body: WidgetNode = if wide {
        Flex::row()
            .cross_axis_alignment(CrossAxisAlignment::Start)
            .spacing(28.0)
            .children(children![
                Flexible::expanded(1).child(
                    Flex::column()
                        .cross_axis_alignment(CrossAxisAlignment::Start)
                        .spacing(10.0)
                        .children(children![
                            Text::new("Change one line. Run a fifth of the project.")
                                .size(17.0)
                                .color(Color::WHITE)
                                .bold(),
                            Text::new(
                                "Every task's inputs are hashed into its cache key, and a \
                                 task whose inputs did not change is served from cache. The \
                                 freshness rules are the product's own honesty: live-data \
                                 steps and delivery steps are never served from cache, \
                                 whatever the key says."
                            )
                            .size(13.5)
                            .color(INK_2),
                            Text::new(
                                "The cache is what makes a schedule affordable — the 8am \
                                 digest pays for one Fetcher, and the rest is yesterday's \
                                 work with today's stories."
                            )
                            .size(13.5)
                            .color(INK_2),
                        ]),
                ),
                Flexible::expanded(1).child(terminal_block(&diff, 12.5, "wright diff — what an edit refires, before anything runs")),
            ])
            .into()
    } else {
        Flex::column()
            .cross_axis_alignment(CrossAxisAlignment::Stretch)
            .spacing(16.0)
            .children(children![
                Text::new("Change one line. Run a fifth of the project.")
                    .size(17.0)
                    .color(Color::WHITE)
                    .bold(),
                Text::new(
                    "Every task's inputs are hashed into its cache key, and a task \
                     whose inputs did not change is served from cache. Live-data \
                     steps and delivery steps are never served from cache, whatever \
                     the key says."
                )
                .size(13.5)
                .color(INK_2),
                terminal_block(&diff, 11.0, "wright diff"),
            ])
            .into()
    };

    section(
        "THE CACHE",
        "Refire, not re-run.",
        None,
        Flex::column()
            .cross_axis_alignment(CrossAxisAlignment::Stretch)
            .children(children![SizedBox::height(6.0), body])
            .into(),
    )
}

// ─── features ─────────────────────────────────────────────────────────────

/// A card in the feature grid.
fn card(tag: &str, title: &str, body: &str) -> WidgetNode {
    Container::new()
        .color(SURFACE)
        .radius(10.0)
        .border(Border::thin(LINE_STRONG))
        .padding(EdgeInsets::all(18.0))
        .child(
            Flex::column()
                .cross_axis_alignment(CrossAxisAlignment::Start)
                .spacing(9.0)
                .children(children![
                    // The stage glyph in its own tinted square, which is what
                    // stops a grid of cards reading as a wall of paragraphs.
                    Container::new()
                        .color(WASH)
                        .radius(8.0)
                        .padding(EdgeInsets::all(7.0))
                        .child(Mark::widget(ACCENT_INK, 18.0)),
                    Text::new(spaced(tag)).style(mono(9.5, INK_3)),
                    Text::new(title.to_string()).size(14.5).color(Color::WHITE).bold(),
                    Text::new(body.to_string()).size(13.0).color(INK_2),
                ]),
        )
        .into()
}

/// Six things the product does, two across when there is room. The words are
/// the README's own claims — each card is a feature the repository can point
/// at.
fn features(_column: f32, wide: bool) -> WidgetNode {
    let cards = [
        (
            "SANDBOX",
            "Landlock and a network namespace",
            "Tools run inside it on Linux — files outside a step's outputs, the \
             network, other programs, all refused. On macOS and Windows the \
             sandboxed tools refuse loudly rather than run unprotected, which \
             is the honest failure.",
        ),
        (
            "MODELS",
            "Built in. No key, no account",
            "Qwen3-1.7B for text and tool calls, Whisper base for your voice, \
             both run in-process by Wright itself. Or bring a provider: \
             Anthropic, OpenAI-compatible, DeepSeek, GLM, Kimi, xAI, or a local \
             server on http://.",
        ),
        (
            "RECEIPTS",
            "Durable runs, resume after a crash",
            "Every run is a fsynced event log with BLAKE3 content-addressed \
             objects. wright resume continues a run that crashed or was \
             stopped; wright replay reconstructs a past run from its log, at \
             any point, without re-executing anything.",
        ),
        (
            "GUARDRAILS",
            "Enforced at compile time, never at 3AM",
            "A budget per firing and per day in micro-USD, a wall clock, a turn \
             limit — compiled into the plan, not checked after the money is \
             spent. The Confirm Gate says what it will spend before you \
             approve it.",
        ),
        (
            "SCHEDULES",
            "cron, in your own time zone",
            "Chips carry the schedule — cron plus an IANA zone, resolved \
             through the OS's own TZif. A background daemon fires them, and the \
             systemd or launchd unit installer puts it in the right place for \
             the machine.",
        ),
        (
            "SECRETS",
            "The OS keychain, by reference",
            "security on macOS, secret-tool on Linux, environment variables \
             for CI. The blueprint names a secret; the value never crosses a \
             prompt, a log, or the config file. wright secret env prints the \
             variable name to set.",
        ),
    ];

    let cells: Vec<WidgetNode> = cards
        .iter()
        .map(|(tag, title, body)| card(tag, title, body))
        .collect();

    let body: WidgetNode = if wide {
        let rows: Vec<WidgetNode> = cells
            .chunks(2)
            .map(|pair| {
                let mut row: Vec<WidgetNode> = pair
                    .iter()
                    .map(|card_node| {
                        Flexible::expanded(1).child(card_node.clone()).into()
                    })
                    .collect();
                if pair.len() == 1 {
                    row.push(Flexible::expanded(1).child(SizedBox::shrink()).into());
                }
                Flex::row()
                    .cross_axis_alignment(CrossAxisAlignment::Stretch)
                    .spacing(14.0)
                    .children(row)
                    .into()
            })
            .collect();
        Flex::column()
            .cross_axis_alignment(CrossAxisAlignment::Stretch)
            .spacing(14.0)
            .children(rows)
            .into()
    } else {
        Flex::column()
            .cross_axis_alignment(CrossAxisAlignment::Stretch)
            .spacing(12.0)
            .children(cells)
            .into()
    };

    section(
        "WHAT IS IN IT",
        "An agent harness with the receipts built in.",
        None,
        Flex::column()
            .cross_axis_alignment(CrossAxisAlignment::Stretch)
            .children(children![SizedBox::height(6.0), body])
            .into(),
    )
}

// ─── get started ─────────────────────────────────────────────────────────

/// From nothing to a firing, in one block. These are the README's own
/// commands, in its own order — `doctor` first, because it answers "what
/// works on this machine" before the question costs an evening.
fn get_started(_column: f32, wide: bool) -> WidgetNode {
    let quickstart = [
        "$ cargo build --release",
        "$ ./target/release/wright doctor",
        "$ ./target/release/wright model pull        # the built-in models, once",
        "$ ./target/release/wright new .             # describe it; confirm the plan",
        "$ ./target/release/wright run . --yes       # fire it; watch the event river",
        "$ ./target/release/wright run . --yes       # again: unchanged work is cached",
    ];

    let steps = [
        (
            "1",
            "Build it, or download it",
            "The headless CLI has zero external crates and builds anywhere a \
             C linker is. The installers carry the same binary with the window.",
        ),
        (
            "2",
            "Ask the doctor",
            "wright doctor reports what works on this machine — the sandbox, \
             the keychain, the built-in models, HTTPS — before a run has to.",
        ),
        (
            "3",
            "Describe the first project",
            "No key set up? It still runs — a model with no provider falls \
             back to the built-in qwen3-1.7b, and the event river says so.",
        ),
    ];

    let step_cells: Vec<WidgetNode> = steps
        .iter()
        .map(|(number, title, body)| {
            Flexible::expanded(1).child(
                Flex::column()
                    .cross_axis_alignment(CrossAxisAlignment::Start)
                    .spacing(6.0)
                    .children(children![
                        Flex::row()
                            .cross_axis_alignment(CrossAxisAlignment::Center)
                            .spacing(9.0)
                            .children(children![
                                Container::new()
                                    .color(WASH)
                                    .radius(999.0)
                                    .border(Border::thin(ACCENT))
                                    .padding(EdgeInsets::all(6.0))
                                    .child(
                                        Text::new((*number).to_string())
                                            .style(mono(11.0, ACCENT_INK).bold()),
                                    ),
                                Text::new((*title).to_string())
                                    .size(14.5)
                                    .color(Color::WHITE)
                                    .bold(),
                            ]),
                        Text::new((*body).to_string()).size(13.0).color(INK_2),
                    ]),
            )
            .into()
        })
        .collect();

    let steps_row: WidgetNode = if wide {
        Flex::row()
            .cross_axis_alignment(CrossAxisAlignment::Start)
            .spacing(18.0)
            .children(step_cells)
            .into()
    } else {
        Flex::column()
            .cross_axis_alignment(CrossAxisAlignment::Stretch)
            .spacing(16.0)
            .children(step_cells)
            .into()
    };

    section(
        "GET STARTED",
        "No key needed for the first project.",
        Some(
            "The built-in models download on first use — 1.9 GB, once, resumable, \
             checked against pinned SHA-256s — or ahead of time with \
             wright model pull.",
        ),
        Flex::column()
            .cross_axis_alignment(CrossAxisAlignment::Stretch)
            .spacing(20.0)
            .children(children![
                SizedBox::height(6.0),
                steps_row,
                terminal_block(
                    &quickstart,
                    if wide { 12.5 } else { 11.0 },
                    "the README's own quick start — no key needed"
                ),
            ])
            .into(),
    )
}

// ─── downloads ───────────────────────────────────────────────────────────

/// One row of the download table, tappable.
fn asset_row(asset: &'static Asset, wide: bool) -> WidgetNode {
    let target = url_for(asset.file);

    CursorArea::new(Cursor::Pointer)
        .child(
            Pressable::sensed(move |sense: Sense| {
                let left = Flex::column()
                    .cross_axis_alignment(CrossAxisAlignment::Start)
                    .spacing(3.0)
                    .children(children![
                        Text::new(asset.label.to_string()).size(13.5).color(Color::WHITE),
                        Text::new(asset.file.to_string()).style(mono(11.5, ACCENT_INK)),
                    ]);

                let row: WidgetNode = if wide {
                    Flex::row()
                        .cross_axis_alignment(CrossAxisAlignment::Center)
                        .spacing(12.0)
                        .children(children![
                            Flexible::expanded(1).child(left),
                            Text::new(asset.size.to_string()).style(mono(12.0, INK_3)),
                            chip("Pending", INK_3),
                        ])
                        .into()
                } else {
                    Flex::column()
                        .cross_axis_alignment(CrossAxisAlignment::Start)
                        .spacing(6.0)
                        .children(children![
                            left,
                            Flex::row().spacing(8.0).children(children![
                                Text::new(asset.size.to_string()).size(12.0).color(INK_3),
                                chip("Pending", INK_3),
                            ]),
                        ])
                        .into()
                };

                AnimatedContainer::new()
                    .duration(Duration::from_millis(120))
                    .curve(Curve::EASE_OUT)
                    .color(press_tint(sense.press.max(sense.hover * 0.55)))
                    .padding(EdgeInsets::symmetric(16.0, 13.0))
                    .child(row)
                    .into()
            })
            .on_tap(move || navigate(&target)),
        )
        .into()
}

/// The row's fill at a given press amount: the surface, moving toward the
/// accent wash. Flat at rest, so an untouched table is a table and not a
/// stack of buttons.
fn press_tint(press: f32) -> Color {
    let mix = press.clamp(0.0, 1.0);
    let lerp = |from: u8, to: u8| -> u8 {
        (f32::from(from) + (f32::from(to) - f32::from(from)) * mix).round() as u8
    };
    Color::rgb(lerp(0x25, 0x2E), lerp(0x25, 0x3F), lerp(0x26, 0x5E))
}

/// The download section: the buttons, then every file.
fn downloads(column: f32, wide: bool, host: Option<Os>) -> WidgetNode {
    let mut rows: Vec<WidgetNode> = Vec::new();
    for (index, asset) in ASSETS.iter().enumerate() {
        if index > 0 {
            rows.push(
                Container::new()
                    .color(LINE)
                    .child(SizedBox::height(1.0))
                    .into(),
            );
        }
        rows.push(asset_row(asset, wide));
    }

    let table = Container::new()
        .color(SURFACE)
        .radius(10.0)
        .border(Border::thin(LINE_STRONG))
        .child(
            Flex::column()
                .cross_axis_alignment(CrossAxisAlignment::Stretch)
                .children(rows),
        );

    let checksums = url_for("SHA256SUMS.txt");

    section(
        "DOWNLOAD",
        "One binary. Every platform its own installer.",
        Some(
            "The headless binary has zero external crates; the window adds \
             exactly one dependency — vieww. Everything Wright remembers lives \
             in ~/.wright, and uninstalling never touches it or your projects.",
        ),
        Flex::column()
            .cross_axis_alignment(CrossAxisAlignment::Stretch)
            .spacing(14.0)
            .children(children![
                SizedBox::height(2.0),
                download_button(host, column),
                table,
                Flex::row()
                    .main_axis_alignment(MainAxisAlignment::Start)
                    .children(children![CursorArea::new(Cursor::Pointer).child(
                        Button::new("Checksums (SHA256SUMS.txt)")
                            .style(ButtonStyle::Text)
                            .on_pressed(move || navigate(&checksums)),
                    ),]),
            ])
            .into(),
    )
}

// ─── the closing call ─────────────────────────────────────────────────────

/// The end of the pitch, in the README's own words: you write it, agents
/// wright it, the artifact is wrought.
fn closing(wide: bool) -> WidgetNode {
    let _ = wide;
    Container::new()
        .padding(EdgeInsets::symmetric(0.0, 64.0))
        .child(
            Flex::column()
                .cross_axis_alignment(CrossAxisAlignment::Center)
                .spacing(0.0)
                .children(children![
                    SizedBox::from_size(Size::new(44.0, 44.0)).child(mark(44.0)),
                    SizedBox::height(22.0),
                    Text::new("You write it. Agents wright it.")
                        .size(34.0)
                        .color(Color::WHITE)
                        .bold()
                        .align(TextAlign::Center),
                    Text::new("The artifact is wrought.")
                        .size(34.0)
                        .color(ACCENT_INK)
                        .bold()
                        .align(TextAlign::Center),
                    SizedBox::height(16.0),
                    Text::new(
                        "Wright (Old English: maker) — MIT, one dependency for the window, \
                         built by the same framework it runs on."
                    )
                    .size(14.0)
                    .color(INK_2)
                    .align(TextAlign::Center),
                ]),
        )
        .into()
}

// ─── the footer ───────────────────────────────────────────────────────────

/// The end of the page: three columns of links, then the small print.
fn footer(column: f32, wide: bool) -> WidgetNode {
    let base = format!("https://github.com/{}", repo());
    let groups: [(&str, Vec<(&str, String)>); 3] = [
        (
            "WRIGHT",
            vec![
                ("Download", "#download".into()),
                ("Get started", "#start".into()),
                ("Live demo", "#demo".into()),
            ],
        ),
        (
            "HOW IT WORKS",
            vec![
                ("The flow", "#flow".into()),
                ("The firing", "#firing".into()),
                ("The blueprint", "#blueprint".into()),
                ("The cache", "#cache".into()),
            ],
        ),
        (
            "PROJECT",
            vec![
                ("Source", base.clone()),
                ("Releases", format!("{base}/releases")),
                ("Issues", format!("{base}/issues")),
            ],
        ),
    ];

    let columns: Vec<WidgetNode> = groups
        .into_iter()
        .map(|(heading, links)| {
            let mut items: Vec<WidgetNode> = vec![
                Text::new(spaced(heading)).style(mono(9.5, INK_3)).into(),
                SizedBox::height(4.0).into(),
            ];
            for (label, url) in links {
                items.push(
                    CursorArea::new(Cursor::Pointer)
                        .child(
                            Pressable::sensed(move |sense: Sense| {
                                Text::new(label.to_string())
                                    .size(13.0)
                                    .color(if sense.emphasis() > 0.0 {
                                        ACCENT_INK
                                    } else {
                                        INK_2
                                    })
                                    .into()
                            })
                            .on_tap(move || navigate(&url)),
                        )
                        .into(),
                );
            }
            Flexible::expanded(1)
                .child(
                    Flex::column()
                        .cross_axis_alignment(CrossAxisAlignment::Start)
                        .spacing(7.0)
                        .children(items),
                )
                .into()
        })
        .collect();

    let link_row: WidgetNode = if wide {
        Flex::row()
            .cross_axis_alignment(CrossAxisAlignment::Start)
            .spacing(24.0)
            .children(columns)
            .into()
    } else {
        Flex::column()
            .cross_axis_alignment(CrossAxisAlignment::Start)
            .spacing(22.0)
            .children(columns)
            .into()
    };

    Container::new()
        .padding(EdgeInsets::symmetric(0.0, 40.0))
        .child(
            Flex::column()
                .cross_axis_alignment(CrossAxisAlignment::Stretch)
                .spacing(28.0)
                .children(children![
                    link_row,
                    Container::new()
                        .color(LINE)
                        .child(SizedBox::from_size(Size::new(column, 1.0))),
                    if wide {
                        Flex::row()
                            .cross_axis_alignment(CrossAxisAlignment::Center)
                            .spacing(9.0)
                            .children(children![
                                SizedBox::from_size(Size::new(16.0, 16.0)).child(mark(16.0)),
                                Text::new("Wright 0.5.1 — MIT")
                                    .style(mono(11.5, INK_3)),
                                Flexible::expanded(1).child(SizedBox::shrink()),
                                Text::new(
                                    "Built with vieww. This page is a vieww widget tree, rendered to DOM."
                                )
                                .style(mono(10.5, INK_3)),
                            ])
                    } else {
                        Flex::column()
                            .cross_axis_alignment(CrossAxisAlignment::Start)
                            .spacing(8.0)
                            .children(children![
                                Flex::row()
                                    .cross_axis_alignment(CrossAxisAlignment::Center)
                                    .spacing(9.0)
                                    .children(children![
                                        SizedBox::from_size(Size::new(16.0, 16.0))
                                            .child(mark(16.0)),
                                        Text::new("Wright 0.5.1 — MIT")
                                            .style(mono(11.5, INK_3)),
                                    ]),
                                Text::new(
                                    "Built with vieww. This page is a vieww widget tree."
                                )
                                .style(mono(10.5, INK_3)),
                            ])
                    },
                ]),
        )
        .into()
}

/// A tiny helper so a section builder can be written as an expression.
trait Pipe: Sized {
    fn pipe<T>(self, f: impl FnOnce(Self) -> T) -> T {
        f(self)
    }
}

impl<T> Pipe for T {}

// ─── the whole page ───────────────────────────────────────────────────────

/// The whole page, mounted on `driver`.
///
/// Takes the driver rather than the pieces because everything the page needs
/// has to come off *this* tree: a signal created on another runtime marks
/// nothing pending, and an animation attached to no tickers never advances.
pub fn root(driver: &mut FrameDriver, host: Option<Os>) -> WidgetNode {
    // Before anything is built: a font store swapped after the first layout
    // would remeasure every string on the page.
    driver.set_fonts(fonts());

    let runtime = driver.elements().runtime().clone();

    let scroll = ScrollController::new(&runtime, ScrollPhysics::android());

    // One second, once. `EASE_OUT` because everything it moves is *arriving*:
    // it has to leave quickly and settle, rather than accelerate away.
    let entrance = Animation::new(
        &runtime,
        Tween::new(0.0_f32, 1.0),
        Duration::from_millis(1000),
    )
    .curve(Curve::EASE_OUT);

    // The demo's own clock, on the same runtime and the same tickers — a
    // firing that plays once to the gate and holds there, waiting.
    let (fire, deliver, gate) = start_demo(&runtime, driver.tickers());

    scroll.attach(driver.tickers());
    entrance.attach(driver.tickers());
    entrance.forward(Duration::ZERO);

    let anchors = Anchors::new(&runtime);
    page(scroll, entrance, host, fire, deliver, gate, Art::embedded(), anchors)
}

/// The tree, with everything it needs already built.
///
/// Split from [`root`] so a caller that has its own animation clock — a
/// test, or `examples/render.rs`, which has to render the page *after* the
/// entrance and the demo's firing rather than during them — can supply the
/// pieces itself.
#[must_use]
pub fn page(
    scroll: ScrollController,
    entrance: Animation<f32>,
    host: Option<Os>,
    fire: Animation<f32>,
    deliver: Animation<f32>,
    gate: Signal<u8>,
    art: Art,
    anchors: Anchors,
) -> WidgetNode {
    Theme::new(theme())
        .child(LayoutBuilder::new(move |constraints: Constraints| {
            // The tree lays out in logical points; the device-pixel ratio
            // belongs to the backend, which applies it to the finished,
            // flattened scene — where there are no repaint boundaries left to
            // lose it.
            let surface = Size::new(constraints.max_width, constraints.max_height);
            SizedBox::from_size(surface)
                .child(Site {
                    scroll: scroll.clone(),
                    surface,
                    host,
                    entrance: entrance.clone(),
                    fire: fire.clone(),
                    deliver: deliver.clone(),
                    gate: gate.clone(),
                    art: art.clone(),
                    anchors: anchors.clone(),
                })
                .into()
        }))
        .into()
}

// ─── the entry point ──────────────────────────────────────────────────────

/// Mount the page on the DOM.
///
/// Called by the browser through wasm-bindgen's start shim, so the host page
/// is one `<script type="module">` and nothing else. **DOM, not canvas**: the
/// page is a vieww widget tree either way; this hands it to
/// `vieww-platform-web-dom`, which walks the element tree and emits elements
/// and CSS instead of scene commands. The rasteriser is still here — the live
/// demo below is a real vieww application mounted on a `<canvas>`, painted
/// by vieww's own renderer.
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen(start)]
pub fn start() -> Result<(), wasm_bindgen::JsValue> {
    let host = detect();

    vieww_platform_web_dom::set_color_tokens(dom::COLOR_TOKENS);
    vieww_platform_web_dom::DomApp::mount("app", dom::page(host))
        .map(std::mem::forget)
        .map_err(|error| error.to_string())?;
    Ok(())
}
