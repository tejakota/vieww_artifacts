/// The viewwstudio shell, rebuilt as a widget tree the film can drive.
///
/// Layout mirrors the real app's chrome: a 40 pt title bar (the mark where
/// the traffic-light dots used to be — `brand.rs`), a 52 pt activity bar
/// with the twelve views `state.rs` actually has, a 248 pt sidebar, the
/// editor, a 620 pt right pane with the device-framed preview, and a
/// 28 pt status bar. Cards are `Pane`s: radius 8, 6 pt gutters, sheen, rim.
///
/// Everything is a parameter: the scenes type, compile, tap and platform-
/// switch the studio while the camera moves, which is what the real film
/// rig does with a `Vec<(f32, Action)>` script.
library;

import 'dart:math' as math;

import 'package:flutter/widgets.dart';

import '../../theme.dart' as t;
import '../primitives.dart';
import '../brand.dart' show BrandMark;
import 'device_frame.dart';
import 'editor.dart';

/// The compile strip's state.
enum Compile { idle, running, done }

/// Which right-pane tab is in front.
enum RightTab { preview, inspector, devices, generated }

class StudioChrome extends StatelessWidget {
  final double time;

  /// The editor buffer.
  final List<CodeLine> buffer;

  /// Typing reveal of the buffer, 0..1.
  final double typed;

  /// Caret's line in the buffer.
  final int caretLine;

  /// The file name in the title bar / active tab.
  final String fileName;

  /// The active file row in the sidebar.
  final String activeFile;

  /// Right pane tab in front.
  final RightTab rightTab;

  /// The preview platform.
  final Platform platform;

  /// The preview screen content.
  final Widget screen;

  /// Mount animation for the device frame.
  final double deviceMount;

  /// Compile strip state + progress (0..1 while running).
  final Compile compile;
  final double compileProgress;

  /// 0..1 pulse on the Render button.
  final double renderPulse;

  /// Optional overlay above everything (command palette, dialogs).
  final Widget? overlay;

  /// Accent cycling for the Tokens-view beat (null = the default purple).
  final Color? accentOverride;

  const StudioChrome({
    super.key,
    required this.time,
    required this.buffer,
    this.typed = 1.0,
    this.caretLine = 0,
    this.fileName = 'counter.say',
    this.activeFile = 'counter.say',
    this.rightTab = RightTab.preview,
    this.platform = Platform.ios,
    required this.screen,
    this.deviceMount = 1.0,
    this.compile = Compile.idle,
    this.compileProgress = 0,
    this.renderPulse = 0,
    this.overlay,
    this.accentOverride,
  });

