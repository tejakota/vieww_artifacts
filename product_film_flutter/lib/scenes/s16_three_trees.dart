/// S16 · three trees — widget, element, render. A rebuild wave passes and
/// the middle column holds: the element persists, so an animation that was
/// running keeps running and a caret keeps its place.
///
/// "the reason a vieww app doesn't need a virtual-DOM diff pass to figure
/// out what to redraw" — docs/architecture/ui-tree.md.
library;

import 'dart:math' as math;

import 'package:flutter/widgets.dart';

import '../../film.dart';
import '../../motion/vieww_motion.dart';
import '../../theme.dart' as t;
import '../../widgets/primitives.dart';
import 'common.dart';

Widget build(SceneCtx ctx) {
  final s = ctx.sec;
  final colsIn = staggerStarts(3, 0.4, offset: 0.5).map((st) => easeOutCubic(clamp01((s - st) / 0.8))).toList();
  // The rebuild wave, passing left to right at ~3.6 s.
  final wave = clamp01((s - 3.6) / 2.4);
  final cap1 = easeOutCubic(window(s, 1.6, 1.2));
  final cap2 = easeOutCubic(window(s, 7.6, 1.2));
  final endFade = easeOutCubic(window(ctx.seconds - s, 0.5, 0.5));

  return sceneFrame([
    Positioned(
      left: 0,
      right: 0,
      top: 170,
      child: Opacity(
        opacity: endFade,
        child: Row(mainAxisAlignment: MainAxisAlignment.center, children: [
          _ColumnHead(label: 'widget', sub: 'what you wrote', p: colsIn[0]),
          const SizedBox(width: 110),
          _ColumnHead(label: 'element', sub: 'what persists', p: colsIn[1]),
          const SizedBox(width: 110),
          _ColumnHead(label: 'render', sub: 'what painted', p: colsIn[2]),
        ]),
      ),
    ),
    // The three columns.
    Positioned(
      left: 0,
      right: 0,
      top: 280,
      child: Opacity(
        opacity: endFade,
        child: Row(mainAxisAlignment: MainAxisAlignment.center, crossAxisAlignment: CrossAxisAlignment.start, children: [
          _TreeColumn(kind: 0, wave: wave, time: ctx.abs, seed: 11),
          const SizedBox(width: 110),
          _TreeColumn(kind: 1, wave: wave, time: ctx.abs, seed: 22),
          const SizedBox(width: 110),
          _TreeColumn(kind: 2, wave: wave, time: ctx.abs, seed: 33),
        ]),
      ),
    ),
    // The proof inset: an animation that never stops, through the rebuild.
    Positioned(
      left: 1450,
      top: 780,
      child: Opacity(
        opacity: endFade,
        child: Pane(
          color: t.kChrome1,
          elevation: 2,
          padding: const EdgeInsets.all(16),
          child: SizedBox(
            // Wide enough for the proof line at its longest — a Row overflow
            // here would paint the yellow-striped error pattern into the film.
            width: 340,
            child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
              const Mono('meanwhile, in the corner', 11, t.kGutter),
              const SizedBox(height: 12),
              Row(children: [
                _WaveDot(time: ctx.abs),
                const SizedBox(width: 14),
                const Mono('this animation survived 3 rebuilds', 12.5, t.kOnSurfaceVariant),
              ]),
              const SizedBox(height: 10),
              const Mono('so did the caret. so did the scroll.', 12, t.kFilmFaint),
            ]),
          ),
        ),
      ),
    ),
    caption(ctx, 1.6, 'three trees. one job each.', end: 6.8),
    caption(ctx, 7.6, 'the element persists. the animation survives the rebuild.', end: 12.2),
    receipt(ctx, 4.4, 'ui-tree.md · element identity across rebuilds'),
  ]);
}

class _ColumnHead extends StatelessWidget {
  final String label;
  final String sub;
  final double p;
  const _ColumnHead({required this.label, required this.sub, required this.p});

  @override
  Widget build(BuildContext context) {
    return Opacity(
      opacity: p,
      child: Transform.translate(
        offset: Offset(0, (1 - p) * 10),
        child: Column(children: [
          Mono(label, 22, t.kInk, weight: FontWeight.w500),
          const SizedBox(height: 6),
          Mono(sub, 14, t.kFilmMuted),
        ]),
      ),
    );
  }
}

