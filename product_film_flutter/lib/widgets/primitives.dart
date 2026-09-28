/// The film's vocabulary: the handful of shapes every scene is written in.
///
/// The `Pane` is the studio's card shell (`theme.rs` chrome shape) — radius 8,
/// sheen from the top, a one-pixel rim of caught light, optional elevation.
/// Everything else is typography on the repo's two fonts.
library;

import 'package:flutter/widgets.dart';

import '../theme.dart' as t;

/// ---------------------------------------------------------------------------
/// Text helpers — Geist for voice, GeistMono for anything a machine said.
/// ---------------------------------------------------------------------------
class Voice extends StatelessWidget {
  final String text;
  final double size;
  final Color color;
  final FontWeight weight;
  final double? letterSpacing;

  const Voice(this.text, this.size, this.color, {super.key, this.weight = FontWeight.w400, this.letterSpacing});

  @override
  Widget build(BuildContext context) => Text(
        text,
        style: TextStyle(
          fontFamily: 'Geist',
          fontSize: size,
          color: color,
          fontWeight: weight,
          letterSpacing: letterSpacing,
          height: 1.35,
        ),
      );
}

class Mono extends StatelessWidget {
  final String text;
  final double size;
  final Color color;
  final FontWeight weight;
  final double? letterSpacing;

  const Mono(this.text, this.size, this.color, {super.key, this.weight = FontWeight.w400, this.letterSpacing});

  @override
  Widget build(BuildContext context) => Text(
        text,
        style: TextStyle(
          fontFamily: 'GeistMono',
          fontSize: size,
          color: color,
          fontWeight: weight,
          letterSpacing: letterSpacing,
          height: 1.55,
        ),
      );
}

/// The receipt tag — a quiet mono footnote citing the file a number came
/// from. Present for every metric the film states; this is the honesty beat.
class Receipt extends StatelessWidget {
  final String text;
  const Receipt(this.text, {super.key});

  @override
  Widget build(BuildContext context) => Mono(text, 11, t.kReceipt, letterSpacing: 0.3);
}

/// ---------------------------------------------------------------------------
/// `Pane` — the studio's card shell.
/// ---------------------------------------------------------------------------
class Pane extends StatelessWidget {
  final Widget? child;
  final Color color;
  final double radius;
  final int elevation; // 0 = resting on the ground, 1..4 = the ramp
  final EdgeInsetsGeometry? padding;
  final bool deepSheen;
  final BorderRadius? customRadius;

  const Pane({
    super.key,
    this.child,
    this.color = t.kChrome1,
    this.radius = t.kCardCorner,
    this.elevation = 0,
    this.padding,
    this.deepSheen = false,
    this.customRadius,
  });

  @override
  Widget build(BuildContext context) {
    final stops = deepSheen ? t.deepSheenStops(color) : t.sheenStops(color);
    final br = customRadius ?? BorderRadius.circular(radius);
    return Container(
      padding: padding,
      decoration: BoxDecoration(
        gradient: LinearGradient(
          begin: Alignment.topCenter,
          end: Alignment.bottomCenter,
          colors: stops,
          stops: const [0.0, 0.34, 1.0],
        ),
        borderRadius: br,
        border: Border(top: BorderSide(color: t.rim(color), width: 1.0)),
        boxShadow: elevation > 0 ? t.elevation(elevation) : null,
      ),
      child: child,
    );
  }
}

/// A hairline rule — `theme.rs`'s `line`, 1 device px.
class Hairline extends StatelessWidget {
  final Color color;
  const Hairline(this.color, {super.key});

  @override
  Widget build(BuildContext context) => Container(height: 1.0, color: color);
}

/// ---------------------------------------------------------------------------
/// `Caret` — the studio's own caret, blinking on the 530 ms interval the
/// editor's frame scheduler drives it at (`ui/caret.rs`).
/// ---------------------------------------------------------------------------
class Caret extends StatelessWidget {
  final double time; // seconds, film time
  final double width;
  final double height;
  final Color color;

  const Caret({super.key, required this.time, this.width = 2.0, this.height = 30.0, this.color = t.kInk});

