//! kit — the film's own foundation.
//!
//! The palette, the clock discipline, the deterministic RNG, the easing
//! curves, and the atmosphere primitives every scene shares: grounds,
//! glows, grain, vignettes, rules, grids, particle fields.
//!
//! **House rules, inherited from the ledger and enforced here where they
//! can be:**
//! - A frame is a *pure function of film-time*. Nothing in this crate reads
//!   the wall clock or a global RNG; `Rng` is seeded per call site, so the
//!   film re-renders to the byte on a given bench.
//! - Nothing is added in post. Every glow, every blur, every grain grain is
//!   a `Sketch` the framework's own rasterizer draws.
//! - Numbers printed on screen come from [`crate::film::Probe`] — the film's
//!   own census — or from files in this repository. None is typed by hand
//!   into a caption.

use vieww_foundation::{
    BlendMode, Color, FontWeight, Gradient, Offset, Path, Rect, Size, Sketchbook, TextAlign,
    TextStyle, Transform,
};
use vieww_widget::prelude::*;
use vieww_widget::{Opacity, PaintWith, Painting};

// ── The frame ───────────────────────────────────────────────────────────────

pub const W: f32 = 1920.0;
pub const H: f32 = 1080.0;
pub const CANVAS: Size = Size::new(W, H);
pub const FPS: f32 = 60.0;

/// The 9:16 safe column — the vertical cutdown's window. Key information
/// stays inside it; spectacle may spill.
pub const SAFE_X0: f32 = W * 0.5 - (H * 9.0 / 16.0) * 0.5;
pub const SAFE_X1: f32 = W * 0.5 + (H * 9.0 / 16.0) * 0.5;

// ── The palette ─────────────────────────────────────────────────────────────
//
// **Taken from `apps/viewwsite`, not invented here.** The brand's ground is a
// *warm* near-black (`#0F0D0B`) with a stone-grey ink and a lavender accent
// — not the cool blue-violet a film about a Rust IDE would reach for by
// reflex. Every constant below is the site's own value, copied from
// `viewwsite/src/lib.rs` (and `index.html`'s custom properties, which agree
// with it), so the film and the product's front door are the same colour.
//
// | role | value | source |
// |---|---|---|
// | ground | `#0F0D0B` | `GROUND` |
// | surface / raised | `#1B1715` / `#2B2521` | `SURFACE`, `SURFACE_2` |
// | hairline | `#2A2826` | `LINE` |
// | ink / second / third | `#F8F4F2` / `#A69C95` / `#78716B` | `INK`, `INK_2`, `INK_3` |
// | accent / deep | `#B491FF` / `#7E5CE8` | `ACCENT`, `ACCENT_DEEP` |
// | accent wash | `#221C33` | `WASH` |
//
// The studio's *chrome* is a different palette again — the IDE's own, in
// `crate::studio` — because the product does not paint its editor in its
// marketing colours, and the film would be lying if it did.

/// `#0F0D0B` — the brand ground. The film's black.
pub const GROUND: Color = Color::rgb(0x0F, 0x0D, 0x0B);
/// Deeper than the ground, for the frames that close in.
pub const VOID: Color = Color::rgb(0x08, 0x07, 0x06);
pub const BG: Color = GROUND;
pub const BG_DEEP: Color = Color::rgb(0x0B, 0x09, 0x08);
/// `#1B1715` — a raised surface.
pub const SURFACE: Color = Color::rgb(0x1B, 0x17, 0x15);
/// `#2B2521` — raised again: cards, chips, pressed things.
pub const SURFACE_2: Color = Color::rgb(0x2B, 0x25, 0x21);
pub const BG_LIFT: Color = SURFACE;

/// `#F8F4F2` — the ink.
pub const INK: Color = Color::rgb(0xF8, 0xF4, 0xF2);
/// `#A69C95` — second-rank text.
pub const INK_SOFT: Color = Color::rgb(0xA6, 0x9C, 0x95);
pub const MUTED: Color = INK_SOFT;
/// `#78716B` — third-rank text; the quietest thing still meant to be read.
pub const FAINT: Color = Color::rgb(0x78, 0x71, 0x6B);
/// `#2A2826` — the hairline.
pub const HAIR: Color = Color::rgb(0x2A, 0x28, 0x26);

