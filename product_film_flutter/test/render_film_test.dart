/// The film's render harness: walks the whole film, frame by frame, at a
/// fixed clock — and pipes every frame straight into ffmpeg as raw RGBA.
/// No PNGs on disk, no wall clock, no dropped frames.
///
///   flutter test test/render_film_test.dart        # the whole film
///   FILM_SCENE=s09  ...                            # one scene, for review
///   FILM_T0=48 FILM_T1=89 ...                      # a range chunk, half-open
///   FILM_FPS=30 ...                                # half the frames
///
/// Range chunks are half-open [t0, t1): the frame at t1 belongs to the next
/// chunk, so concatenated chunks reproduce the single-pass stream exactly —
/// no duplicated frames at the seams. The one exception is the final chunk
/// ending at `filmDuration()`, which adds the closing-hold frame at exactly
/// t = duration so the film ends on the endcard's ground, not a flash.
///
/// Real I/O (the ffmpeg process, `toImage`, `toByteData`) runs inside
/// `tester.runAsync`, the escape hatch for futures that need the real
/// event loop; the frame clock itself stays in the test's fake time, so
/// every frame is still `film(t)` at exactly the t the clock says — the
/// vieww-animation doctrine this film inherits: sample the animation at
/// the timestamp, never integrate by delta.
library;

import 'dart:io';
import 'dart:typed_data';
import 'dart:ui' as ui;

import 'package:flutter/rendering.dart';
import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:product_film/film.dart';
import 'package:product_film/script.dart';

@Timeout(Duration(hours: 6))
void main() {
  final fps = int.tryParse(Platform.environment['FILM_FPS'] ?? '') ?? 60;
  final sceneFilter = Platform.environment['FILM_SCENE'];
  final rangeT0 = double.tryParse(Platform.environment['FILM_T0'] ?? '');
  final rangeT1 = double.tryParse(Platform.environment['FILM_T1'] ?? '');
  final maxFrames = int.tryParse(Platform.environment['FILM_MAX_FRAMES'] ?? '') ?? 0;
  final outPath = Platform.environment['FILM_OUT'] ?? '/home/z/my-project/output/viewwstudio_product_film.mp4';
  final crf = Platform.environment['FILM_CRF'] ?? '19';

  testWidgets('renders the film to $outPath at $fps fps', (tester) async {
    tester.view.physicalSize = const Size(1920.0, 1080.0);
    tester.view.devicePixelRatio = 1.0;

    // Register the brand fonts by hand: in a widget test the engine's
    // default family is Ahem, and the pubspec's fonts only reach the
    // rasteriser through a FontLoader. Geist and GeistMono are the exact
    // faces viewwsite ships.
    await tester.runAsync(() async {
      Future<ByteData> read(String path) async {
        final bytes = await File(path).readAsBytes();
        return ByteData.view(Uint8List.fromList(bytes).buffer);
      }

      final geist = FontLoader('Geist')
        ..addFont(read('assets/fonts/Geist-Regular.ttf'))
        ..addFont(read('assets/fonts/Geist-Medium.ttf'))
        ..addFont(read('assets/fonts/Geist-Bold.ttf'));
      final mono = FontLoader('GeistMono')
        ..addFont(read('assets/fonts/GeistMono-Regular.ttf'))
        ..addFont(read('assets/fonts/GeistMono-Medium.ttf'));
      await geist.load();
      await mono.load();
    });

    // Resolve the range being rendered: one scene, a [t0, t1) chunk, or the
    // whole film.
    final (startSec, endSec, label, endInclusive) = _range(sceneFilter, rangeT0, rangeT1);
    var totalFrames = endInclusive
        ? ((endSec - startSec) * fps).ceil() + 1
        : ((endSec - startSec) * fps).ceil();
    final frames = maxFrames > 0 && maxFrames < totalFrames ? maxFrames : totalFrames;

    final clock = FilmClock();
    final canvasKey = GlobalKey(debugLabel: 'film_canvas');
    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: MediaQuery(
          data: const MediaQueryData(size: Size(1920.0, 1080.0)),
          child: RepaintBoundary(
            key: canvasKey,
            child: SizedBox(
              width: 1920.0,
              height: 1080.0,
              child: _FilmHost(clock: clock),
            ),
          ),
        ),
      ),
    );

    final context = canvasKey.currentContext;
    final ro = context?.findRenderObject();
    if (ro is! RenderRepaintBoundary) {
      fail('film_canvas RepaintBoundary not found in the attached tree');
    }
    final canvas = ro;

    // ffmpeg reads raw RGBA on stdin and writes the MP4.
    final outDir = Directory(File(outPath).parent.path);
    if (!outDir.existsSync()) outDir.createSync(recursive: true);

    final ffmpeg = await tester.runAsync(() => Process.start('ffmpeg', [
          '-y',
          '-loglevel', 'error',
          '-f', 'rawvideo',
          '-pix_fmt', 'rgba',
          '-s', '1920x1080',
          '-framerate', '$fps',
          '-i', '-',
          '-c:v', 'libx264',
          '-preset', 'veryfast',
          '-crf', crf,
          '-pix_fmt', 'yuv420p',
          '-movflags', '+faststart',
          outPath,
        ]));
    if (ffmpeg == null) {
      fail('ffmpeg could not be started');
    }

    // ignore: avoid_print
    print('$label: $frames frames, ${(endSec - startSec).toStringAsFixed(1)} s at $fps fps -> $outPath');

    final sw = Stopwatch()..start();
    var encoded = 0;

    for (var i = 0; i < frames; i++) {
      final t = startSec + i / fps;
      if (t > endSec) break;
      clock.t = t;
      await tester.pump();

      final ok = await tester.runAsync(() async {
        final image = await canvas.toImage(pixelRatio: 1.0);
        final data = await image.toByteData(format: ui.ImageByteFormat.rawRgba);
        image.dispose();
        if (data == null) return false;
        ffmpeg.stdin.add(data.buffer.asUint8List());
        return true;
      });
      if (ok != true) {
        await tester.runAsync(() => ffmpeg.stdin.close());
        fail('frame $i produced no bytes');
      }
      encoded++;

      if (i % 60 == 0) {
        await tester.runAsync(() => ffmpeg.stdin.flush());
        // ignore: avoid_print
        print('$label frame $i/$frames  (${(sw.elapsed.inMilliseconds / 1000).toStringAsFixed(1)}s)');
      }
    }

    final code = await tester.runAsync(() async {
      await ffmpeg.stdin.flush();
      await ffmpeg.stdin.close();
      return ffmpeg.exitCode;
    });
    sw.stop();

    // ignore: avoid_print
    print('$label done: $encoded frames in ${(sw.elapsed.inMilliseconds / 1000).toStringAsFixed(1)}s '
        '(${(encoded / (sw.elapsed.inMilliseconds / 1000)).toStringAsFixed(1)} fps capture)');

    expect(code, 0, reason: 'ffmpeg exited with $code');
    expect(encoded, frames, reason: 'rendered $encoded of $frames frames');
  });
}

