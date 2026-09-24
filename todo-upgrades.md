# upgrades.md — what the bench broke, and what vieww should do about it

Every entry here was found by **rendering something and looking at it**, not by
reading the source. Each one names the plate that found it, what actually
happens, why it matters to the launch film, and the smallest change that would
fix it.

Nothing in this file is a complaint. A framework that survived the ceiling
plates this well has earned a specific list.

**Status key** — `BLOCKER` the film cannot do the thing · `WORKAROUND` the film
can do it, expensively or indirectly · `PAPERCUT` correct but costly in time or
clarity · `TRAP` behaves correctly and is easy to get wrong.

---

## U-01 · `Sketch::Layer` cannot carry a blend mode
> **Status → FIXED in `vieww_base`, 2026-09-24.** `Sketch::Layer` carries a `blend` field, `Sketchbook::blended_layer` records it, and `RenderPainting::replay` (`vieww-render/src/objects/painting.rs`) now passes it to both `push_layer` and `push_filtered_layer` instead of hard-coding `Normal`. The widget form stays for addressable beats, exactly as the silver lining suggested. Test: `a_blended_layer_records_its_mode`.

**Status:** WORKAROUND · **Found by:** T-00b compositing, X-05 light

`RenderPainting::replay` hard-codes `BlendMode::Normal` for every
`Sketch::Layer`, so a `Painter` cannot blend at all. The rasterizer honours 22
of 28 modes; none of them are reachable from a sketch.

**Consequence for the film.** Every additive light — beams, bloom, glow, dust,
the whole of the "Light." beat — has to be lifted out of the painter and
rebuilt as a widget group wrapped in `Opacity::new(1.0).blend(mode)`. That
splits one drawing into N widgets and moves compositing decisions from the
place that draws into the place that lays out.

**The fix.** Add `blend: BlendMode` to `Sketch::Layer` and a
`Sketchbook::blended_layer(alpha, blur, blend, clip, build)`. `Canvas::push_layer`
already takes the mode; `replay` is currently throwing it away.

```rust
Sketch::Layer { alpha, blur, blend, clip, children }   // one field
canvas.push_layer(bounds, *alpha, *blend);             // one argument
```

**Silver lining, recorded honestly.** Being forced into widget groups gave every
light source a name and a separable layer, which makes the film's "no effect in
post" rule checkable — each light is a widget you can point at. Worth keeping
*both*: the sketch-level mode for economy, the widget for beats that need to be
addressable.

---

## U-02 · `vieww_effects::Blend` silently does nothing
> **Status → FIXED in `vieww_base`, 2026-09-24.** `vieww_effects::Blend::build` now routes the foreground through `Opacity::new(1.0).blend(mode)` over the background in a `Stack`, via a `From` conversion onto foundation's modes — the one way the tree reaches a blend mode, rather than a second silent one. The "degrades to plain stacking" doc comment is gone with the behaviour it described. Test: `every_effect_mode_lands_on_its_foundation_twin`.

**Status:** BLOCKER (as documented) · **Found by:** T-00b compositing

```rust
fn build(&self, _ctx: &BuildContext) -> WidgetNode {
    // Degraded: both layers in a Stack.
    let _ = self.mode;
    Stack::new().push(self.background.clone()).push(self.foreground.clone()).into()
}
```

The widget takes a `BlendMode`, discards it, and stacks. Its own doc comment
says "without render-pipeline support, degrades to plain stacking" — but the
render pipeline **does** support it, today, through `Opacity::blend`. The
comment is describing a world that no longer exists, and the widget is the one
thing in the tree named after the feature it does not do.

This is the most dangerous kind of defect in a repository with this culture: it
does not fail, it just quietly produces `Normal`. `SceneReport::unsupported_blends`
does not count it either, because nothing unsupported was ever requested.

**The fix.** Either implement it over `Opacity::blend`, or delete it and let the
prelude's `Opacity::blend` be the one way. Deleting is probably right: two ways
to blend, one of which is a no-op, is worse than one way.

---