/// `#B491FF` — **the accent.** viewwstudio's colour, and the site's.
pub const VIOLET: Color = Color::rgb(0xB4, 0x91, 0xFF);
/// The accent's lighter end, for glows and the specular band.
pub const VIOLET_SOFT: Color = Color::rgb(0xCB, 0xB2, 0xFF);
/// `#7E5CE8` — the accent's deep stop.
pub const VIOLET_DEEP: Color = Color::rgb(0x7E, 0x5C, 0xE8);
/// `#221C33` — the accent's wash, for tinted grounds.
pub const WASH: Color = Color::rgb(0x22, 0x1C, 0x33);

/// The framework's own colour, from the site's syntax ramp (`SYN_TYPE`) —
/// vieww reads as the cyan under the studio's lavender.
pub const CYAN: Color = Color::rgb(0x48, 0xD7, 0xFE);
pub const CYAN_SOFT: Color = Color::rgb(0x9A, 0xE9, 0xFF);
/// `SYN_KEYWORD` — the site's magenta.
pub const MAGENTA: Color = Color::rgb(0xEF, 0xA3, 0xFF);
/// `SYN_STRING` — the green the site prints receipts in.
pub const MINT: Color = Color::rgb(0x59, 0xD3, 0x8C);
/// `SYN_MACRO` — the site's amber.
pub const AMBER: Color = Color::rgb(0xFE, 0xB2, 0x63);
/// `SYN_NUMBER` — the site's red, and the film's only alarm colour.
pub const RED: Color = Color::rgb(0xFF, 0x8F, 0x9A);

/// Chrome tones for the film's own furniture — cards and chips that are not
/// the studio. Warm, like the ground they sit on.
pub const CHROME: Color = SURFACE;
pub const CHROME_HI: Color = SURFACE_2;
pub const CHROME_LO: Color = Color::rgb(0x14, 0x11, 0x10);
pub const PANE: Color = Color::rgb(0x12, 0x10, 0x0F);
pub const EDGE: Color = HAIR;

// ── Colour arithmetic ───────────────────────────────────────────────────────

#[must_use]
pub fn alpha(c: Color, a: f32) -> Color {
    Color::rgba(c.r, c.g, c.b, (a * 255.0).clamp(0.0, 255.0) as u8)
}

#[must_use]
pub fn mix(a: Color, b: Color, t: f32) -> Color {
    a.lerp(b, t.clamp(0.0, 1.0))
}

#[must_use]
pub fn tint(c: Color, t: f32) -> Color {
    mix(c, Color::WHITE, t)
}

#[must_use]
pub fn shade(c: Color, t: f32) -> Color {
    mix(c, Color::BLACK, t)
}

/// Multiply channels by a scalar — the Lambert term's friend.
#[must_use]
pub fn scaled(c: Color, k: f32) -> Color {
    Color::rgb(
        (f32::from(c.r) * k).clamp(0.0, 255.0) as u8,
        (f32::from(c.g) * k).clamp(0.0, 255.0) as u8,
        (f32::from(c.b) * k).clamp(0.0, 255.0) as u8,
    )
}

/// Drain the colour toward its own luminance — Act I's world, before the
/// product arrives. A *measured* desaturation (Rec. 709), not a mood.
#[must_use]
pub fn drained(c: Color, amount: f32) -> Color {
    let y = 0.2126 * f32::from(c.r) + 0.7152 * f32::from(c.g) + 0.0722 * f32::from(c.b);
    let grey = Color::rgb(y as u8, y as u8, y as u8);
    let m = mix(c, grey, amount);
    Color::rgba(m.r, m.g, m.b, c.a)
}

/// A rect from origin + size. `Rect::new` takes *edges*; this is the safe
/// spelling for offset rects.
#[must_use]
pub fn xywh(x: f32, y: f32, w: f32, h: f32) -> Rect {
    Rect::new(x, y, x + w.max(0.0), y + h.max(0.0))
}

// ── Easing ──────────────────────────────────────────────────────────────────

#[must_use]
pub fn clamp01(t: f32) -> f32 {
    t.clamp(0.0, 1.0)
}

