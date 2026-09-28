/// S22 · the endcard — the mark, the wordmark, and the line the whole film
/// has been walking toward.
///
/// The mark is `brand.rs` drawn, not embedded: ground 0.00/1.00 r0.22,
/// editor 0.16,0.20,0.40x0.60 r0.08, preview 0.44,0.32,0.40x0.48 r0.08,
/// accent ramp #B491FF -> #7E5CE8. The last seconds dim to the mark and
/// the title alone, and hold.
library;

import 'dart:math' as math;

import 'package:flutter/widgets.dart';

import '../../film.dart';
import '../../motion/vieww_motion.dart';
import '../../theme.dart' as t;
import '../../widgets/brand.dart';
import '../../widgets/primitives.dart';
import 'common.dart';

Widget build(SceneCtx ctx) {
  final s = ctx.sec;
  // The mark assembles on the splash's own reveal grammar.
  final groundIn = easeOutCubic(window(s, 0.3, 0.8));
  final editorIn = easeOutCubic(window(s, 0.9, 0.8));
  final previewIn = easeOutCubic(window(s, 1.5, 0.8));
  // The wordmark, letter by letter, 30 ms apart.
  final wordStart = 2.8;
  // The line, then the fine print.
  final lineP = easeOutCubic(window(s, 5.2, 1.4));
  final versionP = easeOutCubic(window(s, 6.8, 1.4));
  // Breathing glow behind the mark.
  final breathe = 0.5 + 0.5 * math.sin(s * 1.6);
  // The closing dim — everything but the mark and the title, to black.
  final dim = easeInOutQuad(clamp01((s - 11.0) / 2.6));

  const word = 'viewwstudio';

  return Stack(children: [
    // The ground: pure black at the end, deep charcoal before it.
    Positioned.fill(
      child: ColoredBox(color: Color.lerp(t.kFilmGround, const Color(0xFF060607), dim)!),
    ),
    // The mark, centered high.
    Positioned(
      left: W / 2 - 130,
      top: 240,
      child: SizedBox(
        width: 260,
        height: 260,
        child: Stack(alignment: Alignment.center, children: [
          // The breathing glow — the product, alive.
          Glow(opacity: (0.10 + 0.08 * breathe) * (1.0 - dim * 0.5), color: t.kAccent, radius: 240 + 26 * breathe),
          Opacity(
            opacity: groundIn,
            child: Transform.scale(
              scale: 0.86 + 0.14 * groundIn,
              child: BrandMark(side: 190, editor: editorIn, preview: previewIn),
            ),
          ),
        ]),
      ),
    ),
    // The wordmark, staggered per letter.
    Positioned(
      left: 0,
      right: 0,
      top: 560,
      child: Center(
        child: Opacity(
          opacity: 1.0 - dim * 0.0,
          child: Row(mainAxisAlignment: MainAxisAlignment.center, children: [
            for (var i = 0; i < word.length; i++)
              _Letter(
                ch: word[i],
                p: VSpring(SpringSpec.expressive).position(s - (wordStart + i * 0.03)).clamp(0.0, 1.0),
              ),
          ]),
        ),
      ),
    ),
    // The line.
    Positioned(
      left: 0,
      right: 0,
      top: 700,
      child: Center(
        child: Opacity(
          opacity: lineP * (1.0 - dim * 0.3),
          child: Transform.translate(
            offset: Offset(0, (1 - lineP) * 10.0),
            child: const Mono('beta release available today', 18, t.kAccentFar, letterSpacing: 1.2),
          ),
        ),
      ),
    ),
    // The fine print.
    Positioned(
      left: 0,
      right: 0,
      top: 940,
      child: Center(
        child: Opacity(
          opacity: versionP * (1.0 - dim),
          child: const Mono('version 0.1.0 · apache-2.0', 12, t.kInkFaint, letterSpacing: 1.0),
        ),
      ),
    ),
  ]);
}

class _Letter extends StatelessWidget {
  final String ch;
  final double p;
  const _Letter({required this.ch, required this.p});

  @override
  Widget build(BuildContext context) {
    if (p <= 0.001) {
      // Reserve the space so the word does not reflow as letters land.
      return SizedBox(width: 38, child: Voice(ch, 64, const Color(0x00000000), weight: FontWeight.w700));
    }
    return Transform.translate(
      offset: Offset(0, (1 - p) * 26.0),
      child: Opacity(
        opacity: p,
        child: Voice(ch, 64, t.kInk, weight: FontWeight.w700),
      ),
    );
  }
}
