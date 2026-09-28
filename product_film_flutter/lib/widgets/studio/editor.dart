/// The studio's editor: gutter, syntax-coloured lines, a caret on the
/// 530 ms blink, and a minimap — the shapes every studio screenshot is read
/// by. Colour roles are the VS Code Dark+ set the chrome publishes.
library;

import 'package:flutter/widgets.dart';

import '../../theme.dart' as t;
import '../primitives.dart';

enum Tok { kw, fn, ty, str, num, com, mac, pun, attr, plain }

Color tokColor(Tok tok) => switch (tok) {
      Tok.kw => t.kSyntaxKeyword,
      Tok.fn => t.kSyntaxFunction,
      Tok.ty => t.kSyntaxType,
      Tok.str => t.kSyntaxString,
      Tok.num => t.kSyntaxNumber,
      Tok.com => t.kSyntaxComment,
      Tok.mac => t.kSyntaxMacro,
      Tok.pun => t.kSyntaxPunctuation,
      Tok.attr => t.kSyntaxAttribute,
      Tok.plain => t.kOnSurface,
    };

class CodeLine {
  final List<(String, Tok)> tokens;
  const CodeLine(this.tokens);

  static const empty = CodeLine([(' ', Tok.plain)]);

  String get plainText => tokens.map((e) => e.$1).join();
  double get visualLength => plainText.length.toDouble();
}

/// The same program after the film's edit — "starting at 0" becomes
/// "starting at 1", the one-line change the whole loop scene turns on.
final kCounterSayEdit = List<CodeLine>.of(kCounterSay)..[2] = const CodeLine([
  ('keep', Tok.kw),
  (' a whole ', Tok.plain),
  ('number', Tok.ty),
  (' called count starting at ', Tok.plain),
  ('1', Tok.num),
]);

/// Tokenised `counter.say` — the studio's welcome program, verbatim from
/// the say-codegen fixture and the film workspace's copy of it.
const kCounterSay = <CodeLine>[
  CodeLine([('-- say-language: 1', Tok.com)]),
  CodeLine.empty,
  CodeLine([('keep', Tok.kw), (' a whole ', Tok.plain), ('number', Tok.ty), (' called count starting at ', Tok.plain), ('0', Tok.num)]),
  CodeLine.empty,
  CodeLine([('screen', Tok.kw), (' ', Tok.plain), ('"Home"', Tok.str), (':', Tok.pun)]),
  CodeLine([('    a column, spaced ', Tok.plain), ('16', Tok.num), (', children aligned to the start:', Tok.plain)]),
  CodeLine([('        a heading ', Tok.plain), ('"Counter"', Tok.str)]),
  CodeLine([('        a label ', Tok.plain), ('"Tapped \\(count) times"', Tok.str)]),
  CodeLine([('        a button ', Tok.plain), ('"Add one"', Tok.str), (' which when tapped:', Tok.plain)]),
  CodeLine([('            add ', Tok.kw), ('1', Tok.num), (' to count', Tok.plain)]),
];