/// Map `x` from `[a, b]` onto `[0, 1]`, clamped — the film's workhorse.
/// Every staged reveal is a `seg` and a curve.
#[must_use]
pub fn seg(x: f32, a: f32, b: f32) -> f32 {
    if (b - a).abs() < 1e-6 {
        return if x >= b { 1.0 } else { 0.0 };
    }
    clamp01((x - a) / (b - a))
}

#[must_use]
pub fn smoothstep(t: f32) -> f32 {
    let t = clamp01(t);
    t * t * (3.0 - 2.0 * t)
}

#[must_use]
pub fn smootherstep(t: f32) -> f32 {
    let t = clamp01(t);
    t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
}

#[must_use]
pub fn ease_out_cubic(t: f32) -> f32 {
    let t = clamp01(t);
    1.0 - (1.0 - t).powi(3)
}

#[must_use]
pub fn ease_out_quint(t: f32) -> f32 {
    let t = clamp01(t);
    1.0 - (1.0 - t).powi(5)
}

#[must_use]
pub fn ease_in_cubic(t: f32) -> f32 {
    let t = clamp01(t);
    t * t * t
}

#[must_use]
pub fn ease_out_expo(t: f32) -> f32 {
    let t = clamp01(t);
    if t >= 1.0 {
        1.0
    } else {
        1.0 - 2.0f32.powf(-10.0 * t)
    }
}

#[must_use]
pub fn ease_in_out(t: f32) -> f32 {
    let t = clamp01(t);
    if t < 0.5 {
        2.0 * t * t
    } else {
        1.0 - (-2.0 * t + 2.0).powi(2) / 2.0
    }
}

#[must_use]
pub fn ease_out_back(t: f32) -> f32 {
    let t = clamp01(t);
    let c1 = 1.70158;
    let c3 = c1 + 1.0;
    1.0 + c3 * (t - 1.0).powi(3) + c1 * (t - 1.0).powi(2)
}

/// An analytic underdamped spring settle — the scrub-safe shadow of the
/// real `SpringAnimation`, used where a frame must be a pure function of
/// its own time.
#[must_use]
pub fn spring(t: f32, omega: f32, zeta: f32) -> f32 {
    let t = clamp01(t);
    let decay = (-zeta * omega * t).exp();
    1.0 - decay * ((1.0 - zeta * zeta).sqrt() * omega * t).cos()
}

/// A single strike that rings out and dies — attack, ring, silence.
#[must_use]
pub fn sting(x: f32, span: f32) -> f32 {
    if x < 0.0 {
        return 0.0;
    }
    let u = x / span.max(1e-4);
    (u * 6.2).sin().abs() * (-u * 1.9).exp()
}

/// Triangle window on `[0, 1]`.
#[must_use]
pub fn tri(t: f32) -> f32 {
    let t = t.fract().abs();
    if t < 0.5 { t * 2.0 } else { 2.0 - t * 2.0 }
}

/// A pulse that blooms and fades inside `[a, b]`.
#[must_use]
pub fn bump(x: f32, a: f32, b: f32) -> f32 {
    let u = seg(x, a, b);
    (u * std::f32::consts::PI).sin()
}

/// The wait's cadence: a 24 Hz clock sampled inside a 60 Hz render. Returns
/// the *held* value — the judder is a true 2-3-2-3 hold, not a rate change,
/// and it is the mechanism Act I argues with.
#[must_use]
pub fn held_24(sec: f32) -> f32 {
    (sec * 24.0).floor() / 24.0
}

// ── Deterministic RNG ───────────────────────────────────────────────────────

/// xorshift64* — tiny, seedable, reproducible. Grain re-renders to the byte.
#[derive(Clone)]
pub struct Rng {
    state: u64,
}

impl Rng {
    #[must_use]
    pub const fn new(seed: u64) -> Self {
        Self { state: seed | 1 }
    }

    pub fn u64(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.state = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// Uniform in `[0, 1)`.
    pub fn f01(&mut self) -> f32 {
        (self.u64() >> 11) as f32 / (1u64 << 53) as f32
    }

    /// Uniform in `[-1, 1]`.
    pub fn sym(&mut self) -> f32 {
        self.f01() * 2.0 - 1.0
    }

    /// Uniform in `[lo, hi)`.
    pub fn range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + self.f01() * (hi - lo)
    }

