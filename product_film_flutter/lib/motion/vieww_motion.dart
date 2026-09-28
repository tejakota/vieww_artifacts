/// The vieww motion system, ported to Dart for this film.
///
/// Source of truth: `vieww_base/crates/vieww-animation/src/` (curves,
/// simulations, springs) and `vieww_base/crates/vieww-widget/src/widgets/`
/// (`Animated`, `Motion` tokens). The doctrine carries over unchanged:
///
///   every animation is a function of time, sampled at the frame's
///   timestamp. nothing integrates by frame delta, nothing reads a clock.
///
/// That is what makes the film deterministic: `film(t)` is a pure function,
/// every frame reproducible at whatever fps the renderer asks for.
library;

import 'dart:math' as math;

/// ---------------------------------------------------------------------------
/// `Curve` — `vieww-animation/src/curve.rs`.
///
/// `Cubic { x1, y1, x2, y2 }` is the CSS `cubic-bezier()` parameterisation,
/// solved with Newton-Raphson (4 iterations) falling back to bisection
/// (24 steps), exactly as the Rust implementation does. x must lie in 0..=1.
/// ---------------------------------------------------------------------------
class VCurve {
  final double x1, y1, x2, y2;
  const VCurve(this.x1, this.y1, this.x2, this.y2);

  double transform(double t) {
    if (t <= 0.0) return 0.0;
    if (t >= 1.0) return 1.0;
    final x = _solveX(t);
    return _bezierY(x);
  }

  double _solveX(double x) {
    // Newton-Raphson, four passes — `curve.rs` keeps the same budget.
    var t = x;
    for (var i = 0; i < 4; i++) {
      final d = _bezierDX(t);
      if (d.abs() < 1e-6) break;
      final err = _bezierX(t) - x;
      final step = err / d;
      t = (t - step).clamp(0.0, 1.0);
      if (err.abs() < 1e-7) return t;
    }
    // Bisection, 24 steps — the solver the Rust curve falls back to.
    var lo = 0.0, hi = 1.0;
    for (var i = 0; i < 24; i++) {
      final mid = (lo + hi) / 2.0;
      if (_bezierX(mid) < x) {
        lo = mid;
      } else {
        hi = mid;
      }
    }
    return (lo + hi) / 2.0;
  }

  double _bezierX(double t) =>
      _c(0.0, x1, x2, 1.0, t);
  double _bezierY(double t) =>
      _c(0.0, y1, y2, 1.0, t);
  double _bezierDX(double t) =>
      3.0 * _c1(x1, x2, t);

  static double _c(double a, double b, double c, double d, double t) {
    final u = 1.0 - t;
    return 3.0 * u * u * t * (b - a) + 3.0 * u * t * t * (c - b) + t * t * t * (d - a);
  }

  static double _c1(double a, double b, double t) {
    final u = 1.0 - t;
    return u * u + 2.0 * u * t * (b - a) + t * t * (1.0 - b);
  }
}

/// The repo's built-in curves — `curve.rs` constants.
const kEaseInOut = VCurve(0.42, 0.0, 0.58, 1.0);
const kEaseIn = VCurve(0.42, 0.0, 1.0, 1.0);
const kEaseOut = VCurve(0.0, 0.0, 0.58, 1.0);
const kEase = VCurve(0.25, 0.1, 0.25, 1.0);
/// Material's emphasised curve, the studio's `curve_standard` (`Motion`).
const kFastOutSlowIn = VCurve(0.4, 0.0, 0.2, 1.0);

/// ---------------------------------------------------------------------------
/// Scalar easing — `film_lib.rs`, the lab's own scrub-safe helpers.
/// ---------------------------------------------------------------------------
double clamp01(double t) => t.clamp(0.0, 1.0);

/// `(t - start) / span`, clamped — a beat's local clock.
double window(double t, double start, double span) =>
    clamp01((t - start) / span);