  @override
  Widget build(BuildContext context) {
    final accent = accentOverride ?? t.kAccent;
    return ColoredBox(
      color: t.kWindow,
      child: Stack(children: [
        Positioned.fill(
          child: Padding(
            padding: const EdgeInsets.all(t.kCardGap),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                _titleBar(),
                SizedBox(height: t.kCardGap),
                Expanded(
                  child: Row(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
                    _activityBar(accent),
                    SizedBox(width: t.kCardGap),
                    _sidebar(),
                    SizedBox(width: t.kCardGap),
                    Expanded(child: EditorPane(time: time, lines: buffer, typed: typed, caretLine: caretLine, activeTab: fileName)),
                    SizedBox(width: t.kCardGap),
                    _rightPane(accent),
                  ]),
                ),
                SizedBox(height: t.kCardGap),
                _statusBar(),
              ],
            ),
          ),
        ),
        if (overlay != null) Positioned.fill(child: overlay!),
      ]),
    );
  }

  // --------------------------------------------------------------- title bar

  Widget _titleBar() {
    return Pane(
      color: t.kChrome0,
      elevation: 1,
      padding: const EdgeInsets.symmetric(horizontal: 10),
      child: SizedBox(
        height: 40,
        child: Row(children: [
          const BrandMark(side: 18),
          const SizedBox(width: 10),
          Mono('viewwstudio', 12, t.kOnSurface, weight: FontWeight.w500),
          const SizedBox(width: 12),
          Mono('·  $fileName', 12, t.kGutter),
          const Spacer(),
          Mono('apache-2.0', 11, t.kGutter),
          const SizedBox(width: 14),
          const WindowGlyph(WindowGlyphKind.minus),
          const SizedBox(width: 12),
          const WindowGlyph(WindowGlyphKind.square),
          const SizedBox(width: 12),
          const WindowGlyph(WindowGlyphKind.close),
        ]),
      ),
    );
  }

  // ------------------------------------------------------------ activity bar

  Widget _activityBar(Color accent) {
    // The twelve views `state.rs` has, as the geometric glyphs the studio
    // draws. Selected = the preview view (index 0), per the film's scenes.
    return Pane(
      color: t.kChrome0,
      child: SizedBox(
        width: 52,
        child: Padding(
          padding: const EdgeInsets.symmetric(vertical: 10),
          child: Column(
            mainAxisAlignment: MainAxisAlignment.start,
            children: [
              for (var i = 0; i < 12; i++)
                Padding(
                  padding: const EdgeInsets.symmetric(vertical: 9),
                  child: _viewGlyph(i, i == 0 ? accent : t.kGutter, selected: i == 0),
                ),
            ],
          ),
        ),
      ),
    );
  }

  Widget _viewGlyph(int i, Color color, {bool selected = false}) {
    return Container(
      width: 34,
      height: 30,
      decoration: BoxDecoration(
        color: selected ? t.colorOver(const Color(0x3E7E5CE8), t.kChrome0) : null,
        borderRadius: BorderRadius.circular(6),
      ),
      child: CustomPaint(painter: _ViewGlyphPainter(index: i, color: color)),
    );
  }

  // ---------------------------------------------------------------- sidebar

  Widget _sidebar() {
    const files = ['counter.say', 'card_grid.rs', 'landing_screen.rs', 'live.rs', 'settings_form.rs'];
    return Pane(
      color: t.kChrome1,
      padding: const EdgeInsets.fromLTRB(0, 10, 0, 0),
      child: SizedBox(
        width: 232,
        child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
          Padding(
            padding: const EdgeInsets.only(left: 14, bottom: 8),
            child: Mono('EXPLORER', 10, t.kGutter, weight: FontWeight.w500, letterSpacing: 1.4),
          ),
          Padding(
            padding: const EdgeInsets.only(left: 8, bottom: 4),
            child: Row(children: [
              const DisclosureGlyph(size: 9),
              const SizedBox(width: 6),
              Mono('SCREENS', 10, t.kGutter, weight: FontWeight.w500, letterSpacing: 1.0),
            ]),
          ),
          for (final f in files)
            _fileRow(f, f == activeFile),
          const Spacer(),
          Padding(
            padding: const EdgeInsets.only(left: 14, bottom: 10),
            child: Mono('say · 10 lines', 10, t.kReceipt),
          ),
        ]),
      ),
    );
  }

  Widget _fileRow(String name, bool active) {
    return Container(
      height: 26,
      decoration: BoxDecoration(
        color: active ? t.colorOver(const Color(0x5E7E5CE8), t.kChrome1) : null,
        border: Border(left: BorderSide(color: active ? t.kAccent : const Color(0x00000000), width: 2)),
      ),
      padding: const EdgeInsets.only(left: 30),
      child: Align(
        alignment: Alignment.centerLeft,
        child: Mono(name, 12, active ? t.kOnSurface : t.kGutter),
      ),
    );
  }

  // -------------------------------------------------------------- right pane

  Widget _rightPane(Color accent) {
    return Pane(
      color: t.kChrome1,
      padding: const EdgeInsets.fromLTRB(0, 0, 0, 0),
      child: SizedBox(
        width: 620,
        child: Padding(
          padding: const EdgeInsets.all(10),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              _rightTabs(),
              const SizedBox(height: 8),
              _toolbar(accent),
              const SizedBox(height: 6),
              Expanded(
                child: switch (rightTab) {
                  RightTab.preview => _previewArea(),
                  RightTab.inspector => const InspectorTree(),
                  RightTab.devices => const DevicesGrid(),
                  RightTab.generated => _generatedPane(),
                },
              ),
              const SizedBox(height: 8),
              _renderBar(accent),
            ],
          ),
        ),
      ),
    );
  }

  Widget _rightTabs() {
    const tabs = [('Preview', RightTab.preview), ('Inspector', RightTab.inspector), ('Devices', RightTab.devices), ('Generated', RightTab.generated)];
    return SizedBox(
      height: 30,
      child: Row(children: [
        for (final (label, tab) in tabs)
          Padding(
            padding: const EdgeInsets.only(right: 16),
            child: Column(mainAxisAlignment: MainAxisAlignment.center, children: [
              Mono(label, 12, tab == rightTab ? t.kOnSurface : t.kGutter),
              const SizedBox(height: 4),
              Container(width: label.length * 7.2, height: 2, color: tab == rightTab ? t.kAccent : const Color(0x00000000)),
            ]),
          ),
      ]),
    );
  }

  Widget _toolbar(Color accent) {
    return SizedBox(
      height: 32,
      child: Row(children: [
        _segment(Platform.ios, 'iOS', accent),
        _segment(Platform.android, 'Android', accent),
        _segment(Platform.desktop, 'Desktop', accent),
        const SizedBox(width: 12),
        const ThemeGlyph(),
        const SizedBox(width: 8),
        Mono('auto-render 600ms', 11, t.kReceipt),
      ]),
    );
  }

  Widget _segment(Platform p, String label, Color accent) {
    final active = p == platform;
    return Padding(
      padding: const EdgeInsets.only(right: 4),
      child: Container(
        height: 26,
        padding: const EdgeInsets.symmetric(horizontal: 10),
        decoration: BoxDecoration(
          color: active ? t.colorOver(const Color(0x3E7E5CE8), t.kChrome2) : t.kChrome2,
          borderRadius: BorderRadius.circular(6),
          border: Border.all(color: active ? accent : t.kLine),
        ),
        child: Center(child: Mono(label, 11, active ? t.kOnSurface : t.kGutter)),
      ),
    );
  }

  Widget _previewArea() {
    return Padding(
      padding: const EdgeInsets.fromLTRB(8, 4, 8, 4),
      child: DeviceFrame(platform: platform, screen: screen, mount: deviceMount),
    );
  }

  Widget _generatedPane() {
    return Pane(
      color: t.kChrome2,
      padding: const EdgeInsets.fromLTRB(10, 10, 0, 10),
      child: SingleChildScrollView(
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            for (var i = 0; i < kCounterGen.length; i++)
              SizedBox(
                height: 19,
                child: _genLine(kCounterGen[i]),
              ),
          ],
        ),
      ),
    );
  }

  Widget _genLine(CodeLine line) {
    return Row(mainAxisSize: MainAxisSize.min, children: [
      for (final (text, tok) in line.tokens)
        Text(
          text,
          style: TextStyle(
            fontFamily: 'GeistMono',
            fontSize: 11.5,
            color: tokColor(tok),
            height: 1.0,
            letterSpacing: 0.3,
          ),
        ),
    ]);
  }

  Widget _renderBar(Color accent) {
    return SizedBox(
      height: 68,
      child: Row(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
        // Render button, filled, pulsing on compile.
        Container(
          width: 150,
          decoration: BoxDecoration(
            color: t.colorOver(Color.fromRGBO(0, 0, 0, 0.15 * renderPulse.clamp(0.0, 1.0)), accent),
            borderRadius: BorderRadius.circular(t.kCardCorner),
            boxShadow: t.elevation(1),
          ),
          child: Center(
            child: Mono('Render', 13, const Color(0xFFFFFFFF), weight: FontWeight.w500),
          ),
        ),
        const SizedBox(width: 10),
        Expanded(child: _compileStrip()),
      ]),
    );
  }

  Widget _compileStrip() {
    Widget body;
    switch (compile) {
      case Compile.idle:
        body = Column(
          mainAxisAlignment: MainAxisAlignment.center,
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Mono('auto-render after 600 ms', 11, t.kReceipt),
          ],
        );
      case Compile.running:
        final p = compileProgress.clamp(0.0, 1.0);
        body = Column(
          mainAxisAlignment: MainAxisAlignment.center,
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Mono('rustc · one invocation · ${(p * 100).round()}%', 11, t.kOnSurfaceVariant),
            const SizedBox(height: 5),
            _progressBar(p),
          ],
        );
      case Compile.done:
        body = Row(
          children: [
            const CheckGlyph(size: 11),
            const SizedBox(width: 6),
            Mono('rendered in 0.8 s', 11, t.kOnSurfaceVariant),
          ],
        );
    }
    return Pane(color: t.kChrome2, padding: const EdgeInsets.symmetric(horizontal: 12), child: body);
  }

  Widget _progressBar(double p) {
    return SizedBox(
      height: 4,
      child: Stack(children: [
        Container(decoration: BoxDecoration(color: t.kChrome4, borderRadius: BorderRadius.circular(2))),
        FractionallySizedBox(
          widthFactor: p.clamp(0.0, 1.0),
          child: Container(decoration: BoxDecoration(color: t.kAccent, borderRadius: BorderRadius.circular(2))),
        ),
      ]),
    );
  }

  // -------------------------------------------------------------- status bar

  Widget _statusBar() {
    return Pane(
      color: t.kChrome0,
      padding: const EdgeInsets.symmetric(horizontal: 12),
      child: SizedBox(
        height: 28,
        child: Row(children: [
          const CheckGlyph(size: 10, color: t.kGutter),
          const SizedBox(width: 6),
          Mono('0 problems', 11, t.kGutter),
          const SizedBox(width: 16),
          Mono('say · UTF-8 · LF', 11, t.kGutter),
          const Spacer(),
          Mono('Ln ${caretLine + 1}, Col 8', 11, t.kGutter),
          const SizedBox(width: 16),
          Mono('vieww 0.1.0', 11, t.kGutter),
        ]),
      ),
    );
  }
}

