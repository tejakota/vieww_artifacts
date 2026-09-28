/// The film engine: one clock, one pure function of time.
///
/// `FilmRoot` rebuilds its entire tree whenever the clock moves — the same
/// rebuild-everything model vieww's films use, where every scene is
/// `fn(t) -> WidgetNode`. Flutter's element tree then does what vieww's
/// element tree does: keeps identity where the widget configuration matches,
/// so the paint cost of a frame is the parts that changed.
library;

import 'package:flutter/widgets.dart';

import 'motion/vieww_motion.dart' show clamp01, easeOutCubic;
import 'script.dart';

/// The film's single source of time, in seconds. The render harness sets it;
/// nothing inside the film ever reads a wall clock.
class FilmClock extends ChangeNotifier {
  double _t = 0.0;
  double get t => _t;
  set t(double v) {
    if (v == _t) return;
    _t = v;
    notifyListeners();
  }
}

/// What every scene builds against.
class SceneCtx {
  /// This scene's local time, 0..1 across its duration.
  final double t;

  /// This scene's local time in seconds.
  final double sec;

  /// Absolute film time in seconds (for scene-bridging effects).
  final double abs;

  /// The scene's declared duration in seconds.
  final double seconds;

  const SceneCtx({
    required this.t,
    required this.sec,
    required this.abs,
    required this.seconds,
  });
}

typedef SceneBuilder = Widget Function(SceneCtx ctx);

/// One scene of the film.
class SceneSpec {
  final String id;
  final String name;
  final double seconds;
  final SceneBuilder build;

  /// Hard cut instead of the default fade-through-ground.
  final bool cut;

  const SceneSpec({
    required this.id,
    required this.name,
    required this.seconds,
    required this.build,
    this.cut = false,
  });
}

/// The root widget: renders the film at the clock's current time.
class FilmRoot extends StatelessWidget {
  final FilmClock clock;
  const FilmRoot({super.key, required this.clock});

  @override
  Widget build(BuildContext context) {
    return ListenableBuilder(
      listenable: clock,
      builder: (context, _) {
        final t = clock.t;
        final hit = sceneAt(t);
        if (hit == null) {
          // After the last scene: the closing hold — the endcard's ground,
          // kept so the film never ends on a flash of un-painted black.
          return const ColoredBox(color: filmEndGround);
        }
        final (scene, start, index) = hit;
        final sec = t - start;
        final local = scene.seconds <= 0 ? 1.0 : clamp(sec / scene.seconds, 0.0, 1.0);
        final ctx = SceneCtx(t: local, sec: sec, abs: t, seconds: scene.seconds);

        // Scene envelope: fade up over the first 0.45 s, down over the last
        // 0.35 s — a dip through the ground rather than a cross-fade, so
        // scenes read as cards being dealt rather than smeared together.
        // Hard-cut scenes (the splash, the studio act) skip the envelope.
        double opacity = 1.0;
        if (!scene.cut) {
          final fadeIn = easeOutCubic(clamp(sec / 0.45, 0.0, 1.0));
          final fadeOut = easeOutCubic(clamp((scene.seconds - sec) / 0.35, 0.0, 1.0));
          opacity = fadeIn * fadeOut;
        }

        final body = KeyedSubtree(
          key: ValueKey('scene_${scene.id}_$index'),
          child: scene.build(ctx),
        );

        return Stack(children: [
          const Positioned.fill(child: ColoredBox(color: kFilmBg)),
          Positioned.fill(
            child: opacity >= 0.999
                ? body
                : Opacity(opacity: opacity.clamp(0.0, 1.0), child: body),
          ),
          const Positioned.fill(child: Vignette()),
        ]);
      },
    );
  }
}

/// The film's ground — the lab canvas colour, warmed a step toward the
/// studio's window so acts feel lit by the same room.
const kFilmBg = Color(0xFF0B0C0F);

/// The end hold's ground — the endcard's own deep black.
const filmEndGround = Color(0xFF0A0A0C);

/// A quiet radial vignette: one paint, no allocation per frame.
class Vignette extends StatelessWidget {
  const Vignette({super.key});

  @override
  Widget build(BuildContext context) {
    return const DecoratedBox(
      decoration: BoxDecoration(
        gradient: RadialGradient(
          center: Alignment(0.0, -0.05),
          radius: 1.35,
          colors: [Color(0x00000000), Color(0x00000000), Color(0x3D000000)],
          stops: [0.0, 0.62, 1.0],
        ),
      ),
    );
  }
}

/// The full film length in seconds — the sum of the scene table.
double filmDuration() {
  var total = 0.0;
  for (final s in filmScenes) {
    total += s.seconds;
  }
  return total;
}

/// The scene playing at film time `t`, with its start offset and index.
(SceneSpec, double, int)? sceneAt(double t) {
  var start = 0.0;
  for (var i = 0; i < filmScenes.length; i++) {
    final s = filmScenes[i];
    if (t < start + s.seconds) {
      return (s, start, i);
    }
    start += s.seconds;
  }
  return null;
}

double clamp(double v, double lo, double hi) => v < lo ? lo : (v > hi ? hi : v);