## U-03 · No public text-to-path
> **Status → FIXED in `vieww_base`, 2026-09-24.** `vieww_paint::outline_glyph(face_bytes, face_index, glyph_id, variations)` and `units_per_em` are public, re-exported at the crate root behind the `native` feature — the door onto the same extraction layer the rasterizer already uses, with the scale-then-translate recipe in its docs. The film's sixty-line `ttf-parser` re-parse can collapse onto it. Test: `a_non_font_reports_none_rather_than_a_default`.

**Status:** WORKAROUND (external parse) · **Found by:** X-03 wordmark

The framework can *draw* text and cannot *hand you its shape*. Outlines are
built in `vieww-paint::native::glyph` and everything there is `pub(crate)`.
`vieww-text` exposes `Paragraph::runs()` → a `GlyphRun` carrying the font bytes,
the glyph ids and their offsets — everything needed to build the outline, and
not the outline.

**Consequence for the film.** `examples/film-lab/src/glyphs.rs` exists: sixty
lines re-parsing the same embedded face with `ttf-parser`, one layer above where
the rasterizer already does it. Every text effect the film wants — stroke-on
titles, letters that morph, a wordmark that scatters into the frames it was
rendered from, a manifest whose digits are paths — runs through that file.

**The fix.**

```rust
// vieww-text
pub fn outline(store: &mut FontStore, text: &str, style: TextStyle) -> Vec<Path>;
// or, honouring shaping:
impl Paragraph { pub fn outlines(&self) -> Vec<Path>; }
```

`native::glyph::outline_glyph` already returns exactly this; it needs a public
door, not an implementation. **Highest-value single change in this file.**

---

## U-04 · `Transform` is 2×3 — there is no 3D anywhere
> **Status → FIXED in `vieww_base`, 2026-09-24.** `vieww_foundation::Transform3` exists — perspective projection with `project_rect`, finite-guarded, doctested — implemented despite "not required", so the honesty ledger can say the projection is framework arithmetic either way. The stronger phrasing in this entry stands: no external engine, and now the perspective door is a first-class path.

**Status:** WORKAROUND · **Found by:** P-01 unfold, X-02 avatar, X-07 knot

`Transform` is `a b c d tx ty` and `Sketch::Transformed` takes that same affine.
No 3×4, no perspective divide, nothing in `vieww-foundation` that knows what a
Z is.

**Consequence for the film.** `examples/film-lab/src/proj.rs` is the film's
camera: pinhole projection, quads, triangles, Lambert, painter-order depth
sorting. Roughly 250 lines, all of it arithmetic in the example.

**Consequence for the honesty ledger.** The line "the 3D is the framework's own
transforms" is **false as written** and must change. The true claim is stronger:

> No external engine. The projection is forty lines of arithmetic in the
> example, and every pixel of it was rasterised by vieww.

**The fix, if one is wanted.** A `Transform3` with `project(Rect) -> Path` would
make perspective a first-class widget property (`Transformed::perspective`).
Not required — the arithmetic version works and costs nothing — but it would
move a capability from "the film happens to have written it" to "the framework
has it".

---

## U-05 · Abutting fills leave hairline seams
> **Status → FIXED in `vieww_base`, 2026-09-24.** `Sketchbook::mesh` applies the bleed itself (plus a `debug_assert!` on empty grids), so the conflation wireframe cannot be re-derived per call site. The artifact itself is now also named where it bites: the blur edge-clamp work under U-15 documents the coverage-based-compositing family it belongs to.

**Status:** TRAP · **Found by:** X-01 ocean

Two paths that share an edge do not meet. Each is anti-aliased against what is
already in the target, so the shared edge receives two partial coverages and a
hairline of background survives between them. Across a 96×64 grid of shaded
quads this reads as a wireframe nobody asked for — clearly visible in the
ocean's second render, gone in the third.

This is the classic **conflation artifact** and it is inherent to
coverage-based compositing, not a bug in this rasterizer. It is on this list
because it is *invisible until it is catastrophic*: one quad looks perfect, ten
thousand look like graph paper.

**The workaround the film uses.** Overlap every cell a fraction into its
neighbours (`BLEED = 0.0035` of a cell, plus 0.4% on the depth axis).

**What would help.** A line in the `native` module docs naming it, and a
`Sketchbook::mesh(cells)` helper that applies the bleed itself — anybody
tessellating anything will hit this, and every one of them will spend an hour
on it first.

