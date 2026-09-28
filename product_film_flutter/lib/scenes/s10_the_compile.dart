/// S10 · the compile — the pipeline that makes the loop true, then the tap.
///
/// "A buffer here is not a template or a restricted subset — it is normal
/// Rust, compiled by the real compiler." — docs/03-rust.md. One rustc
/// invocation, a cdylib, a dlopen, and the screen mounts as itself.
library;

import 'package:flutter/widgets.dart';

import '../../film.dart';
import '../../motion/vieww_motion.dart';
import '../../theme.dart' as t;
import '../../widgets/primitives.dart';
import '../../widgets/studio/device_frame.dart';
import '../../widgets/studio/preview_screens.dart';
import 'common.dart';

Widget build(SceneCtx ctx) {
  final s = ctx.sec;
  final diagramIn = easeOutCubic(window(s, 0.4, 1.0));
  final callout = easeOutCubic(window(s, 2.2, 1.0));
  // The tap beat.
  final phoneIn = VSpring(SpringSpec.expressive).position(s - 6.0);
  final compileBar = clamp01((s - 6.8) / 0.8);
  final tap = VSpring(SpringSpec(500.0, 0.85)).position(s - 8.6);
  final countIn = VSpring(SpringSpec.expressive).position(s - 8.9);
  final endFade = easeOutCubic(window(ctx.seconds - s, 0.5, 0.5));

  return sceneFrame([
    // The pipeline.
    Positioned(
      left: 0,
      right: 0,
      top: 210,
      child: Opacity(
        opacity: diagramIn * (1.0 - easeOutCubic(window(s, 5.6, 0.8))) * endFade,
        child: Center(
          child: Column(children: [
            _Pipeline(time: ctx.abs),
            const SizedBox(height: 26),
            Opacity(
              opacity: callout,
              child: const Mono('the real compiler — one rustc invocation, not cargo, not a template', 17, t.kAccentFar),
            ),
            const SizedBox(height: 12),
            Opacity(
              opacity: easeOutCubic(window(s, 3.4, 1.0)),
              child: const Mono('the preview mounts as a live subtree — same types, same theme, real taps', 17, t.kFilmMuted),
            ),
          ]),
        ),
      ),
    ),
    // The phone and the tap.
    Positioned(
      left: 180,
      top: 400,
      child: Opacity(
        opacity: phoneIn.clamp(0.0, 1.0) * endFade,
        child: Transform.scale(
          scale: 0.55 + 0.45 * phoneIn.clamp(0.0, 1.0),
          child: SizedBox(
            width: 420,
            height: 640,
            child: DeviceFrame(
              platform: Platform.ios,
              screen: CounterScreen(
                count: tap > 0.5 ? 1 : 0,
                pressed: tap < 1.0 ? (1.0 - tap) : 0.0,
              ),
            ),
          ),
        ),
      ),
    ),
    // The ripple ring around the tap.
    if (tap > 0.02 && tap < 1.0)
      Positioned(
        left: 0,
        right: 0,
        top: 0,
        child: CustomPaint(painter: _RipplePainter(tap), size: const Size(W, H)),
      ),
    // What the tap meant.
    Positioned(
      left: 720,
      top: 640,
      child: Opacity(
        opacity: easeOutCubic(window(s, 9.4, 1.2)) * endFade,
        child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
          const Voice('the preview is not a mockup', 30, t.kInk, weight: FontWeight.w500),
          const SizedBox(height: 8),
          const Voice('of your screen.', 30, t.kFilmMuted, weight: FontWeight.w500),
          const SizedBox(height: 20),
          const Voice('it is your screen.', 30, t.kAccentFar, weight: FontWeight.w500),
          const SizedBox(height: 26),
          Transform.scale(
            scale: 0.9 + 0.1 * countIn.clamp(0.0, 1.0),
            child: const Mono('tapped 1 times · count = 0 -> 1 · state kept', 15, t.kFilmFaint),
          ),
        ]),
      ),
    ),
    caption(ctx, 1.2, 'how a line becomes a picture.'),
    receipt(ctx, 4.2, 'buffer -> rustc -> cdylib -> dlopen -> mounted · docs/03-rust.md'),
  ]);
}

/// The five-node pipeline with a packet riding it forever.
class _Pipeline extends StatelessWidget {
  final double time;
  const _Pipeline({required this.time});

