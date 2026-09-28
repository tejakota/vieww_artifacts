/// The film's palette — read from the viewwstudio source tree, not invented.
///
/// Every constant below cites the file it was lifted from. Where the studio
/// has two themes the dark one is used: the film is a dark room.
library;

import 'package:flutter/painting.dart';

/// ---------------------------------------------------------------------------
/// viewwstudio chrome — `apps/viewwstudio/src/theme.rs`, `StudioTheme::dark()`.
/// ---------------------------------------------------------------------------
const kWindow = Color(0xFF0E0E0E);
const kChrome0 = Color(0xFF181818); // activity + status bars
const kChrome1 = Color(0xFF1A1A1A); // sidebar / panels
const kChrome2 = Color(0xFF1F1F1F); // editor surface
const kChrome3 = Color(0xFF262626); // raised: active tab, popover
const kChrome4 = Color(0xFF2F2F2F); // hover
const kLine = Color(0xFF2B2B2B);
const kGutter = Color(0xFF82898F);
const kGutterActive = Color(0xFFCCCCCC);
const kWarning = Color(0xFFCCA700);

/// `ColorScheme` overrides the dark chrome publishes (`theme.rs`).
const kOnSurface = Color(0xFFCCCCCC);
const kOnSurfaceVariant = Color(0xFF9D9D9D);
const kOutline = Color(0xFF7A7A7A);
const kSurfaceVariant = Color(0xFF2D2D2D);
const kError = Color(0xFFF85149);
const kSuccess = Color(0xFF3FB950);

/// The studio's card shell — `theme.rs` chrome shape.
const kCardCorner = 8.0;
const kCardGap = 6.0;

/// Sheen: a white veil (alpha 9/255) fading out by 34% of the height.
/// `StudioTheme::sheen`.
List<Color> sheenStops(Color base) {
  const veil = Color(0x09FFFFFF);
  return [colorOver(veil, base), base, base];
}

/// Deep sheen for tall regions — veil 14, shade 46, knees at 0.28.
List<Color> deepSheenStops(Color base) {
  const veil = Color(0x0EFFFFFF);
  const shade = Color(0x2E000000);
  return [colorOver(veil, base), base, colorOver(shade, base)];
}

/// `StudioTheme::rim` — white alpha 20 *over* the card's own background,
/// pre-composited to an opaque colour so every backend draws it identically.
Color rim(Color background) => colorOver(const Color(0x14FFFFFF), background);

/// `Color::over` — straight-alpha source-over, done in f32 like the repo.
Color colorOver(Color src, Color dst) {
  final sa = src.alpha / 255.0;
  final ia = 1.0 - sa;
  final r = (src.r * sa + dst.r * ia).round();
  final g = (src.g * sa + dst.g * ia).round();
  final b = (src.b * sa + dst.b * ia).round();
  final a = (src.a * sa + dst.a * ia).round();
  return Color.fromARGB(a, r, g, b);
}

/// `StudioTheme::elevation` — the 1..4 shadow ramp.
/// alpha = 46 + 30L (dark), offset y = 1.6L, blur = L^2*1.6 + 2.
List<BoxShadow> elevation(int level) {
  final l = level.toDouble();
  final alpha = ((46.0 + l * 30.0).clamp(0.0, 190.0) / 255.0);
  return [
    BoxShadow(
      color: Color.fromRGBO(0, 0, 0, alpha),
      offset: Offset(0, l * 1.6),
      blurRadius: l * l * 1.6 + 2.0,
    ),
  ];
}

/// ---------------------------------------------------------------------------
/// Syntax — the VS Code Dark+ set the editor ships (`theme.rs`).
/// ---------------------------------------------------------------------------
const kSyntaxKeyword = Color(0xFF569CD6);
const kSyntaxFunction = Color(0xFFDCDCAA);
const kSyntaxType = Color(0xFF4EC9B0);
const kSyntaxString = Color(0xFFCE9178);
const kSyntaxNumber = Color(0xFFB5CEA8);
const kSyntaxComment = Color(0xFF6A9955);
const kSyntaxMacro = Color(0xFFC586C0);
const kSyntaxPunctuation = Color(0xFFD4D4D4);
const kSyntaxAttribute = Color(0xFF9CDCFE);

