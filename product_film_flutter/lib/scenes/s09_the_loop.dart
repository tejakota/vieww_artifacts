/// S09 · the loop — the whole argument, played once inside the real studio:
///
///   "That is the whole loop this studio exists to shorten: edit a line,
///    see the picture change, edit the next line." — docs/01-first-screen.md
///
/// The buffer types, rustc runs one invocation in 0.8 s, the screen mounts
/// in the device frame. Then the film makes the one-line edit —
/// "starting at 0" becomes "starting at 1" — and the preview changes with
/// it, because the preview is the screen, not a picture of it.
library;

import 'package:flutter/widgets.dart';

import '../../film.dart';
import '../../motion/vieww_motion.dart';
import '../../widgets/studio/editor.dart';
import '../../widgets/studio/preview_screens.dart';
import '../../widgets/studio/studio_shell.dart';
import 'common.dart';

Widget build(SceneCtx ctx) {
  final s = ctx.sec;

  // Phase A: the program finishes typing itself (an open file, typing in).
  final typedA = easeOutCubic(clamp01((s - 0.3) / 1.2));
  // Compile 1: auto after the 600 ms debounce, 0.8 s of rustc.
  final compile1 = clamp01((s - 1.8) / 0.8);
  // The preview mounts when the compile lands.
  final mount1 = VSpring(SpringSpec.expressive).position(s - 2.8);

  // Phase B: the edit. Caret moves to line 3 (index 2), the 0 becomes 1.
  final editing = s > 5.6;
  // Compile 2 after the edit.
  final compile2 = clamp01((s - 7.4) / 0.8);
  final changed = s > 8.2;

  // Phase D: the camera drifts toward the device frame, quietly — a slight
  // zoom with a pan, framed so nothing the status bar says leaves the cut.
  final dolly = easeInOutQuad(clamp01((s - 12.0) / 3.4));

  final buffer = editing ? kCounterSayEdit : kCounterSay;

  final compile = s < 1.8
      ? Compile.idle
      : s < 2.6
          ? Compile.running
          : s < 7.4
              ? Compile.done
              : s < 8.2
                  ? Compile.running
                  : Compile.done;
  final compileProgress = s < 2.6 ? compile1 : compile2;
  final deviceMount = s < 2.8 ? 0.0 : mount1.clamp(0.0, 1.0);
  final renderPulse = (s >= 1.8 && s < 2.6) || (s >= 7.4 && s < 8.2) ? 1.0 : 0.0;

  // The count label pops when the recompiled screen mounts — the label,
  // not the screen: the frame is the app's, the surprise is the number's.
  final countPop = changed ? VSpring(SpringSpec.expressive).position(s - 8.25).clamp(0.0, 1.0) : 0.0;

  final studio = StudioChrome(
    time: ctx.abs,
    buffer: buffer,
    typed: typedA,
    caretLine: editing ? 2 : 7,
    fileName: 'counter.say',
    screen: CounterScreen(
      count: changed ? 1 : 0,
      labelScale: 0.94 + 0.06 * countPop,
    ),
    compile: compile,
    compileProgress: compileProgress,
    deviceMount: deviceMount,
    renderPulse: renderPulse,
  );

  return Stack(children: [
    Positioned.fill(
      child: Transform.translate(
        offset: Offset(-80.0 * dolly, 0.0),
        child: Transform.scale(
          scale: 1.0 + 0.06 * dolly,
          child: SizedBox(width: 1920, height: 1080, child: studio),
        ),
      ),
    ),
    // The voice — the studio's own sentence, split across the loop.
    caption(ctx, 1.0, 'edit a line.', dur: 0.8, end: 4.8, scrim: true),
    caption(ctx, 4.6, 'see the picture change.', dur: 0.8, end: 9.2, scrim: true),
    caption(ctx, 9.0, 'edit the next line.', dur: 0.8, end: 12.5, scrim: true),
    receipt(ctx, 10.6, '0.8 s warm rebuild · src/compile.rs'),
  ]);
}