  @override
  Widget build(BuildContext context) {
    final phase = (time * 1000.0 / t.kCaretBlinkMs) % 2.0;
    final on = phase < 1.0;
    return IgnorePointer(
      child: SizedBox(
        width: width,
        height: height,
        child: DecoratedBox(
          decoration: BoxDecoration(color: on ? color : const Color(0x00000000)),
        ),
      ),
    );
  }
}

/// ---------------------------------------------------------------------------
/// Typewriter — characters as a pure function of time, the studio's welcome
/// line and every "typed" beat in the film drawn with it.
/// ---------------------------------------------------------------------------
class TypeLine extends StatelessWidget {
  final String text;
  final double revealed; // 0..1 — how much of the line has landed
  final double size;
  final Color color;
  final bool mono;
  final FontWeight weight;

  const TypeLine({
    super.key,
    required this.text,
    required this.revealed,
    this.size = 28,
    this.color = t.kInk,
    this.mono = false,
    this.weight = FontWeight.w400,
  });

  @override
  Widget build(BuildContext context) {
    final n = (revealed.clamp(0.0, 1.0) * text.length).round();
    final shown = text.substring(0, n);
    final style = mono
        ? TextStyle(fontFamily: 'GeistMono', fontSize: size, color: color, fontWeight: weight, height: 1.55)
        : TextStyle(fontFamily: 'Geist', fontSize: size, color: color, fontWeight: weight, height: 1.35);
    return Text(shown, style: style);
  }
}

/// ---------------------------------------------------------------------------
/// Caption — the film's narration line. Bottom-left of the frame, lowercase,
/// mono, ink-muted; lands with a 10 pt rise like the splash's lines.
/// ---------------------------------------------------------------------------
class Caption extends StatelessWidget {
  final String text;
  final double progress; // 0..1
  final double size;
  final Color color;
  final bool mono;

  const Caption({
    super.key,
    required this.text,
    required this.progress,
    this.size = 21,
    this.color = t.kFilmMuted,
    this.mono = true,
  });

  @override
  Widget build(BuildContext context) {
    final p = progress.clamp(0.0, 1.0);
    if (p <= 0.0) return const SizedBox.shrink();
    return Opacity(
      opacity: p,
      child: Transform.translate(
        offset: Offset(0.0, (1.0 - p) * 10.0),
        child: mono
            ? Mono(text, size, color)
            : Voice(text, size, color),
      ),
    );
  }
}

/// A soft accent glow behind a subject — one radial gradient, no blur layers,
/// so a frame full of them still rasterises in a few milliseconds.
class Glow extends StatelessWidget {
  final double opacity;
  final Color color;
  final double radius;
  const Glow({super.key, required this.opacity, required this.color, required this.radius});

  @override
  Widget build(BuildContext context) {
    if (opacity <= 0.001) return const SizedBox.shrink();
    return IgnorePointer(
      child: Container(
        width: radius * 2,
        height: radius * 2,
        decoration: BoxDecoration(
          shape: BoxShape.circle,
          gradient: RadialGradient(
            colors: [Color.fromRGBO(color.r.round(), color.g.round(), color.b.round(), opacity), const Color(0x00000000)],
            stops: const [0.0, 1.0],
          ),
        ),
      ),
    );
  }
}

/// A small filled accent dot — the film's "idea" particle.
class Dot extends StatelessWidget {
  final double size;
  final Color color;
  const Dot({super.key, required this.size, required this.color});

  @override
  Widget build(BuildContext context) => Container(
        width: size,
        height: size,
        decoration: BoxDecoration(color: color, shape: BoxShape.circle),
      );
}

/// Small painted glyphs — the characters the font subsets may not carry
/// (✓ ▾ ◐ ▢), drawn as geometry the way the studio draws its view icons.
class CheckGlyph extends StatelessWidget {
  final double size;
  final Color color;
  const CheckGlyph({super.key, this.size = 10, this.color = t.kSuccess});

  @override
  Widget build(BuildContext context) => CustomPaint(
        size: Size.square(size),
        painter: _CheckPainter(size, color),
      );
}

class _CheckPainter extends CustomPainter {
  final double s;
  final Color c;
  _CheckPainter(this.s, this.c);

