/// S05 · the waste — one keystroke, and the whole screen pays for it.
///
/// The receipts: a keystroke rasterises an 86 x 23 px region; a full-surface
/// repaint overstates that by 462x (measure.sh, damaged-region raster).
/// The scanline arrives at the old world's 24 Hz. The magnifier is the
/// moment the number becomes visible.
library;

import 'dart:math' as math;

import 'package:flutter/widgets.dart';

import '../../film.dart';
import '../../motion/vieww_motion.dart';
import '../../theme.dart' as t;
import '../../widgets/primitives.dart';
import 'common.dart';

Widget build(SceneCtx ctx) {
  // Beat 1: a keystroke lands in a small editor card.
  final keystroke = VSpring(SpringSpec.standard).position(ctx.sec - 1.2);
  // Beat 2: the full-screen repaint — a flash and a scanline, 24 Hz judder.
  final flash = window(ctx.sec, 2.2, 0.14);
  final sweep = easeInOutQuad(clamp01((ctx.sec - 2.2) / 1.2));
  // Beat 3: the magnifier glides in and shows what actually changed.
  final lens = VSpring(SpringSpec.expressive).position(ctx.sec - 4.2);
  final leftNum = easeOutCubic(window(ctx.sec, 5.4, 1.0));
  final rightNum = VSpring(SpringSpec(360.0, 0.72)).position(ctx.sec - 6.4);
  final shake = rightNum < 1.0 ? 0.0 : 0.0;
  final cap = easeOutCubic(window(ctx.sec, 9.4, 1.2));
  final endFade = easeOutCubic(window(ctx.seconds - ctx.sec, 0.5, 0.5));

  const caretX = 870.0; // where the keystroke lands, in frame coords
  const caretY = 520.0;

  return sceneFrame([
    // The editor card.
    Positioned(
      left: 640,
      top: 420,
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
              Row(crossAxisAlignment: CrossAxisAlignment.center, children: [
                const Mono('render(', 16, t.kSyntaxFunction),
                Transform.scale(
                  scale: 0.2 + 0.8 * keystroke,
                  child: Mono('1', 16, t.kSyntaxNumber),
                ),
                const Mono(');', 16, t.kOnSurface),
                if (keystroke > 0.99) Caret(time: ctx.abs, height: 20),
              ]),
            ]),
          ),
        ),
      ),
    ),
    // The full-surface repaint: one flash, then the sweeping line.
    if (flash > 0.001)
      Positioned.fill(
        child: IgnorePointer(child: ColoredBox(color: Color.fromRGBO(255, 255, 255, (flash * 0.07).clamp(0.0, 1.0)))),
      ),
    if (sweep > 0.001 && sweep < 1.0)
      Positioned.fill(
        child: IgnorePointer(
          child: _Scanline(progress: held24In60(sweep)),
        ),
      ),
    // The magnifier: what a keystroke actually costs.
    Positioned(
      left: caretX - 130 + (1 - lens) * 60,
      top: caretY - 150 + (1 - lens) * 40,
      child: Opacity(
        opacity: lens.clamp(0.0, 1.0) * endFade,
        child: _Magnifier(time: ctx.abs),
      ),
    ),
    // The two numbers.
    Positioned(
      left: 340,
      top: 760,
      child: Opacity(
        opacity: leftNum * endFade,
        child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
          const Mono('86 × 23 px', 40, t.kSuccess, weight: FontWeight.w500),
          const SizedBox(height: 6),
          const Mono('what the keystroke actually changed', 15, t.kFilmMuted),
        ]),
      ),
    ),
    Positioned(
      left: 1080,
      top: 748,
      child: Opacity(
        opacity: rightNum.clamp(0.0, 1.0) * endFade,
        child: Transform.translate(
          offset: Offset(shake, 0),
          child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
            const Mono('462×', 54, t.kError, weight: FontWeight.w500),
            const SizedBox(height: 6),
            const Mono('what a full-surface repaint claims it cost', 15, t.kFilmMuted),
          ]),
        ),
      ),
    ),
    caption(ctx, 9.4, 'a change should cost the change. not the screen.'),
    receipt(ctx, 8.2, 'measure.sh · damaged-region raster'),
  ]);
}

