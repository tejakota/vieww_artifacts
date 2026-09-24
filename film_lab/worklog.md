# vieww launch film — multi-agent worklog

---
Task ID: 1
Agent: main (Super Z)
Task: Understand complete context from vieww_artifacts repo (handoff, scene graph v0.7, strategy, sheets, code browsers); establish runnable environment for the create-evaluate iterate loop.

Work Log:
- Cloned https://github.com/tejakota/vieww_artifacts.git → /home/z/my-project/vieww_artifacts (32MB)
- Read handoff.md fully (500 lines): project model, mission, strategy, scene graph v0.7 checkpoint, animatic pass 1 state, aesthetic verdict ("lower than the basic I guessed"), the reframe (no gray-box stage; pass 2 = rich pass), verified API cheatsheet, next actions, collaboration protocol
- VLM-analyzed: sheet-bx.png (2000×1688), sheet-vf.png (2000×750), Keynote strategy.png
- Parsed scene_graph_v07.svg (3000×2300, 602 text elements) — all 12 scenes, 21 effects (E-01..E-22), transitions, counter ladder 1-7, E-20 reveal F1-F8, escalation curve
- Extracted embedded JSON data from both code browser HTMLs:
  - vieww_beta_code_browser_updated.html → window data in <script id="data"> (13.88MB JSON): tree "vieww_release-main", 1044 files, 795 .rs, 36 crates
  - vieww_crates_syntax_browser.html → 8.88MB JSON: 613 files + syntax reference (10 topics)
- Reconstructed full Rust workspace → /home/z/my-project/vieww_ws (crates/, examples/ 23 example crates, apps/, ci/, docs/, Cargo.toml+lock, rust-toolchain.toml pinned 1.98.1)
- Installed Rust 1.98.1 via rustup (+ rustfmt, clippy, llvm-tools-preview — the last one hung once, fixed with direct `rustup component add`)
- ffmpeg 7.1.5 available
- Started `cargo build --release -p test-premium-ui` (build in progress)
- Mined APIs from examples + crates:
  * test-premium-ui (771 lines): function-style widgets returning WidgetNode, FrameDriver+NativeRenderer loop, Filtered::blur().with_backdrop().tint() (frosted glass), BoxDecoration (radius/border/shadow/gradient), Gradient (radial/vertical + with_stops), Transformed::translate, Painting::sized + PaintWith + Sketchbook custom paint (book.circle/fill + Path), LineChart/BarChart/Chip/Avatar/Badge/LinearProgress/CircularProgress/Icon/Switch/Checkbox/Slider/Button, gif output
  * audit-sheets (1337 lines): vieww umbrella crate prelude, RouteTransition::wrap(screen,t,surface) pure fn, Theme/ThemeData, signal granularity proof (read in build vs construction — Handle widget + widget_node_from! macro), vieww::animation::{Tickers,Tween}, vieww_element::Animation, ScrollTimeline, Filtered vs blur_rgba kernel parity
  * animation-showcase: vieww_platform_winit::App window path, TimelineBuilder.stagger().build().attach(driver.tickers()) → TimelinePlayer.play(), PhysicsDrag + SpringPreset::{Expressive,Standard}, runtime.signal()
  * vieww-animation crate: SpringSpec, SpringPreset, SpringAnimation::new(from,preset).retarget/.retune/.jump/.value, Spring2D
  * foundation: Sketchbook (fill/stroke/stroke_styled/rect/rrect/circle/ring/arc/line/shadow/layer/transformed), Path (move_to/line_to/cubic_to/close/rect/rounded_rect/rounded_ring/arc/arc_ring/extend/reversed/transformed/fitted/bounds/stroke_outline), Transform (2D affine: translate/scale/rotate/rotate_around/scale_around/then/apply/invert — NO 3D matrix), Gradient (Linear/Radial/Sweep conic, with_stops, between, faded, Bayer 4×4 dither!), StrokeDash (even/dotted/offset — dash-phase progressive stroke possible), BlendMode (12 modes SrcOver..Exclusion)
  * vieww-effects: BackdropFilter/BackdropBlur/Filter (brightness/saturation/grayscale/sepia)/FilterChain/Blend
  * Filtered widget: blur/saturation/grayscale/brightness/sepia/tint/matrix/with_backdrop
  * full-showcase: render_to_png direct, ShapeMorph (Circle→RoundedRectangle .progress()), Carousel/Grid/ColorPicker
  * Text widget: style/color/size/weight/bold/italic/align/direction/overflow/max_lines

