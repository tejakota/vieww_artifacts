/// S02 · the distance — the gap between writing a line and seeing it.
///
/// Code on the left, the picture on the right, and between them a wait that
/// only ever grows. The seconds counter is the honest kind: it just counts.
library;

import 'dart:math' as math;

import 'package:flutter/widgets.dart';

import '../../film.dart';
import '../../motion/vieww_motion.dart';
import '../../theme.dart' as t;
import '../../widgets/primitives.dart';
import 'common.dart';

Widget build(SceneCtx ctx) {
  // The gap breathes wider across the scene.
  final widen = easeInOutQuad(clamp01(ctx.sec / 9.0));
  final leftX = 300.0 - widen * 120.0;
  final rightX = 1420.0 + widen * 140.0;
  final codeScale = 1.0 - widen * 0.12;
  final phoneScale = 1.0 - widen * 0.12;

  // The wait counter: 0.8 -> 6.2 s, eased, held, never resolved.
  final waitP = easeOutCubic(window(ctx.sec, 2.0, 5.5));
  final wait = 0.8 + (6.2 - 0.8) * waitP;
  final waitColor = t.colorOver(
    Color.fromRGBO(248, 81, 73, (waitP * 0.55).clamp(0.0, 1.0)),
    t.kInkMuted,
  );

  final cap1 = easeOutCubic(window(ctx.sec, 8.6, 1.2));
  final cap2 = easeOutCubic(window(ctx.sec, 11.0, 1.2));
  final endFade = easeOutCubic(window(ctx.seconds - ctx.sec, 0.5, 0.5));

  return sceneFrame([
    // The code end.
    Positioned(
      left: leftX,
      top: 400,
      child: Transform.scale(
        scale: codeScale,
        child: Pane(
          color: t.kChrome2,
          elevation: 1,
          padding: const EdgeInsets.all(18),
          child: SizedBox(
            width: 320,
            child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
              const Mono('main.rs', 11, t.kGutter),
              const SizedBox(height: 10),
              const Mono('let count = 0;', 15, t.kSyntaxKeyword),
              const SizedBox(height: 6),
              const Mono('render(count);', 15, t.kSyntaxFunction),
              const SizedBox(height: 6),
              Mono('save. compile. reload.', 13, t.kSyntaxComment),
            ]),
          ),
        ),
      ),
    ),
    // The picture end.
    Positioned(
      left: rightX,
      top: 300,
      child: Transform.scale(
        scale: phoneScale,
        child: PhoneGhost(
          width: 250,
          height: 500,
          child: Column(children: [
            const SizedBox(height: 58),
            Mono('0', 84, t.kOnSurface.withValues(alpha: 0.8)),
          ]),
        ),
      ),
    ),
    // The gap: a hairline field of dots between the two, drifting.
    Positioned.fill(
      child: _GapMotes(time: ctx.abs, spread: widen),
    ),
    // The wait itself, centered.
    Positioned(
      left: 890,
      top: 470,
      child: Opacity(
        opacity: window(ctx.sec, 1.6, 0.8) * endFade,
        child: Column(children: [
          WaitSpinner(time: held24In60(ctx.abs), size: 26),
          const SizedBox(height: 16),
          Mono('${wait.toStringAsFixed(1)} s', 30, waitColor, weight: FontWeight.w500),
          const SizedBox(height: 8),
          const Mono('save · build · wait · reload', 13, t.kFilmFaint),
        ]),
      ),
    ),
    // The voice.
    Positioned(
      left: 0,
      right: 0,
      top: 850,
      child: Opacity(
        opacity: endFade,
        child: Column(children: [
          Opacity(
            opacity: cap1,
            child: const Mono('the distance between writing and seeing.', 21, t.kFilmMuted),
          ),
          const SizedBox(height: 14),
          Opacity(
            opacity: cap2,
            child: const Mono('every iteration, paid in seconds.', 21, t.kFilmFaint),
          ),
        ]),
      ),
    ),
  ]);
}

/// Faint motes drifting across the gap — the space itself, drawn.
class _GapMotes extends StatelessWidget {
  final double time;
  final double spread;
  const _GapMotes({required this.time, required this.spread});

  @override
  Widget build(BuildContext context) {
    final rng = Rng(0x5EED);
    return CustomPaint(painter: _MotesPainter(time, spread, rng), size: const Size(W, H));
  }
}

class _MotesPainter extends CustomPainter {
  final double time;
  final double spread;
  final Rng rng;
  _MotesPainter(this.time, this.spread, this.rng);

  @override
  void paint(Canvas canvas, Size size) {
    final paint = Paint()..color = t.kOnSurfaceVariant.withValues(alpha: 0.10);
    for (var i = 0; i < 90; i++) {
      final x0 = rng.range(760, 1160);
      final y0 = rng.range(180, 900);
      final drift = rng.range(6, 26);
      final x = x0 + math.sin(time * 0.7 + i) * drift * (0.4 + spread);
      final y = y0 + math.cos(time * 0.5 + i * 1.7) * drift * 0.4;
      canvas.drawCircle(Offset(x, y), rng.range(0.8, 1.9), paint);
    }
  }

  @override
  bool shouldRepaint(_MotesPainter old) => old.time != time || old.spread != spread;
}
