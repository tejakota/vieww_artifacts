/// S18 · the contracts — quality as checks that can fail a build.
///
/// The four contracts, all real: glyph pixels within 1/255 rim included;
/// zero allocations over 60 steady frames; web canvas byte-for-byte equal
/// to native (2,304,000 bytes); 4,225 tests in one pass.
library;

import 'package:flutter/widgets.dart';

import '../../film.dart';
import '../../motion/vieww_motion.dart';
import '../../theme.dart' as t;
import '../../widgets/primitives.dart';
import 'common.dart';

Widget build(SceneCtx ctx) {
  final s = ctx.sec;
  final starts = staggerStarts(4, 1.3, offset: 0.8);
  final cap = easeOutCubic(window(s, 2.0, 1.2));
  final endFade = easeOutCubic(window(ctx.seconds - s, 0.5, 0.5));

  return sceneFrame([
    Positioned(
      left: 0,
      right: 0,
      top: 150,
      child: Opacity(
        opacity: endFade,
        child: Center(child: risingLine(cap, 'quality as contracts — checks that can fail a build.', 26, t.kInk)),
      ),
    ),
    // The 2x2 contract grid.
    Positioned(
      left: 240,
      top: 290,
      child: Opacity(
        opacity: endFade,
        child: SizedBox(
          width: 1440,
          child: Column(children: [
            Row(crossAxisAlignment: CrossAxisAlignment.start, children: [
              Expanded(child: _ContractCard(p: easeOutCubic(clamp01((s - starts[0]) / 0.9)), child: const _GlyphContract())),
              const SizedBox(width: 24),
              Expanded(child: _ContractCard(p: easeOutCubic(clamp01((s - starts[1]) / 0.9)), child: const _AllocContract())),
            ]),
            const SizedBox(height: 24),
            Row(crossAxisAlignment: CrossAxisAlignment.start, children: [
              Expanded(child: _ContractCard(p: easeOutCubic(clamp01((s - starts[2]) / 0.9)), child: _ParityContract(time: ctx.abs))),
              const SizedBox(width: 24),
              Expanded(child: _ContractCard(p: easeOutCubic(clamp01((s - starts[3]) / 0.9)), child: const _TestsContract())),
            ]),
          ]),
        ),
      ),
    ),
    receipt(ctx, 5.6, 'VIEWW-QUALITY-CONTRACTS.md · twelve measured clauses'),
  ]);
}

class _ContractCard extends StatelessWidget {
  final double p;
  final Widget child;
  const _ContractCard({required this.p, required this.child});

  @override
  Widget build(BuildContext context) {
    if (p <= 0.001) return const SizedBox(height: 300);
    return Opacity(
      opacity: p,
      child: Transform.translate(
        offset: Offset(0, (1 - p) * 18.0),
        child: Pane(
          color: t.kChrome1,
          elevation: 1,
          padding: const EdgeInsets.all(24),
          child: SizedBox(height: 300, child: child),
        ),
      ),
    );
  }
}

/// Contract 1: the glyph, magnified, its rim lit.
class _GlyphContract extends StatelessWidget {
  const _GlyphContract();

  @override
  Widget build(BuildContext context) {
    return Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
      const Mono('CONTRACT · TEXT', 11, t.kGutter, letterSpacing: 1.2),
      const SizedBox(height: 18),
      Row(children: [
        SizedBox(
          width: 150,
          height: 150,
          child: CustomPaint(painter: _GlyphPainter()),
        ),
        const SizedBox(width: 26),
        Expanded(
          child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
            const Mono('every pixel of every glyph', 17, t.kOnSurface),
            const SizedBox(height: 6),
            const Mono('within 1/255 — rim included', 17, t.kAccentFar, weight: FontWeight.w500),
            const SizedBox(height: 10),
            const Mono('a mirrored frame, a glyph one pixel off, or colour applied before coverage cannot fit inside that bound', 12.5, t.kFilmFaint),
          ]),
        ),
      ]),
    ]);
  }
}

class _GlyphPainter extends CustomPainter {
  @override
  void paint(Canvas canvas, Size size) {
    // A big "a" drawn as geometry with its rim highlighted — the contract
    // drawn, not stated.
    final fill = Paint()..color = t.kOnSurface;
    final rim = Paint()
      ..color = t.kAccentFar
      ..style = PaintingStyle.stroke
      ..strokeWidth = 2.0;
    final path = Path()
      ..moveTo(size.width * 0.30, size.height * 0.82)
      ..cubicTo(size.width * 0.18, size.height * 0.66, size.width * 0.20, size.height * 0.30, size.width * 0.42, size.height * 0.22)
      ..cubicTo(size.width * 0.62, size.height * 0.16, size.width * 0.74, size.height * 0.34, size.width * 0.74, size.height * 0.56)
      ..lineTo(size.width * 0.74, size.height * 0.84)
      ..close();
    // The bowl of the a.
    path.moveTo(size.width * 0.42, size.height * 0.84);
    path.cubicTo(size.width * 0.42, size.height * 0.62, size.width * 0.58, size.height * 0.56, size.width * 0.58, size.height * 0.72);
    path.lineTo(size.width * 0.42, size.height * 0.84);
    path.close();
    canvas.drawPath(path, fill);
    canvas.drawPath(path, rim);
  }