/// ---------------------------------------------------------------------------
/// The brand accent — `theme.rs::PURPLE`, the default `ACCENT`.
/// ---------------------------------------------------------------------------
const kAccent = Color(0xFF7E5CE8); // dark_near — "holds 4.7:1 in the dark theme"
const kAccentFar = Color(0xFFB491FF); // dark_far
const kAccentWash = Color(0x3E7E5CE8); // dark wash, alpha 0x3E

/// All eight accents, for the Tokens-view montage (`theme.rs::ACCENTS`).
const kAccents = <Color>[
  Color(0xFF7E5CE8), // purple
  Color(0xFF0078D4), // blue
  Color(0xFF0E8174), // teal
  Color(0xFF2E7D32), // green
  Color(0xFFA05F08), // amber
  Color(0xFFC2410C), // orange
  Color(0xFFC02853), // rose
  Color(0xFFA5309E), // magenta
];

/// The accent ramp is diagonal (0,0)->(1,1) in the studio; a vertical
/// near->far ramp is what the mark's preview panel uses (`brand.rs`).
const kAccentRamp = LinearGradient(
  begin: Alignment(0, -1),
  end: Alignment(0, 1),
  colors: [kAccentFar, kAccent],
);

/// ---------------------------------------------------------------------------
/// The mark — `apps/viewwstudio/src/ui/brand.rs`.
/// ---------------------------------------------------------------------------
const kGround = Color(0xFF14161A); // GROUND
const kGroundHigh = Color(0xFF24272E); // top of the ground's vertical ramp
const kPanel = Color(0xFF464E5E); // the editor panel, PANEL
const kMarkGroundShadow = Color(0x82000000); // black alpha 130
const kPanelShadow = Color(0x5A000000); // black alpha 90

/// Panel rectangles as fractions of the mark's side — `brand.rs`.
const kEditorPanel = (0.16, 0.20, 0.40, 0.60);
const kPreviewPanel = (0.44, 0.32, 0.40, 0.48);
const kGroundRadiusFrac = 0.22;
const kPanelRadiusFrac = 0.08;

/// ---------------------------------------------------------------------------
/// The splash — `apps/viewwstudio/src/ui/splash.rs`.
/// ---------------------------------------------------------------------------
const kSplashDurationMs = 1900.0;
const kBackdrop = Color(0xFF0E1013);
const kInk = Color(0xFFE8EAF0);
const kInkMuted = Color(0xFF9AA2B1);
const kInkFaint = Color(0xFF6B7383);

/// Reveal windows, in milliseconds — `splash.rs`.
const tMark = (0.0, 420.0);
const tEditor = (180.0, 620.0);
const tPreview = (320.0, 780.0);
const tWordmark = (520.0, 900.0);
const tTagline = (660.0, 1040.0);
const tVersion = (820.0, 1180.0);
const tExit = (1480.0, kSplashDurationMs);

/// The studio's own rhythm — real intervals from the source.
const kCaretBlinkMs = 530.0; // `ui/caret.rs`
const kRenderDebounceMs = 600.0; // `state/editing.rs`
const kSplashExitLift = -18.0; // `splash.rs`
const kSplashExitSwell = 0.04; // `splash.rs`

/// ---------------------------------------------------------------------------
/// The film's own ground and ink (the lab's canvas, `film_lib.rs` BG).
/// ---------------------------------------------------------------------------
const kFilmGround = Color(0xFF0A0A0C);
const kFilmGroundTop = Color(0xFF101116);
const kFilmGroundBottom = Color(0xFF080809);
const kFilmInk = Color(0xFFE8EAF0);
const kFilmMuted = Color(0xFF9AA2B1);
const kFilmFaint = Color(0xFF6B7383);
const kReceipt = Color(0xFF5C636F); // receipt tags, quiet by design

/// Device metrics — `state.rs::Platform`, used by the device frames.
const kIOSLogical = (393.0, 852.0, 3.0); // w, h, dpr; safe 47/34
const kAndroidLogical = (412.0, 915.0, 2.625); // insets 30/24
const kDesktopLogical = (1280.0, 800.0, 1.0);