/// One column of six cards. `kind` 0 reshuffles on the wave, 1 holds with a
/// steady identity glow, 2 flashes only the cards that actually repainted.
class _TreeColumn extends StatelessWidget {
  final int kind;
  final double wave;
  final double time;
  final int seed;
  const _TreeColumn({required this.kind, required this.wave, required this.time, required this.seed});

  @override
  Widget build(BuildContext context) {
    const n = 6;
    final children = <Widget>[];
    for (var i = 0; i < n; i++) {
      // Widget tree: cards reorder as the wave passes.
      var order = i;
      if (kind == 0) {
        order = (i + (wave * 2.2).floor()) % n;
      }
      final flash = kind == 2 && (i == 2 || i == 3) && wave > 0.3 && wave < 0.95;
      final held = kind == 1;
      children.add(Padding(
        padding: const EdgeInsets.only(bottom: 10),
        child: _NodeCard(
          depth: (order * 0.7).floor() % 3,
          kind: kind,
          flash: flash,
          held: held,
          holdGlow: held ? 0.5 + 0.5 * math.sin(time * 2.0 + i) : 0.0,
        ),
      ));
    }
    return SizedBox(width: 330, child: Column(children: children));
  }
}

class _NodeCard extends StatelessWidget {
  final int depth;
  final int kind;
  final bool flash;
  final bool held;
  final double holdGlow;
  const _NodeCard({required this.depth, required this.kind, required this.flash, required this.held, required this.holdGlow});

  @override
  Widget build(BuildContext context) {
    final labels = ['Column', 'Heading', 'Label', 'Button', 'Gesture', 'SafeArea'];
    final color = flash ? t.colorOver(const Color(0x3F3FB950), t.kChrome2) : t.kChrome2;
    return Padding(
      padding: EdgeInsets.only(left: depth * 22.0),
      // Top rim + conditional left accent are two visible border colours,
      // illegal under a borderRadius in Flutter: the card keeps only the rim
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
                colors: [t.colorOver(const Color(0x09FFFFFF), color), color],
              ),
              border: Border(top: BorderSide(color: t.rim(color))),
            ),
            padding: const EdgeInsets.symmetric(horizontal: 14),
            child: Row(children: [
              Container(
                width: 6,
                height: 6,
                decoration: BoxDecoration(
                  color: held ? t.kAccent : (kind == 2 ? t.kSuccess.withValues(alpha: flash ? 1.0 : 0.4) : t.kGutter),
                  shape: BoxShape.circle,
                ),
              ),
              const SizedBox(width: 10),
              Mono(labels[(depth + kind * 2) % labels.length], 12.5, t.kOnSurfaceVariant),
              const Spacer(),
              if (held) Mono('id 0x${(kind * 31 + depth * 7).toRadixString(16)}', 10, t.kReceipt),
            ]),
          ),
          if (held)
            Positioned(
              left: 0,
              top: 0,
              bottom: 0,
              child: ColoredBox(
                color: t.kAccent.withValues(alpha: 0.35 + 0.3 * holdGlow),
                child: const SizedBox(width: 2),
              ),
            ),
        ]),
      ),
    );
  }
}

/// The dot whose animation never stops — the whole point of the scene.
class _WaveDot extends StatelessWidget {
  final double time;
  const _WaveDot({required this.time});

  @override
  Widget build(BuildContext context) {
    return SizedBox(
      width: 26,
      height: 26,
      child: CustomPaint(painter: _WaveDotPainter(time)),
    );
  }
}

class _WaveDotPainter extends CustomPainter {
  final double time;
  _WaveDotPainter(this.time);

  @override
  void paint(Canvas canvas, Size size) {
    final c = Offset(size.width / 2, size.height / 2);
    // The orbiting satellite — running since before the wave and after it.
    final angle = time * 2.6;
    final orbit = Paint()..color = t.kAccent;
    canvas.drawCircle(c, 7, Paint()..color = t.kAccentFar);
    canvas.drawCircle(Offset(c.dx + 10 * math.cos(angle), c.dy + 10 * math.sin(angle)), 3.2, orbit);
  }

  @override
  bool shouldRepaint(_WaveDotPainter old) => true;
}