---

## U-06 · Blurred layers are priced per layer, and it is easy to buy eleven
> **Status → NOTED in `vieww_base`, 2026-09-24.** Not implemented: `SceneReport` is the bench's instrument, and surfacing `filtered_layers` in `PerformanceOverlay` is a framework-side convenience that did not make this pass. The receipts culture that found the 9x stays the real guard; the number is one PR away.

**Status:** TRAP · **Found by:** X-05 light

`Sketch::Layer` with `blur > 0` costs one offscreen buffer and one kernel pass.
Wrapping each of eleven light shafts in its own blurred layer took the plate
from **15 ms to 140 ms**; moving the blur up one level so the *group* of shafts
shares a single layer brought it to **60 ms** and looked better.

Nothing warns you. The API makes the expensive version and the cheap version
look identical at the call site, and the expensive one is the one you write
first, because you are thinking about one shaft at a time.

**What would help.** `SceneReport` already counts `filtered_layers`. Surfacing
that in the `PerformanceOverlay`, or a debug assertion above some threshold per
frame, would turn an invisible 9× into a visible number.

---

## U-07 · `Camera`-shaped footgun: a heading is not an orbit
> **Status → N/A in `vieww_base`, 2026-09-24.** Film-side, not framework: the bench's own `Camera::orbit` already fixed it. Kept for the warning it carries into any future `Transform3` — which now exists (U-04) and does not repeat the mistake.