Stage Summary:
- CONTEXT FULLY ABSORBED: 3:00 film, kishōtenketsu (K0-K4 twists), 3 acts (45+95+40), 12 scenes S01-S12, 21 effects, counter ladder 1-7, E-20 reveal F1-F8 (3+1+2+3+2+1+2+6=20s), 5 rules, honesty ledger 10 lines
- AESTHETIC FLOOR (from sheets): dark premium (#0a0a0c-#121214 bg), electric violet accent (#8b5cf6-#a78bfa), glassmorphism/frosted surfaces, gradient mesh nebula backgrounds (#4c1d95/#be185d/#0ea5e9), glows, soft shadows, 3D perspective planes, spring-curve graphs, stat typography (95.1%, 3.6ms), Inter/SF-style sans + mono for data
- KEY STRATEGY: kishōtenketsu twists (reveals recontextualize) + single-take Act II as evidence (no cuts to hide latency)
- FULL SOURCE RECONSTRUCTED: complete workspace buildable locally — the create→run→evaluate loop CAN run here
- PENDING: build finishing; then run test-premium-ui to verify pipeline; then start element library experiments in examples/
- Scene graph expects scene sources at examples/s01_the_wait.rs .. s12_close.rs (flat files) + animatic crate (examples/animatic/ per handoff §5 — not in release snapshot, needs creating)

---
Task ID: 2
Agent: main (Super Z)
Task: Prove the create-evaluate loop runs end-to-end in this environment.

Work Log:
- Fixed llvm-tools hang (direct rustup component add)
- cargo fetch + cargo build --release -p test-premium-ui → SUCCESS (2m20s, all 36 crates)
- First run panicked: cosmic-text "no default font found" (binary needs LD_LIBRARY_PATH=toolchain/rustlib/x86_64-unknown-linux-gnu/lib due to prefer-dynamic; then embedded font path failed)
- FIX: driver.use_system_fonts() (vieww-render/src/frame.rs:1096) — one line, text now renders
- Second run: 56 frames, 55/55 moving, render mean 52.55ms, worst 94.61ms, 0 overflows, GIF+PNGs written
- VLM verification: text fully legible ("Overview", "184.2k", "Performance", "Frosted glass"...), quality rated 9/10
- Framework boundary confirmed: Transform = 2D affine only (perspective deliberately out of scope, transform.rs:17) → true 3D requires manual projection in custom Painting closures
- Performance receipt: ~53ms/frame at 1120×720 release → offline film rendering viable; 1080p×10,800 frames needs render-time planning

Stage Summary:
- THE LOOP IS LIVE: create → cargo build/run → PNG/GIF frames → VLM visual evaluate → iterate, all in-environment (no author paste-backs needed — mode upgrade vs handoff §9 protocol)
- Critical film-pipeline fact: FrameDriver default fonts are embedded-only DejaVu subsets; a text-bearing render MUST call driver.use_system_fonts() (or with_application_faces with shipped typefaces) or text silently fails/panics — applies to animatic pass 2 directly
- Environment: /home/z/my-project/vieww_ws (source), deps cached, release profile warm

---
Task ID: 3
Agent: main (Super Z)
Task: Build film_lab (element experiments) per ratified berserk brief: 3D zone first, match-the-floor calibration, flat+gradient shading, full send. Iterate via render → contact sheet → VLM audit.

Work Log:
- Created examples/film_lab (registered in workspace): film_lib.rs (palette from VLM sheet analysis, deterministic xorshift RNG, clock incl. held_24_in_60, easing incl. spring_out, render harness with SceneReport receipts, ffmpeg 4x4@10fps contact sheets, FILM_DUMP_CMDS command dumper), three_d.rs (Vec3 math, Camera with manual perspective projection — Transform is 2D affine only, quad Mesh, painter's sort, back-face cull via screen-space cross, Lambert + depth fog + Blinn-Phong specular + per-face 2-stop vertical gradient ramps, grid/torus/box mesh builders), exp_light.rs, exp_mesh.rs, exp_ocean.rs
- Round 1: receipts exposed ocean drawing only 11 shapes/frame → grid winding bug (Newell normal pointed DOWN, back-face cull ate the floor) → fixed push_quad(a,d,c,b)
- Round 2: VLM audits (light 4/10, mesh 4/10, ocean 6/10) → dither (Gradient::with_dither) on all big ramps, card shadow, mesh grounding ellipse, drop slower/bigger, glint bloom layer
- Round 3: Blinn-Phong specular added to MeshStyle (specular/shininess), ocean densified 26→44 cells (985 shapes/frame), heart blob + breathing bloom behind card
- Round 4: rim strokes (stroke_rrect hugging card edge in blurred layer) replacing filled halo, tags → plain muted text, crisp white border, starfield 90→230
- THE INCIDENT (receipts culture): card rendered opaque white through rounds 2-4 while bg was correct. Bisection ladder: noglass ✓ bg fine → plasticard (proven test-premium-ui pattern) ALSO white → mini repro (1 painting + 1 glass card) white → swatch forensics: EVERY translucent swatch opaque → FILM_DUMP_CMDS revealed Paint{color: a:255} — MY alpha() helper takes 0-1 f32 but card/sweep construction passed 0-255-style values (alpha(WHITE,30.0) → 30*255 → clamp 255). The framework (renderer, premul Porter-Duff, backdrop filter, tint_matrix) was INNOCENT throughout — verified by reading native/color.rs blend, target.rs snapshot_from, reference.rs PushLayer/PopLayer. Fixed all call sites to 0-1; glass card now renders (75,57,124) frosted violet.
- Round 5 (post-fix): VLM distance to sheet-vf reference: 8/10 (from 4/10) — "card clearly reads as translucent frosted glass, text highly legible". Mesh 6/10 (specular glint visible), ocean 7/10 (smoother, glints integrated).
- FILM_BISECT env gates kept as documentation: mini, mini_widbg, mini_nofilt, mini_plain, mini_noblur, micro, micro2, swatch, noglow, noglass, norim, nosweep, flatlayers

Stage Summary:
- film_lab live at examples/film_lab; outputs /home/z/my-project/download/film_lab/{light,mesh,ocean}/ (16 PNGs + 4x4 sheet + metrics.txt each)
- Element receipts: light 108-114 shapes/frame (75 glyph_runs — text renders), mesh 605 shapes/frame (line→grid→sweep-shaded floor→specular torus with real SpringAnimation orbit + mid-flight retarget at t=0.63), ocean 985 shapes/frame (drop→strike→ripple field→44x44 heightfield ocean growing from landing point + glint bloom pass)
- Render cost: light ~133ms/frame (layers+backdrop), mesh ~39ms, ocean ~41ms at 1280x720 release — offline film rendering fully viable
- AESTHETIC FLOOR MET: 8/10 VLM distance to author's own reference frame
- API seams verified along the way: Gradient with_dither (Bayer 4x4) works; StrokeStyle+Dash::new([draw_len, total]) dash-phase line drawing works (E-17 technique); Sketchbook::layer(alpha, blur, clip) group blur works; Shadow colored glow works; Filtered backdrop+tint works (when caller's alpha units are correct!)
- NEW INCIDENT LOGGED (like handoff's 2550-vs-2551): "the alpha() helper convention" — a caller-side unit bug masquerading as a rasterizer bug; found by command-stream dump (FILM_DUMP_CMDS), the cheapest instrument in the lab
- Next: mesh/ocean still have headroom (VLM: wireframe overlay prominence, glint integration, nebula color distribution); then globe + three-planes-fan (F4) experiments; then text-kinetics family
