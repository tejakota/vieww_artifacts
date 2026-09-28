/// The screens the device frame previews — every one of them redrawn from
/// the studio's own `screens/` directory and the say fixture:
///
/// - `counter.say` -> a heading "Counter", a label "Tapped N times", a
///   filled "Add one" button.
/// - `card_grid.rs` -> Inbox / Drafts / Archive / Sent, `surface_variant`
///   cards, radius, 16 padding, 12 spacing.
/// - `settings_form.rs` -> Notifications switch, Volume slider, Done.
/// - `live.rs` -> the Live Preview flow: Home (Inbox rows), Settings
///   (toggle, counter), nav buttons between them.
library;

import 'package:flutter/widgets.dart';

import '../../theme.dart' as t;
import '../primitives.dart';

/// Shared scaffold: safe-area padding, surface, 16 padding, column spaced 12.
class ScreenScaffold extends StatelessWidget {
  final List<Widget> children;
  final double topInset;
  final double bottomInset;
  final double spacing;

  const ScreenScaffold({super.key, required this.children, this.topInset = 47, this.bottomInset = 34, this.spacing = 12});

  @override
  Widget build(BuildContext context) {
    return Container(
      color: t.kWindow,
      padding: EdgeInsets.fromLTRB(16, topInset + 8, 16, bottomInset + 8),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        mainAxisSize: MainAxisSize.max,
        mainAxisAlignment: MainAxisAlignment.start,
        children: [
          for (var i = 0; i < children.length; i++) ...[
            if (i > 0) SizedBox(height: spacing),
            children[i],
          ],
        ],
      ),
    );
  }
}

class ScreenHeading extends StatelessWidget {
  final String text;
  const ScreenHeading(this.text, {super.key});

  @override
  Widget build(BuildContext context) => Voice(text, 24, t.kOnSurface, weight: FontWeight.w700);
}

/// The counter — the film's protagonist screen, from `counter.say`.
class CounterScreen extends StatelessWidget {
  final int count;

  /// 0..1 press state of the Add-one button.
  final double pressed;

  /// Scale for the count label alone — the pop when a recompile lands.
  final double labelScale;

  const CounterScreen({super.key, required this.count, this.pressed = 0, this.labelScale = 1.0});

  @override
  Widget build(BuildContext context) {
    final scale = 1.0 - 0.04 * pressed.clamp(0.0, 1.0);
    return ScreenScaffold(
      spacing: 16,
      children: [
        const ScreenHeading('Counter'),
        Transform.scale(
          scale: labelScale.clamp(0.5, 1.6),
          child: Mono('Tapped $count times', 15, t.kOnSurfaceVariant),
        ),
        const SizedBox(height: 8),
        Transform.scale(
          scale: scale,
          child: FilledButton('Add one', pressed: pressed),
        ),
      ],
    );
  }
}

/// Filled button — the accent, white label, the studio's corner radius.
class FilledButton extends StatelessWidget {
  final String label;
  final double pressed;
  const FilledButton(this.label, {super.key, this.pressed = 0});

  @override
  Widget build(BuildContext context) {
    final p = pressed.clamp(0.0, 1.0);
    return Container(
      height: 44,
      decoration: BoxDecoration(
        color: t.colorOver(Color.fromRGBO(0, 0, 0, 0.18 * p), t.kAccent),
        borderRadius: BorderRadius.circular(t.kCardCorner),
        boxShadow: t.elevation(1),
      ),
      child: Center(
        child: Voice(label, 15, const Color(0xFFFFFFFF), weight: FontWeight.w500),
      ),
    );
  }
}

/// `card_grid.rs` — four cards on a surface.
class CardGridScreen extends StatelessWidget {
  const CardGridScreen({super.key});

  @override
  Widget build(BuildContext context) {
    const titles = ['Inbox', 'Drafts', 'Archive', 'Sent'];
    return ScreenScaffold(
      children: [
        const ScreenHeading('Screens'),
        for (final title in titles)
          Pane(
            color: t.kSurfaceVariant,
            radius: t.kCardCorner,
            padding: const EdgeInsets.all(16),
            child: Align(alignment: Alignment.centerLeft, child: Voice(title, 15, t.kOnSurface)),
          ),
      ],
    );
  }
}

/// `settings_form.rs` — switch row, slider row, Done.
class SettingsScreen extends StatelessWidget {
  final bool notifications;
  final double volume;
  const SettingsScreen({super.key, this.notifications = true, this.volume = 0.6});