    pub fn below(&mut self, n: usize) -> usize {
        if n == 0 { 0 } else { (self.u64() % n as u64) as usize }
    }
}

/// Value noise in one dimension — smooth, seeded, cheap. The film's drifts
/// (camera float, flicker, dust) come from here rather than from a sine.
#[must_use]
pub fn noise1(x: f32, seed: u64) -> f32 {
    let i = x.floor();
    let f = x - i;
    let h = |k: f32| {
        let mut r = Rng::new(seed ^ ((k as i64 as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)));
        r.f01() * 2.0 - 1.0
    };
    let (a, b) = (h(i), h(i + 1.0));
    let u = f * f * (3.0 - 2.0 * f);
    a + (b - a) * u
}

/// Layered value noise — the atmosphere's texture.
#[must_use]
pub fn fbm1(x: f32, seed: u64, octaves: u32) -> f32 {
    let mut sum = 0.0;
    let mut amp = 0.5;
    let mut freq = 1.0;
    for o in 0..octaves {
        sum += amp * noise1(x * freq, seed ^ u64::from(o + 1) * 0x9E37);
        amp *= 0.5;
        freq *= 2.03;
    }
    sum
}

// ── Type ────────────────────────────────────────────────────────────────────

/// A positioned run of text — the film's only text spelling, so every
/// caption shares one set of habits.
pub struct Type {
    text: String,
    size: f32,
    color: Color,
    weight: FontWeight,
    tracking: f32,
    mono: bool,
    align: TextAlign,
    x: f32,
    y: f32,
    w: f32,
    line: f32,
    opacity: f32,
}

impl Type {
    #[must_use]
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            size: 22.0,
            color: INK,
            weight: FontWeight::Regular,
            tracking: 0.0,
            mono: false,
            align: TextAlign::Start,
            x: 0.0,
            y: 0.0,
            w: W,
            line: 1.35,
            opacity: 1.0,
        }
    }

    #[must_use]
    pub const fn size(mut self, s: f32) -> Self {
        self.size = s;
        self
    }
    #[must_use]
    pub const fn color(mut self, c: Color) -> Self {
        self.color = c;
        self
    }
    #[must_use]
    pub const fn weight(mut self, w: FontWeight) -> Self {
        self.weight = w;
        self
    }
    #[must_use]
    pub const fn bold(self) -> Self {
        self.weight(FontWeight::Bold)
    }
    #[must_use]
    pub const fn medium(self) -> Self {
        self.weight(FontWeight::Medium)
    }
    #[must_use]
    pub const fn light(self) -> Self {
        self.weight(FontWeight::Light)
    }
    #[must_use]
    pub const fn track(mut self, t: f32) -> Self {
        self.tracking = t;
        self
    }
    #[must_use]
    pub const fn mono(mut self) -> Self {
        self.mono = true;
        self
    }
    #[must_use]
    pub const fn center(mut self) -> Self {
        self.align = TextAlign::Center;
        self
    }
    #[must_use]
    pub const fn right(mut self) -> Self {
        self.align = TextAlign::End;
        self
    }
    #[must_use]
    pub const fn at(mut self, x: f32, y: f32) -> Self {
        self.x = x;
        self.y = y;
        self
    }
    #[must_use]
    pub const fn width(mut self, w: f32) -> Self {
        self.w = w;
        self
    }
    #[must_use]
    pub const fn leading(mut self, l: f32) -> Self {
        self.line = l;
        self
    }
    #[must_use]
    pub const fn opacity(mut self, a: f32) -> Self {
        self.opacity = a;
        self
    }

    /// Centre the run on the full frame width at `y`.
    #[must_use]
    pub const fn banner(mut self, y: f32) -> Self {
        self.x = 0.0;
        self.y = y;
        self.w = W;
        self.align = TextAlign::Center;
        self
    }

    #[must_use]
    pub fn node(self) -> WidgetNode {
        if self.opacity <= 0.004 || self.text.is_empty() {
            return Stack::new().into();
        }
        let mut style = TextStyle::new(self.size)
            .weight(self.weight)
            .letter_spacing(self.tracking)
            .line_height(self.line)
            .color(alpha(self.color, self.opacity * (f32::from(self.color.a) / 255.0)));
        if self.mono {
            style = style.monospace();
        }
        let lines = self.text.lines().count().max(1) as f32;
        let h = self.size * self.line * lines + 8.0;
        Stack::new()
            .push(
                Positioned::new()
                    .left(self.x)
                    .top(self.y)
                    .width(self.w)
                    .height(h)
                    .child(Text::new(self.text).style(style).align(self.align)),
            )
            .into()
    }
}

