/* ============================================================================
   keynote v4 — data.js
   The film's script: scene beat-sheets, copy, and the receipts.
   Every number below is quoted from this repository's own artifacts
   (metrics.txt files, the README, the census manifests) — the web film
   inherits the house rule: receipts over claims. Sources noted inline.
   ========================================================================== */

const RECEIPTS = {
  // film_lab/keynote_v3/manifest.txt — census3, this bench
  alive:        { v: '0.246 s', src: 'census3 manifest — alive probe' },
  // film_lab/renders/swarm/metrics.txt (frames=16, shapes=40179 → 2,511/frame)
  swarm:        { boids: 2400, shapes: '2,511', build: '66.4 ms', raster: '214.1 ms',
                  src: 'renders/swarm/metrics.txt' },
  // repo README, round 6 receipts
  galaxy:       { shapes: '60,527', ms: '101', unit: 'shapes / frame', src: 'README — round 6' },
  mandel:       { rects: '57,602', ms: '69', src: 'README — round 6' },
  megapath:     { segments: '40,000', note: 'one stroke verb', src: 'README — round 7' },
  hero4k:       { res: '3840 × 2160', ms: '440–489', src: 'README — round 6' },
  longplay:     { frames: '256', rss: '38.3 → 39.0 MiB', flat: true, src: 'README — round 7' },
  han:          { runs: '107', note: 'CJK glyph runs / frame', src: 'README — round 7' },
  blends:       { count: '28', matrix: '16 / 16 distinct', src: 'README — round 7' },
  // ci/mobile device-suite artifacts, quoted in README §2.4
  android:      { fps: '59.3', ms: '4.09', device: 'Redmi Note 7 Pro (2019)',
                  src: 'device-suite CI artifacts' },
  // repo README — the green ledger
  ledger:       { plates: 90, tests: '518 + 237 + 13', upgrades: 'U-01 … U-23',
                  open: 0, apps: ['three', 'vavlt', 'upxcale', 'snapsearch'] },
  engine:       { crates: 36, supersample: '4×', gradstops: 16, trees: 'Widget → Element → Render' },
  repo:         'github.com/tejakota/vieww_artifacts',
  domain:       'vieww.dev',
};

/* ---- the code the studio types (verbatim from the repo) ------------------ */
const SAY_CODE = [
  '-- say-language: 1',
  'keep a whole number called count starting at 0',
  '',
  'screen "Home":',
  '    a column, spaced 16, children aligned to the start:',
  '        a heading "Counter"',
  '        a label "Tapped \\(count) times"',
  '        a button "Add one" which when tapped:',
  '            add 1 to count',
];

const SAY_DIAL = '        a dial "Volume" from 0 to 10 starting at 7';
const SAY_DIAL_ERR = '        a diall "Volume" from 0 to 10 starting at 7';

/* guide-counter/src/lib.rs, condensed to the descent's window */
const RUST_CODE = [
  'use vieww::prelude::*;',
  '',
  'fn main() -> Result<(), Box<dyn std::error::Error>> {',
  '    App::new().title("Counter").run(|driver| {',
  '        // signals live outside the tree, on the runtime',
  '        let count = driver.elements().runtime().signal(0i32);',
  '        driver.set_root(Counter { count });',
  '    })',
  '}',
  '',
  '#[widget]',
  'impl Counter {',
  '    fn build(&self, _ctx: &BuildContext) -> impl Into<WidgetNode> {',
  '        // reading the signal here IS the subscription',
  '        let value = self.count.get();',
  '        Container::new().padding(EdgeInsets::all(24.0))',
  '            .child(Flex::column().children(children![',
  '                Text::new(format!("Count: {value}")),',
  '                Button::new("Add one")',
  '                    .on_pressed(move || count.update(|n| *n += 1)),',
  '            ]))',
  '    }',
  '}',
];

