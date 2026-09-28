/// S11 · say — the English-facing front door. Say in, Rust out.
///
/// "counter.say --say-codegen--> counter.rs --studio rustc--> cdylib
/// --dlopen--> preview" — and every generated line points back at the say
/// line that produced it with a `// say:` marker.
library;

import 'package:flutter/widgets.dart';

import '../../film.dart';
import '../../motion/vieww_motion.dart';
import '../../theme.dart' as t;
import '../../widgets/primitives.dart';
import '../../widgets/studio/editor.dart';
import 'common.dart';

Widget build(SceneCtx ctx) {
  final s = ctx.sec;
  // Left types the say program; right fills with the Rust it becomes.
  final sayTyped = easeOutCubic(clamp01((s - 0.6) / 5.2));
  final genTyped = easeOutCubic(clamp01((s - 1.4) / 5.4));
  final syncP = easeOutCubic(window(s, 2.0, 1.2));
  final cap1 = easeOutCubic(window(s, 7.6, 1.2));
  final cap2 = easeOutCubic(window(s, 9.6, 1.2));
  final endFade = easeOutCubic(window(ctx.seconds - s, 0.5, 0.5));

  return sceneFrame([
    // Say, on the left.
    Positioned(
      left: 210,
      top: 210,
      child: Opacity(
        opacity: endFade,
        child: Pane(
          color: t.kChrome2,
          elevation: 1,
          padding: const EdgeInsets.all(22),
          child: SizedBox(
            width: 620,
            child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
              const Mono('counter.say', 12, t.kGutter),
              const SizedBox(height: 16),
              _SayLines(revealed: sayTyped),
            ]),
          ),
        ),
      ),
    ),
    // The Rust it becomes, on the right.
    Positioned(
      left: 1010,
      top: 210,
      child: Opacity(
        opacity: endFade,
        child: Pane(
          color: t.kChrome2,
          elevation: 1,
          padding: const EdgeInsets.all(22),
          child: SizedBox(
            width: 700,
            child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
              Row(children: [
                const Mono('generated rust', 12, t.kGutter),
                const SizedBox(width: 10),
                Opacity(opacity: syncP, child: const Mono('· every line says where it came from', 12, t.kReceipt)),
              ]),
              const SizedBox(height: 16),
              _GenLines(revealed: genTyped),
            ]),
          ),
        ),
      ),
    ),
    // The arrow between them — say-codegen.
    Positioned(
      left: 840,
      top: 470,
      child: Opacity(
        opacity: syncP * endFade,
        child: Column(children: [
          const Mono('say-codegen', 13, t.kAccentFar),
          const SizedBox(height: 10),
          Transform.scale(
            scale: 1.0,
            child: const _CodegenArrow(),
          ),
        ]),
      ),
    ),
    caption(ctx, 7.6, 'english in. rust out.', size: 24, color: t.kInk),
    caption(ctx, 9.6, 'no borrow checker required to start.', end: 12.4),
    receipt(ctx, 3.4, 'vieww-say-codegen · counter_gen.rs'),
  ]);
}

class _SayLines extends StatelessWidget {
  final double revealed;
  const _SayLines({required this.revealed});

  @override
  Widget build(BuildContext context) {
    // A focused excerpt of counter.say, revealed as a typing budget.
    final lines = kCounterSay;
    final total = lines.fold(0, (sum, l) => sum + l.plainText.length);
    var budget = revealed * total;
    final rows = <Widget>[];
    for (var i = 0; i < lines.length; i++) {
      final line = lines[i];
      final lineBudget = budget;
      budget -= line.plainText.length;
      rows.add(Padding(
        padding: const EdgeInsets.only(bottom: 10),
        child: SizedBox(
          height: 22,
          child: Row(mainAxisSize: MainAxisSize.min, children: _line(line, lineBudget, 16)),
        ),
      ));
    }
    return Column(crossAxisAlignment: CrossAxisAlignment.start, children: rows);
  }

  List<Widget> _line(CodeLine line, double budget, double fontPx) {
    final out = <Widget>[];
    for (final (text, tok) in line.tokens) {
      if (budget <= 0) break;
      final n = budget.clamp(0.0, text.length.toDouble()).round();
      if (n > 0) {
        out.add(Text(
          text.substring(0, n),
          style: TextStyle(fontFamily: 'GeistMono', fontSize: fontPx, color: tokColor(tok), height: 1.0, letterSpacing: 0.4),
        ));
      }
      budget -= text.length;
    }
    return out;
  }
}

class _GenLines extends StatelessWidget {
  final double revealed;
  const _GenLines({required this.revealed});

  @override
  Widget build(BuildContext context) {
    final lines = kCounterGen;
    final total = lines.fold(0, (sum, l) => sum + l.plainText.length);
    var budget = revealed * total;
    final rows = <Widget>[];
    for (var i = 0; i < lines.length; i++) {
      final line = lines[i];
      final lineBudget = budget;
      budget -= line.plainText.length;
      rows.add(Padding(
        padding: const EdgeInsets.only(bottom: 7),
        child: SizedBox(
          height: 17,
          child: Row(mainAxisSize: MainAxisSize.min, children: _line(line, lineBudget, 12.5)),
        ),
      ));
    }
    return Column(crossAxisAlignment: CrossAxisAlignment.start, children: rows);
  }

  List<Widget> _line(CodeLine line, double budget, double fontPx) {
    final out = <Widget>[];
    for (final (text, tok) in line.tokens) {
      if (budget <= 0) break;
      final n = budget.clamp(0.0, text.length.toDouble()).round();
      if (n > 0) {
        out.add(Text(
          text.substring(0, n),
          style: TextStyle(fontFamily: 'GeistMono', fontSize: fontPx, color: tokColor(tok), height: 1.0, letterSpacing: 0.3),
        ));
      }
      budget -= text.length;
    }
    return out;
  }
}

/// The arrow with a packet riding it — codegen as a direction, not a box.
class _CodegenArrow extends StatelessWidget {
  const _CodegenArrow();

  @override
  Widget build(BuildContext context) {
    return const SizedBox(width: 130, height: 90, child: CustomPaint(painter: _ArrowPainter()));
  }
}

class _ArrowPainter extends CustomPainter {
  const _ArrowPainter();

  @override
  void paint(Canvas canvas, Size size) {
    final paint = Paint()
      ..color = t.kAccentFar.withValues(alpha: 0.85)
      ..strokeWidth = 2.2
      ..strokeCap = StrokeCap.round;
    final path = Path()
      ..moveTo(8, 18)
      ..relativeCubicTo(50, -4, 66, 14, 104, 42);
    canvas.drawPath(path, paint);
    canvas.drawLine(const Offset(96, 52), const Offset(114, 62), paint);
    canvas.drawLine(const Offset(104, 42), const Offset(114, 62), paint);
  }

  @override
  bool shouldRepaint(_ArrowPainter old) => false;
}
