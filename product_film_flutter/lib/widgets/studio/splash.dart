/// The studio's launch splash, recreated beat-for-beat from
/// `apps/viewwstudio/src/ui/splash.rs`:
///
/// - 1900 ms total; reveal windows T_MARK (0,420) T_EDITOR (180,620)
///   T_PREVIEW (320,780) T_WORDMARK (520,900) T_TAGLINE (660,1040)
///   T_VERSION (820,1180); exit (1480,1900).
/// - every reveal is `1 - (1-t)^3` — decelerating, "a reveal that eases in
///   as well begins by looking like a dropped frame."
/// - the mark grows from 0.78; the lines rise 10 pt; the exit lifts -18 pt
///   and swells x1.04 — "a departure, not a dissolve."
/// - backdrop #0E1013, ink #E8EAF0 / #9AA2B1 / #6B7383.
///
/// In the real app the chrome sits fully drawn behind the splash the whole
/// time and is *uncovered* by the exit; `child` is that chrome.
library;

import 'package:flutter/widgets.dart';

import '../../theme.dart' as t;
import '../brand.dart';
import '../primitives.dart';

class StudioSplash extends StatelessWidget {
  /// Milliseconds into the 1900 ms splash.
  final double nowMs;

  /// What sits behind — the studio chrome, drawn the entire time.
  final Widget child;

  const StudioSplash({super.key, required this.nowMs, required this.child});

  @override
  Widget build(BuildContext context) {
    if (nowMs >= t.kSplashDurationMs) return child;

    final exit = _reveal(t.tExit);
    final veil = (1.0 - exit).clamp(0.0, 1.0);

    return Stack(children: [
      Positioned.fill(child: child),
      Positioned.fill(
        child: IgnorePointer(
          child: Opacity(
            opacity: veil,
            child: ColoredBox(color: t.kBackdrop, child: Center(child: _composition(context, exit))),
          ),
        ),
      ),
    ]);
  }

  double _reveal((double, double) w) {
    if (nowMs <= w.$1) return 0.0;
    if (nowMs >= w.$2) return 1.0;
    final x = (nowMs - w.$1) / (w.$2 - w.$1);
    return 1.0 - (1.0 - x) * (1.0 - x) * (1.0 - x);
  }

  Widget _composition(BuildContext context, double exit) {
    final mq = MediaQuery.of(context);
    final width = mq.size.width;
    final height = mq.size.height;
    final side = ((width < height ? width : height) * 0.30).clamp(96.0, 300.0);
    final gap = side * 0.16;

    final markReveal = _reveal(t.tMark);
    final editorReveal = _reveal(t.tEditor);
    final previewReveal = _reveal(t.tPreview);

    final column = Column(
      mainAxisSize: MainAxisSize.min,
      crossAxisAlignment: CrossAxisAlignment.center,
      children: [
        _grown(markReveal, 0.78, side, BrandMark(side: side, editor: editorReveal, preview: previewReveal)),
        SizedBox(height: gap),
        _rising(_reveal(t.tWordmark), Voice('vieww Studio', (side * 0.20).clamp(20.0, 58.0), t.kInk, weight: FontWeight.w700)),
        SizedBox(height: side * 0.055),
        _rising(_reveal(t.tTagline), Voice('a code editor and device-framed preview for vieww screens', (side * 0.062).clamp(11.0, 17.0), t.kInkMuted)),
        SizedBox(height: side * 0.10),
        _rising(_reveal(t.tVersion), Voice('version 0.1.0', (side * 0.045).clamp(9.5, 13.0), t.kInkFaint)),
      ],
    );

    // The exit: lift -18 pt and swell x1.04 over the last 420 ms — the mark
    // moves off toward the viewer and the chrome is uncovered behind it.
    final lift = t.kSplashExitLift * exit;
    final swell = 1.0 + t.kSplashExitSwell * exit;
    return Transform.translate(
      offset: Offset(0.0, lift),
      child: Transform.scale(scale: swell, child: column),
    );
  }

  /// Faded and scaled up into place from `from`, about its own centre —
  /// `splash.rs::grown`.
  Widget _grown(double reveal, double from, double side, Widget child) {
    final scale = from + (1.0 - from) * reveal.clamp(0.0, 1.0);
    return SizedBox(
      width: side,
      height: side,
      child: Opacity(
        opacity: reveal.clamp(0.0, 1.0),
        child: Transform.scale(scale: scale, child: child),
      ),
    );
  }

  /// Faded up, arriving from 10 pt below — `splash.rs::rising`.
  Widget _rising(double reveal, Widget child) {
    final p = reveal.clamp(0.0, 1.0);
    return Opacity(
      opacity: p,
      child: Transform.translate(offset: Offset(0.0, (1.0 - p) * 10.0), child: child),
    );
  }
}
