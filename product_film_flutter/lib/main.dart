import 'package:flutter/scheduler.dart';
import 'package:flutter/widgets.dart';

import 'film.dart';

/// Interactive preview of the film — `flutter run -d linux` plays it at
/// wall-clock speed in a window. The renderer never uses this; the harness
/// in `test/render_film_test.dart` drives the same tree frame by frame.
void main() {
  runApp(const Directionality(textDirection: TextDirection.ltr, child: _FilmPlayer()));
}

class _FilmPlayer extends StatefulWidget {
  const _FilmPlayer();

  @override
  State<_FilmPlayer> createState() => _FilmPlayerState();
}

class _FilmPlayerState extends State<_FilmPlayer> with SingleTickerProviderStateMixin {
  final FilmClock _clock = FilmClock();
  late final Ticker _ticker;
  Duration _last = Duration.zero;

  @override
  void initState() {
    super.initState();
    _ticker = createTicker((elapsed) {
      final dt = (elapsed - _last).inMicroseconds / 1e6;
      _last = elapsed;
      _clock.t = (_clock.t + dt) % filmDuration();
    });
    _ticker.start();
  }

  @override
  void dispose() {
    _ticker.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return LayoutBuilder(builder: (context, constraints) {
      final scale = constraints.biggest.width / 1920.0;
      return ClipRect(
        child: Transform.scale(
          scale: scale,
          alignment: Alignment.topLeft,
          child: SizedBox(width: 1920, height: 1080, child: FilmRoot(clock: _clock)),
        ),
      );
    });
  }
}
