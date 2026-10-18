# Why each open cell in vieww's row is open, and what closes it

The matrix's rule 3: every `false`/`partial` cell in vieww's row carries a
plan here, with the real file or real command to start from — the same
four-question shape PENDING uses, in the smaller scope a matrix row needs.
When a cell turns `true`, its section here is deleted and the row (with a
`Matrix-Changed:` trailer) is the receipt.

## sr_ios — screen reader, iOS (false)

Owned by PENDING §3.6, which has the whole shape: post
`UIAccessibility.Notification.screenChanged` from the iOS semantics bridge,
assert a vieww button is heard with an XCUITest (`app.buttons["<label>"]`),
run it through `ci/mobile/device-farm/run-ios.sh`, and record the session
beside the TalkBack record. The static audit half exists
(`vieww-accessibility`'s `audit.rs`); what is missing is the OS side and a
recording.

## sr_desktop — screen reader, desktop (false)

Not yet owned by a PENDING entry — this section is the plan until it is.
The semantics tree the audit walks (`vieww-accessibility`'s `SemanticsTree`)
is the data all three desktop bridges want: UI Automation on Windows
(IRawElementProviderSimple), NSAccessibility on macOS, AT-SPI2 on Linux.
Start: the Windows one, via the `windows` crate's UIA bindings behind the
existing platform-gated module shape (`vieww-platform-winit` already
carries cfg-gated platform code), exposing one labelled button to
`Inspect`/`Accessibility Insights`. First proof is one button that
Accessibility Insights can name and focus; a full tree walk is the second
commit, not the first. Blocked on a Windows/macOS session to verify against
— the container this work is planned in has none, which is why the plan is
written and not started.

## ime — composition input (false)

Two halves. The out-of-process preview's keyboard gap is PENDING §3.3's to
close (it names winit's event forwarding). The main path has no composition
handling on record at all: `winit`'s `Ime` events (preedit/commit) are not
wired into the text pipeline anywhere the records show. Start: read
`vieww-platform-winit`'s window-event switch and `vieww-text`'s input
handling side by side; the commit that wires `Ime::Commit` into the editing
path with a test that feeds a preedit string through is the first cell
move. Dead status: the removed second text stack (`TRACKER`, the
2,262-line entry) is the warning — an editing path nobody drives is how
this got orphaned the first time.

## bidi_text — bidi text (partial)

Shaping is real (cosmic-text/harfrust — `TRACKER`'s notes on the removed
second stack say what it is *not*: `NaiveShaper` is gone). What is missing
is proof on mixed-direction content: no fixture in the gallery renders an
Arabic or Hebrew string, so nothing fails if bidi resolution breaks. Start:
one fixture with an RTL string beside an LTR one, run through
`examples/fixtures`, receipt checked in. If cosmic-text's output is right,
the cell moves to `true` on that receipt; if it is wrong, the fix is
downstream of the truth this fixture produces.

## rtl_layout — RTL layout (false)

Nothing in the record mirrors layout. The honest order of work is after
`bidi_text`'s fixture: mirroring is a transform at the layout layer
(`vieww-foundation`'s layout pass) gated on the resolved direction, and its
test is the same fixture with `Direction` flipped. Start after the bidi
fixture exists; until then any mirroring code would be untestable against
content that actually needs it.

## runtime_locale_switch — runtime locale switch (false)

Depends on nothing being wrong with the framework's own re-layout (it
relayouts on demand already — damage tracking's whole business). The missing
piece is the plumbing: a locale signal that reaches widgets holding
formatted content, and a test that flips locale mid-run and diffs the
frame. Start: `vieww-foundation`'s environment/`LocalText` surface (wherever
strings resolve — grep `localize`), add the switch, use the editor fixture
with a second locale file. Registered nowhere else; this section is its
registry until it moves to PENDING on first contact with the code.

## native_backends — Metal and D3D12 scene rendering (partial)

Owned by PENDING §1.2 and §1.3, which carry the starts verbatim: port
`SceneRenderer` onto `MetalDevice::pipeline`/`render_target`/`read_texture`
(post.wgsl's `Edge` flag included), and the same for D3D12, then run the
suites on hardware and record receipts. The smoke paths that prove the
drivers work are already in (`tests/metal_smoke.rs`, `tests/d3d12_smoke.rs`,
hardware receipts from 2026-10-15).

## shipped_apps — shipped applications (false)

The record's own answer: nothing ships, and the matrix does not count
samples. This cell moves when an application built on vieww ships to users
and says so — the plan is not a code start but the framework being good
enough that someone ships; every other cell in this file is on that path.
The one deliberate thing recorded here: do not move this cell for a demo, a
pilot, or a design partner. Shipped means a version number in the wild.