impl From<Type> for WidgetNode {
    fn from(t: Type) -> Self {
        t.node()
    }
}

/// The house caption — bottom-left, mono, tracked, faint. Every beat in the
/// film is captioned, because the cut has to work muted.
#[must_use]
pub fn caption(text: &str, a: f32) -> WidgetNode {
    if a <= 0.004 {
        return Stack::new().into();
    }
    let rule = Painting::sized(
        Size::new(3.0, 34.0),
        PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
            book.rrect(xywh(0.0, 0.0, 3.0, 34.0 * ease_out_cubic(a)), 1.5, alpha(VIOLET, 0.85));
        }),
    );
    Stack::new()
        .push(
            Positioned::new()
                .left(146.0)
                .top(H - 92.0)
                .width(3.0)
                .height(34.0)
                .child(rule),
        )
        .push(
            Type::new(text)
                .mono()
                .size(17.0)
                .track(1.6)
                .color(INK_SOFT)
                .opacity(a)
                .at(168.0, H - 88.0)
                .width(W - 336.0),
        )
        .into()
}

/// The act card — the movement's name and its kishōtenketsu character,
/// bottom-right, quiet, permanent for the act's first beats.
#[must_use]
pub fn act_card(roman: &str, name: &str, kanji: &str, a: f32) -> WidgetNode {
    if a <= 0.004 {
        return Stack::new().into();
    }
    Stack::new()
        .push(
            Type::new(kanji)
                .size(58.0)
                .light()
                .color(alpha(VIOLET_SOFT, 0.5))
                .opacity(a)
                .right()
                .at(W - 520.0, H - 172.0)
                .width(460.0),
        )
        .push(
            Type::new(format!("{roman} · {name}"))
                .mono()
                .size(15.0)
                .track(4.0)
                .color(alpha(MUTED, 0.9))
                .opacity(a)
                .right()
                .at(W - 520.0, H - 88.0)
                .width(460.0),
        )
        .into()
}

// ── Atmosphere ──────────────────────────────────────────────────────────────

/// The film's ground: a deep vertical ramp with a violet floor-glow and a
/// seeded star field that twinkles on film-time. Every dark scene starts
/// here so the black is the *same* black across a cut.
pub fn ground(book: &mut Sketchbook, size: Size, abs: f32, warmth: f32) {
    let (w, h) = (size.width, size.height);
    book.rect(
        Rect::new(0.0, 0.0, w, h),
        Gradient::vertical().with_dither().with_stops(&[
            (0.0, Color::rgb(7, 8, 12)),
            (0.48, BG_DEEP),
            (1.0, mix(BG_DEEP, VIOLET_DEEP, 0.12 * warmth)),
        ]),
    );
    let mut rng = Rng::new(0x5E_ED_51);
    for _ in 0..70 {
        let x = rng.f01() * w;
        let y = rng.f01() * h * 0.92;
        let r = 0.4 + rng.f01() * 1.0;
        let ph = rng.f01() * 9.0;
        let tw = 0.5 + 0.5 * (abs * 1.7 + ph).sin();
        book.circle(
            Offset::new(x, y),
            r,
            alpha(Color::WHITE, (0.025 + 0.055 * tw) * (0.4 + 0.6 * warmth)),
        );
    }
}

/// One low, wide glow — the horizon. Priced at one blurred layer, once.
pub fn horizon(book: &mut Sketchbook, size: Size, color: Color, strength: f32) {
    if strength <= 0.005 {
        return;
    }
    let (w, h) = (size.width, size.height);
    book.layer(1.0, 56.0, None, |g| {
        g.circle(
            Offset::new(w * 0.5, h * 1.04),
            w * 0.44,
            Gradient::radial_fill().with_dither().with_stops(&[
                (0.0, alpha(color, 0.16 * strength)),
                (1.0, alpha(color, 0.0)),
            ]),
        );
    });
}