  @override
  bool shouldRepaint(_GlyphPainter old) => false;
}

/// Contract 2: the allocator counter that stays at zero.
class _AllocContract extends StatelessWidget {
  const _AllocContract();

  @override
  Widget build(BuildContext context) {
    return Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
      const Mono('CONTRACT · MEMORY', 11, t.kGutter, letterSpacing: 1.2),
      const SizedBox(height: 18),
      const Mono('allocations over 60 steady frames', 17, t.kOnSurface),
      const SizedBox(height: 14),
      Row(crossAxisAlignment: CrossAxisAlignment.center, children: [
        const Mono('0', 64, t.kSuccess, weight: FontWeight.w500),
        const SizedBox(width: 18),
        // Expanded so the sentence wraps instead of overflowing the card.
        const Expanded(
          child: Mono('counted by a real global allocator — the first honest reading was 7, and it was fixed', 12.5, t.kFilmFaint),
        ),
      ]),
    ]);
  }
}

/// Contract 3: web == native, byte for byte.
class _ParityContract extends StatelessWidget {
  final double time;
  const _ParityContract({required this.time});

  @override
  Widget build(BuildContext context) {
    return Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
      const Mono('CONTRACT · PARITY', 11, t.kGutter, letterSpacing: 1.2),
      const SizedBox(height: 18),
      Row(children: [
        _PixelGrid(time: time, seed: 5),
        const SizedBox(width: 16),
        const Mono('==', 34, t.kAccentFar, weight: FontWeight.w500),
        const SizedBox(width: 16),
        _PixelGrid(time: time, seed: 5),
      ]),
      const SizedBox(height: 16),
      const Mono('web canvas read back, native rendered — 2,304,000 bytes, equal', 15, t.kOnSurface),
      const SizedBox(height: 6),
      const Mono('the same pixels, on both sides of the browser', 12.5, t.kFilmFaint),
    ]);
  }
}

class _PixelGrid extends StatelessWidget {
  final double time;
  final int seed;
  const _PixelGrid({required this.time, required this.seed});

  @override
  Widget build(BuildContext context) {
    return SizedBox(
      width: 120,
      height: 120,
      child: CustomPaint(painter: _GridPainter(time, seed)),
    );
  }
}

class _GridPainter extends CustomPainter {
  final double time;
  final int seed;
  _GridPainter(this.time, this.seed);

  @override
  void paint(Canvas canvas, Size size) {
    final rng = Rng(seed * 0x9E37);
    const n = 9;
    final cell = size.width / n;
    // The grids shimmer subtly in sync — identical by construction.
    final phase = time * 0.4;
    for (var y = 0; y < n; y++) {
      for (var x = 0; x < n; x++) {
        final v = rng.next01();
        if (v < 0.35) {
          final a = 0.25 + 0.35 * (0.5 + 0.5 * (phase + v * 6.28).abs() % 1.0);
          final paint = Paint()..color = t.kAccentFar.withValues(alpha: a.clamp(0.0, 1.0));
          canvas.drawRect(Rect.fromLTWH(x * cell + 1, y * cell + 1, cell - 2, cell - 2), paint);
        } else if (v < 0.5) {
          final paint = Paint()..color = t.kOnSurfaceVariant.withValues(alpha: 0.3);
          canvas.drawRect(Rect.fromLTWH(x * cell + 1, y * cell + 1, cell - 2, cell - 2), paint);
        }
      }
    }
    // The grid rim.
    final rim = Paint()
      ..color = t.kOnSurfaceVariant.withValues(alpha: 0.5)
      ..style = PaintingStyle.stroke
      ..strokeWidth = 1.2;
    canvas.drawRect(Rect.fromLTWH(0, 0, size.width, size.height), rim);
  }

  @override
  bool shouldRepaint(_GridPainter old) => true;
}

/// Contract 4: the big number.
class _TestsContract extends StatelessWidget {
  const _TestsContract();

  @override
  Widget build(BuildContext context) {
    return Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
      const Mono('CONTRACT · THE GATE', 11, t.kGutter, letterSpacing: 1.2),
      const SizedBox(height: 18),
      Row(crossAxisAlignment: CrossAxisAlignment.center, children: [
        const Mono('4,225', 64, t.kInk, weight: FontWeight.w500),
        const SizedBox(width: 20),
        const Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
          Mono('tests, one pass', 17, t.kOnSurface),
          SizedBox(height: 6),
          Mono('zero failures · 2026-09-15 receipt', 12.5, t.kFilmFaint),
        ]),
      ]),
    ]);
  }
}