/// ------------------------------------------------------------- the overlays

/// The Inspector — the live render tree, one row per node.
class InspectorTree extends StatelessWidget {
  const InspectorTree({super.key});

  @override
  Widget build(BuildContext context) {
    const rows = [
      (0, 'SafeArea'),
      (1, 'Container · surface'),
      (2, 'Column · spacing 16'),
      (3, 'Heading · "Counter"'),
      (3, 'Label · "Tapped 1 times"'),
      (3, 'Button · "Add one"'),
      (4, 'GestureDetector'),
    ];
    return Pane(
      color: t.kChrome2,
      padding: const EdgeInsets.all(14),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Mono('ELEMENT TREE · 7 NODES · 1 REBUILT', 10, t.kGutter, letterSpacing: 1.2),
          const SizedBox(height: 10),
          for (final (depth, label) in rows)
            Padding(
              padding: EdgeInsets.only(left: depth * 18.0, bottom: 7),
              child: Row(children: [
                Container(width: 5, height: 5, decoration: const BoxDecoration(color: t.kGutter, shape: BoxShape.circle)),
                const SizedBox(width: 8),
                Mono(label, 12, depth == 4 ? t.kSyntaxFunction : t.kOnSurfaceVariant),
                if (depth == 4) ...[
                  const SizedBox(width: 8),
                  Container(
                    padding: const EdgeInsets.symmetric(horizontal: 5),
                    decoration: BoxDecoration(color: t.kAccentWash, borderRadius: BorderRadius.circular(3)),
                    child: const Mono('rebuilt', 9, t.kOnSurface),
                  ),
                ],
              ]),
            ),
        ],
      ),
    );
  }
}