/// A soft bloom around a point — the film's only way to make light. One
/// blurred layer per call, so callers group before they bloom (U-06).
pub fn bloom(book: &mut Sketchbook, at: Offset, radius: f32, color: Color, strength: f32) {
    if strength <= 0.005 {
        return;
    }
    book.blended_layer(1.0, radius * 0.22, BlendMode::Plus, None, |g| {
        g.circle(
            at,
            radius,
            Gradient::radial_fill().with_dither().with_stops(&[
                (0.0, alpha(color, 0.55 * strength)),
                (0.45, alpha(color, 0.18 * strength)),
                (1.0, alpha(color, 0.0)),
            ]),
        );
    });
}

/// The closing-in vignette. Last thing drawn, always.
pub fn vignette(book: &mut Sketchbook, size: Size, strength: f32) {
    book.rect(
        Rect::new(0.0, 0.0, size.width, size.height),
        Gradient::radial(Offset::new(0.5, 0.5), 0.78)
            .with_dither()
            .with_stops(&[
                (0.42, alpha(Color::BLACK, 0.0)),
                (1.0, alpha(Color::BLACK, 0.58 * strength)),
            ]),
    );
}

/// Seeded film grain — a sparse field of one-pixel-ish squares whose seed
/// advances with the *frame*, so it shimmers and still re-renders exactly.
pub fn grain(book: &mut Sketchbook, size: Size, frame: u32, strength: f32, count: usize) {
    if strength <= 0.004 {
        return;
    }
    let mut rng = Rng::new(0x_67A1_u64 ^ u64::from(frame).wrapping_mul(0x9E37_79B9));
    for _ in 0..count {
        let x = rng.f01() * size.width;
        let y = rng.f01() * size.height;
        let s = 1.0 + rng.f01() * 2.0;
        let v = rng.f01();
        book.rect(
            xywh(x, y, s, s),
            alpha(if v > 0.5 { Color::WHITE } else { Color::BLACK }, strength * (0.2 + 0.8 * v)),
        );
    }
}

/// Act I's scanlines — the degraded world's texture, drawn as real rects.
pub fn scanlines(book: &mut Sketchbook, size: Size, abs: f32, strength: f32) {
    if strength <= 0.004 {
        return;
    }
    let pitch = 3.0;
    let drift = (abs * 14.0) % pitch;
    let mut y = -pitch + drift;
    while y < size.height {
        book.rect(
            xywh(0.0, y, size.width, 1.0),
            alpha(Color::BLACK, 0.22 * strength),
        );
        y += pitch;
    }
    // The roll bar — one soft band sweeping down, the CRT's own tell.
    let band = ((abs * 0.28) % 1.0) * (size.height + 300.0) - 150.0;
    book.rect(
        xywh(0.0, band, size.width, 150.0),
        Gradient::vertical().with_dither().with_stops(&[
            (0.0, alpha(Color::WHITE, 0.0)),
            (0.5, alpha(Color::WHITE, 0.022 * strength)),
            (1.0, alpha(Color::WHITE, 0.0)),
        ]),
    );
}

/// A hairline rule — the film's separators, always one pixel, always faint.
pub fn rule_h(book: &mut Sketchbook, x: f32, y: f32, w: f32, c: Color) {
    book.rect(xywh(x, y, w, 1.0), c);
}

pub fn rule_v(book: &mut Sketchbook, x: f32, y: f32, h: f32, c: Color) {
    book.rect(xywh(x, y, 1.0, h), c);
}

/// The blueprint grid — a measured lattice that fades at the edges. The
/// film's "we are inside a system" cue.
pub fn grid(book: &mut Sketchbook, size: Size, pitch: f32, phase: Offset, strength: f32, c: Color) {
    if strength <= 0.004 {
        return;
    }
    let (w, h) = (size.width, size.height);
    book.layer(strength, 0.0, None, |g| {
        let mut x = phase.dx % pitch;
        while x < w {
            g.rect(
                xywh(x, 0.0, 1.0, h),
                Gradient::vertical().with_dither().with_stops(&[
                    (0.0, alpha(c, 0.0)),
                    (0.4, alpha(c, 0.26)),
                    (1.0, alpha(c, 0.0)),
                ]),
            );
            x += pitch;
        }
        let mut y = phase.dy % pitch;
        while y < h {
            g.rect(
                xywh(0.0, y, w, 1.0),
                Gradient::horizontal().with_dither().with_stops(&[
                    (0.0, alpha(c, 0.0)),
                    (0.5, alpha(c, 0.26)),
                    (1.0, alpha(c, 0.0)),
                ]),
            );
            y += pitch;
        }
    });
}

