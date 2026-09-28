/// S07 · the promise — demos that look like they work, until they don't;
/// then the line the whole repository is built on.
///
/// The quote is verbatim: "An honest stub beats code that looks like it
/// works and doesn't." — vieww_base/README.md. The last beat of the old
/// world, and the hinge into the relief.
library;

import 'dart:math' as math;

import 'package:flutter/widgets.dart';

import '../../film.dart';
import '../../motion/vieww_motion.dart';
import '../../theme.dart' as t;
import '../../widgets/primitives.dart';
import 'common.dart';

Widget build(SceneCtx ctx) {
  final spin = ctx.sec * 0.35;
  final crack = easeInOutQuad(clamp01((ctx.sec - 3.6) / 2.4));
  final fade = easeOutCubic(window(ctx.sec, 6.2, 1.6));
  final cap = easeOutCubic(window(ctx.sec, 6.8, 1.2));
  final quoteP = easeOutCubic(window(ctx.sec, 9.8, 1.8));
  final quoteHold = easeOutCubic(window(ctx.seconds - ctx.sec, 0.6, 0.6));

  return sceneFrame([
    // The glossy demo card.
    Positioned(
      left: W / 2 - 210,
      top: 300,
      child: Opacity(
        opacity: (1.0 - fade) ,
        child: _DemoCard(spin: spin, crack: crack, judder: held24In60(ctx.abs)),
      ),
    ),
    // The hollow it leaves.
    if (fade > 0.001)
      Positioned(
        left: W / 2 - 210,
        top: 300,
        child: Opacity(
          opacity: fade * 0.5,
          child: _HollowCard(spin: spin),
        ),
      ),
    caption(ctx, 6.8, 'things that look like they work. until they don\'t.'),
    // The quote — the repo's own spine.
    Positioned(
      left: 0,
      right: 0,
      top: 640,
      child: Opacity(
        opacity: quoteP * quoteHold,
        child: Column(children: [
          Transform.translate(
            offset: Offset(0, (1 - quoteP) * 14.0),
            child: const Voice(
              'an honest stub beats code that looks like it works.',
              34,
              t.kInk,
              weight: FontWeight.w500,
            ),
          ),
          const SizedBox(height: 26),
          Opacity(
            opacity: easeOutCubic(window(ctx.sec, 11.2, 1.2)),
            child: const Voice('— vieww_base/README.md', 14, t.kReceipt),
          ),
        ]),
      ),
    ),
  ]);
}

/// The demo: a pretty card with a sheen it did not earn, rotating slowly,
/// cracking on the judder clock.
class _DemoCard extends StatelessWidget {
  final double spin;
  final double crack;
  final double judder;
  const _DemoCard({required this.spin, required this.crack, required this.judder});

  @override
  Widget build(BuildContext context) {
    return SizedBox(
      width: 420,
      height: 300,
      child: Transform.rotate(angle: math.sin(judder * 2.0) * 0.02 * crack + spin * 0.0, child: CustomPaint(painter: _DemoPainter(crack: crack, spin: spin))),
    );
  }
}

class _DemoPainter extends CustomPainter {
  final double crack;
  final double spin;
  _DemoPainter({required this.crack, required this.spin});

  @override
  void paint(Canvas canvas, Size size) {
    final rect = Rect.fromLTWH(0, 0, size.width, size.height);
    final rrect = RRect.fromRectAndRadius(rect, const Radius.circular(14));

    // The borrowed gloss: a blue-magenta sheen no product earned.
    final paint = Paint()
      ..shader = LinearGradient(
        begin: Alignment.topLeft,
        end: Alignment.bottomRight,
        colors: [
          const Color(0xFF2B3A67),
          Color.lerp(const Color(0xFF5B4B8A), const Color(0xFF394A6B), crack)!,
          const Color(0xFF151824),
        ],
      ).createShader(rect);
    canvas.drawRRect(rrect, paint);

    // The label.
    final tp = TextPainter(
      text: const TextSpan(text: 'demo', style: TextStyle(fontFamily: 'GeistMono', fontSize: 15, color: Color(0xB8C6D5EE))),
      textDirection: TextDirection.ltr,
    )..layout();
    tp.paint(canvas, Offset(24, 22));

    // The shine band, drifting — cheap gloss, and it knows it.
    final band = Paint()
      ..shader = LinearGradient(
        begin: Alignment.topLeft,
        end: Alignment.bottomRight,
        colors: [const Color(0x33FFFFFF), const Color(0x00FFFFFF)],
      ).createShader(rect);
    final bandRect = Rect.fromLTWH((spin * 60) % (size.width + 200) - 200, 0, 160, size.height);
    canvas.save();
    canvas.clipRRect(rrect);
    canvas.drawRect(bandRect, band);
    canvas.restore();

    // The cracks.
    if (crack > 0.001) {
      final crackPaint = Paint()
        ..color = t.kError.withValues(alpha: 0.75)
        ..strokeWidth = 1.6
        ..style = PaintingStyle.stroke;
      final path = Path()
        ..moveTo(size.width * 0.28, 0)
        ..lineTo(size.width * 0.34, size.height * 0.3)
        ..lineTo(size.width * 0.26, size.height * 0.52)
        ..lineTo(size.width * 0.36, size.height);
      final path2 = Path()
        ..moveTo(size.width * 0.7, 0)
        ..lineTo(size.width * 0.62, size.height * 0.4)
        ..lineTo(size.width * 0.72, size.height * 0.7);
      final metric = crack;
      canvas.drawPath(_partial(path, metric), crackPaint);
      canvas.drawPath(_partial(path2, metric), crackPaint);
    }
  }

  Path _partial(Path p, double f) {
    final metric = p.computeMetrics().first;
    return metric.extractPath(0, metric.length * f.clamp(0.0, 1.0));
  }

  @override
  bool shouldRepaint(_DemoPainter old) => old.crack != crack || old.spin != spin;
}

/// What the demo was hiding: a wireframe, honest and empty.
class _HollowCard extends StatelessWidget {
  final double spin;
  const _HollowCard({required this.spin});

  @override
  Widget build(BuildContext context) {
    return SizedBox(
      width: 420,
      height: 300,
      child: CustomPaint(painter: _HollowPainter()),
    );
  }
}

class _HollowPainter extends CustomPainter {
  @override
  void paint(Canvas canvas, Size size) {
    final paint = Paint()
      ..color = t.kOnSurfaceVariant.withValues(alpha: 0.35)
      ..style = PaintingStyle.stroke
      ..strokeWidth = 1.2;
    canvas.drawRRect(RRect.fromRectAndRadius(Rect.fromLTWH(0, 0, size.width, size.height), const Radius.circular(14)), paint);
    // Interior wireframe: a heading bar, two lines, a button outline.
    canvas.drawRRect(RRect.fromRectAndRadius(const Rect.fromLTWH(24, 60, 120, 18), const Radius.circular(3)), paint);
    canvas.drawRRect(RRect.fromRectAndRadius(const Rect.fromLTWH(24, 96, 240, 10), const Radius.circular(3)), paint);
    canvas.drawRRect(RRect.fromRectAndRadius(const Rect.fromLTWH(24, 116, 180, 10), const Radius.circular(3)), paint);
    canvas.drawRRect(RRect.fromRectAndRadius(const Rect.fromLTWH(24, 170, 110, 32), const Radius.circular(6)), paint);
  }

  @override
  bool shouldRepaint(_HollowPainter old) => false;
}
