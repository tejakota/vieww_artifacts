/// Shared scene vocabulary: the frame, the caption and receipt slots, the
/// typing clock, the chips every scene speaks in.
library;

import 'package:flutter/widgets.dart';

import '../film.dart';
import '../motion/vieww_motion.dart';
import '../theme.dart' as t;
import '../widgets/primitives.dart';

const W = 1920.0;
const H = 1080.0;

/// The caption slot — bottom-left, where the film's voice lives.
/// Fades out from `end` (defaults to the scene's last 0.5 s).
/// `scrim` puts a quiet plate behind the line when it must sit on top of
/// busy chrome — the studio scenes — so it reads as the film speaking,
/// not a stray label.
Widget caption(SceneCtx ctx, double start, String text, {double size = 21, Color? color, double dur = 1.2, double? end, bool scrim = false}) {
  final out = easeOutCubic(window(ctx.seconds - ctx.sec, 0.5, 0.5));
  final fadeOut = end == null ? out : easeOutCubic(clamp01((end - ctx.sec) / 0.6));
  final p = easeOutCubic(window(ctx.sec, start, dur)) * fadeOut;
  return Positioned(
    left: 96,
    bottom: 84,
    child: scrim
        ? Opacity(
            opacity: p,
            child: Container(
              padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 10),
              decoration: BoxDecoration(
                color: t.colorOver(const Color(0xD9000000), t.kFilmGround),
                borderRadius: BorderRadius.circular(8),
                border: Border.all(color: const Color(0x14FFFFFF)),
              ),
              child: Caption(text: text, progress: 1.0, size: size, color: color ?? t.kInk),
            ),
          )
        : Caption(text: text, progress: p, size: size, color: color ?? t.kFilmMuted),
  );
}

/// The receipt slot — bottom-right, the file a number came from.
Widget receipt(SceneCtx ctx, double start, String text) {
  return Positioned(
    right: 96,
    bottom: 84,
    child: Opacity(
      opacity: easeOutCubic(window(ctx.sec, start, 1.0)),
      child: Receipt(text),
    ),
  );
}

/// A centered voice line that rises like the splash's lines do.
Widget risingLine(double p, String text, double size, Color color, {FontWeight weight = FontWeight.w400}) {
  final e = easeOutCubic(p);
  return Opacity(
    opacity: e,
    child: Transform.translate(
      offset: Offset(0, (1 - e) * 10.0),
      child: Voice(text, size, color, weight: weight),
    ),
  );
}

/// The typed reveal of a string across `span` seconds starting at `start`.
double typedBy(SceneCtx ctx, double start, double span, String text, {double cps = 22.0}) {
  if (ctx.sec <= start) return 0.0;
  final chars = text.length.toDouble();
  final spanSec = span > 0 ? span : chars / cps;
  return window(ctx.sec, start, spanSec);
}

/// A small mono chip — the "107 commands" class of fact.
class Chip extends StatelessWidget {
  final String text;
  final Color? color;
  const Chip(this.text, {super.key, this.color});

  @override
  Widget build(BuildContext context) {
    final c = color ?? t.kOnSurfaceVariant;
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 6),
      decoration: BoxDecoration(
        color: t.colorOver(const Color(0x14FFFFFF), t.kChrome1),
        borderRadius: BorderRadius.circular(6),
        border: Border.all(color: t.kLine),
      ),
      child: Mono(text, 13, c),
    );
  }
}

/// A number that counts up — mono, tabular-feeling, receipt-ready.
class CountUp extends StatelessWidget {
  final double progress;
  final double value;
  final int decimals;
  final String suffix;
  final double size;
  final Color color;

  const CountUp({
    super.key,
    required this.progress,
    required this.value,
    this.decimals = 0,
    this.suffix = '',
    this.size = 44,
    this.color = t.kInk,
  });

  @override
  Widget build(BuildContext context) {
    final p = progress.clamp(0.0, 1.0);
    final v = value * p;
    final shown = decimals == 0
        ? v.round().toString()
        : v.toStringAsFixed(decimals);
    return Mono('$shown$suffix', size, color, weight: FontWeight.w500, letterSpacing: 1.0);
  }
}

/// The film's ground gradient — one paint under every scene's Stack.
class FilmGround extends StatelessWidget {
  const FilmGround({super.key});

  @override
  Widget build(BuildContext context) {
    return const DecoratedBox(
      decoration: BoxDecoration(
        gradient: LinearGradient(
          begin: Alignment.topCenter,
          end: Alignment.bottomCenter,
          colors: [t.kFilmGroundTop, t.kFilmGround, t.kFilmGroundBottom],
          stops: [0.0, 0.45, 1.0],
        ),
      ),
    );
  }
}

/// Wrap a scene's content in the standard frame: ground + children.
Widget sceneFrame(List<Widget> children) {
  return Stack(children: [
    const Positioned.fill(child: FilmGround()),
    ...children,
  ]);
}

/// A pill spinner — the wait, drawn once and reused by the old-world scenes.
class WaitSpinner extends StatelessWidget {
  final double time; // seconds
  final double size;
  final Color color;
  const WaitSpinner({super.key, required this.time, this.size = 22, this.color = t.kGutter});

  @override
  Widget build(BuildContext context) {
    return SizedBox(
      width: size,
      height: size,
      child: CustomPaint(painter: _SpinnerPainter(time, size, color)),
    );
  }
}

class _SpinnerPainter extends CustomPainter {
  final double time;
  final double s;
  final Color c;
  _SpinnerPainter(this.time, this.s, this.c);

  @override
  void paint(Canvas canvas, Size size) {
    final paint = Paint()
      ..color = c
      ..style = PaintingStyle.stroke
      ..strokeWidth = 2.2
      ..strokeCap = StrokeCap.round;
    // One arc sweeping around, its start drifting — the canonical wait.
    final sweep = 4.4;
    final start = time * 3.6;
    canvas.drawArc(Rect.fromLTWH(2, 2, s - 4, s - 4), start, sweep, false, paint);
  }

  @override
  bool shouldRepaint(_SpinnerPainter old) => true;
}

/// A tiny phone silhouette — the "picture" end of the distance.
class PhoneGhost extends StatelessWidget {
  final double width;
  final double height;
  final Color color;
  final Widget? child;
  const PhoneGhost({super.key, required this.width, required this.height, this.color = t.kChrome4, this.child});

  @override
  Widget build(BuildContext context) {
    return Container(
      width: width,
      height: height,
      decoration: BoxDecoration(
        color: color,
        borderRadius: BorderRadius.circular(width * 0.14),
        border: Border.all(color: t.kLine),
      ),
      child: child,
    );
  }
}
