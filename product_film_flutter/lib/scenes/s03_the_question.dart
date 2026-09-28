/// S03 · the question — everything collapses to one dot, and the first drop
/// of the studio's accent enters the film. Curiosity turning into want.
library;

import 'dart:math' as math;

import 'package:flutter/widgets.dart';

import '../../film.dart';
import '../../motion/vieww_motion.dart';
import '../../theme.dart' as t;
import '../../widgets/primitives.dart';
import 'common.dart';

Widget build(SceneCtx ctx) {
  final spring = VSpring(SpringSpec.expressive);
  final dotIn = spring.position(ctx.sec - 0.4);
  final q1 = easeOutCubic(window(ctx.sec, 1.8, 1.3));
  final q2 = easeOutCubic(window(ctx.sec, 5.6, 1.3));
  final tinge = easeOutCubic(window(ctx.sec, 3.4, 2.2));
  final glowP = easeOutCubic(window(ctx.sec, 7.6, 1.8));
  final pulse = 1.0 + 0.05 * math.sin(ctx.sec * 2.2);
  final endFade = easeOutCubic(window(ctx.seconds - ctx.sec, 0.5, 0.5));

  // The dot tinges from ink toward the accent as the question lands.
  final dotColor = Color.lerp(t.kInk, t.kAccent, tinge)!;

  return sceneFrame([
    // The dot — one idea, arriving.
    Positioned(
      left: W / 2 - 110,
      top: 430,
      child: SizedBox(
        width: 220,
        height: 220,
        child: Opacity(
          opacity: endFade,
          child: Stack(alignment: Alignment.center, children: [
            Glow(opacity: 0.05 + 0.22 * glowP, color: t.kAccent, radius: 200 * pulse),
            Transform.scale(
              scale: dotIn * pulse,
              child: Container(
                width: 14,
                height: 14,
                decoration: BoxDecoration(color: dotColor, shape: BoxShape.circle),
              ),
            ),
          ]),
        ),
      ),
    ),
    // The questions.
    Positioned(
      left: 0,
      right: 0,
      top: 700,
      child: Opacity(
        opacity: endFade,
        child: Column(children: [
          risingLine(q1, 'what if the distance was zero?', 24, t.kFilmMuted),
          const SizedBox(height: 22),
          risingLine(q2, 'what if the line was the picture?', 24, t.kFilmInk),
        ]),
      ),
    ),
  ]);
}