/// The first lines of what Say generates — modelled on
/// `vieww-say-codegen/tests/fixtures/counter_gen.rs`, markers and all.
const kCounterGen = <CodeLine>[
  CodeLine([('// Generated from a `.say` file by Say — edit the `.say` file, not this.', Tok.com)]),
  CodeLine.empty,
  CodeLine([('use', Tok.kw), (' std::cell::', Tok.plain), ('RefCell', Tok.ty), (';', Tok.pun)]),
  CodeLine([('use', Tok.kw), (' vieww::', Tok.plain), ('prelude', Tok.ty), ('::*;', Tok.pun)]),
  CodeLine.empty,
  CodeLine([('#[derive(', Tok.attr), ('Debug', Tok.attr), (')]', Tok.attr)]),
  CodeLine([('struct', Tok.kw), (' Say', Tok.ty), ('State', Tok.ty), (' {', Tok.pun)]),
  CodeLine([('    // say: counter.say:3  keep count', Tok.com)]),
  CodeLine([('    count', Tok.fn), (': ', Tok.pun), ('i64', Tok.ty), (',', Tok.pun)]),
  CodeLine([('    dirty', Tok.fn), (': ', Tok.pun), ('bool', Tok.ty), (',', Tok.pun)]),
  CodeLine([('}', Tok.pun)]),
  CodeLine.empty,
  CodeLine([('pub fn', Tok.kw), (' screen', Tok.fn), ('() -> ', Tok.pun), ('impl Widget', Tok.ty), (' {', Tok.pun)]),
  CodeLine([('    // say: counter.say:6  a column, spaced 16', Tok.com)]),
  CodeLine([('    Column', Tok.ty), ('::', Tok.pun), ('new', Tok.fn), ('().', Tok.pun), ('spacing', Tok.fn), ('(', Tok.pun), ('16', Tok.num), (')', Tok.pun)]),
  CodeLine([('        .', Tok.pun), ('child', Tok.fn), ('(', Tok.pun), ('Heading', Tok.ty), ('::', Tok.pun), ('new', Tok.fn), ('(', Tok.pun), ('"Counter"', Tok.str), ('))', Tok.pun)]),
  CodeLine([('        .', Tok.pun), ('child', Tok.fn), ('(', Tok.pun), ('Button', Tok.ty), ('::', Tok.pun), ('new', Tok.fn), ('(', Tok.pun), ('"Add one"', Tok.str), (')', Tok.pun)]),
  CodeLine([('            .', Tok.pun), ('on_tap', Tok.fn), ('(move || count.set(count.get() + ', Tok.plain), ('1', Tok.num), (')))', Tok.pun)]),
  CodeLine([('}', Tok.pun)]),
];

/// `live.rs` — the Live Preview sketch, verbatim from `screens/live.rs`.
const kLiveRs = <CodeLine>[
  CodeLine([('live! ', Tok.mac), ('{', Tok.pun)]),
  CodeLine([('    screen', Tok.kw), (' ', Tok.plain), ('"Home"', Tok.str), (' {', Tok.pun)]),
  CodeLine([('        title', Tok.kw), (' ', Tok.plain), ('"Inbox"', Tok.str)]),
  CodeLine([('        row', Tok.kw), (' ', Tok.plain), ('"Design Review"', Tok.str), (' ', Tok.plain), ('"3 new comments on the card layout"', Tok.str)]),
  CodeLine([('        button', Tok.kw), (' ', Tok.plain), ('"Open settings"', Tok.str), (' -> ', Tok.pun), ('"Settings"', Tok.str)]),
  CodeLine([('    }', Tok.pun)]),
  CodeLine.empty,
  CodeLine([('    screen', Tok.kw), (' ', Tok.plain), ('"Settings"', Tok.str), (' {', Tok.pun)]),
  CodeLine([('        toggle', Tok.kw), (' ', Tok.plain), ('"Notifications"', Tok.str)]),
  CodeLine([('        counter', Tok.kw), (' ', Tok.plain), ('"Badge count"', Tok.str)]),
  CodeLine([('        button', Tok.kw), (' ', Tok.plain), ('"Done"', Tok.str), (' -> ', Tok.pun), ('"Home"', Tok.str)]),
  CodeLine([('    }', Tok.pun)]),
  CodeLine([('    nav', Tok.kw), (' ', Tok.plain), ('"Home" "Settings"', Tok.str)]),
  CodeLine([('}', Tok.pun)]),
];

/// The editor pane: tab strip, gutter, code, caret, minimap.
class EditorPane extends StatelessWidget {
  final List<CodeLine> lines;

  /// How many characters of the whole buffer are revealed (typing).
  final double typed;

  /// 0-based line the caret sits on.
  final int caretLine;

  /// Film time, for the caret blink.
  final double time;

  /// Active tab label.
  final String activeTab;

  /// Inactive tabs.
  final List<String> otherTabs;

  const EditorPane({
    super.key,
    required this.lines,
    this.typed = 1.0,
    this.caretLine = 0,
    required this.time,
    this.activeTab = 'counter.say',
    this.otherTabs = const ['card_grid.rs', 'live.rs'],
  });

