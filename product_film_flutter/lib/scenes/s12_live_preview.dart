/// S12 · live preview — the sketch that runs at typing speed, no compiler
/// in the loop, and nav you can actually press.
///
/// "Live Preview sketches whole multi-screen flows at typing speed without
/// compiling" — `screens/live.rs` is the file being typed.
library;

import 'package:flutter/widgets.dart';

import '../../film.dart';
import '../../motion/vieww_motion.dart';
import '../../theme.dart' as t;
import '../../widgets/studio/editor.dart';
import '../../widgets/studio/preview_screens.dart';
import '../../widgets/studio/studio_shell.dart';
import 'common.dart';

Widget build(SceneCtx ctx) {
  final s = ctx.sec;
  final typed = easeOutCubic(clamp01((s - 0.5) / 4.0));
  // The nav: tap "Open settings" at 6 s, back at 9 s.
  final goSettings = easeInOutQuad(clamp01((s - 6.0) / 0.5));
  final goBack = easeInOutQuad(clamp01((s - 9.0) / 0.5));
  final onSettings = goSettings - goBack; // 0 -> 1 -> 0

  final studio = StudioChrome(
    time: ctx.abs,
    buffer: kLiveRs,
    typed: typed,
    caretLine: 9,
    fileName: 'live.rs',
    activeFile: 'live.rs',
    compile: Compile.idle,
    screen: _LiveFlow(onSettings: onSettings.clamp(0.0, 1.0)),
    deviceMount: 1.0,
  );

  return Stack(children: [
    Positioned.fill(
      child: SizedBox(width: 1920, height: 1080, child: studio),
    ),
    caption(ctx, 1.6, 'sketch whole flows at typing speed.', end: 5.6, scrim: true),
    caption(ctx, 6.6, 'no compiler in the loop. no wait to see.', end: 11.0, scrim: true),
    receipt(ctx, 2.4, 'live.rs · parsed, drawn while typed · docs/02-widgets.md'),
  ]);
}

/// The Home/Settings pair with a slide between them.
class _LiveFlow extends StatelessWidget {
  final double onSettings;
  const _LiveFlow({required this.onSettings});

  @override
  Widget build(BuildContext context) {
    return Stack(children: [
      // Home slides left as Settings slides in from the right.
      Positioned.fill(
        child: Opacity(
          opacity: (1.0 - onSettings).clamp(0.0, 1.0),
          child: Transform.translate(
            offset: Offset(-onSettings * 120, 0),
            child: const LiveHomeScreen(),
          ),
        ),
      ),
      Positioned.fill(
        child: Opacity(
          opacity: onSettings.clamp(0.0, 1.0),
          child: Transform.translate(
            offset: Offset((1.0 - onSettings) * 120, 0),
            child: const LiveSettingsScreen(),
          ),
        ),
      ),
    ]);
  }
}
