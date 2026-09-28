/// S15 · under the hood — the studio shrinks to a card and the stack it
/// stands on assembles beneath it, one enforced layer at a time.
///
/// "widget -> element -> render -> scene -> render graph -> render
/// planner -> CPU | GPU" — vieww_base/README.md. And the dogfood line the
/// repo earns: the studio is built with the framework it ships, and so is
/// its website — one canvas, no divs.
library;

import 'package:flutter/widgets.dart';

import '../../film.dart';
import '../../motion/vieww_motion.dart';
import '../../theme.dart' as t;
import '../../widgets/studio/editor.dart';
import '../../widgets/studio/preview_screens.dart';
import '../../widgets/studio/studio_shell.dart';
import '../../widgets/primitives.dart';
import 'common.dart';

const kStackLayers = [
  'widget — what you wrote',
  'element — what persists',
  'render — what measured',
  'scene — what to draw',
  'render graph — the plan',
  'render planner — cpu · gpu · hybrid',
];

Widget build(SceneCtx ctx) {
  final s = ctx.sec;
  final shrink = easeInOutQuad(clamp01(s / 1.6));
  final layerStarts = staggerStarts(kStackLayers.length, 0.42, offset: 1.2);
  final word = easeOutCubic(window(s, 4.6, 1.2));
  final dogfood = easeOutCubic(window(s, 6.6, 1.4));
  final endFade = easeOutCubic(window(ctx.seconds - s, 0.5, 0.5));

  final studio = StudioChrome(
    time: ctx.abs,
    buffer: kCounterSay,
    typed: 1.0,
    caretLine: 7,
    screen: const CounterScreen(count: 1),
    compile: Compile.done,
  );

  return sceneFrame([
    // The studio, shrinking to a card at the top of the frame.
    Positioned(
      left: 0,
      top: 0,
      child: Transform.scale(
        scale: 0.42 * shrink + 1.0 * (1 - shrink),
        alignment: Alignment.topCenter,
        child: SizedBox(
          width: 1920,
          height: 1080,
          child: Opacity(opacity: 0.9, child: studio),
        ),
      ),
    ),
    // The stack, assembling beneath.
    Positioned(
      left: 560,
      top: 470,
      child: Opacity(
        opacity: endFade,
        child: SizedBox(
          width: 800,
          child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
            for (var i = 0; i < kStackLayers.length; i++)
              _LayerBar(
                label: kStackLayers[i],
                index: i,
                progress: easeOutCubic(clamp01((s - layerStarts[i]) / 0.7)),
              ),
          ]),
        ),
      ),
    ),
    // The connector hairlines from the studio card down through the layers.
    Positioned.fill(
      child: IgnorePointer(child: CustomPaint(painter: _Connectors(shrink: shrink, layerP: List.generate(kStackLayers.length, (i) => easeOutCubic(clamp01((s - layerStarts[i]) / 0.7)))), size: const Size(W, H))),
    ),
    // The word.
    Positioned(
      left: 1400,
      top: 500,
      child: Opacity(
        opacity: word * endFade,
        child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
          const Voice('vieww', 52, t.kInk, weight: FontWeight.w700, letterSpacing: 1.0),
          const SizedBox(height: 8),
          const Mono('the engine the studio stands on', 15, t.kFilmMuted),
        ]),
      ),
    ),
    // The dogfood line.
    Positioned(
      left: 0,
      right: 0,
      top: 966,
      child: Opacity(
        opacity: dogfood * endFade,
        child: const Center(
          child: Mono('the studio is built with the framework it ships. so is its website — one canvas, no divs.', 15, t.kFilmFaint),
        ),
      ),
    ),
    caption(ctx, 2.2, 'one engine. every layer enforced, not conventional.', end: 9.0),
    receipt(ctx, 5.4, 'architecture · docs/architecture/README.md'),
  ]);
}

class _LayerBar extends StatelessWidget {
  final String label;
  final int index;
  final double progress;
  const _LayerBar({required this.label, required this.index, required this.progress});

  @override
  Widget build(BuildContext context) {
    if (progress <= 0.001) return const SizedBox(height: 6);
    return Padding(
      padding: const EdgeInsets.only(bottom: 6),
      child: Opacity(
        opacity: progress,
        child: Transform.translate(
          offset: Offset((1 - progress) * 30.0, 0),
          // Two visible border colours (top rim + left accent) are illegal
          // under a borderRadius in Flutter, so the card keeps only the rim
          // and the accent edge is a strip pinned inside the clip.
          child: ClipRRect(
            borderRadius: BorderRadius.circular(8),
            child: Stack(children: [
              Container(
                height: 44,
                decoration: BoxDecoration(
                  gradient: LinearGradient(
                    begin: Alignment.topCenter,
                    end: Alignment.bottomCenter,
                    colors: [t.colorOver(const Color(0x09FFFFFF), t.kChrome1), t.kChrome1],
                  ),
                  border: Border(top: BorderSide(color: t.rim(t.kChrome1))),
                ),
                padding: const EdgeInsets.symmetric(horizontal: 16),
                child: Row(children: [
                  Mono(label, 13.5, t.kOnSurfaceVariant),
                  const Spacer(),
                  if (index == 5)
                    Mono('the cheap plan wins', 11, t.kReceipt)
                  else if (index == 1)
                    Mono('identity across rebuilds', 11, t.kReceipt),
                ]),
              ),
              Positioned(
                left: 0,
                top: 0,
                bottom: 0,
                child: ColoredBox(
                  color: t.kAccent.withValues(alpha: 0.25 + 0.1 * index / 6),
                  child: const SizedBox(width: 2),
                ),
              ),
            ]),
          ),
        ),
      ),
    );
  }
}

class _Connectors extends CustomPainter {
  final double shrink;
  final List<double> layerP;
  _Connectors({required this.shrink, required this.layerP});

  @override
  void paint(Canvas canvas, Size size) {
    if (shrink < 0.98) return;
    final paint = Paint()
      ..color = t.kAccent.withValues(alpha: 0.35)
      ..strokeWidth = 1.4;
    final x1 = size.width / 2 - 330;
    final x2 = size.width / 2 - 240;
    for (var i = 0; i < layerP.length; i++) {
      if (layerP[i] <= 0.01) continue;
      final y = 472 + i * 50.0;
      canvas.drawLine(Offset(x1, 440), Offset(x2, y + 22), paint);
    }
  }

  @override
  bool shouldRepaint(_Connectors old) => true;
}