/// The Devices view — the three platforms with their real metrics.
class DevicesGrid extends StatelessWidget {
  const DevicesGrid({super.key});

  @override
  Widget build(BuildContext context) {
    const devices = [
      ('iOS', '393x852 · @3.0 · safe 47/34'),
      ('Android', '412x915 · @2.625 · insets 30/24'),
      ('Desktop', '1280x800 · @1.0'),
    ];
    return Pane(
      color: t.kChrome2,
      padding: const EdgeInsets.all(14),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          for (final (name, spec) in devices)
            Padding(
              padding: const EdgeInsets.only(bottom: 8),
              child: Pane(
                color: t.kChrome3,
                padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 10),
                child: Row(children: [
                  Mono(name, 12, t.kOnSurface, weight: FontWeight.w500),
                  const Spacer(),
                  Mono(spec, 11, t.kGutter),
                ]),
              ),
            ),
          const Spacer(),
          const Receipt('device metrics · state.rs'),
        ],
      ),
    );
  }
}

/// The command palette overlay — 107 commands, one input.
class CommandPalette extends StatelessWidget {
  final String query;
  final double open; // 0..1

  const CommandPalette({super.key, required this.query, required this.open});

  @override
  Widget build(BuildContext context) {
    if (open <= 0.001) return const SizedBox.shrink();
    const commands = [
      'vieww: render buffer',
      'vieww: toggle damage overlay',
      'vieww: switch platform',
      'editor: format document',
      'editor: find in files',
      'tokens: export theme as rust',
      'build: android (debug apk)',
      'build: ios',
    ];
    final q = query.toLowerCase();
    final hits = commands.where((c) => c.contains(q)).toList();
    return IgnorePointer(
      child: Opacity(
        opacity: open.clamp(0.0, 1.0),
        child: Container(
          color: const Color(0x68000000),
          alignment: Alignment.topCenter,
          child: Padding(
            padding: const EdgeInsets.only(top: 90),
            child: Pane(
              color: t.kChrome3,
              elevation: 3,
              padding: const EdgeInsets.all(0),
              child: SizedBox(
                width: 620,
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.stretch,
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    Padding(
                      padding: const EdgeInsets.fromLTRB(16, 14, 16, 10),
                      child: Row(children: [
                        Mono('>', 15, t.kAccent, weight: FontWeight.w500),
                        const SizedBox(width: 10),
                        Mono(query, 15, t.kOnSurface),
                        const SizedBox(width: 2),
                        const Caret(time: 0, height: 17, width: 2),
                      ]),
                    ),
                    const Hairline(t.kLine),
                    for (var i = 0; i < hits.length && i < 6; i++)
                      Container(
                        height: 30,
                        color: i == 0 ? t.colorOver(const Color(0x5E7E5CE8), t.kChrome3) : null,
                        padding: const EdgeInsets.symmetric(horizontal: 38),
                        child: Align(alignment: Alignment.centerLeft, child: Mono(hits[i], 12.5, t.kOnSurfaceVariant)),
                      ),
                    const SizedBox(height: 8),
                    Padding(
                      padding: const EdgeInsets.only(left: 16, bottom: 10),
                      child: Mono('${hits.length} of 107 commands', 10, t.kReceipt),
                    ),
                  ],
                ),
              ),
            ),
          ),
        ),
      ),
    );
  }
}

