/// S13 · build & run — the picker is real, the toolchain is named, and the
/// build refuses before it fails.
///
/// "with a toolchain checklist that names the missing tool *and its install
/// command*; an export planner that refuses (rather than fails) when a
/// toolchain is absent." The .apk size is the CI size budget, a receipt.
library;

import 'package:flutter/widgets.dart';

import '../../film.dart';
import '../../motion/vieww_motion.dart';
import '../../theme.dart' as t;
import '../../widgets/primitives.dart';
import '../../widgets/studio/device_frame.dart';
import '../../widgets/studio/preview_screens.dart';
import 'common.dart';

Widget build(SceneCtx ctx) {
  final s = ctx.sec;
  // The platform cycle: iOS -> Android -> Desktop, ~1.2 s each.
  final platformIdx = s < 1.4 ? 0 : (s < 2.6 ? 1 : 2);
  final platform = Platform.values[platformIdx];
  final swapPops = [
    VSpring(SpringSpec.expressive).position(s - 0.2),
    VSpring(SpringSpec.expressive).position(s - 1.4),
    VSpring(SpringSpec.expressive).position(s - 2.6),
  ];
  final devicePop = swapPops[platformIdx].clamp(0.0, 1.0);

  // The build panel.
  final panelIn = easeOutCubic(window(s, 3.6, 1.0));
  final rowStarts = staggerStarts(4, 0.55, offset: 4.4);
  final apkIn = easeOutCubic(window(s, 8.4, 1.2));
  final cap = easeOutCubic(window(s, 2.0, 1.2));
  final endFade = easeOutCubic(window(ctx.seconds - s, 0.5, 0.5));

  return sceneFrame([
    // The device, cycling platforms.
    Positioned(
      left: 240,
      top: 210,
      child: Opacity(
        opacity: endFade,
        child: SizedBox(
          width: 480,
          height: 660,
          child: Transform.scale(
            scale: 0.94 + 0.06 * devicePop,
            child: platform == Platform.desktop
                ? const DeviceFrame(platform: Platform.desktop, screen: CounterScreen(count: 1))
                : DeviceFrame(platform: platform, screen: const CounterScreen(count: 1)),
          ),
        ),
      ),
    ),
    // Platform labels riding the cycle.
    Positioned(
      left: 240,
      top: 930,
      child: Opacity(
        opacity: endFade,
        child: SizedBox(
          width: 480,
          child: Row(children: [
            _platformLabel('iOS · 393x852', platformIdx == 0),
            const SizedBox(width: 10),
            _platformLabel('Android · 412x915', platformIdx == 1),
            const SizedBox(width: 10),
            _platformLabel('Desktop · 1280x800', platformIdx == 2),
          ]),
        ),
      ),
    ),
    // The build panel.
    Positioned(
      left: 860,
      top: 240,
      child: Opacity(
        opacity: panelIn * endFade,
        child: Pane(
          color: t.kChrome1,
          elevation: 2,
          padding: const EdgeInsets.all(26),
          child: SizedBox(
            width: 760,
            child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
              const Mono('BUILD AND RUN — ANDROID', 13, t.kOnSurface, weight: FontWeight.w500, letterSpacing: 1.2),
              const SizedBox(height: 20),
              _CheckRow(text: 'rustc 1.95 · the same one the studio runs', start: rowStarts[0], s: s),
              _CheckRow(text: 'gradle · harness from your scaffold', start: rowStarts[1], s: s),
              _CheckRow(text: 'android sdk · 34', start: rowStarts[2], s: s),
              _RefuseRow(text: 'adb — not found', sub: 'install: apt install adb', start: rowStarts[3], s: s),
              const SizedBox(height: 24),
              // The result: named, sized, receipted.
              Opacity(
                opacity: apkIn,
                child: Pane(
                  color: t.kChrome2,
                  padding: const EdgeInsets.symmetric(horizontal: 18, vertical: 14),
                  child: Row(children: [
                    const Mono('counter-0.1.0.apk', 14, t.kOnSurface, weight: FontWeight.w500),
                    const Spacer(),
                    Mono('593,128 bytes', 14, t.kSuccess),
                    const SizedBox(width: 12),
                    const Mono('debug-signed', 12, t.kReceipt),
                  ]),
                ),
              ),
              const SizedBox(height: 16),
              const Mono('refusal before start — the missing tool is named, never a linker error at 40 seconds', 13, t.kFilmFaint),
            ]),
          ),
        ),
      ),
    ),
    caption(ctx, 2.0, 'to a real phone. refusal before failure.', end: 12.2),
    receipt(ctx, 5.2, 'size budget · ci/check/size-budget.txt'),
  ]);
}

Widget _platformLabel(String text, bool active) => Container(
      padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 6),
      decoration: BoxDecoration(
        color: active ? t.kAccentWash : t.colorOver(const Color(0x14FFFFFF), t.kChrome1),
        borderRadius: BorderRadius.circular(6),
        border: Border.all(color: active ? t.kAccent : t.kLine),
      ),
      child: Mono(text, 12, active ? t.kOnSurface : t.kGutter),
    );

class _CheckRow extends StatelessWidget {
  final String text;
  final double start;
  final double s;
  const _CheckRow({required this.text, required this.start, required this.s});

  @override
  Widget build(BuildContext context) {
    final p = easeOutCubic(clamp01((s - start) / 0.6));
    return Padding(
      padding: const EdgeInsets.only(bottom: 12),
      child: Opacity(
        opacity: p,
        child: Row(children: [
          Transform.scale(scale: p, child: const CheckGlyph(size: 14)),
          const SizedBox(width: 12),
          Mono(text, 14, t.kOnSurfaceVariant),
        ]),
      ),
    );
  }
}

/// The refusal row — warning colour, the tool, and how to install it.
class _RefuseRow extends StatelessWidget {
  final String text;
  final String sub;
  final double start;
  final double s;
  const _RefuseRow({required this.text, required this.sub, required this.start, required this.s});

  @override
  Widget build(BuildContext context) {
    final p = easeOutCubic(clamp01((s - start) / 0.6));
    return Padding(
      padding: const EdgeInsets.only(bottom: 4),
      child: Opacity(
        opacity: p,
        child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
          Row(children: [
            const Mono('—', 14, t.kWarning, weight: FontWeight.w700),
            const SizedBox(width: 14),
            Mono(text, 14, t.kWarning),
          ]),
          Padding(
            padding: const EdgeInsets.only(left: 27, top: 3),
            child: Mono(sub, 12.5, t.kWarning.withValues(alpha: 0.7)),
          ),
        ]),
      ),
    );
  }
}
