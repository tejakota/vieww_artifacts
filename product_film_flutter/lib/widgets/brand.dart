/// The viewwstudio mark — `apps/viewwstudio/src/ui/brand.rs`, drawn rather
/// than embedded, exactly as the studio does it:
///
///   "Two overlapping rounded panels on a dark ground — the editor and the
///    preview, which is what the application is. It reads at 16 pixels
///    because it is two shapes and one accent."
///
/// The geometry is the repo's fractions of a side:
///   ground  0.00 0.00 1.00 x 1.00   radius 0.22
///   editor  0.16 0.20 0.40 x 0.60   radius 0.08
///   preview 0.44 0.32 0.40 x 0.48   radius 0.08
library;

import 'package:flutter/widgets.dart';

import '../theme.dart' as t;

/// The mark, whole, `side` points square.
class BrandMark extends StatelessWidget {
  final double side;
  final double editor; // 0..1 reveal of the editor panel
  final double preview; // 0..1 reveal of the preview panel

  const BrandMark({super.key, required this.side, this.editor = 1.0, this.preview = 1.0});

  @override
  Widget build(BuildContext context) {
    return SizedBox(
      width: side,
      height: side,
      child: CustomPaint(painter: _MarkPainter(side: side, editor: editor.clamp(0.0, 1.0), preview: preview.clamp(0.0, 1.0))),
    );
  }
}

class _MarkPainter extends CustomPainter {
  final double side;
  final double editor;
  final double preview;

  _MarkPainter({required this.side, required this.editor, required this.preview});

  @override
  void paint(Canvas canvas, Size size) {
    // The ground: vertical ramp #24272E -> #14161A, rim of caught light
    // (white alpha 26, one point inside the edge), shadow underneath once
    // the mark is a real object (side >= 20).
    final groundRect = Rect.fromLTWH(0, 0, side, side);
    final groundRadius = side * t.kGroundRadiusFrac;

    // Exact ground shadow per brand.rs: black alpha 130, dy 0.05*side, blur
    // 0.14*side — only once the mark is a real object (side >= 20).
    if (side >= 20.0) {
      final shadowPaint = Paint()
        ..color = t.kMarkGroundShadow
        ..maskFilter = MaskFilter.blur(BlurStyle.normal, side * 0.14);
      canvas.drawRRect(
        RRect.fromRectAndRadius(groundRect.translate(0, side * 0.05), Radius.circular(groundRadius)),
        shadowPaint,
      );
    }

    canvas.drawRRect(
      RRect.fromRectAndRadius(groundRect, Radius.circular(groundRadius)),
      Paint()
        ..shader = LinearGradient(
          begin: Alignment.topCenter,
          end: Alignment.bottomCenter,
          colors: [t.kGroundHigh, t.kGround],
        ).createShader(groundRect),
    );

    // The rim: a filled ring, not a stroke — brand.rs's own argument for why
    // (GPU stroke rasterisers smear hairlines; a ring is the same one point
    // of ink drawn as geometry). Rebuilt here as an inner-path difference.
    final rimOuter = RRect.fromRectAndRadius(groundRect, Radius.circular(groundRadius));
    final rimInner = RRect.fromRectAndRadius(groundRect.deflate(1.0), Radius.circular((groundRadius - 1.0).clamp(0.0, groundRadius)));
    final rimPath = Path.combine(PathOperation.difference, Path()..addRRect(rimOuter), Path()..addRRect(rimInner));
    canvas.drawPath(rimPath, Paint()..color = const Color(0x1AFFFFFF));

    _panel(canvas, t.kEditorPanel, t.kPanel, editor, accent: false);
    _panel(canvas, t.kPreviewPanel, t.kAccent, preview, accent: true);
  }

  void _panel(Canvas canvas, (double, double, double, double) frac, Color color, double reveal, {required bool accent}) {
    if (reveal <= 0.001) return;
    final (fx, fy, fw, fh) = frac;
    var w = side * fw;
    var h = side * fh;
    var x = side * fx;
    var y = side * fy;
    // Scale about the panel's own centre, as `panel()` in brand.rs does.
    final scale = reveal;
    final cx = x + w / 2.0;
    final cy = y + h / 2.0;
    w *= scale;
    h *= scale;
    x = cx - w / 2.0;
    y = cy - h / 2.0;
    final rect = Rect.fromLTWH(x, y, w, h);
    final r = Radius.circular(side * t.kPanelRadiusFrac * scale.clamp(0.35, 1.0));

    if (accent) {
      // The preview panel is the one piece of colour: the accent ramp, and a
      // shadow under it once it has weight.
      if (reveal > 0.35) {
        canvas.drawRRect(
          RRect.fromRectAndRadius(rect.translate(0, side * 0.02), r),
          Paint()
            ..color = t.kPanelShadow
            ..maskFilter = MaskFilter.blur(BlurStyle.normal, side * 0.07 * reveal),
        );
      }
      canvas.drawRRect(
        RRect.fromRectAndRadius(rect, r),
        Paint()
          ..shader = LinearGradient(
            begin: Alignment.topCenter,
            end: Alignment.bottomCenter,
            colors: [t.kAccentFar, t.kAccent],
          ).createShader(rect),
      );
    } else {
      canvas.drawRRect(RRect.fromRectAndRadius(rect, r), Paint()..color = color);
    }
  }

  @override
  bool shouldRepaint(_MarkPainter old) =>
      old.side != side || old.editor != editor || old.preview != preview;
}