double easeOutCubic(double t) => 1.0 - math.pow(1.0 - clamp01(t), 3).toDouble();

/// `splash.rs::reveal` — the reveal curve every splash element shares.
double revealWindow(double nowMs, (double, double) w) {
  if (nowMs <= w.$1) return 0.0;
  if (nowMs >= w.$2) return 1.0;
  final t = (nowMs - w.$1) / (w.$2 - w.$1);
  return easeOutCubic(t);
}

double easeOutExpo(double t) =>
    t >= 1.0 ? 1.0 : 1.0 - math.pow(2.0, -10.0 * clamp01(t)).toDouble();

double easeInOutQuad(double t) {
  final x = clamp01(t);
  return x < 0.5 ? 2.0 * x * x : 1.0 - math.pow(-2.0 * x + 2.0, 2.0) / 2.0;
}

double easeOutBack(double t, {double overshoot = 1.70158}) {
  final x = clamp01(t);
  final c3 = overshoot + 1.0;
  return 1.0 + c3 * math.pow(x - 1.0, 3) + overshoot * math.pow(x - 1.0, 2);
}

double tri(double t) {
  final f = t - t.floor();
  return f < 0.5 ? f * 2.0 : 2.0 - f * 2.0;
}

/// `held_24_in_60` — 24 Hz judder inside a 60 Hz timeline. The lab uses it to
/// *show* jank; this film uses it for the old-world scenes, then drops it the
/// moment viewwstudio arrives, which is the whole argument in one function.
double held24In60(double t) {
  final step = 1.0 / 24.0;
  return (t / step).floor().toDouble() * step;
}

/// ---------------------------------------------------------------------------
/// Springs — `vieww-animation/src/spring.rs`, closed form.
///
/// `SpringSpec { stiffness, damping_ratio }` with unit mass:
///   omega = sqrt(stiffness), zeta = damping_ratio.
/// The presets are the repo's own: Expressive (380, 0.8) is "one small
/// overshoot — hero moments"; Standard (200, 1.0) "arrives ~200ms, no
/// overshoot". `Spring::duration` settles at a relative 0.25% tolerance
/// with a 4 s cap.
/// ---------------------------------------------------------------------------
class SpringSpec {
  final double stiffness;
  final double dampingRatio;
  const SpringSpec(this.stiffness, [this.dampingRatio = 1.0]);

  /// `SpringPreset::Expressive` — `spring.rs`.
  static const expressive = SpringSpec(380.0, 0.8);

  /// `SpringPreset::Standard` — `spring.rs`.
  static const standard = SpringSpec(200.0, 1.0);
}

class VSpring {
  final SpringSpec spec;
  const VSpring(this.spec);

  double get _omega => math.sqrt(spec.stiffness.clamp(0.0, 1e6));
  double get _zeta => spec.dampingRatio;

  /// Normalised position 0 -> 1 at time `t` seconds, initial velocity 0.
  double position(double t) {
    if (t <= 0) return 0.0;
    final w = _omega;
    final z = _zeta;
    if (z < 0.999) {
      final wd = w * math.sqrt(1.0 - z * z);
      final decay = math.exp(-z * w * t);
      final c = math.cos(wd * t);
      final s = (z * w / wd) * math.sin(wd * t);
      return 1.0 - decay * (c + s);
    }
    // Critically damped (and effectively so above 0.999).
    final decay = math.exp(-w * t);
    return 1.0 - decay * (1.0 + w * t);
  }

  /// Velocity at `t` (units per second) — for chaining throws.
  ///
  /// Underdamped: x(t) = 1 - e^{-a t}(cos bt + (a/b) sin bt), so
  /// x'(t) = e^{-a t} (a^2/b + b) sin(bt) with a = zeta*omega, b = wd.
  double velocity(double t) {
    if (t <= 0) return 0.0;
    final w = _omega;
    final z = _zeta;
    if (z < 0.999) {
      final a = z * w;
      final b = w * math.sqrt(1.0 - z * z);
      return math.exp(-a * t) * (a * a / b + b) * math.sin(b * t);
    }
    final decay = math.exp(-w * t);
    return w * w * t * decay;
  }

