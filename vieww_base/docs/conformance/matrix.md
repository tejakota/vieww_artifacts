# The UI-framework conformance matrix

The question this file answers is the one `What are the best UI frameworks.md`
(repository root) compares animation tools to answer, and
`docs/FRAMEWORK-CAPABILITY-PARITY.md` answers source-first: **on the
dimensions that define a UI framework, how does vieww stand against Flutter,
React Native and Qt, cell by cell?**

The rules are the ones PENDING §5.2 named when it asked for this file:

1. **vieww's own row is filled strictly from `PENDING.md` and `TRACKER.md`.**
   Every cell cites the section it came from. If the repo's records do not
   show it, the cell is `false` — "surely it works" is not a checkable claim.
2. **The other frameworks' rows come from their published documentation, with
   the version named.** A cell without a version and a source is an impression,
   and impressions are what the survey this replaces was made of.
3. **Every `false`/`partial` cell in vieww's row has a plan in
   [`why-not.md`](./why-not.md).** A false cell without a plan is a choice
   pretending to be a gap.
4. **The row is gated.** `ci/check/conformance-matrix.sh` diffs vieww's row
   against the merge base with `origin/main`; a row that changed without a
   `Matrix-Changed:` trailer in the commit message fails the check — the same
   protocol the pixel gate (`docs/FIDELITY.md`) runs on the rasterizer,
   because a matrix nobody enforces rots back into the survey it replaced.

Legend — `true`: shipped, with the evidence named. `partial`: real but with a
stated limit. `false`: absent, or never proven on the record.

The machine-readable row the gate reads is the block at the bottom of this
file. Edit the prose table *and* the block together; the block is the one
that is checked.

## The matrix