  @override
  Widget build(BuildContext context) {
    return Pane(
      color: t.kChrome2,
      child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
        _tabStrip(),
        const Hairline(t.kLine),
        Expanded(child: _codeArea()),
      ]),
    );
  }

  Widget _tabStrip() {
    return Container(
      height: 36,
      color: t.kChrome1,
      child: Row(children: [
        _tab(activeTab, true),
        for (final name in otherTabs) _tab(name, false),
        const SizedBox(width: 8),
        Mono('+', 14, t.kGutter),
        const Spacer(),
      ]),
    );
  }

  Widget _tab(String label, bool active) {
    return Container(
      height: 36,
      padding: const EdgeInsets.symmetric(horizontal: 14),
      decoration: BoxDecoration(
        color: active ? t.kChrome3 : t.kChrome1,
        border: Border(top: BorderSide(color: active ? t.kAccent : const Color(0x00000000), width: 2.0)),
      ),
      child: Center(child: Mono(label, 12, active ? t.kOnSurface : t.kGutter)),
    );
  }

  Widget _codeArea() {
    const lineH = 20.0;
    const fontPx = 13.0;
    var budget = typed * _totalChars();

    return Stack(children: [
      Positioned.fill(
        child: Row(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
          // Gutter.
          SizedBox(
            width: 46,
            child: Padding(
              padding: const EdgeInsets.only(top: 10, right: 8),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.end,
                children: [
                  for (var i = 0; i < lines.length; i++)
                    SizedBox(
                      height: lineH,
                      child: Mono('${i + 1}', 10, i == caretLine ? t.kGutterActive : t.kGutter),
                    ),
                ],
              ),
            ),
          ),
          // Code.
          Expanded(
            child: Padding(
              padding: const EdgeInsets.fromLTRB(2, 10, 0, 10),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  for (var i = 0; i < lines.length; i++)
                    SizedBox(
                      height: lineH,
                      child: _line(lines[i], budget),
                    ),
                ],
              ),
            ),
          ),
          // Minimap: one faint bar per line, viewport block.
          SizedBox(
            width: 84,
            child: Padding(
              padding: const EdgeInsets.symmetric(vertical: 12, horizontal: 10),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  for (var i = 0; i < lines.length; i++)
                    Container(
                      margin: const EdgeInsets.only(bottom: 4),
                      width: (lines[i].visualLength * 1.9).clamp(4.0, 56.0),
                      height: 2,
                      color: i == caretLine ? t.kGutterActive : t.kOnSurface.withValues(alpha: 0.10),
                    ),
                ],
              ),
            ),
          ),
        ]),
      ),
      // The caret, on its line, after the revealed text.
      Positioned(
        left: 50 + 8.0 * _caretCol(budget),
        top: 10 + caretLine * lineH + 3,
        child: Caret(time: time, height: fontPx + 3, width: 2),
      ),
    ]);
  }

  Widget _line(CodeLine line, double budget) {
    final children = <Widget>[];
    for (final (text, tok) in line.tokens) {
      if (budget <= 0) break;
      final n = (budget.clamp(0.0, text.length.toDouble())).round();
      if (n > 0) {
        children.add(Text(
          text.substring(0, n),
          style: TextStyle(
            fontFamily: 'GeistMono',
            fontSize: 13,
            color: tokColor(tok),
            height: 1.0,
            letterSpacing: 0.4,
          ),
        ));
      }
      budget -= text.length;
    }
    return Row(mainAxisSize: MainAxisSize.min, crossAxisAlignment: CrossAxisAlignment.center, children: children);
  }

  int _totalChars() => lines.fold(0, (sum, l) => sum + l.plainText.length);

  int _caretCol(double budget) {
    // The caret rides the typing position: chars revealed on the caret's line.
    var remaining = typed >= 1.0 ? _totalChars().toDouble() : budget;
    for (var i = 0; i <= caretLine && i < lines.length; i++) {
      final len = lines[i].plainText.length;
      if (remaining <= len) return remaining.floor();
      remaining -= len;
    }
    return lines.isEmpty ? 0 : lines[caretLine].plainText.length - 1;
  }
}
