/// S01 · the blank — the cold open. A caret, a line of text, a wait.
///
/// The caret blinks on the studio's own 530 ms interval; the typing is the
/// welcome-screen idiom. Nothing else moves: curiosity is silence with a
/// pulse in it.
library;

import 'package:flutter/widgets.dart';

import '../../film.dart';
import '../../motion/vieww_motion.dart';
import '../../theme.dart' as t;
import '../../widgets/primitives.dart';
import 'common.dart';

Widget build(SceneCtx ctx) {
  const line1 = 'every screen you have ever used';
  const line2 = 'began as a line of text.';
  const line3 = 'someone wrote it. then waited to see it.';

  final type1 = typedBy(ctx, 1.4, 3.2, line1);
  final type2 = typedBy(ctx, 5.0, 2.0, line2);
  final dim = easeOutCubic(window(ctx.sec, 8.4, 1.2));
  final line3P = easeOutCubic(window(ctx.sec, 8.8, 1.4));
  final endFade = easeOutCubic(window(ctx.seconds - ctx.sec, 0.7, 0.7));

  return sceneFrame([
    Positioned(
      left: 720,
      top: 496,
      child: Opacity(
        opacity: (1.0 - dim * 0.62) * endFade,
        child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
          Row(crossAxisAlignment: CrossAxisAlignment.end, children: [
            TypeLine(text: line1, revealed: type1, size: 30, color: t.kInk, mono: true),
            if (type2 <= 0.0) Caret(time: ctx.abs, height: 34),
          ]),
          const SizedBox(height: 14),
          Row(crossAxisAlignment: CrossAxisAlignment.end, children: [
            TypeLine(text: line2, revealed: type2, size: 30, color: t.kInk, mono: true),
            if (type2 > 0.0 && type2 < 1.0) Caret(time: ctx.abs, height: 34),
            if (type2 >= 1.0 && dim < 0.5) Caret(time: ctx.abs, height: 34),
          ]),
        ]),
      ),
    ),
    Positioned(
      left: 748,
      top: 620,
      child: Opacity(
        opacity: line3P * 0.9 * endFade,
        child: Transform.translate(
          offset: Offset(0, (1 - line3P) * 10),
          child: const Mono(line3, 21, t.kInkMuted),
        ),
      ),
    ),
  ]);
}