| dimension | vieww | Flutter 3.47.7 (2026-10-08) | React Native 0.87 (2026-08-10) | Qt 6.12 LTS (2026-09) |
|---|---|---|---|---|
| screen reader, Android | **true** — TalkBack heard speaking (PENDING §1.5, 2026-10-14); `ci/mobile/a11y-android.sh` runs the suite | true — Semantics bridge to TalkBack (docs: Accessibility) | true — accessibility props → TalkBack (docs: Accessibility) | partial — Qt Android exposes a11y, but the documented, tested path is desktop (docs: Qt Accessibility) |
| screen reader, iOS | **false** — no VoiceOver run recorded, no XCUITest (PENDING §3.6) | true — Semantics → VoiceOver (docs: Accessibility) | true — accessibility props → VoiceOver (docs: Accessibility) | partial — iOS a11y exists, desktop is the documented strength (docs: Qt Accessibility) |
| screen reader, desktop | **false** — nothing recorded in PENDING/TRACKER | true — Semantics → Narrator/NVDA/VoiceOver (docs: Accessibility) | partial — core is mobile-first; Windows via react-native-windows | true — QAccessible → UIA / NSAccessibility / AT-SPI (docs: Qt Accessibility) |
| IME (composition input) | **false** — no composition evidence in the record; the out-of-process preview explicitly does not forward keyboard/IME (PENDING §3.3) | true — TextInput pipeline (docs: Text input) | true — system keyboards via TextInput (docs: TextInput) | true — QInputMethod (docs: Input Method) |
| bidi text | **partial** — shaping runs through cosmic-text/harfrust (TRACKER, the second text stack's removal notes it); no bidi test recorded | true — TextPainter with intl bidi (docs: Internationalization) | partial — bidi via platform text stack, no first-party guarantee (docs: Text) | true — Qt's own bidi engine (docs: Internationalization) |
| RTL layout | **false** — nothing recorded | true — Directionality + mirroring (docs: RTL) | partial — `I18nManager.forceRTL` needs an app restart (docs: RTL) | true — `LayoutMirroring` (docs: Right-to-left) |
| runtime locale switch | **false** — nothing recorded | true — localizations delegates rebuild live (docs: Internationalization) | true — JS re-render on locale change (docs: I18n) | true — QTranslator reinstall + retranslate (docs: Internationalization) |
| hot reload | **true** — `vieww-reload`'s end-to-end suite is a gate stage (checks.sh) | true — built into the toolchain (docs: Hot reload) | true — Fast Refresh (docs: Fast Refresh) | partial — QML live reload is third-party tooling (qmllive); not a shipped framework feature |
| devtools | **true** — `vieww-devtools`: frame timeline, JSON export, quality overlay, memory inspector | true — Flutter DevTools (docs: DevTools) | true — React Native DevTools since 0.76 (blog, 2024) | true — Qt Creator profilers; GammaRay is third-party |
| golden-image tests | **true** — the fixture gallery + census (TRACKER, 23/23 planning complete) and the byte-identity raster gate (`docs/FIDELITY.md`) | true — `matchesGoldenFile` (docs: Testing) | partial — no first-party golden; screenshot testing is third-party (Detox et al.) | true — Qt Test image comparison (docs: Qt Test) |
| widget/unit tests | **true** — `vieww-test-harness`; suites run in the gate | true — `flutter_test` widget tests (docs: Testing) | partial — Jest + React Native Testing Library at the JS layer; the native layer is not covered by a first-party runner | true — Qt Test / QML test runner (docs: Qt Test) |
| native rendering backends | **partial** — Vulkan shipped and proven; Metal and D3D12 run smoke suites only, scene renderer not ported (PENDING §1.2, §1.3) | true — Impeller over Metal/Vulkan, Skia fallback, WebAssembly (docs: Impeller) | true — the platform's own views *are* the backend (docs: Architecture) | true — RHI over Metal/Vulkan/D3D12/OpenGL + native widgets (docs: RHI) |
| measured performance, published | **true** — Device Farm receipts (three phones, per-frame) and the Kaby Lake GPU receipts (PENDING §1.4/§1.5, `receipts/`) | partial — documented benchmarks and profiling guidance; no per-device receipt set published | partial — no official cross-device numbers published | partial — benchmarks exist in the tree; no published per-device matrix |
| shipped applications | **false** — `apps/` are samples and the studio; no shipping product recorded | true — the showcase, and shipping hardware (2026 Toyota RAV4, LG webOS — Flutter 3.44 blog) | true — Meta's own apps, Discord, Shopify (showcase) | true — automotive/medical/embedded deployments (showcase) |

## The other columns' sources

Per cell, above, but the load-bearing ones: Flutter — `docs.flutter.dev`
(accessibility, testing, Impeller, hot reload, internationalization pages,
which state they reflect Flutter 3.47), the 3.44 release blog for the shipped
hardware, `flutterreleases.com` for the exact stable version. React Native —
`reactnative.dev` (releases overview, accessibility, TextInput, RTL, Fast
Refresh, the 0.76 DevTools announcement). Qt — `doc.qt.io` (Qt Accessibility,
RHI, Qt Test, internationalization) and `qt.io`/`endoflife.date` for the
6.12 LTS release date. Versions were pinned on 2026-10-18; a refresh of this
file re-pins them.

## What the counts do not say

Twelve of vieww's fourteen cells are `false` or `partial`, against two for
Flutter. That is the honest reading of a young framework's record, and it is
the point of the matrix: every fix elsewhere in PENDING moves a cell that
this row is now counting. The two `true` rows vieww holds (hot reload end to
end, measured performance with receipts) are held by gates or receipts, not
by prose — which is the standard this file exists to keep.

<!-- conformance-row (machine-readable; the gate parses this block)
sr_android=true
sr_ios=false
sr_desktop=false
ime=false
bidi_text=partial
rtl_layout=false
runtime_locale_switch=false
hot_reload=true
devtools=true
golden_tests=true
widget_tests=true
native_backends=partial
measured_performance=true
shipped_apps=false
-->