/// A dust field drifting on film-time — the air in the room, so a still
/// frame is never actually still.
pub fn dust(book: &mut Sketchbook, size: Size, abs: f32, count: usize, color: Color, strength: f32) {
    if strength <= 0.004 {
        return;
    }
    let mut rng = Rng::new(0xD_057);
    book.blended_layer(1.0, 0.0, BlendMode::Plus, None, |g| {
        for _ in 0..count {
            let x0 = rng.f01();
            let y0 = rng.f01();
            let sp = 0.02 + rng.f01() * 0.07;
            let ph = rng.f01() * 20.0;
            let r = 0.6 + rng.f01() * 1.7;
            let x = ((x0 + noise1(abs * 0.12 + ph, 0x11) * 0.03) % 1.0).abs() * size.width;
            let y = ((y0 - abs * sp * 0.1).rem_euclid(1.0)) * size.height;
            let tw = 0.35 + 0.65 * (0.5 + 0.5 * (abs * 2.1 + ph).sin());
            g.circle(Offset::new(x, y), r, alpha(color, 0.10 * tw * strength));
        }
    });
}

// ── Composition helpers ─────────────────────────────────────────────────────

/// A full-frame painter — the spelling every scene's background uses.
pub fn painter(f: impl Fn(&mut Sketchbook, Size) + Send + Sync + 'static) -> WidgetNode {
    Painting::sized(CANVAS, PaintWith::new(f)).into()
}

/// Stack children over a full-frame background painter.
pub fn frame_stack(bg: WidgetNode, layers: Vec<WidgetNode>) -> WidgetNode {
    let mut s = Stack::new().push(Positioned::fill().child(bg));
    for l in layers {
        s = s.push(Positioned::fill().child(l));
    }
    s.into()
}

/// Fade a subtree.
#[must_use]
pub fn fade(a: f32, child: WidgetNode) -> WidgetNode {
    if a <= 0.004 {
        return Stack::new().into();
    }
    Opacity::new(clamp01(a)).child(child).into()
}

/// The cut's own punctuation: a white flash the *scene* draws, so no
/// transition is ever added in post.
pub fn flash(book: &mut Sketchbook, size: Size, strength: f32, color: Color) {
    if strength <= 0.003 {
        return;
    }
    book.rect(
        Rect::new(0.0, 0.0, size.width, size.height),
        alpha(color, 0.9 * strength),
    );
}

/// A rounded panel with a hairline edge and an inner lift — the chrome
/// primitive the studio and the cards are both built from.
pub fn panel(book: &mut Sketchbook, r: Rect, radius: f32, fill: Color, edge: Color) {
    book.rrect(r, radius, fill);
    book.stroke_rrect(r, radius, edge, 1.0);
}

/// A soft drop shadow beneath a panel — one shadow, priced once.
pub fn panel_shadow(book: &mut Sketchbook, r: Rect, radius: f32, strength: f32) {
    book.shadow(
        r,
        radius,
        vieww_foundation::Shadow::new(
            alpha(Color::BLACK, 0.55 * strength),
            Offset::new(0.0, 18.0),
            42.0,
        ),
    );
}

/// The lower third: a scrim that darkens the bottom of a panel so a line of
/// the film's own type can be printed over the product without fighting it.
/// Cinema's oldest device, and the honest one — the chrome is still there,
/// still being drawn, just stepped back for a sentence.
pub fn lower_third(book: &mut Sketchbook, r: Rect, strength: f32) {
    if strength <= 0.004 {
        return;
    }
    let top = r.bottom - r.height() * 0.34;
    book.layer(1.0, 0.0, Some(Path::rounded_rect(r, 16.0)), |g| {
        g.rect(
            xywh(r.left, top, r.width(), r.bottom - top),
            Gradient::vertical().with_dither().with_stops(&[
                (0.0, alpha(Color::BLACK, 0.0)),
                (0.55, alpha(Color::BLACK, 0.62 * strength)),
                (1.0, alpha(Color::BLACK, 0.86 * strength)),
            ]),
        );
    });
}

