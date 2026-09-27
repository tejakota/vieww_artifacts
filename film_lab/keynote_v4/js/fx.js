/* ============================================================================
   keynote v4 — fx.js
   The persistent visual substrate: an ambient dust field on one canvas
   (additive motes whose mood follows the act), glow sprites, springs,
   and the glyph-sprite helper the question scene rides.
   ========================================================================== */
const FX = (() => {
  'use strict';
  const { $, lerp, clamp } = FilmInternals;

  let cv, cx, W, H, dpr;
  let motes = [];
  let mood = { hue: 265, density: 0.5, drift: 1 };     // violet default
  let running = false;
  let sprite;

  function makeSprite() {
    // a soft round glow dot, pre-rendered
    const s = document.createElement('canvas');
    s.width = s.height = 64;
    const g = s.getContext('2d');
    const rg = g.createRadialGradient(32, 32, 0, 32, 32, 32);
    rg.addColorStop(0, 'rgba(255,255,255,1)');
    rg.addColorStop(0.25, 'rgba(255,255,255,0.55)');
    rg.addColorStop(1, 'rgba(255,255,255,0)');
    g.fillStyle = rg;
    g.fillRect(0, 0, 64, 64);
    return s;
  }

  function seed() {
    motes = [];
    const n = 130;
    for (let i = 0; i < n; i++) {
      motes.push({
        x: Math.random(), y: Math.random(),
        r: 0.6 + Math.random() * 2.6,
        vx: (Math.random() - 0.5) * 0.006,
        vy: (Math.random() - 0.5) * 0.004 - 0.002,
        tw: Math.random() * Math.PI * 2,
        tws: 0.4 + Math.random() * 1.2,
        a: 0.12 + Math.random() * 0.5,
      });
    }
  }

  function resize() {
    dpr = Math.min(2, window.devicePixelRatio || 1);
    W = cv.width = Math.floor(innerWidth * dpr);
    H = cv.height = Math.floor(innerHeight * dpr);
  }

  function setMood(m) { Object.assign(mood, m); }

  function tick(dt, playing) {
    if (!running) return;
    cx.clearRect(0, 0, W, H);
    cx.globalCompositeOperation = 'lighter';
    const t = performance.now() / 1000;
    const hue = mood.hue;
    for (const m of motes) {
      m.x += m.vx * dt * mood.drift; m.y += m.vy * dt * mood.drift;
      if (m.x < -0.02) m.x = 1.02; if (m.x > 1.02) m.x = -0.02;
      if (m.y < -0.02) m.y = 1.02; if (m.y > 1.02) m.y = -0.02;
      const tw = 0.55 + 0.45 * Math.sin(m.tw + t * m.tws);
      const alpha = m.a * tw * mood.density;
      if (alpha <= 0.01) continue;
      const r = m.r * dpr * 3;
      cx.globalAlpha = alpha;
      cx.drawImage(sprite, m.x * W - r, m.y * H - r, r * 2, r * 2);
    }
    cx.globalAlpha = 1;
    cx.globalCompositeOperation = 'source-over';
  }

  /* ---- spring helper (critically/under-damped, used all over the acts) ---- */
  function spring(x0, v0, k, c) {
    let x = x0, v = v0;
    return (dt) => {
      const a = -k * x - c * v;
      v += a * dt; x += v * dt;
      return x;
    };
  }

  /* ---- glyph sprite sheet (the question scene's 39 passengers) ----------- */
  function glyphSprites(text, font, color) {
    const out = [];
    for (const ch of text) {
      const c = document.createElement('canvas');
      const m = c.getContext('2d');
      m.font = font;
      const w = Math.ceil(m.measureText(ch).width) + 8, h = Math.ceil(parseInt(font) * 1.5);
      c.width = w; c.height = h;
      const g = c.getContext('2d');
      g.font = font; g.fillStyle = color;
      g.textBaseline = 'middle';
      g.fillText(ch, 4, h / 2);
      out.push({ c, w, h });
    }
    return out;
  }

  function countUp(el, to, dur, fmt) {
    const t0 = performance.now();
    fmt = fmt || (v => String(Math.round(v)));
    function step(t) {
      const p = clamp((t - t0) / (dur * 1000), 0, 1);
      const e = 1 - Math.pow(1 - p, 3);
      el.textContent = fmt(to * e);
      if (p < 1) requestAnimationFrame(step);
    }
    requestAnimationFrame(step);
  }

  function init() {
    cv = $('#ambient'); cx = cv.getContext('2d');
    sprite = makeSprite();
    seed(); resize(); running = true;
    addEventListener('resize', resize);
  }

  return { init, tick, setMood, spring, glyphSprites, countUp, get canvas() { return cv; } };
})();

