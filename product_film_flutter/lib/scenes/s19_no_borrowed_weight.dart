/// S19 · no borrowed weight — the stack it owns, end to end.
///
/// "There is no wgpu in this workspace and that is a deliberate choice
/// rather than an omission: the render graph, the damage tracking, the
/// CPU/GPU placement decision and the frame scheduling are the things vieww
/// exists to own." — README.md. No serde either. C-ABI plugins any rustc.
library;

import 'package:flutter/widgets.dart';

import '../../film.dart';
import '../../motion/vieww_motion.dart';
import '../../theme.dart' as t;
import '../../widgets/primitives.dart';
import 'common.dart';

Widget build(SceneCtx ctx) {
  final s = ctx.sec;
  final stackIn = easeOutCubic(window(s, 0.4, 1.0));
  final notes = staggerStarts(3, 1.5, offset: 1.4);
  final counts = staggerStarts(3, 0.9, offset: 6.2);
  final cap = easeOutCubic(window(s, 2.2, 1.2));
  final endFade = easeOutCubic(window(ctx.seconds - s, 0.5, 0.5));

  const rows = [
    ('no wgpu', 'its own gpu stack — vulkan first, metal and d3d12 behind the same traits'),
    ('no serde', 'hand-rolled where that is the smaller surface, and said so in the doc comment'),
    ('C-ABI plugins', 'a cdylib from any rustc, loaded at any time — repr(C), not a trait object'),
  ];

  return sceneFrame([
    // The stack, small, on the left.
    Positioned(
      left: 200,
      top: 260,
      child: Opacity(
        opacity: stackIn * endFade,
        child: SizedBox(
          width: 460,
          child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
            for (var i = 0; i < 6; i++)
              Padding(
                padding: const EdgeInsets.only(bottom: 5),
                child: Container(
                  height: 34,
                  decoration: BoxDecoration(
                    color: t.kChrome1,
                    borderRadius: BorderRadius.circular(7),
                    border: Border(left: BorderSide(color: t.kAccent.withValues(alpha: 0.2 + 0.05 * i), width: 2)),
                  ),
                  padding: const EdgeInsets.symmetric(horizontal: 14),
                  child: Align(
                    alignment: Alignment.centerLeft,
                    child: Mono(['widget', 'element', 'render', 'scene', 'render graph', 'planner'][i], 12, t.kOnSurfaceVariant),
                  ),
                ),
              ),
            const SizedBox(height: 8),
            Row(children: [
              Container(
                height: 34,
                width: 224,
                alignment: Alignment.center,
                decoration: BoxDecoration(color: t.kChrome3, borderRadius: BorderRadius.circular(7)),
                child: const Mono('cpu', 12, t.kOnSurface, weight: FontWeight.w500),
              ),
              const SizedBox(width: 12),
              Container(
                height: 34,
                width: 224,
                alignment: Alignment.center,
                decoration: BoxDecoration(color: t.kChrome3, borderRadius: BorderRadius.circular(7)),
                child: const Mono('gpu', 12, t.kOnSurface, weight: FontWeight.w500),
              ),
            ]),
          ]),
        ),
      ),
    ),
    // The three notes, on the right.
    Positioned(
      left: 800,
      top: 250,
      child: Opacity(
        opacity: endFade,
        child: SizedBox(
          width: 880,
          child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
            for (var i = 0; i < rows.length; i++) ...[
              _Note(title: rows[i].$1, sub: rows[i].$2, p: easeOutCubic(clamp01((s - notes[i]) / 1.0))),
              const SizedBox(height: 26),
            ],
          ]),
        ),
      ),
    ),
    // The scale figures.
    Positioned(
      left: 800,
      top: 800,
      child: Opacity(
        opacity: endFade,
        child: Row(children: [
          _Count(p: easeOutCubic(clamp01((s - counts[0]) / 0.9)), value: 36, label: 'crates'),
          const SizedBox(width: 60),
          _Count(p: easeOutCubic(clamp01((s - counts[1]) / 0.9)), value: 391679, label: 'lines of rust', format: true),
          const SizedBox(width: 60),
          _Count(p: easeOutCubic(clamp01((s - counts[2]) / 0.9)), value: 56, label: 'feature examples'),
        ]),
      ),
    ),
    caption(ctx, 2.2, 'the parts that matter, owned end to end.', end: 9.2),
    receipt(ctx, 5.0, 'README.md · PENDING.md non-goals · workspace count'),
  ]);
}

class _Note extends StatelessWidget {
  final String title;
  final String sub;
  final double p;
  const _Note({required this.title, required this.sub, required this.p});

  @override
  Widget build(BuildContext context) {
    if (p <= 0.001) return const SizedBox(height: 80);
    return Opacity(
      opacity: p,
      child: Transform.translate(
        offset: Offset((1 - p) * 26.0, 0),
        child: Row(crossAxisAlignment: CrossAxisAlignment.start, children: [
          Container(
            margin: const EdgeInsets.only(top: 6),
            width: 10,
            height: 10,
            decoration: const BoxDecoration(color: t.kAccent, shape: BoxShape.circle),
          ),
          const SizedBox(width: 18),
          Expanded(
            child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
              Mono(title, 24, t.kInk, weight: FontWeight.w500),
              const SizedBox(height: 6),
              Mono(sub, 14, t.kFilmMuted),
            ]),
          ),
        ]),
      ),
    );
  }
}

class _Count extends StatelessWidget {
  final double p;
  final int value;
  final String label;
  final bool format;
  const _Count({required this.p, required this.value, required this.label, this.format = false});

  @override
  Widget build(BuildContext context) {
    final v = (value * p).round();
    final shown = format ? v.toString().replaceAllMapped(RegExp(r'(\d)(?=(\d{3})+$)'), (m) => '${m[1]},') : v.toString();
    return Opacity(
      opacity: p,
      child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
        Mono(shown, 36, t.kOnSurface, weight: FontWeight.w500),
        const SizedBox(height: 4),
        Mono(label, 13, t.kFilmFaint),
      ]),
    );
  }
}
