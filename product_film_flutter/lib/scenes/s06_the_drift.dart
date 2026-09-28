/// S06 · the drift — the mockup and the build, two truths pulling apart.
///
/// No receipt needed; every maker knows this one. The two cards start as
/// one composition and spring apart, and the differences annotate
/// themselves: two pixels here, a button that lost its colour there.
library;

import 'package:flutter/widgets.dart';

import '../../film.dart';
import '../../motion/vieww_motion.dart';
import '../../theme.dart' as t;
import '../../widgets/primitives.dart';
import 'common.dart';

Widget build(SceneCtx ctx) {
  final apart = VSpring(SpringSpec(300.0, 0.78)).position(ctx.sec - 1.2);
  final desat = easeOutCubic(window(ctx.sec, 9.6, 2.0));
  final callout = easeOutCubic(window(ctx.sec, 3.6, 1.4));
  final cap = easeOutCubic(window(ctx.sec, 7.2, 1.2));
  final endFade = easeOutCubic(window(ctx.seconds - ctx.sec, 0.5, 0.5));

  final sep = apart * 460.0;
  final lx = W / 2 - 190 - sep / 2;
  final rx = W / 2 - 190 + sep / 2;
  final tilt = apart * 0.035;

  return sceneFrame([
    // The mockup.
    Positioned(
      left: lx,
      top: 330,
      child: Opacity(
        opacity: (1.0 - desat * 0.7) * endFade,
        child: Transform.rotate(angle: -tilt, child: _ScreenCard(mock: true)),
      ),
    ),
    // The build.
    Positioned(
      left: rx,
      top: 342,
      child: Opacity(
        opacity: (1.0 - desat * 0.7) * endFade,
        child: Transform.rotate(angle: tilt, child: _ScreenCard(mock: false)),
      ),
    ),
    // The callouts between them.
    if (callout > 0.001)
      Positioned(
        left: 0,
        right: 0,
        top: 320,
        child: Opacity(
          opacity: callout * (1.0 - desat * 0.6) * endFade,
          child: _Callouts(apart: apart),
        ),
      ),
    // Labels.
    Positioned(left: lx + 40, top: 770, child: Opacity(opacity: endFade * (1 - desat), child: const Mono('the mockup', 15, t.kFilmMuted))),
    Positioned(left: rx + 40, top: 782, child: Opacity(opacity: endFade * (1 - desat), child: const Mono('the build', 15, t.kFilmFaint))),
    caption(ctx, 7.2, 'the picture and the code drift. nobody notices which lied first.'),
  ]);
}

class _ScreenCard extends StatelessWidget {
  final bool mock;
  const _ScreenCard({required this.mock});

  @override
  Widget build(BuildContext context) {
    return Pane(
      color: t.kChrome2,
      elevation: 2,
      padding: const EdgeInsets.all(16),
      child: SizedBox(
        width: 340,
        child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
          Mono(mock ? 'counter — designed' : 'counter — built', 11, t.kGutter),
          const SizedBox(height: 14),
          Voice('Counter', 22, t.kOnSurface, weight: FontWeight.w700),
          const SizedBox(height: 8),
          Mono('Tapped 1 times', 14, t.kOnSurfaceVariant),
          const SizedBox(height: 18),
          // The button: designed accent vs built grey — the drift, in one
          // control.
          Container(
            height: 42,
            margin: EdgeInsets.only(right: mock ? 0.0 : 8.0),
            decoration: BoxDecoration(
              color: mock ? t.kAccent : t.kChrome4,
              borderRadius: BorderRadius.circular(8),
            ),
            child: Center(
              child: Voice('Add one', 14, mock ? const Color(0xFFFFFFFF) : t.kOnSurfaceVariant),
            ),
          ),
        ]),
      ),
    );
  }
}

/// Hairline callouts: the 2 px misalignment and the wrong button.
class _Callouts extends StatelessWidget {
  final double apart;
  const _Callouts({required this.apart});

  @override
  Widget build(BuildContext context) {
    return CustomPaint(painter: _CalloutPainter(apart), size: const Size(W, 500));
  }
}

class _CalloutPainter extends CustomPainter {
  final double apart;
  _CalloutPainter(this.apart);

  @override
  void paint(Canvas canvas, Size size) {
    if (apart < 0.3) return;
    final line = Paint()
      ..color = t.kError.withValues(alpha: 0.5)
      ..strokeWidth = 1.2;
    // The 2px gap marker between the two cards' button rows.
    final x1 = size.width / 2 - 20;
    final x2 = size.width / 2 + 20;
    final y = 218.0;
    canvas.drawLine(Offset(x1, y - 6), Offset(x1, y + 6), line);
    canvas.drawLine(Offset(x2, y - 6), Offset(x2, y + 6), line);
    // A bracket between them.
    canvas.drawLine(Offset(x1, y), Offset(x2, y), line);
    final tp = TextPainter(
      text: const TextSpan(text: '2 px', style: TextStyle(fontFamily: 'GeistMono', fontSize: 12, color: t.kError)),
      textDirection: TextDirection.ltr,
    )..layout();
    tp.paint(canvas, Offset(size.width / 2 - tp.width / 2, y + 10));
  }

  @override
  bool shouldRepaint(_CalloutPainter old) => old.apart != apart;
}