/// Rebuilds the film on every clock tick — one listenable, one rebuild,
/// the same shape vieww's films use (`fn(t) -> WidgetNode`).
class _FilmHost extends StatelessWidget {
  final FilmClock clock;
  const _FilmHost({required this.clock});

  @override
  Widget build(BuildContext context) {
    return ListenableBuilder(listenable: clock, builder: (context, _) => FilmRoot(clock: clock));
  }
}

/// Which slice of the film to render. A scene filter keeps the probe
/// behaviour (both endpoints included); a [t0, t1) range is a concat chunk.
(double, double, String, bool) _range(String? sceneFilter, double? t0, double? t1) {
  if (t0 != null || t1 != null) {
    final start = t0 ?? 0.0;
    final end = t1 ?? filmDuration();
    if (start < 0 || end <= start || end > filmDuration() + 1e-9) {
      fail('bad range [$start, $end) for a ${filmDuration()}s film');
    }
    // The final chunk owns the closing-hold frame at exactly t = duration.
    final ownsEnd = (end - filmDuration()).abs() < 1e-9;
    return (start, end, 'film[${_fmt(start)},${_fmt(end)}${ownsEnd ? ']' : ')'}]', ownsEnd);
  }
  if (sceneFilter == null || sceneFilter.isEmpty) {
    return (0.0, filmDuration(), 'film', true);
  }
  var start = 0.0;
  for (final s in filmScenes) {
    if (s.id == sceneFilter || s.name == sceneFilter) {
      return (start, start + s.seconds, s.id, true);
    }
    start += s.seconds;
  }
  fail('no scene "$sceneFilter" in the script');
}

String _fmt(double v) => v == v.truncateToDouble() ? v.toInt().toString() : v.toStringAsFixed(3);