/// A progress meter — the film uses it for builds, for bars, for loads.
pub fn meter(book: &mut Sketchbook, r: Rect, p: f32, track: Color, fill: Gradient) {
    let radius = r.height() * 0.5;
    book.rrect(r, radius, track);
    let p = clamp01(p);
    if p > 0.001 {
        book.rrect(
            Rect::new(r.left, r.top, r.left + r.width() * p, r.bottom),
            radius,
            fill,
        );
    }
}

/// The violet→cyan ramp: viewwstudio resting on vieww, in one gradient.
/// It appears wherever the film wants to say "these two are one thing".
#[must_use]
pub fn lineage(a: f32) -> Gradient {
    Gradient::horizontal().with_dither().with_stops(&[
        (0.0, alpha(VIOLET, a)),
        (0.5, alpha(mix(VIOLET, CYAN, 0.5), a)),
        (1.0, alpha(CYAN, a)),
    ])
}

/// A ring that draws itself in — `Path::arc_ring` with a swept angle, the
/// cheapest honest "reveal" for anything circular.
pub fn arc_sweep(
    book: &mut Sketchbook,
    at: Offset,
    radius: f32,
    width: f32,
    from: f32,
    sweep: f32,
    brush: impl Into<vieww_foundation::Brush>,
) {
    if sweep.abs() < 1e-3 {
        return;
    }
    book.fill(Path::arc_ring(at, radius, width, from, sweep), brush);
}

/// A polyline through points, as one stroked path — the film's charts,
/// traces and timelines are all this.
pub fn polyline(book: &mut Sketchbook, pts: &[Offset], brush: impl Into<vieww_foundation::Brush>, width: f32) {
    if pts.len() < 2 {
        return;
    }
    let mut p = Path::new();
    p.move_to(pts[0]);
    for q in &pts[1..] {
        p.line_to(*q);
    }
    book.stroke(p, brush, width);
}

/// A rounded-corner bracket — the devtools' selection marker.
pub fn brackets(book: &mut Sketchbook, r: Rect, len: f32, c: Color, width: f32) {
    let l = len.min(r.width() * 0.45).min(r.height() * 0.45);
    let corners = [
        (r.left, r.top, 1.0, 1.0),
        (r.right, r.top, -1.0, 1.0),
        (r.left, r.bottom, 1.0, -1.0),
        (r.right, r.bottom, -1.0, -1.0),
    ];
    for (x, y, sx, sy) in corners {
        let mut p = Path::new();
        p.move_to(Offset::new(x + sx * l, y));
        p.line_to(Offset::new(x, y));
        p.line_to(Offset::new(x, y + sy * l));
        book.stroke(p, c, width);
    }
}

/// The typewriter: how much of `s` is visible at progress `p`, plus the
/// caret's own blink. Typing in this film is a function of time, never a
/// recorded keystroke stream.
#[must_use]
pub fn typed(s: &str, p: f32) -> &str {
    let n = (s.chars().count() as f32 * clamp01(p)).round() as usize;
    let end = s
        .char_indices()
        .nth(n)
        .map_or(s.len(), |(i, _)| i);
    &s[..end]
}

/// A caret that blinks at 1.6 Hz off film-time, and holds solid while the
/// line is being typed.
#[must_use]
pub fn caret_on(abs: f32, typing: bool) -> bool {
    typing || (abs * 1.6).fract() < 0.55
}

/// Format a count with thin separators — the receipts' own voice.
#[must_use]
pub fn thousands(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::new();
    let b = s.as_bytes();
    for (i, ch) in b.iter().enumerate() {
        if i > 0 && (b.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(*ch as char);
    }
    out
}

/// `m:ss` — the elapsed chip's format.
#[must_use]
pub fn clock_mmss(sec: f32) -> String {
    let s = sec.max(0.0) as u32;
    format!("{}:{:02}", s / 60, s % 60)
}

/// Transform that scales about a centre and then translates — the film's
/// "push in" and "pull back", written once.
#[must_use]
pub fn push_in(center: Offset, k: f32, shift: Offset) -> Transform {
    Transform::scale_around(center, k, k).then(Transform::translate(shift))
}
