/// S17 · the damage — the same keystroke as S05, paid for honestly this
/// time: 86 x 23 px repaint, the rest of the frame not touched at all.
///
/// Receipts: retained repaint of the heaviest glass screen 21.25 -> 9.85 ms
/// (2.2x), asserted byte-identical, damage 0.72% of the surface
/// (TRACKER.md, the fixtures gallery's own assertion).
library;

import 'package:flutter/widgets.dart';

import '../../film.dart';
import '../../motion/vieww_motion.dart';
import '../../theme.dart' as t;
import '../../widgets/primitives.dart';
import 'common.dart';

Widget build(SceneCtx ctx) {
  final s = ctx.sec;
  final keySpring = VSpring(SpringSpec.standard).position(s - 1.0);
  // Only the region flashes — green, small, and nothing else moves.
  final regionFlash = window(s, 2.0, 0.6) * (1.0 - window(s, 3.0, 0.8));
  final nums = easeOutCubic(window(s, 3.8, 1.2));
  final stamp = VSpring(SpringSpec.expressive).position(s - 5.4);
  final barP = easeOutCubic(window(s, 6.6, 1.4));
  final endFade = easeOutCubic(window(ctx.seconds - s, 0.5, 0.5));

  const caretX = 890.0;
  const caretY = 460.0;

  return sceneFrame([
    // The mini editor — identical to S05's, deliberately.
    Positioned(
      left: 640,
      top: 360,
      child: Opacity(
        opacity: endFade,
        child: Pane(
          color: t.kChrome2,
          elevation: 1,
          padding: const EdgeInsets.all(20),
          child: SizedBox(
            width: 640,
            child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
              const Mono('editor', 11, t.kGutter),
              const SizedBox(height: 12),
              const Mono('let count = 0;', 16, t.kOnSurface),
              const SizedBox(height: 10),
              Row(children: [
                const Mono('render(', 16, t.kSyntaxFunction),
                Transform.scale(scale: 0.2 + 0.8 * keySpring, child: const Mono('1', 16, t.kSyntaxNumber)),
                const Mono(');', 16, t.kOnSurface),
              ]),
            ]),
          ),
        ),
      ),
    ),
    // The region — 86 x 23, flashing green, everything else still.
    Positioned(
      left: caretX - 43,
      top: caretY + 16,
      child: IgnorePointer(
        child: Opacity(
          opacity: regionFlash.clamp(0.0, 1.0),
          child: Container(
            width: 86,
            height: 23,
            decoration: BoxDecoration(
              color: t.kSuccess.withValues(alpha: 0.28),
              borderRadius: BorderRadius.circular(4),
              border: Border.all(color: t.kSuccess, width: 1.4),
            ),
          ),
        ),
      ),
    ),
    // The numbers, landing together.
    Positioned(
      left: 0,
      right: 0,
      top: 660,
      child: Opacity(
        opacity: nums * endFade,
        child: Center(
          child: Row(mainAxisAlignment: MainAxisAlignment.center, crossAxisAlignment: CrossAxisAlignment.center, children: [
            Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
              const Mono('21.25 -> 9.85 ms', 38, t.kSuccess, weight: FontWeight.w500),
              const SizedBox(height: 6),
              const Mono('retained repaint, heaviest glass screen', 15, t.kFilmMuted),
            ]),
            const SizedBox(width: 70),
            Transform.scale(
              scale: 0.85 + 0.15 * stamp.clamp(0.0, 1.0),
              child: Container(
                padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 8),
                decoration: BoxDecoration(
                  color: t.kAccentWash,
                  borderRadius: BorderRadius.circular(6),
                  border: Border.all(color: t.kAccent),
                ),
                child: const Mono('byte-identical', 16, t.kOnSurface, weight: FontWeight.w500),
              ),
            ),
          ]),
        ),
      ),
    ),
    // The damage hairline: 0.72% of the surface.
    Positioned(
      left: 660,
      top: 840,
      child: Opacity(
        opacity: barP * endFade,
        child: SizedBox(
          width: 600,
          child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
            const Mono('damage · 0.72% of the surface', 13, t.kFilmMuted),
            const SizedBox(height: 8),
            SizedBox(
              height: 4,
              child: Stack(children: [
                Container(decoration: BoxDecoration(color: t.kChrome4, borderRadius: BorderRadius.circular(2))),
                FractionallySizedBox(
                  widthFactor: 0.0072 * 100 / 100,
                  child: Container(decoration: BoxDecoration(color: t.kSuccess, borderRadius: BorderRadius.circular(2))),
                ),
              ]),
            ),
          ]),
        ),
      ),
    ),
    caption(ctx, 4.8, 'cost proportional to the change. not the screen.', end: 11.4),
    receipt(ctx, 3.4, 'TRACKER.md · fixtures gallery · damage assertion'),
  ]);
}