  /// Settle duration at 0.25% relative tolerance, 4 s cap — `Spring::duration`.
  double get duration {
    const tol = 0.0025;
    for (var t = 0.0; t < 4.0; t += 1.0 / 240.0) {
      if ((1.0 - position(t)).abs() < tol) {
        // Run a little past tolerance so a caller animating to the duration
        // never samples the last visible motion.
        return math.min(4.0, t + 1.0 / 60.0);
      }
    }
    return 4.0;
  }

  /// The lab's scrub-safe shadow: `spring_out(t, omega, zeta)` from
  /// `film_lib.rs` — same maths, sampled directly.
  static double out(double t, double omega, double zeta) =>
      VSpring(SpringSpec(omega * omega, zeta)).position(t);
}

/// ---------------------------------------------------------------------------
/// `Motion` tokens — `vieww-widget/src/widgets/theme.rs`.
///
/// "effects never overshoot" is enforced in the repo by
/// `effects_never_overshoot()`; here it is a doc comment and a discipline.
/// ---------------------------------------------------------------------------
class MotionTokens {
  final double durationShort;
  final double durationMedium;
  final double durationLong;
  final VCurve curveStandard;
  final SpringSpec spatialFast;
  final SpringSpec spatialDefault;
  final SpringSpec effectsDefault;

  const MotionTokens({
    this.durationShort = 0.12,
    this.durationMedium = 0.20,
    this.durationLong = 0.32,
    this.curveStandard = kFastOutSlowIn,
    this.spatialFast = const SpringSpec(500.0, 1.0),
    this.spatialDefault = const SpringSpec(200.0, 1.0),
    this.effectsDefault = const SpringSpec(140.0, 0.9),
  });

  /// `Motion::expressive()` — 120/220/380, `SpringPreset::Expressive` spatial.
  static const expressive = MotionTokens(
    durationShort: 0.12,
    durationMedium: 0.22,
    durationLong: 0.38,
    spatialDefault: SpringSpec(380.0, 0.8),
  );

  /// `Motion::standard()` — 120/200/320.
  static const standard = MotionTokens();
}

/// The default motion every scene in this film speaks.
const kMotion = MotionTokens.expressive;

/// ---------------------------------------------------------------------------
/// `Timeline` staggering — `vieww-element/src/timeline.rs`.
///
/// A `Stagger { interval, children }` starts child `i` at `i * interval`
/// after its own start — cascades, not delays.
/// ---------------------------------------------------------------------------
List<double> staggerStarts(int count, double interval, {double offset = 0.0}) =>
    List<double>.generate(count, (i) => offset + i * interval);

/// A child's eased progress given the film time, its start and its span.
double staggerAt(double t, double start, double span, {VCurve? curve}) {
  final p = clamp01((t - start) / span);
  return curve == null ? p : curve.transform(p);
}

/// ---------------------------------------------------------------------------
/// Determinism — `film_lib.rs::Rng` (xorshift64*), so stars, motes and dust
/// re-render identically on every machine that runs this film.
/// ---------------------------------------------------------------------------
class Rng {
  int _state;
  Rng([int seed = 0x9E3779B97F4A7C15]) : _state = seed == 0 ? 1 : seed;

  int nextU64() {
    var x = _state;
    x ^= x >> 12;
    x ^= x << 25;
    x ^= x >> 27;
    _state = x;
    return x * 0x2545F4914F6CDD1D;
  }

  double next01() => (nextU64() & 0x1FFFFFFFFFFFFF).toDouble() / 4503599627370496.0;

  double range(double lo, double hi) => lo + (hi - lo) * next01();
}