  @override
  Widget build(BuildContext context) {
    return ScreenScaffold(
      children: [
        const ScreenHeading('Settings'),
        _row('Notifications', Switch(value: notifications)),
        _row('Volume', Slider(value: volume)),
        const Spacer(),
        const FilledButton('Done'),
      ],
    );
  }

  Widget _row(String label, Widget control) => Pane(
        color: t.kSurfaceVariant,
        padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 12),
        child: Row(
          children: [
            Expanded(child: Voice(label, 15, t.kOnSurface)),
            control,
          ],
        ),
      );
}

class Switch extends StatelessWidget {
  final bool value;
  const Switch({super.key, required this.value});

  @override
  Widget build(BuildContext context) {
    final on = value;
    return Container(
      width: 46,
      height: 28,
      decoration: BoxDecoration(
        color: on ? t.kAccent : t.kChrome4,
        borderRadius: BorderRadius.circular(14),
      ),
      child: Stack(children: [
        Positioned(
          right: on ? 3 : 21,
          top: 3,
          child: Container(
            width: 22,
            height: 22,
            decoration: const BoxDecoration(color: Color(0xFFFFFFFF), shape: BoxShape.circle),
          ),
        ),
      ]),
    );
  }
}

class Slider extends StatelessWidget {
  final double value;
  const Slider({super.key, required this.value});

  @override
  Widget build(BuildContext context) {
    return SizedBox(
      width: 120,
      child: SizedBox(
        height: 28,
        child: Stack(children: [
          Align(
            alignment: Alignment.centerLeft,
            child: Container(height: 4, decoration: BoxDecoration(color: t.kChrome4, borderRadius: BorderRadius.circular(2))),
          ),
          Align(
            alignment: Alignment(-1.0 + 2.0 * value.clamp(0.0, 1.0), 0.0),
            child: FractionallySizedBox(
              widthFactor: value.clamp(0.0, 1.0),
              child: Container(height: 4, decoration: BoxDecoration(color: t.kAccent, borderRadius: BorderRadius.circular(2))),
            ),
          ),
          Align(
            alignment: Alignment(-1.0 + 2.0 * value.clamp(0.0, 1.0), 0.0),
            child: Transform.translate(
              offset: const Offset(-8, 0),
              child: Container(
                width: 16,
                height: 16,
                decoration: BoxDecoration(color: t.kOnSurface, shape: BoxShape.circle, boxShadow: t.elevation(1)),
              ),
            ),
          ),
        ]),
      ),
    );
  }
}

/// `live.rs` Home — the Inbox rows and the nav button.
class LiveHomeScreen extends StatelessWidget {
  const LiveHomeScreen({super.key});

  @override
  Widget build(BuildContext context) {
    return ScreenScaffold(
      spacing: 10,
      children: [
        const ScreenHeading('Inbox'),
        _row('Design Review', '3 new comments on the card layout'),
        _row('Sprint Planning', '5 tickets moved to In Progress'),
        _row('Release Notes', 'Draft for v0.3 is ready to review'),
        const SizedBox(height: 6),
        const FilledButton('Open settings'),
      ],
    );
  }

  Widget _row(String title, String sub) => Pane(
        color: t.kSurfaceVariant,
        padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 12),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Voice(title, 14, t.kOnSurface, weight: FontWeight.w500),
            const SizedBox(height: 3),
            Voice(sub, 12, t.kOnSurfaceVariant),
          ],
        ),
      );
}

/// `live.rs` Settings — the sketch: toggle, counter, note, Done.
class LiveSettingsScreen extends StatelessWidget {
  const LiveSettingsScreen({super.key});

  @override
  Widget build(BuildContext context) {
    return ScreenScaffold(
      spacing: 10,
      children: [
        const ScreenHeading('Settings'),
        Pane(
          color: t.kSurfaceVariant,
          padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 12),
          child: Row(children: [
            const Expanded(child: Voice('Notifications', 14, t.kOnSurface)),
            Switch(value: true),
          ]),
        ),
        Pane(
          color: t.kSurfaceVariant,
          padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 12),
          child: Row(children: [
            const Expanded(child: Voice('Badge count', 14, t.kOnSurface)),
            Mono('12', 14, t.kOnSurface),
          ]),
        ),
        Voice('Changes here are a sketch, not state your app keeps.', 12, t.kOnSurfaceVariant),
        const SizedBox(height: 6),
        const FilledButton('Done'),
      ],
    );
  }
}