/// The scanline — the whole surface repainting top to bottom.
class _Scanline extends StatelessWidget {
  final double progress;
  const _Scanline({required this.progress});

  @override
  Widget build(BuildContext context) => CustomPaint(painter: _ScanPainter(progress), size: const Size(W, H));
}

class _ScanPainter extends CustomPainter {
  final double p;
  _ScanPainter(this.p);

  @override
  void paint(Canvas canvas, Size size) {
    final y = p * size.height;
    final paint = Paint()
      ..shader = LinearGradient(
        begin: Alignment.topCenter,
        end: Alignment.bottomCenter,
        colors: [const Color(0x00F85149), const Color(0x26F85149), const Color(0x00F85149)],
      ).createShader(Rect.fromLTWH(0, y - 90, size.width, 180));
    canvas.drawRect(Rect.fromLTWH(0, y - 90, size.width, 180), paint);
    final line = Paint()
      ..color = const Color(0x66F85149)
      ..strokeWidth = 1.5;
    canvas.drawLine(Offset(0, y), Offset(size.width, y), line);
  }

  @override
  bool shouldRepaint(_ScanPainter old) => old.p != p;
}

/// The magnifier: a lens over the caret with the 86 x 23 region glowing.
class _Magnifier extends StatelessWidget {
  final double time;
  const _Magnifier({required this.time});

  @override
  Widget build(BuildContext context) {
    return SizedBox(
      width: 260,
      height: 300,
      child: CustomPaint(painter: _LensPainter(time)),
    );
  }
}

class _LensPainter extends CustomPainter {
  final double time;
  _LensPainter(this.time);

  @override
  void paint(Canvas canvas, Size size) {
    const cx = 130.0, cy = 130.0, r = 96.0;
    // The lens rim.
    final rim = Paint()
      ..color = t.kOnSurface.withValues(alpha: 0.5)
      ..style = PaintingStyle.stroke
      ..strokeWidth = 2.0;
    canvas.drawCircle(const Offset(cx, cy), r, rim);
    // The glass: a faint fill.
    canvas.drawCircle(const Offset(cx, cy), r, Paint()..color = const Color(0x0FFFFFFF));
    // The zoomed caret: a bold slab, slightly magnified.
    final caret = Paint()..color = t.kInk;
    canvas.drawRRect(RRect.fromRectAndRadius(const Rect.fromLTWH(cx - 4, cy - 44, 8, 88), const Radius.circular(2)), caret);
    // The 86 x 23 region, drawn to scale inside the lens, breathing accent.
    final breathe = 0.5 + 0.5 * math.sin(time * 3.0);
    final region = Paint()
      ..color = t.kAccent.withValues(alpha: 0.18 + 0.12 * breathe)
      ..style = PaintingStyle.fill;
    canvas.drawRRect(
      RRect.fromRectAndRadius(const Rect.fromLTWH(cx - 86, cy - 23, 172, 46), const Radius.circular(6)),
      region,
    );
    final regionRim = Paint()
      ..color = t.kAccentFar.withValues(alpha: 0.8)
      ..style = PaintingStyle.stroke
      ..strokeWidth = 1.5;
    canvas.drawRRect(
      RRect.fromRectAndRadius(const Rect.fromLTWH(cx - 86, cy - 23, 172, 46), const Radius.circular(6)),
      regionRim,
    );
    // The handle.
    final handle = Paint()
      ..color = t.kOnSurface.withValues(alpha: 0.4)
      ..strokeWidth = 5
      ..strokeCap = StrokeCap.round;
    canvas.drawLine(const Offset(cx + 68, cy + 68), const Offset(cx + 110, cy + 110), handle);
    // The label under the lens.
    final tp = TextPainter(
      text: const TextSpan(
        text: '86 × 23 px',
        style: TextStyle(fontFamily: 'GeistMono', fontSize: 13, color: t.kFilmMuted),
      ),
      textDirection: TextDirection.ltr,
    )..layout();
    tp.paint(canvas, Offset(cx - tp.width / 2, cy + r + 26));
  }

  @override
  bool shouldRepaint(_LensPainter old) => true;
}
