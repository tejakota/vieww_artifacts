/// S21 · beta — the checklist closes, and the sentence the film exists to
/// say: beta release, available today.
library;

import 'package:flutter/widgets.dart';

import '../../film.dart';
import '../../motion/vieww_motion.dart';
import '../../theme.dart' as t;
import '../../widgets/primitives.dart';
import 'common.dart';

Widget build(SceneCtx ctx) {
  final s = ctx.sec;
  final line1 = typedBy(ctx, 0.6, 2.2, 'the beta ships when every p0 is closed.');
  final cardIn = easeOutCubic(window(s, 3.2, 1.0));
  final ticks = staggerStarts(5, 0.55, offset: 3.8);
  final cardOut = easeOutCubic(window(s, 6.4, 0.8));
  final word1 = easeOutCubic(window(s, 7.6, 1.2));
  final word2 = easeOutCubic(window(s, 8.6, 1.2));
  final sweep = clamp01((s - 9.4) / 1.6);
  final glowP = easeOutCubic(window(s, 9.0, 2.2));

  const items = ['p0 · windowing on every os', 'p0 · signed installers', 'p0 · scaffolded projects build', 'p1 · animation budget held', 'p1 · changelog, release notes'];

  return Stack(children: [
    const Positioned.fill(child: FilmGround()),
    // The opening line.
    Positioned(
      left: 0,
      right: 0,
      top: 300,
      child: Center(
        child: Opacity(
          opacity: 1.0 - cardOut * 0.85,
          child: TypeLine(text: 'the beta ships when every p0 is closed.', revealed: line1, size: 24, color: t.kFilmMuted, mono: true),
        ),
      ),
    ),
    // The closing checklist.
    Positioned(
      left: W / 2 - 360,
      top: 420,
      child: Opacity(
        opacity: cardIn * (1.0 - cardOut),
        child: Pane(
          color: t.kChrome1,
          elevation: 2,
          padding: const EdgeInsets.all(28),
          child: SizedBox(
            width: 720,
            child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
              const Mono('THE GATE', 11, t.kGutter, letterSpacing: 1.4),
              const SizedBox(height: 16),
              for (var i = 0; i < items.length; i++)
                Padding(
                  padding: const EdgeInsets.only(bottom: 12),
                  child: Row(children: [
                    Transform.scale(
                      scale: VSpring(SpringSpec.expressive).position(s - ticks[i]).clamp(0.0, 1.0),
                      child: const CheckGlyph(size: 15),
                    ),
                    const SizedBox(width: 14),
                    Mono(items[i], 14, t.kOnSurfaceVariant),
                  ]),
                ),
            ]),
          ),
        ),
      ),
    ),
    // The sentence.
    Positioned(
      left: 0,
      right: 0,
      top: 560,
      child: IgnorePointer(
        child: Column(children: [
          Transform.translate(
            offset: Offset(0, (1 - word1) * 16.0),
            child: Opacity(
              opacity: word1,
              child: const Voice('beta release', 58, t.kInk, weight: FontWeight.w700),
            ),
          ),
          const SizedBox(height: 18),
          Transform.translate(
            offset: Offset(0, (1 - word2) * 16.0),
            child: Opacity(
              opacity: word2,
              child: const Voice('available today.', 44, t.kAccentFar, weight: FontWeight.w500),
            ),
          ),
        ]),
      ),
    ),
    // The underline sweep.
    if (sweep > 0.001)
      Positioned(
        left: W / 2 - 220,
        top: 720,
        child: SizedBox(
          width: 440,
          height: 3,
          child: CustomPaint(painter: _SweepPainter(sweep)),
        ),
      ),
    // The rising warmth.
    if (glowP > 0.001)
      Positioned(
        left: W / 2 - 700,
        top: 300,
        child: IgnorePointer(
          child: Opacity(
            opacity: glowP,
            child: Container(
              width: 1400,
              height: 700,
              decoration: BoxDecoration(
                gradient: RadialGradient(
                  center: const Alignment(0.0, 1.2),
                  radius: 1.1,
                  colors: [t.kAccent.withValues(alpha: 0.14), const Color(0x00000000)],
                  stops: const [0.0, 1.0],
                ),
              ),
            ),
          ),
        ),
      ),
  ]);
}

class _SweepPainter extends CustomPainter {
  final double p;
  _SweepPainter(this.p);

  @override
  void paint(Canvas canvas, Size size) {
    final w = size.width * p.clamp(0.0, 1.0);
    final paint = Paint()
      ..shader = LinearGradient(
        colors: [t.kAccent.withValues(alpha: 0.0), t.kAccentFar, t.kAccent.withValues(alpha: 0.0)],
      ).createShader(Rect.fromLTWH(0, 0, size.width, size.height))
      ..strokeWidth = 2.4
      ..strokeCap = StrokeCap.round;
    canvas.drawLine(Offset(0, 1), Offset(w, 1), paint);
  }

  @override
  bool shouldRepaint(_SweepPainter old) => old.p != p;
}