  @override
  Widget build(BuildContext context) {
    const labels = ['buffer', 'rustc', 'cdylib', 'dlopen', 'mounted'];
    return SizedBox(
      width: 1240,
      height: 120,
      child: CustomPaint(painter: _PipelinePainter(time, labels)),
    );
  }
}

class _PipelinePainter extends CustomPainter {
  final double time;
  final List<String> labels;
  _PipelinePainter(this.time, this.labels);

  @override
  void paint(Canvas canvas, Size size) {
    const gap = 60.0;
    final nodeW = (size.width - gap * 4) / 5;
    // The connector hairline.
    final line = Paint()
      ..color = t.kLine
      ..strokeWidth = 1.5;
    canvas.drawLine(Offset(0, size.height / 2 - 20), Offset(size.width, size.height / 2 - 20), line);

    // A packet riding the line, wrapping — the loop the pipeline serves.
    final loop = (time * 0.22) % 1.0;
    final px = loop * size.width;
    final packet = Paint()..color = t.kAccent;
    canvas.drawCircle(Offset(px, size.height / 2 - 20), 5, packet);
    // The packet's glow.
    final glowPaint = Paint()
      ..color = t.kAccent.withValues(alpha: 0.25)
      ..maskFilter = const MaskFilter.blur(BlurStyle.normal, 8);
    canvas.drawCircle(Offset(px, size.height / 2 - 20), 9, glowPaint);

    for (var i = 0; i < labels.length; i++) {
      final x = i * (nodeW + gap);
      final active = (px > x - 8 && px < x + nodeW + 8);
      final r = RRect.fromRectAndRadius(
        Rect.fromLTWH(x, size.height / 2 - 52, nodeW, 64),
        const Radius.circular(10),
      );
      // Node card.
      canvas.drawRRect(
        r,
        Paint()
          ..shader = LinearGradient(
            begin: Alignment.topCenter,
            end: Alignment.bottomCenter,
            colors: [t.colorOver(const Color(0x09FFFFFF), t.kChrome2), t.kChrome2],
          ).createShader(Rect.fromLTWH(x, size.height / 2 - 52, nodeW, 64)),
      );
      // Node rim — brighter when the packet is passing.
      canvas.drawRRect(
        r,
        Paint()
          ..color = active ? t.kAccentFar : t.rim(t.kChrome2)
          ..style = PaintingStyle.stroke
          ..strokeWidth = active ? 2.0 : 1.0,
      );
      // Label.
      final tp = TextPainter(
        text: TextSpan(
          text: labels[i],
          style: TextStyle(
            fontFamily: 'GeistMono',
            fontSize: 16,
            color: active ? t.kInk : t.kOnSurfaceVariant,
            fontWeight: active ? FontWeight.w500 : FontWeight.w400,
          ),
        ),
        textDirection: TextDirection.ltr,
      )..layout();
      tp.paint(canvas, Offset(x + nodeW / 2 - tp.width / 2, size.height / 2 - 20 - tp.height / 2));
      // Arrows between nodes.
      if (i < labels.length - 1) {
        final ax = x + nodeW + 12;
        final arrow = Paint()
          ..color = t.kGutter
          ..strokeWidth = 1.6
          ..strokeCap = StrokeCap.round;
        final ay = size.height / 2 - 20;
        canvas.drawLine(Offset(ax, ay), Offset(ax + gap - 24, ay), arrow);
        canvas.drawLine(Offset(ax + gap - 30, ay - 4), Offset(ax + gap - 22, ay), arrow);
        canvas.drawLine(Offset(ax + gap - 30, ay + 4), Offset(ax + gap - 22, ay), arrow);
      }
    }
  }

  @override
  bool shouldRepaint(_PipelinePainter old) => true;
}

/// One expanding ring at the point of the tap.
class _RipplePainter extends CustomPainter {
  final double progress;
  _RipplePainter(this.progress);

  @override
  void paint(Canvas canvas, Size size) {
    // The tap lands on the phone's Add-one button, at a fixed film point.
    const cx = 390.0;
    const cy = 860.0;
    final r = 30 + 90 * progress;
    final paint = Paint()
      ..color = t.kAccent.withValues(alpha: (1.0 - progress) * 0.5)
      ..style = PaintingStyle.stroke
      ..strokeWidth = 2.5 * (1.0 - progress) + 0.5;
    canvas.drawCircle(const Offset(cx, cy), r, paint);
  }

  @override
  bool shouldRepaint(_RipplePainter old) => old.progress != progress;
}