  @override
  void paint(Canvas canvas, Size size) {
    final paint = Paint()
      ..color = c
      ..style = PaintingStyle.stroke
      ..strokeWidth = s * 0.16
      ..strokeCap = StrokeCap.round;
    final p = Path()
      ..moveTo(s * 0.18, s * 0.55)
      ..lineTo(s * 0.42, s * 0.80)
      ..lineTo(s * 0.84, s * 0.26);
    canvas.drawPath(p, paint);
  }

  @override
  bool shouldRepaint(_CheckPainter old) => old.s != s || old.c != c;
}

/// A downward disclosure triangle (the sidebar's folder caret).
class DisclosureGlyph extends StatelessWidget {
  final double size;
  final Color color;
  const DisclosureGlyph({super.key, this.size = 10, this.color = t.kGutter});

  @override
  Widget build(BuildContext context) => CustomPaint(
        size: Size.square(size),
        painter: _DisclosurePainter(size, color),
      );
}

class _DisclosurePainter extends CustomPainter {
  final double s;
  final Color c;
  _DisclosurePainter(this.s, this.c);

  @override
  void paint(Canvas canvas, Size size) {
    final paint = Paint()..color = c;
    final p = Path()
      ..moveTo(s * 0.22, s * 0.32)
      ..lineTo(s * 0.78, s * 0.32)
      ..lineTo(s * 0.5, s * 0.72)
      ..close();
    canvas.drawPath(p, paint);
  }

  @override
  bool shouldRepaint(_DisclosurePainter old) => old.s != s || old.c != c;
}

/// The theme toggle: a circle, half filled.
class ThemeGlyph extends StatelessWidget {
  final double size;
  final Color color;
  const ThemeGlyph({super.key, this.size = 12, this.color = t.kGutter});

  @override
  Widget build(BuildContext context) => CustomPaint(
        size: Size.square(size),
        painter: _ThemePainter(size, color),
      );
}

class _ThemePainter extends CustomPainter {
  final double s;
  final Color c;
  _ThemePainter(this.s, this.c);

  @override
  void paint(Canvas canvas, Size size) {
    final stroke = Paint()
      ..color = c
      ..style = PaintingStyle.stroke
      ..strokeWidth = 1.2;
    canvas.drawCircle(Offset(s / 2, s / 2), s * 0.38, stroke);
    final fill = Paint()..color = c;
    final p = Path()..addArc(Rect.fromCircle(center: Offset(s / 2, s / 2), radius: s * 0.38), 1.5708, 3.1416);
    canvas.drawPath(p, fill);
  }

  @override
  bool shouldRepaint(_ThemePainter old) => old.s != s || old.c != c;
}

/// A window control: an outlined square (maximise) or a line (minimise).
class WindowGlyph extends StatelessWidget {
  final WindowGlyphKind kind;
  final double size;
  final Color color;
  const WindowGlyph(this.kind, {super.key, this.size = 11, this.color = t.kGutter});
  @override
  Widget build(BuildContext context) => CustomPaint(
        size: Size.square(size),
        painter: _WindowPainter(kind, size, color),
      );
}

enum WindowGlyphKind { minus, square, close }

class _WindowPainter extends CustomPainter {
  final WindowGlyphKind kind;
  final double s;
  final Color c;
  _WindowPainter(this.kind, this.s, this.c);

  @override
  void paint(Canvas canvas, Size size) {
    final paint = Paint()
      ..color = c
      ..style = PaintingStyle.stroke
      ..strokeWidth = 1.2;
    switch (kind) {
      case WindowGlyphKind.minus:
        canvas.drawLine(Offset(s * 0.18, s / 2), Offset(s * 0.82, s / 2), paint);
      case WindowGlyphKind.square:
        canvas.drawRect(Rect.fromCenter(center: Offset(s / 2, s / 2), width: s * 0.52, height: s * 0.52), paint);
      case WindowGlyphKind.close:
        canvas.drawLine(Offset(s * 0.25, s * 0.25), Offset(s * 0.75, s * 0.75), paint);
        canvas.drawLine(Offset(s * 0.75, s * 0.25), Offset(s * 0.25, s * 0.75), paint);
    }
  }

  @override
  bool shouldRepaint(_WindowPainter old) => old.kind != kind || old.s != s || old.c != c;
}

/// Positioned helpers so scene files stay declarative.
Positioned fill(Widget child, {double x = 0, double y = 0}) => Positioned.fill(
      child: Transform.translate(offset: Offset(x, y), child: child),
    );
