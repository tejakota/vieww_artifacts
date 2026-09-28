/// S08 · the arrival — the studio's own splash, recreated beat for beat,
/// uncovering the real chrome behind it exactly as the real app does.
///
/// This scene is a hard cut in, not a fade: the splash is the transition.
library;

import 'package:flutter/widgets.dart';

import '../../film.dart';
import '../../motion/vieww_motion.dart';
import '../../theme.dart' as t;
import '../../widgets/studio/editor.dart';
import '../../widgets/studio/preview_screens.dart';
import '../../widgets/studio/splash.dart';
import '../../widgets/studio/studio_shell.dart';

Widget build(SceneCtx ctx) {
  final nowMs = ctx.sec * 1000.0;
  // After the splash (1.9 s), a slow confident push-in on the whole chrome.
  final push = easeInOutQuad(clamp01((ctx.sec - 2.4) / 6.0));

  final studio = StudioChrome(
    time: ctx.abs,
    buffer: kCounterSay,
    typed: 1.0,
    caretLine: 7,
    fileName: 'counter.say',
    screen: CounterScreen(count: 0),
    compile: Compile.done,
    deviceMount: 1.0,
  );

  return ColoredBox(
    color: t.kWindow,
    child: Stack(children: [
      Positioned.fill(
        child: Transform.scale(
          scale: 1.0 + 0.028 * push,
          child: SizedBox(
            width: 1920,
            height: 1080,
            child: StudioSplash(nowMs: nowMs, child: studio),
          ),
        ),
      ),
    ]),
  );
}
