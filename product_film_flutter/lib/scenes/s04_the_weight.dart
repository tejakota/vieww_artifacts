/// S04 · the weight — a hello world that weighed a quarter gigabyte.
///
/// The receipt: "A hello-world weighed 1.3 GB, and 90% of the binary was
/// other people's symbols" (TRACKER.md, scaffold measurements: 260.5 MB
/// debug, 7.6 MB release). The pile arrives in 24 Hz judder — the old
/// world's frame rate.
library;

import 'package:flutter/widgets.dart';

import '../../film.dart';
import '../../motion/vieww_motion.dart';
import '../../theme.dart' as t;
import '../../widgets/primitives.dart';
import 'common.dart';

Widget build(SceneCtx ctx) {
  final title = typedBy(ctx, 0.8, 2.6, 'a hello world weighed a quarter gigabyte.');
  final barP = easeOutCubic(window(ctx.sec, 2.4, 4.2));
  final bracketP = easeOutCubic(window(ctx.sec, 7.6, 1.2));
  final dustP = easeOutCubic(window(ctx.sec, 10.6, 1.8));
  final capP = easeOutCubic(window(ctx.sec, 11.4, 1.2));
  final endFade = easeOutCubic(window(ctx.seconds - ctx.sec, 0.5, 0.5));

  const totalMb = 260.5;

  return sceneFrame([
    Positioned(
      left: 0,
      right: 0,
      top: 170,
      child: Opacity(
        opacity: endFade,
        child: Center(child: TypeLine(text: 'a hello world weighed a quarter gigabyte.', revealed: title, size: 28, color: t.kInk, mono: true)),
      ),
    ),
    // The binary, piling up under a tiny hello world card.
    Positioned(
      left: W / 2 - 360,
      top: 300,
      child: Opacity(
        opacity: (1.0 - dustP) * endFade,
        child: SizedBox(
          width: 720,
          child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
            Pane(
              color: t.kChrome2,
              elevation: 1,
              padding: const EdgeInsets.all(14),
              child: Center(child: Mono('hello, world', 14, t.kOnSurfaceVariant)),
            ),
            const SizedBox(height: 18),
            _BinaryPile(progress: barP, time: held24In60(ctx.abs)),
            const SizedBox(height: 14),
            Row(crossAxisAlignment: CrossAxisAlignment.end, children: [
              CountUp(progress: barP, value: totalMb, decimals: 1, suffix: ' MB', size: 40, color: t.kInk),
              const SizedBox(width: 18),
              Padding(
                padding: const EdgeInsets.only(bottom: 8),
                child: Opacity(
                  opacity: bracketP,
                  child: const Mono('debug build · 90% of it someone else\'s symbols', 14, t.kError),
                ),
              ),
            ]),
          ]),
        ),
      ),
    ),
    // The dust the borrowed weight becomes.
    if (dustP > 0.001)
      Positioned.fill(
        child: Opacity(
          opacity: endFade,
          child: _Dust(progress: dustP, time: ctx.abs),
        ),
      ),
    caption(ctx, 11.4, 'the weight was borrowed. borrowed weight stays.'),
    receipt(ctx, 8.8, 'TRACKER.md · scaffolded hello-world, debug'),
  ]);
}

/// The pile: segments stacking in, most of them not yours.
class _BinaryPile extends StatelessWidget {
  final double progress;
  final double time;
  const _BinaryPile({required this.progress, required this.time});

  @override
  Widget build(BuildContext context) {
    const rows = 14;
    return SizedBox(
      height: rows * 15.0 + 4,
      child: CustomPaint(painter: _PilePainter(progress, time), size: const Size(720, rows * 15.0 + 4)),
    );
  }
}

class _PilePainter extends CustomPainter {
  final double progress;
  final double time;
  _PilePainter(this.progress, this.time);

  @override
  void paint(Canvas canvas, Size size) {
    final rng = Rng(0xB1A4A7);
    const rows = 14;
    final shown = (progress * rows).floor();
    for (var i = 0; i < shown; i++) {
      final y = size.height - (i + 1) * 15.0;
      // Widths jitter deterministically; ~10% of the mass is "yours".
      final yours = rng.next01() < 0.1;
      final w = rng.range(0.55, 1.0) * size.width;
      final x = rng.range(0, size.width - w);
      final paint = Paint()
        ..color = yours
            ? t.kOnSurfaceVariant.withValues(alpha: 0.55)
            : t.kOutline.withValues(alpha: 0.30);
      final r = Rect.fromLTWH(x, y, w, 11.5);
      canvas.drawRRect(RRect.fromRectAndRadius(r, const Radius.circular(2.5)), paint);
    }
    // The topmost segment still arriving — drawn at the judder clock.
    if (shown < rows && progress > 0.02) {
      final y = size.height - (shown + 1) * 15.0;
      final w = (progress * rows - shown) * size.width * 0.8;
      final paint = Paint()..color = t.kError.withValues(alpha: 0.65);
      canvas.drawRRect(RRect.fromRectAndRadius(Rect.fromLTWH(0, y, w, 11.5), const Radius.circular(2.5)), paint);
    }
  }

  @override
  bool shouldRepaint(_PilePainter old) => old.progress != progress;
}

/// The collapse: the pile falls apart into slow dust.
class _Dust extends StatelessWidget {
  final double progress;
  final double time;
  const _Dust({required this.progress, required this.time});

  @override
  Widget build(BuildContext context) => CustomPaint(painter: _DustPainter(progress, time), size: const Size(W, H));
}

class _DustPainter extends CustomPainter {
  final double progress;
  final double time;
  _DustPainter(this.progress, this.time);

  @override
  void paint(Canvas canvas, Size size) {
    final rng = Rng(0xD57E);
    final p = progress.clamp(0.0, 1.0);
    final cx = size.width / 2;
    for (var i = 0; i < 260; i++) {
      final ox = rng.range(-330, 330);
      final oy = rng.range(300, 760);
      final fall = p * rng.range(120, 420);
      final x = cx + ox * (1 + p * 0.3);
      final y = oy + fall;
      final a = (1.0 - p) * rng.range(0.12, 0.4);
      final paint = Paint()..color = t.kOnSurfaceVariant.withValues(alpha: a.clamp(0.0, 1.0));
      canvas.drawCircle(Offset(x, y), rng.range(0.8, 2.2), paint);
    }
  }

  @override
  bool shouldRepaint(_DustPainter old) => old.progress != progress;
}