**Status:** TRAP (in the film's own code) · **Found by:** X-02 avatar

Not a framework defect — a `proj.rs` defect — recorded because it cost two full
renders and will cost anyone else the same. Yawing the camera rotates where it
*points* while the eye stays put, so a turntable written with `looking()` swings
the subject off the side of the frame. Three of six frames came back empty and
the cause looked like a culling bug.

Fixed with `Camera::orbit(target, radius, azimuth, height)`, which solves the
eye position so the target lands on the optical axis. Kept here because the
same confusion is waiting in any `Transform3` that ever gets added.

---

## U-08 · Gradient stops are not sorted, and a rotating phase folds the ramp
> **Status → FIXED in `vieww_base`, 2026-09-24.** `Gradient::with_stops`'s doc carries the sentence — "an animated phase must re-sort; wrapping offsets is the common case" — and the U-17 assert makes the out-of-order frame a panic in debug rather than a one-frame flicker nobody can see in a still.

**Status:** TRAP · **Found by:** X-06 chrome

`Gradient::with_stops` keeps stops in the order given — deliberately, and the
doc says so: "a caller listing them out of order has made a mistake worth seeing
rather than one worth silently repairing." Correct, and the right call.

But an *animated* ramp — a chrome sweep whose phase advances every frame —
wraps its offsets through 1.0, and the moment one wraps the list is out of
order and the ramp folds over itself. The letters flicker for exactly one frame
per stop per cycle, which is the hardest class of bug to see in a still.

**What would help.** Nothing in the API. A sentence in `with_stops`'s docs:
*"an animated phase must re-sort; wrapping offsets is the common case."*

---

## U-09 · Font assets ship as placeholders through the HTML browser export
> **Status → FIXED in `vieww_base`, 2026-09-24.** `register_embedded` (`vieww-text/src/font.rs`) parses every embedded face before it is registered and panics naming the files if none parsed — the blank screen is now a message that says which asset is corrupt. The real binaries are restored: DejaVu subsets for the five Latin faces, and `scripts/make_test_fonts.py` — the repository's own recipe, which this environment could run — for the CJK face, the VF fixture, the emoji subset and the COLRv0 face. Test: the embedded-only suite (`103/103`) now actually draws.

**Status:** PAPERCUT (tooling, not framework) · **Found by:** the first render

`crates/vieww-text/assets/*.ttf` arrived as 250-byte notes —
`[binary asset] … use the release ZIP for the original binary bytes` — through
the code-browser export. `include_bytes!` happily embedded the notes,
`fontdb` accepted them, and **every glyph in every render came back as nothing**
with no error anywhere. The first audit sheet was a blank page with four
coloured squares on it.

A font that fails to parse should not be a silent zero-face database.

**What would help.** `FontStore::new` asserting that at least one embedded face
parsed. One line, and it converts a blank screen into a panic that names the
file.

---

---

## U-10 · A sweep gradient seams unless its ramp is a palindrome
> **Status → FIXED in `vieww_base`, 2026-09-24.** `Gradient::with_stops_mirrored` builds the palindrome (non-decreasing by construction), and the sweep docs name the same-ray discontinuity and the two-sharp-edges problem explicitly.

**Status:** TRAP · **Found by:** X-06 chrome

A sweep gradient's start and end angles are **the same ray**. Unless the first
and last stop carry the same colour, the ramp has a discontinuity there — and
because `Gradient::conic()` starts at twelve o'clock, the artifact is a hard
vertical line straight down the middle of whatever you filled. The first chrome
render had one through every letter and it looked exactly like a rasterizer bug.

It is not a bug. It is the only thing a sweep gradient can do with an open ramp.
But nothing says so, the failure looks like somebody else's fault, and the fix
is non-obvious: mirror the ramp so the wrap is continuous by construction.

**What would help.** A `Gradient::sweep` doc line, and possibly a
`with_stops_mirrored(&[..])` that builds the palindrome for you. Combined with
U-08 — animated phases must also re-sort — the sweep gradient currently has two
sharp edges and no signposts for either.

---

## U-11 · A painter cannot read what has already been painted
> **Status → FIXED in `vieww_base`, 2026-09-24.** (As documentation, which is what the entry asked for first.) The transform-outside/clip-inside pattern — a window onto the source that flies away with the window still attached — is documented on `Sketchbook` with the N-redraws cost spelled out, so every shatter, wipe and split-screen does not re-derive it.

**Status:** WORKAROUND · **Found by:** X-08 shatter

`Filtered::with_backdrop` reads the real destination pixels beneath a widget —
the only thing in the tree that can. Inside a `Painter` there is no equivalent:
a `Sketchbook` records operations, it does not have a target, so there is no way
to ask "what is under me".

**Consequence.** To make eighty glass shards each carry a piece of the artwork
they broke from, the shatter plate **redraws the entire source inside every
shard**, clipped to that shard's outline. Eighty redraws of a fifteen-shape
drawing, or 1,200 shapes where 80 should have done — and it renders in 67 ms, so
the price is real but affordable.

The structure that makes it work is worth recording on its own:

```rust
book.transformed(transform, |g| {           // outside: the shard's flight
    g.layer(alpha, 0.0, Some(shard), |h| {  // inside: the window
        source(h, size);                    // the original artwork, again
    });
});
```

The clip path is in the group's pre-transform space, so the transform carries
the clip *and* its contents together — a window onto the original that flies
away with the window still attached.

**The fix, if one is wanted.** Either a `Sketch::Backdrop` that captures the
destination into a layer the way `Filtered::with_backdrop` does, or simply
documenting the transform-outside/clip-inside pattern, which is the thing every
shatter, wipe, reveal and split-screen transition needs and which nothing in the
tree demonstrates.

---

## U-12 · `Shadow` is rounded rectangles only
> **Status → FIXED in `vieww_base`, 2026-09-24.** `Sketchbook::path_shadow` performs the blurred-translated-fill construction as one call. Test: `a_path_shadow_is_a_blurred_translated_fill`.

**Status:** WORKAROUND · **Found by:** X-06 chrome

`Sketch::Shadow` takes `rect` and `radius`, because `Command::DrawShadow` does.
A letterform is not a rounded rectangle, so a drop shadow under a glyph — or
under any generated path — has to be a blurred layer containing a translated
copy of the path.

The sketch docs already say this ("a shadow under an arbitrary path is a blurred
layer containing a fill of that path"), which is good, and the workaround costs
one offscreen per shadow, which is not. At eighty shards or five letters it is
fine; at a hundred it would not be.

**What would help.** `Sketchbook::path_shadow(path, shadow)` that performs the
blurred-layer construction, so the cost is at least visible as one call and the
pattern is not re-derived at every call site.

---

## U-13 · `Path` has no boolean operations
> **Status → NO CHANGE in `vieww_base`, 2026-09-24.** Recorded as not-a-defect, and left that way: Sutherland–Hodgman and marching squares stay in the example where they belong, and this note is the signpost that stops the next afternoon being spent looking for `Path::intersect`.

**Status:** NOT A DEFECT — recorded so the film stops looking for them

No union, intersection or difference. `Path::extend` concatenates and
`Path::reversed` flips; nothing combines.

This is almost certainly the right scope for a UI framework, and the bench found
two clean ways round it:

- **Convex clipping by half-plane** (Sutherland–Hodgman, ~25 lines) gives real
  Voronoi cells — X-08.
- **Marching squares over an implicit field** gives arbitrary organic contours —
  liquid, ink, damage regions that are not rectangles.

Both live in the example, where they belong. Recorded here so the next person
does not spend an afternoon looking for `Path::intersect`.

---

## U-14 · Blur is isotropic — there is no directional blur
> **Status → FIXED in `vieww_base`, 2026-09-24.** `ImageFilter::blur_angle` + `ImageFilter::directional_blur(sigma, angle)` exist, with a directed box pass in both the foundation kernel (`directional_blur_rgba`) and the native renderer's Premul path — edge-clamped like U-15 so a motion blur cannot re-grow the rim bug. Whether the film wants motion blur at all remains the taste decision this entry says it is. Tests: `a_directional_blur_spreads_along_its_ray_and_not_across_it`, `a_directional_blur_smears_along_its_ray_through_the_renderer`.

**Status:** GAP · **Found by:** X-08 shatter

`ImageFilter::blur(sigma)` is a symmetric Gaussian. There is no angle and no
length, so there is no motion blur.

For a film locked at 60fps this matters less than it would at 24 — fast motion
at 60 reads acceptably sharp — but the shatter's shards are travelling several
hundred pixels a frame at their fastest and they are crisp, which is the one
thing in that plate that says "render" rather than "photograph".

**What would help.** `ImageFilter::directional_blur(sigma, angle)`. The box-blur
passes in `filter.rs` already do the work; they would need a sheared kernel
rather than a separable H/V pair. Worth it only if the film decides it wants
motion blur at all — which is a taste decision, not a technical one.

---

## U-15 · **Blur is zero-padded at the layer boundary — every blurred surface darkens at its own edges**
> **Status → FIXED in `vieww_base`, 2026-09-24.** Both box-blur kernels now clamp to edge — `vieww-foundation/src/filter.rs`'s `blur_rgba` **and** the native renderer's own Premul passes in `vieww-paint/src/native/effects.rs`, which is the path every blurred surface in the product actually takes (the entry's file pointer named only the first). A uniform full-bleed field is now a fixed point of the blur, asserted at the renderer, not just the kernel: `a_full_bleed_blurred_layer_stays_solid_to_the_frame_corners` — the 100/255 edges and 39/255 corners of the original probe now render 255/255 to the corners.

**Status:** DEFECT · **Found by:** the breaker's edge-padding probe · **Severity: highest in this file**

Blur a field that extends 400 pixels past the target on all four sides — a field
with no edge anywhere inside the frame — and measure the result:

| sigma | centre | edge | corner |
|---:|---:|---:|---:|
| 0 | 255 | 255 | 255 |
| 8 | 255 | 100 | **39** |
| 24 | 255 | 87 | **29** |
| 48 | 255 | 83 | **27** |
| 96 | 239 | 76 | **26** |

A uniform field is not uniform after blur. The corner loses **89% of its
luminance** at a sigma as gentle as 8. The blur is sampling outside its buffer
and getting transparent black.

**Why this is the most serious entry here.** It is not a corner case — it is
*every blurred surface in the product*. A frosted navigation bar darkens along
its own edges. A backdrop-blurred sheet gets a black rim. A rack focus vignettes
toward the frame. None of it can be fixed from the outside, because the
application never sees the intermediate buffer. And it is invisible in a
screenshot of a single panel on a dark background, which is exactly where
everybody looks.

It is also the reason the `blur-sigma-400` breaker case came back black — which
I first assumed was a clamp, and which the sigma sweep disproved: the light is
conserved right up until it reaches a boundary.

**The fix.** `blur_rgba` in `vieww-foundation/src/filter.rs` runs three box-blur
passes; `box_blur_horizontal` / `box_blur_vertical` treat out-of-buffer samples
as zero. They should **clamp to edge** — repeat the boundary pixel — which is
what every other 2D blur does and what CSS `filter: blur()` specifies. It is a
change to the index arithmetic in two functions and nothing else:

```rust
let sx = x.clamp(0, width - 1);   // instead of skipping / reading zero
```

A test that would have caught it: blur a full-bleed uniform field and assert
every pixel is still the same value. Three lines, and it belongs in
`native_parity.rs` beside the other coverage assertions.

---

## U-16 · `NaN` produces corrupt geometry; infinity produces nothing. Neither says anything.
> **Status → FIXED in `vieww_base`, 2026-09-24.** `debug_assert!(is_finite)` on `move_to`, `line_to`, `quad_to` and `cubic_to` in `vieww-foundation/src/path.rs` — NaN and ±inf both fail loudly in development, at the verb that received them, at no release cost.

**Status:** TRAP · **Found by:** the breaker

Two adjacent cases, two different silent behaviours:

- **`NaN` mid-path** → the path renders, *wrong*: a thin red wedge where a
  four-sided shape was asked for. The poison propagates into the geometry and
  comes out as a plausible-looking shape nobody drew.
- **`±inf` mid-path** → the entire path is dropped. Nothing renders at all.

Both arise the same way in real code: a `normalise` that divided by a
zero-length vector on frame 0, or a perspective divide by a Z that reached the
camera plane. In a 10,800-frame master, one of these lands on one frame, and the
first is far worse than the second because it does not look like an error.

**The fix.** Pick one behaviour and document it. Dropping the whole path is
defensible; silently emitting a different shape is not. A `debug_assert!` on
`is_finite` in `Path::line_to`/`cubic_to`/`move_to` would catch it in every
development build at a cost of nothing in release.

---

## U-17 · Reversed gradient stops fail silently, and the failure looks intentional
> **Status → FIXED in `vieww_base`, 2026-09-24.** `debug_assert!` that stops are non-decreasing, with a message that keeps the entry's own philosophy ("a mistake worth seeing") and makes it actually visible: in a panic, not a flat purple rectangle.

**Status:** TRAP · **Found by:** the breaker

`with_stops(&[(1.0, purple), (0.5, green), (0.0, red)])` renders a **flat purple
rectangle**. The doc says stops are not sorted on purpose — "a caller listing
them out of order has made a mistake worth seeing rather than one worth silently
repairing" — and the reasoning is sound. But what you *see* is a solid fill,
which is a perfectly normal thing for a rectangle to be. The mistake is not
visible; it is camouflaged.

Related to U-08 (an animated phase must re-sort) and U-10 (a sweep ramp must be
a palindrome). All three are the same shape of problem: `Gradient` has sharp
edges and no guard rails.

**The fix.** `debug_assert!` that offsets are non-decreasing. Keeps the
zero-cost release behaviour and the "mistake worth seeing" philosophy, and makes
the mistake actually visible — in a panic message rather than in a rectangle.

---

## U-18 · Sub-pixel strokes vanish rather than clamping to a hairline
> **Status → OPEN in `vieww_base`, 2026-09-24.** Deliberately not implemented, with the analysis that stopped it: the entry's suggested ink-preserving clamp (one pixel wide, alpha scaled by the device width) is mathematically equivalent to what the coverage-based rasterizer already produces at 8-bit coverage — a 0.01px stroke at full alpha and a 1px stroke at 1% alpha land within a quantisation step of each other. What the complaint actually asks for is a minimum-visibility *policy* (bloom the ink on the way down), which is the same class of taste decision as U-14's motion blur. Recorded here so the next pass starts from the analysis, not the surprise.

**Status:** GAP · **Found by:** the breaker

Fifty thousand strokes at 0.01px wide render as very nearly nothing. That is
arithmetically correct — 1% coverage — and it is not what any drawing API does.
Every other 2D renderer clamps thin strokes to a minimum visible width, because
a diagram whose lines disappear when the user zooms out is a broken diagram, not
an accurate one.

`Stroke::hairline()` exists. What is missing is the *clamp on the way down*: a
stroke whose computed device width falls below one pixel should be drawn at one
pixel with proportionally reduced alpha, which preserves both visibility and
total ink.

---

## U-19 · An invisible layer still costs the full recording
> **Status → FIXED in `vieww_base`, 2026-09-24.** `Painter::paint`'s doc now says it: "an invisible painter is not a free painter; skip the work in `paint`, not with `alpha`" — with the 41.8ms-for-one-shape receipt quoted beside it.

**Status:** PAPERCUT · **Found by:** the breaker

`alpha: 0.0` over 100,000 primitives: the rasterizer correctly skips it —
`SceneReport` says 1 shape — and the frame still takes **41.8 ms**, because the
`Sketchbook` built all 100,000 `Sketch` items before anything could look at the
alpha.

`Sketch::is_invisible` is checked at replay. By then the allocation has
happened. For a film that fades things in and out constantly, the frames where
something is invisible cost the same as the frames where it is not.

**What would help.** Nothing in the framework — this is a property of recording
into a value rather than issuing calls, and that design pays for itself
elsewhere. But it is worth a line in the `Painter` docs: *an invisible painter
is not a free painter; skip the work in `paint`, not with `alpha`.*

---

## What the breaker could **not** break

Recorded with equal weight, because it is the more surprising half of the
result. **25 of 25 hostile frames survived with no panic:** a 200,000-verb path
(408 ms), 64 nested blurred offscreens (520 ms), 512 nested transforms,
10,000-subpath clips, a 5,001-point self-intersecting star resolved by nonzero
winding (112 ms), 8,000 uncacheable shadows (345 ms), coordinates at 1e7, a
singular transform, a scale of 1e-9 over content at 1e9, a corner radius a
thousand times the box, a `[0, 0]` dash pattern, an arc swept a thousand turns,
inverted rectangles, and a 1-degree miter with the limit at 10,000.

No hangs, no overflow, no allocation failure, no silent recursion blow-up. For a
ground-up rasterizer this is genuinely unusual, and it is the reason the rest of
this file is short and specific rather than long and vague.

---

## U-20 · Marching squares' chaining pass is not optional (a correction)
> **Status → NOTED in `vieww_base`, 2026-09-24.** `SceneReport` is the bench's instrument, so the open-subpath-fill counter belongs there with it. The general property this entry names — `fill` on an open subpath is silent — is now documented in the U-13/U-11 family, so the symptom at least points at the cause in the docs.

**Status:** NOT A DEFECT — a mistake of mine, recorded because the failure is instructive

The hero frame tried to skip the segment-chaining half of marching squares, on
the theory that a few hundred loose two-point segments inside a single `fill`
would still close under nonzero winding. They do not. Every segment starts with
its own `move_to`, so the path is a hundred *open* subpaths, and a fill of open
subpaths is nothing at all. What rendered was the stroke — a thread where a blob
should have been, and no warning anywhere.

Worth keeping because it names a general property: **`fill` on an open subpath
is silent.** A path that is 99% correct and missing its `close()` calls renders
as empty, not as a hint. Anyone generating geometry — contours, tessellations,
projections — will meet this, and the symptom (nothing, plus whatever the stroke
shows) points away from the cause.

**What would help.** `SceneReport` could count subpaths that were filled while
open. One integer, and it turns a blank frame into a number that says why.

---

## Budget, measured — the hero frame

The worst frame the film could plausibly ask for, at true master resolution:
**1920×1080, 3,614 shapes, 10 layers, 229 ms.**

In it: a vignetted stage and a projected floor grid; eleven `Plus`-blended
volumetric shafts through one blurred group; 6,000 lit dust motes; a swept 3D
tube knot of 3,300 depth-sorted, lit, gradient-filled quads; a marching-squares
liquid mass with under-glow and surface tension; the wordmark as real glyph
outlines with a sweep-gradient chrome fill and a faded reflection; and a frosted
caption panel through a genuine backdrop blur.

**A 10,800-frame master of frames like this one is 41 minutes.** Nothing in the
film is limited by the renderer. The constraint is taste, which is the good kind.


*This file is appended to as the bench finds things. Entries are never deleted —
a downgraded finding gets its status changed and a note, so the reasoning
survives.*
