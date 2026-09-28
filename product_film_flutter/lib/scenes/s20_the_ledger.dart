/// S20 · the ledger — every number the film has claimed, in one place,
/// each with the file it was measured in. The honesty beat, played as a
/// roll call.
library;

import 'package:flutter/widgets.dart';

import '../../film.dart';
import '../../motion/vieww_motion.dart';
import '../../theme.dart' as t;
import '../../widgets/primitives.dart';
import 'common.dart';

const kRows = [
  ('0.8 s', 'warm rebuild — one rustc invocation', 'src/compile.rs'),
  ('p95 11.4 ms', 'frame times against a 16.6 ms budget', 'VIEWW-PHASE-STATUS.md'),
  ('27.5 ms', 'startup, 2-core container', 'TRACKER.md'),
  ('7.6 MB', 'stripped release binary', 'TRACKER.md'),
  ('1.73×', 'full repaint — 7.0 B -> 4.0 B instructions', 'TRACKER.md'),
  ('462×', 'overstatement a full repaint makes of one keystroke', 'measure.sh'),
  ('4,225', 'tests, one pass, zero failures', 'TRACKER.md'),
];

Widget build(SceneCtx ctx) {
  final s = ctx.sec;
  final title = easeOutCubic(window(s, 0.4, 1.0));
  final rowStarts = staggerStarts(kRows.length, 1.75, offset: 1.2);
  final footnote = easeOutCubic(window(s, 13.0, 1.4));
  final endFade = easeOutCubic(window(ctx.seconds - s, 0.5, 0.5));

  return sceneFrame([
    Positioned(
      left: 260,
      top: 130,
      child: Opacity(
        opacity: title * endFade,
        child: const Voice('the ledger.', 40, t.kInk, weight: FontWeight.w700),
      ),
    ),
    // The table.
    Positioned(
      left: 260,
      top: 240,
      child: Opacity(
        opacity: endFade,
        child: SizedBox(
          width: 1400,
          child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
            // Header rule.
            Container(
              height: 1,
              color: t.kLine,
              margin: const EdgeInsets.only(bottom: 4),
            ),
            for (var i = 0; i < kRows.length; i++)
              _LedgerRow(
                number: kRows[i].$1,
                what: kRows[i].$2,
                where: kRows[i].$3,
                p: easeOutCubic(clamp01((s - rowStarts[i]) / 1.1)),
              ),
            Container(height: 1, color: t.kLine),
          ]),
        ),
      ),
    ),
    Positioned(
      left: 260,
      top: 940,
      child: Opacity(
        opacity: footnote * endFade,
        child: const Mono('measured on a 2-core container, cpu renderer. every number above has a receipt in-tree.', 14, t.kFilmFaint),
      ),
    ),
  ]);
}

class _LedgerRow extends StatelessWidget {
  final String number;
  final String what;
  final String where;
  final double p;
  const _LedgerRow({required this.number, required this.what, required this.where, required this.p});

  @override
  Widget build(BuildContext context) {
    final landed = VSpring(SpringSpec(320.0, 0.85)).position(p).clamp(0.0, 1.0);
    if (p <= 0.001) return const SizedBox(height: 86);
    return Opacity(
      opacity: landed,
      child: Transform.translate(
        offset: Offset((1 - landed) * 36.0, 0),
        child: SizedBox(
          height: 86,
          child: Row(crossAxisAlignment: CrossAxisAlignment.center, children: [
            SizedBox(
              width: 300,
              child: Mono(number, 42, t.kInk, weight: FontWeight.w500, letterSpacing: 0.5),
            ),
            Expanded(child: Mono(what, 16, t.kOnSurfaceVariant)),
            SizedBox(
              width: 300,
              child: Align(alignment: Alignment.centerRight, child: Mono(where, 12, t.kReceipt)),
            ),
          ]),
        ),
      ),
    );
  }
}