/* ============================================================================
   keynote v4 — audio.js (folded into fx.js load order via separate script)
   A synthesized score, muted by default (S toggles): a detuned drone pad,
   a soft pulse on scene change, ticks for typing, a thock for taps.
   No samples, no network — WebAudio only.
   ========================================================================== */
const Audio = (() => {
  'use strict';
  let ctx = null, master = null, on = false, started = false;
  let droneA, droneB, padGain, lfo, lfoGain;

  function ensure() {
    if (ctx) return;
    ctx = new (window.AudioContext || window.webkitAudioContext)();
    master = ctx.createGain(); master.gain.value = 0; master.connect(ctx.destination);
  }

  function startDrone() {
    if (started) return; started = true;
    padGain = ctx.createGain(); padGain.gain.value = 0.05; padGain.connect(master);
    const filt = ctx.createBiquadFilter(); filt.type = 'lowpass'; filt.frequency.value = 240;
    filt.Q.value = 0.6; filt.connect(padGain);
    droneA = ctx.createOscillator(); droneA.type = 'sawtooth'; droneA.frequency.value = 55;
    droneB = ctx.createOscillator(); droneB.type = 'sawtooth'; droneB.frequency.value = 55.4;
    const ga = ctx.createGain(); ga.gain.value = 0.5;
    droneA.connect(ga); droneB.connect(ga); ga.connect(filt);
    // slow breathing on the filter
    lfo = ctx.createOscillator(); lfo.frequency.value = 0.07;
    lfoGain = ctx.createGain(); lfoGain.gain.value = 90;
    lfo.connect(lfoGain); lfoGain.connect(filt.frequency);
    droneA.start(); droneB.start(); lfo.start();
    // airy shimmer: high sine, very quiet
    const air = ctx.createOscillator(); air.type = 'sine'; air.frequency.value = 220;
    const ag = ctx.createGain(); ag.gain.value = 0.012;
    air.connect(ag); ag.connect(master); air.start();
  }

  function blip(freq, dur, type, gain, sweep) {
    if (!on || !ctx) return;
    const o = ctx.createOscillator(), g = ctx.createGain();
    o.type = type || 'sine'; o.frequency.value = freq;
    if (sweep) o.frequency.exponentialRampToValueAtTime(sweep, ctx.currentTime + dur);
    g.gain.value = 0;
    g.gain.linearRampToValueAtTime(gain || 0.12, ctx.currentTime + 0.012);
    g.gain.exponentialRampToValueAtTime(0.0001, ctx.currentTime + dur);
    o.connect(g); g.connect(master);
    o.start(); o.stop(ctx.currentTime + dur + 0.05);
  }

  function toggle() {
    ensure();
    on = !on;
    if (on) { ctx.resume(); startDrone(); master.gain.linearRampToValueAtTime(0.9, ctx.currentTime + 1.2); }
    else master.gain.linearRampToValueAtTime(0, ctx.currentTime + 0.4);
    const b = document.querySelector('#btn-sound');
    if (b) { b.classList.toggle('active', on); b.textContent = on ? 'sound · on' : 'sound · off'; }
  }
  function start() { if (on && ctx) ctx.resume(); }
  function duck() { /* keep drone breathing under pause */ }

  // engine hooks
  FilmInternals.setAudioHooks({
    tap: () => blip(180, 0.22, 'sine', 0.35, 60),
    whoosh: () => {},
    witness: () => blip(660, 0.3, 'sine', 0.08, 880),
    scene: (s) => { blip(110, 0.5, 'sine', 0.10, 82); if (s.kind === 'actcard') blip(220, 1.2, 'triangle', 0.06); },
  });

  return { toggle, start, duck, blip, get enabled() { return on; } };
})();
