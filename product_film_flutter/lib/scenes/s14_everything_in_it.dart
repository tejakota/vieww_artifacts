/// S14 · everything in it — the montage: palette, inspector, tokens, and
/// the count of everything the studio carries.
library;

import 'package:flutter/widgets.dart';

import '../../film.dart';
import '../../motion/vieww_motion.dart';
import '../../theme.dart' as t;
import '../../widgets/studio/editor.dart';
import '../../widgets/studio/preview_screens.dart';
import '../../widgets/studio/studio_shell.dart';
import 'common.dart';

Widget build(SceneCtx ctx) {
  final s = ctx.sec;

  // Beat 1 (0-3.6): command palette types and filters.
  final paletteOpen = easeOutCubic(window(s, 0.4, 0.5)) * (1.0 - easeOutCubic(window(s, 3.2, 0.5)));
  final paletteQuery = s < 1.4 ? '' : (s < 2.2 ? 'swi' : 'plat');

  // Beat 2 (3.6-6.8): the inspector.
  final inspector = s >= 3.4 && s < 6.6;
  final rightTab = inspector ? RightTab.inspector : RightTab.preview;

  // Beat 3 (6.6-9.6): the tokens view — accent cycling through all eight.
  final tokensBeat = s >= 6.6;
  final accentIdx = ((s - 6.6) / 0.38).floor() % 8;
  final accent = tokensBeat ? t.kAccents[accentIdx] : null;

  final studio = StudioChrome(
    time: ctx.abs,
    buffer: kCounterSay,
    typed: 1.0,
    caretLine: 7,
    rightTab: rightTab,
    screen: const CounterScreen(count: 1),
    deviceMount: inspector ? 0.0 : 1.0,
    accentOverride: accent,
    overlay: CommandPalette(query: paletteQuery, open: paletteOpen),
  );

  // The chips — what the studio carries, staggered in at the end.
  final chipStarts = staggerStarts(4, 0.5, offset: 9.6);

  return Stack(children: [
    Positioned.fill(
      child: SizedBox(width: 1920, height: 1080, child: studio),
    ),
    Positioned(
      left: 96,
      bottom: 150,
      child: Row(children: [
        _ChipIn(label: '107 commands', start: chipStarts[0], s: s),
        const SizedBox(width: 12),
        _ChipIn(label: '12 lessons', start: chipStarts[1], s: s),
        const SizedBox(width: 12),
        _ChipIn(label: '19 snippets', start: chipStarts[2], s: s),
        const SizedBox(width: 12),
        _ChipIn(label: '8 accents', start: chipStarts[3], s: s),
      ]),
    ),
    caption(ctx, 0.8, 'everything an ide owes you. nothing it doesn\'t.', end: 11.4, scrim: true),
  ]);
}

class _ChipIn extends StatelessWidget {
  final String label;
  final double start;
  final double s;
  const _ChipIn({required this.label, required this.start, required this.s});

  @override
  Widget build(BuildContext context) {
    final p = VSpring(SpringSpec.expressive).position(s - start).clamp(0.0, 1.0);
    if (p <= 0.001) return const SizedBox.shrink();
    return Opacity(
      opacity: p,
      child: Transform.scale(scale: 0.85 + 0.15 * p, child: Chip(label)),
    );
  }
}