/// ------------------------------------------------------------- view glyphs

/// Twelve geometric glyphs, 18x18, one stroke weight, the studio's way.
class _ViewGlyphPainter extends CustomPainter {
  final int index;
  final Color color;
  _ViewGlyphPainter({required this.index, required this.color});

  @override
  void paint(Canvas canvas, Size size) {
    final paint = Paint()
      ..color = color
      ..style = PaintingStyle.stroke
      ..strokeWidth = 1.4;
    final c = Offset(size.width / 2, size.height / 2);
    switch (index) {
      case 0: // explorer — two overlapping squares
        canvas.drawRect(const Rect.fromLTWH(3, 5, 9, 9), paint);
        canvas.drawRect(const Rect.fromLTWH(7, 2, 8, 8), paint);
      case 1: // search — circle + handle
        canvas.drawCircle(Offset(8, 8), 4.5, paint);
        canvas.drawLine(const Offset(11.5, 11.5), const Offset(15, 15), paint);
      case 2: // snippets — braces
        canvas.drawLine(const Offset(6, 3), const Offset(4, 6), paint);
        canvas.drawLine(const Offset(4, 6), const Offset(6, 9), paint);
        canvas.drawLine(const Offset(4, 9), const Offset(6, 12), paint);
        canvas.drawLine(const Offset(12, 3), const Offset(14, 6), paint);
        canvas.drawLine(const Offset(14, 6), const Offset(12, 9), paint);
        canvas.drawLine(const Offset(14, 9), const Offset(12, 12), paint);
      case 3: // problems — triangle + dot
        final path = Path()
          ..moveTo(8, 3)
          ..lineTo(14, 14)
          ..lineTo(2, 14)
          ..close();
        canvas.drawPath(path, paint);
        canvas.drawCircle(Offset(8, 11), 1.1, paint..style = PaintingStyle.fill);
      case 4: // inspector — tree
        canvas.drawLine(const Offset(3, 4), const Offset(3, 13), paint);
        canvas.drawLine(const Offset(3, 6), const Offset(9, 6), paint);
        canvas.drawLine(const Offset(3, 11), const Offset(9, 11), paint);
        canvas.drawCircle(const Offset(11.5, 6), 1.6, paint);
        canvas.drawCircle(const Offset(11.5, 11), 1.6, paint);
      case 5: // learn — book
        canvas.drawRect(const Rect.fromLTWH(3, 4, 12, 9), paint);
        canvas.drawLine(const Offset(9, 4), const Offset(9, 13), paint);
      case 6: // docs — page
        final path = Path()
          ..moveTo(5, 2)
          ..lineTo(12, 2)
          ..lineTo(14, 5)
          ..lineTo(14, 14)
          ..lineTo(5, 14)
          ..close();
        canvas.drawPath(path, paint);
        canvas.drawLine(const Offset(7, 8), const Offset(12, 8), paint);
        canvas.drawLine(const Offset(7, 11), const Offset(12, 11), paint);
      case 7: // toolchain — wrench
        canvas.drawCircle(Offset(6, 6), 4, paint);
        canvas.drawLine(const Offset(9, 9), const Offset(14, 14), paint);
      case 8: // source — node graph
        canvas.drawCircle(const Offset(5, 5), 2.4, paint);
        canvas.drawCircle(const Offset(12, 6), 2.4, paint);
        canvas.drawCircle(const Offset(8, 12), 2.4, paint);
        canvas.drawLine(const Offset(7, 6), const Offset(10, 6), paint);
        canvas.drawLine(const Offset(6, 7), const Offset(7.5, 10.5), paint);
        canvas.drawLine(const Offset(11, 8), const Offset(9.5, 10.5), paint);
      case 9: // export — arrow out of box
        canvas.drawRect(const Rect.fromLTWH(3, 8, 12, 6), paint);
        canvas.drawLine(const Offset(9, 11), const Offset(9, 3), paint);
        canvas.drawLine(const Offset(6, 5), const Offset(9, 2), paint);
        canvas.drawLine(const Offset(12, 5), const Offset(9, 2), paint);
      case 10: // tokens — swatches
        canvas.drawCircle(const Offset(6, 6), 3.4, paint);
        canvas.drawCircle(const Offset(11.5, 10.5), 3.4, paint);
      case 11: // settings — gear
        canvas.drawCircle(Offset(9, 8), 3.6, paint);
        for (var a = 0; a < 8; a++) {
          final rad = a * 3.14159 / 4.0;
          canvas.drawLine(
            Offset(9 + 5.2 * math.cos(rad), 8 + 5.2 * math.sin(rad)),
            Offset(9 + 6.8 * math.cos(rad), 8 + 6.8 * math.sin(rad)),
            paint,
          );
        }
    }
  }

  @override
  bool shouldRepaint(_ViewGlyphPainter old) => old.index != index || old.color != color;
}