/* the 36 crates, as the foundation scene blooms them (real names) */
const CRATES = [
  'vieww', 'vieww-widget', 'vieww-element', 'vieww-render', 'vieww-paint',
  'vieww-animation', 'vieww-runtime', 'vieww-scene', 'vieww-foundation',
  'vieww-text', 'vieww-asset', 'vieww-image', 'vieww-effects', 'vieww-gpu',
  'vieww-hal', 'vieww-shaders', 'vieww-render-graph', 'vieww-render-planner',
  'vieww-scroll', 'vieww-gestures', 'vieww-interaction', 'vieww-accessibility',
  'vieww-platform', 'vieww-platform-winit', 'vieww-platform-web',
  'vieww-platform-web-dom', 'vieww-hardware', 'vieww-build', 'vieww-reload',
  'vieww-plugin', 'vieww-plugin-macros', 'vieww-widget-macros',
  'vieww-say-codegen', 'vieww-devtools', 'vieww-test-harness', 'vieww-cli',
];

/* ---- act cards ------------------------------------------------------------ */
const ACTS = {
  ki:   { kanji: '起', romaji: 'KI', name: 'THE WAIT',         sub: 'introduce — the world as it is' },
  sho:  { kanji: '承', romaji: 'SHŌ', name: 'THE SESSION',     sub: 'develop — one session, every surface' },
  ten:  { kanji: '転', romaji: 'TEN', name: 'THE TWISTS',      sub: 'turn — what it was, from outside' },
  ketsu:{ kanji: '結', romaji: 'KETSU', name: 'THE RECONCILIATION', sub: 'reconcile — the new world settles' },
};

/* ---- the scene table -------------------------------------------------------
   dur: seconds on the film clock. name: timeline tooltip.
   builder + beats are wired in the act modules (act1..act4.js).
--------------------------------------------------------------------------- */
const SCENES = [
  { id: 'act1',    act: 'I',  name: 'Act I — The Wait',            dur: 3.0,  kind: 'actcard', card: 'ki' },
  { id: 'wait',    act: 'I',  name: 'The Wait',                    dur: 13.0 },
  { id: 'question',act: 'I',  name: 'The Question',               dur: 12.0 },
  { id: 'genesis', act: 'I',  name: 'viewwstudio',                dur: 11.0 },

  { id: 'act2',    act: 'II', name: 'Act II — The Session',       dur: 3.0,  kind: 'actcard', card: 'sho' },
  { id: 'studio',  act: 'II', name: 'The Studio Opens',           dur: 10.0 },
  { id: 'paint',   act: 'II', name: 'First Paint',                dur: 16.0 },
  { id: 'descent', act: 'II', name: 'The Descent',                dur: 12.0 },
  { id: 'compose', act: 'II', name: 'Compose Live',               dur: 16.0 },
  { id: 'receipt', act: 'II', name: 'The Receipt',                dur: 14.0 },
  { id: 'carry',   act: 'II', name: 'State Survives',             dur: 10.0 },
  { id: 'damage',  act: 'II', name: 'Damage in Pixels',           dur: 12.0 },
  { id: 'world',   act: 'II', name: 'The World Tour',             dur: 16.0 },

  { id: 'act3',    act: 'III',name: 'Act III — The Twists',       dur: 3.0,  kind: 'actcard', card: 'ten' },
  { id: 'mirror',  act: 'III',name: 'The Mirror',                 dur: 12.0 },
  { id: 'machine', act: 'III',name: 'The Machine Room',           dur: 22.0 },
  { id: 'bench',   act: 'III',name: 'The Receipts',               dur: 12.0 },
  { id: 'unfold',  act: 'III',name: 'The Unfold',                 dur: 18.0 },

  { id: 'act4',    act: 'IV', name: 'Act IV — The Reconciliation',dur: 3.0, kind: 'actcard', card: 'ketsu' },
  { id: 'foldback',act: 'IV', name: 'The Fold-Back',              dur: 7.0 },
  { id: 'stack',   act: 'IV', name: 'Built on vieww',             dur: 16.0 },
  { id: 'ledger',  act: 'IV', name: 'The Green Ledger',           dur: 15.0 },
  { id: 'endcard', act: 'IV', name: 'The End Card',               dur: 16.0 },
  { id: 'loop',    act: 'IV', name: 'The Loop',                   dur: 10.0 },
];

/* the witness ladder — which scene sets which value */
const LADDER = { paint: 1, descent: 2, compose: 3, carry: 4, damage: 5, world: 7 };
