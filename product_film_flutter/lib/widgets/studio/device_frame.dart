/// The device frame — the studio's simulated window, with the three
/// platforms `state.rs::Platform` actually models:
///
/// - iOS     393 x 852 logical @3.0, safe area 47/34, a notch.
/// - Android 412 x 915 logical @2.625, insets 30/24, a punch-hole.
/// - Desktop 1280 x 800, a plain window card.
///
/// The bezel is drawn at frame scale; the screen inside is authored at
/// logical size and scaled with a top-left transform, so every screen
/// number (16 px padding, 12 spacing) is the number the source file says.
library;

import 'package:flutter/widgets.dart';

import '../../theme.dart' as t;
import '../primitives.dart';

enum Platform { ios, android, desktop }

class DeviceFrame extends StatelessWidget {
  final Platform platform;
  final Widget screen;

  /// 0..1 — the frame's mount animation (scale + fade).
  final double mount;

  const DeviceFrame({super.key, required this.platform, required this.screen, this.mount = 1.0});

  @override
  Widget build(BuildContext context) {
    final (logicalW, logicalH) = switch (platform) {
      Platform.ios => (393.0, 852.0),
      Platform.android => (412.0, 915.0),
      Platform.desktop => (1280.0, 800.0),
    };

    return LayoutBuilder(builder: (context, constraints) {
      final maxH = constraints.biggest.height;
      final maxW = constraints.biggest.width;
      final fit = _fitScale(logicalW + 24, logicalH + 24, maxW, maxH);
      final m = mount.clamp(0.0, 1.0);

      return Opacity(
        opacity: m,
        child: Transform.scale(
          scale: 0.92 + 0.08 * m,
          child: SizedBox(
            width: (logicalW + 24) * fit,
            height: (logicalH + 24) * fit,
            child: Transform.scale(
              scale: fit,
              alignment: Alignment.topLeft,
              child: SizedBox(
                width: logicalW + 24,
                height: logicalH + 24,
                child: Center(
                  child: platform == Platform.desktop
                      ? _desktopWindow(logicalW, logicalH)
                      : _phone(logicalW, logicalH),
                ),
              ),
            ),
          ),
        ),
      );
    });
  }

  double _fitScale(double w, double h, double maxW, double maxH) {
    final s = maxW / w;
    final s2 = maxH / h;
    return (s < s2 ? s : s2).clamp(0.0, 1.0);
  }

  // ------------------------------------------------------------------ phone

  Widget _phone(double w, double h) {
    final bezel = 10.0;
    final bezelRadius = (w * 0.135).clamp(40.0, 58.0);
    final screenRadius = bezelRadius - 9.0;
    final (topInset, bottomInset) = platform == Platform.ios ? (47.0, 34.0) : (30.0, 24.0);

    return Container(
      width: w + bezel * 2,
      height: h + bezel * 2,
      decoration: BoxDecoration(
        color: t.kChrome4,
        borderRadius: BorderRadius.circular(bezelRadius),
        boxShadow: t.elevation(3),
        border: Border.all(color: t.kChrome0, width: 2),
      ),
      child: Padding(
        padding: EdgeInsets.all(bezel),
        child: ClipRRect(
          borderRadius: BorderRadius.circular(screenRadius),
          child: SizedBox(
            width: w,
            height: h,
            child: Stack(children: [
              Positioned.fill(child: screen),
              Positioned(left: 0, right: 0, top: 0, height: topInset, child: _statusBar()),
              if (platform == Platform.ios)
                Positioned(top: 9, left: w / 2 - 62, child: _notch())
              else
                Positioned(top: 10, left: w / 2 - 6, child: _punchHole()),
              if (platform == Platform.ios)
                Positioned(bottom: 8, left: w / 2 - 66, child: _homeIndicator()),
            ]),
          ),
        ),
      ),
    );
  }

  Widget _notch() => Container(
        width: 124,
        height: 30,
        decoration: BoxDecoration(color: t.kWindow, borderRadius: BorderRadius.circular(16)),
      );

  Widget _punchHole() => Container(width: 12, height: 12, decoration: BoxDecoration(color: t.kWindow, shape: BoxShape.circle));

  Widget _homeIndicator() => Container(
        width: 132,
        height: 5,
        decoration: BoxDecoration(
          color: t.kOnSurface.withValues(alpha: 0.35),
          borderRadius: BorderRadius.circular(3),
        ),
      );

  Widget _statusBar() => Padding(
        padding: const EdgeInsets.symmetric(horizontal: 26),
        child: Row(children: [
          Mono('9:41', 12, t.kOnSurface, weight: FontWeight.w500),
          const Spacer(),
          _barGlyph(),
          const SizedBox(width: 5),
          _barGlyph(),
          const SizedBox(width: 5),
          Container(
            width: 22,
            height: 11,
            decoration: BoxDecoration(border: Border.all(color: t.kOnSurface, width: 1), borderRadius: BorderRadius.circular(3)),
          ),
        ]),
      );

  Widget _barGlyph() => Container(width: 4, height: 11, decoration: BoxDecoration(color: t.kOnSurface, borderRadius: BorderRadius.circular(1)));

  // ---------------------------------------------------------------- desktop

  Widget _desktopWindow(double w, double h) {
    return Pane(
      color: t.kChrome1,
      elevation: 2,
      customRadius: BorderRadius.circular(10),
      child: SizedBox(
        width: w,
        height: h,
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            Container(
              height: 34,
              decoration: const BoxDecoration(
                color: t.kChrome0,
                borderRadius: BorderRadius.vertical(top: Radius.circular(10)),
              ),
              child: Padding(
                padding: const EdgeInsets.symmetric(horizontal: 12),
                child: Row(children: [
                  const BrandDot(),
                  const SizedBox(width: 8),
                  Mono('counter', 11, t.kGutter),
                  const Spacer(),
                  const WindowGlyph(WindowGlyphKind.minus, size: 10),
                  const SizedBox(width: 10),
                  const WindowGlyph(WindowGlyphKind.square, size: 10),
                  const SizedBox(width: 10),
                  const WindowGlyph(WindowGlyphKind.close, size: 10),
                ]),
              ),
            ),
            Expanded(child: screen),
          ],
        ),
      ),
    );
  }
}

/// The tiny brand mark used in window title bars.
class BrandDot extends StatelessWidget {
  const BrandDot({super.key});

  @override
  Widget build(BuildContext context) {
    return Container(
      width: 10,
      height: 10,
      decoration: BoxDecoration(
        gradient: const LinearGradient(begin: Alignment.topCenter, end: Alignment.bottomCenter, colors: [t.kAccentFar, t.kAccent]),
        borderRadius: BorderRadius.circular(3),
      ),
    );
  }
}
