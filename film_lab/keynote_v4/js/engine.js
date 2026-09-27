/* ============================================================================
   keynote v4 — engine.js
   The film engine: a clock, a scene stack, beat scheduling, the chrome
   (session rail · caption bar · progress timeline · the hand), and the
   self-audit counters the end card prints. Plays itself; obeys a presenter.
   ========================================================================== */
(function () {
  'use strict';

  const $ = (s, r) => (r || document).querySelector(s);
  const $$ = (s, r) => Array.from((r || document).querySelectorAll(s));
  const clamp = (v, a, b) => Math.max(a, Math.min(b, v));
  const lerp = (a, b, t) => a + (b - a) * t;

  /* ------------------------------------------------------------------ clock */
  const Clock = {
    t: 0,            // film time (s), excluding pauses
    sceneT: 0,       // time in current scene
    playing: false,
    raf: 0,
    last: 0,
    judder: false,   // when true, visible scene time steps at ~10fps (the wait)
    _acc: 0,
    reset() { this.t = 0; this.sceneT = 0; },
  };

  /* ------------------------------------------------------------ scene table */
  const scenes = SCENES;                     // from data.js
  let idx = 0;
  const total = scenes.reduce((a, s) => a + s.dur, 0);
  const starts = [];
  scenes.reduce((acc, s, i) => { starts[i] = acc; return acc + s.dur; }, 0);

  /* -------------------------------------------------------------- self-audit */
  const audit = { frames: 0, shapes: 0, glyphs: 0, taps: 0, startedAt: 0 };

  /* ------------------------------------------------------------------- state */
  const state = {
    witness: 0,                                // the counter ladder
    sessionStart: null,                        // wall clock of first play
    ended: false,
  };

  /* ================================================================== chrome */
  const rail = {
    el: null, witnessEl: null, chipEl: null, actEl: null,
    setWitness(n) {
      state.witness = n;
      if (!rail.witnessEl) return;
      rail.witnessEl.textContent = n === 0 ? '—' : String(n);
      rail.witnessEl.classList.remove('pop'); void rail.witnessEl.offsetWidth;
      rail.witnessEl.classList.add('pop');
      AudioBlip.witness && AudioBlip.witness();
    },
    tickChip() {
      if (!rail.chipEl) return;
      const e = state.sessionStart ? (performance.now() - state.sessionStart) / 1000 : 0;
      const m = Math.floor(e / 60), s = Math.floor(e % 60);
      const txt = String(m).padStart(2, '0') + ':' + String(s).padStart(2, '0');
      rail.chipEl.textContent = txt;
      const st = document.querySelector('#st-chip-time');
      if (st) st.textContent = txt;
    },
    setAct(a) {
      if (rail.actEl) rail.actEl.textContent = 'ACT ' + a;
    },
  };

  /* caption bar — every beat captioned; receipts carry their tag */
  let capTimer = 0;
  function caption(text, opts) {
    const bar = $('#caption');
    if (!bar) return;
    clearTimeout(capTimer);
    opts = opts || {};
    bar.classList.remove('show');
    void bar.offsetWidth;
    $('#caption-text').textContent = text;
    const tag = $('#caption-tag');
    tag.style.display = opts.measured ? '' : 'none';
    if (opts.measured) tag.textContent = '◦ ' + opts.measured;
    bar.classList.add('show');
    if (opts.hold) capTimer = setTimeout(() => bar.classList.remove('show'), opts.hold * 1000);
  }
  function captionOff() { clearTimeout(capTimer); $('#caption').classList.remove('show'); }

  /* ---------------------------------------------------------------- the hand */
  const hand = {
    el: null, x: 50, y: 40, tx: 50, ty: 40, down: false, hidden: true,
    init() { this.el = $('#hand'); },
    moveTo(x, y, dur) {         // percent of viewport
      this.tx = x; this.ty = y;
      if (this.hidden) { this.hidden = false; this.el.classList.add('on'); }
      AudioBlip.whoosh && AudioBlip.whoosh();
    },
    hide() { this.hidden = true; this.el.classList.remove('on'); },
    tap() {
      this.down = true; this.el.classList.add('down');
      setTimeout(() => { this.down = false; this.el.classList.remove('down'); }, 180);
      audit.taps++;
      ripple(this.x, this.y);
      AudioBlip.tap && AudioBlip.tap();
    },
    tick(dt) {
      const k = 1 - Math.exp(-8.5 * dt);      // critically damped follow
      this.x = lerp(this.x, this.tx, k);
      this.y = lerp(this.y, this.ty, k);
      this.el.style.left = this.x + 'vw';
      this.el.style.top = this.y + 'vh';
    },
  };
  function ripple(x, y) {
    const r = document.createElement('div');
    r.className = 'tap-ripple';
    r.style.left = x + 'vw'; r.style.top = y + 'vh';
    document.body.appendChild(r);
    setTimeout(() => r.remove(), 900);
  }

  /* ------------------------------------------------------------- the timeline */
  const tl = {
    el: null, fill: null, head: null, tip: null, tipName: null,
    build() {
      const segs = $('#tl-segs');
      scenes.forEach((s, i) => {
        const d = document.createElement('div');
        d.className = 'tl-seg act' + s.act;
        d.style.flexGrow = s.dur;
        d.dataset.i = i;
        segs.appendChild(d);
      });
      this.el = $('#timeline'); this.fill = $('#tl-fill'); this.head = $('#tl-head');
      this.tip = $('#tl-tip'); this.tipName = $('#tl-name');
      segs.addEventListener('mousemove', (e) => {
        const seg = e.target.closest('.tl-seg');
        if (!seg) return;
        const i = +seg.dataset.i;
        this.tipName.textContent = scenes[i].name;
        this.tip.style.left = clamp(e.clientX, 130, innerWidth - 130) + 'px';
        this.tip.classList.add('on');
      });
      segs.addEventListener('mouseleave', () => this.tip.classList.remove('on'));
      segs.addEventListener('click', (e) => {
        const seg = e.target.closest('.tl-seg');
        if (seg) Film.goto(+seg.dataset.i, true);
      });
    },
    tick() {
      const p = clamp(Clock.t / total, 0, 1);
      this.fill.style.width = (p * 100) + '%';
      const cur = $('#tl-time'), tot = $('#tl-total');
      cur.textContent = fmt(Clock.t); tot.textContent = fmt(total);
      this.head.style.left = (p * 100) + '%';
      const sb = $('#sb-name');
      if (sb) sb.textContent = scenes[idx].name;
    },
  };
  function fmt(t) {
    const m = Math.floor(t / 60), s = Math.floor(t % 60);
    return m + ':' + String(s).padStart(2, '0');
  }

  /* ================================================================ the film */
  /* ---------------------------------------------------- scene-scheduled beats
     Film-time timers: they freeze when the film pauses, and are dropped
     when the scene changes. Use these instead of setTimeout. */
  const pending = [];
  function after(sec, fn) { pending.push({ at: Clock.sceneT + sec, fn }); }
  function firePending() {
    for (let i = pending.length - 1; i >= 0; i--) {
      if (Clock.sceneT >= pending[i].at) {
        const f = pending[i].fn;
        pending.splice(i, 1);
        try { f(); } catch (e) { console.error(e); }
      }
    }
  }

  const hooks = {};                            // sceneId -> {enter, exit, tick, beats}
  let active = null;                           // current hook set

  function define(id, def) { hooks[id] = def; }

  function mount(i, fast) {
    const s = scenes[i];
    idx = i;
    Clock.sceneT = 0;
    // teardown previous
    if (active && active.exit) { try { active.exit(fast); } catch (e) { console.error(e); } }
    pending.length = 0;
    $$('.scene.on').forEach(el => el.classList.remove('on', 'entered'));
    active = hooks[s.id] || {};
    const root = $('#sc-' + s.id);
    if (root) { root.classList.add('on'); }
    // the shared studio build belongs only to scenes that declare it
    if (window.Studio && !active.studio) window.Studio.show(false);
    rail.setAct(s.act);
    document.title = 'viewwstudio — ' + s.name;
    if (active.enter) { try { active.enter(fast); } catch (e) { console.error(e); } }
    AudioBlip.scene && AudioBlip.scene(s);
  }

  function fireBeats(hook) {
    if (!hook.beats) return;
    hook.beats.forEach(b => {
      if (!b.done && Clock.sceneT >= b.t) { b.done = true; try { b.fn(); } catch (e) { console.error(e); } }
    });
  }

  function advance() {
    if (idx + 1 < scenes.length) mount(idx + 1);
    else finish();
  }

  function finish() {
    Clock.playing = false;
    state.ended = true;
    document.body.classList.add('film-ended');
    captionOff();
    hand.hide();
    const bp = $('#btn-play');
    if (bp) bp.innerHTML = '▶&nbsp;&nbsp;replay the film';
    $('#launch').classList.add('on');
    $('#launch').classList.add('replay');
  }

  const Film = {
    scenes, get idx() { return idx; }, get total() { return total; },
    clock: Clock, audit, state, rail, caption, captionOff, hand, define, RECEIPTS,
    mount,

    play() {
      if (Clock.playing) return;
      if (state.ended) { this.restart(); return; }
      Clock.playing = true;
      if (!state.sessionStart) state.sessionStart = performance.now();
      document.body.classList.add('playing');
      $('#launch').classList.remove('on');
      Audio.start && Audio.start();
    },
    pause() {
      Clock.playing = false;
      document.body.classList.remove('playing');
      Audio.duck && Audio.duck();
    },
    toggle() { Clock.playing ? this.pause() : this.play(); },
    goto(i, play) {
      state.ended = false;
      document.body.classList.remove('film-ended');
      // rewind the witness to the ladder point at or before the target
      let w = 0;
      for (let k = 0; k <= i; k++) if (LADDER[scenes[k].id] !== undefined) w = LADDER[scenes[k].id];
      // fast render (final state) only when scrubbing to a paused hold;
      // when we will play, mount the full choreography from the top
      mount(i, !play);
      rail.setWitness(w || 0);
      Clock.t = starts[i];
      if (play) this.play(); else this.pause();
    },
    restart() {
      state.ended = false;
      document.body.classList.remove('film-ended');
      rail.setWitness(0);
      Clock.t = 0;
      state.sessionStart = performance.now();
      mount(0, false);
      this.play();
    },
    start() {                       // from the launch screen
      $('#launch').classList.remove('on');
      state.ended = false;
      mount(0, false);
      rail.setWitness(0);
      Clock.t = 0;
      this.play();
    },

    
  };

  /* --------------------------------------------------------------- main loop */
  function frame(now) {
    const dt = Math.min(0.05, (now - (Clock.last || now)) / 1000);
    Clock.last = now;
    if (Clock.playing) {
      Clock.t += dt;
      // judder: the scene-visible clock steps at ~10Hz while film time runs true
      let stepDt = dt;
      if (Clock.judder) {
        Clock._acc += dt;
        if (Clock._acc >= 0.1) { stepDt = Clock._acc; Clock._acc = 0; Clock.sceneT += stepDt; }
        else stepDt = 0;
      } else {
        Clock.sceneT += dt;
      }
      audit.frames++;
      if (active) {
        fireBeats(active);
        firePending();
        if (active.tick) { try { active.tick(Clock.sceneT, stepDt); } catch (e) { console.error(e); } }
      }
      if (Clock.sceneT >= scenes[idx].dur) advance();
      rail.tickChip();
    }
    // always-alive layers
    hand.tick(dt);
    tl.tick();
    FX.tick(dt, Clock.playing);
    requestAnimationFrame(frame);
  }

  /* ------------------------------------------------------------------ input */
  function bindKeys() {
    addEventListener('keydown', (e) => {
      if (e.key === ' ') { e.preventDefault(); Film.toggle(); }
      else if (e.key === 'ArrowRight') Film.goto(Math.min(idx + 1, scenes.length - 1), Clock.playing);
      else if (e.key === 'ArrowLeft') Film.goto(Math.max(idx - 1, 0), Clock.playing);
      else if (e.key === 'ArrowUp') {   // previous act
        let a = scenes[idx].act, j = idx; while (j > 0 && scenes[j].act === a) j--;
        Film.goto(j, Clock.playing);
      }
      else if (e.key === 'ArrowDown') { // next act
        let a = scenes[idx].act, j = idx; while (j < scenes.length - 1 && scenes[j].act === a) j++;
        Film.goto(Math.min(j + 1, scenes.length - 1), Clock.playing);
      }
      else if (e.key === 'f' || e.key === 'F') {
        document.fullscreenElement ? document.exitFullscreen() : document.documentElement.requestFullscreen().catch(() => {});
      }
      else if (e.key === 's' || e.key === 'S') Audio.toggle();
      else if (e.key === 'g' || e.key === 'G') ChapterGrid.toggle();
      else if (e.key === '?' || e.key === 'h' || e.key === 'H') $('#help').classList.toggle('on');
      else if (e.key === 'Escape') { $('#help').classList.remove('on'); ChapterGrid.hide(); }
      else if (e.key === 'Home') Film.goto(0, Clock.playing);
    });
    // click on stage toggles (but not on chrome)
    $('#stage').addEventListener('click', (e) => {
      if (e.target.closest('#timeline, #launch, #chapter-grid, #help, .tl-seg')) return;
      Film.toggle();
    });
    document.addEventListener('visibilitychange', () => { if (document.hidden && Clock.playing) Film.pause(); });
    // idle-fade the hint cluster
    let idleT = 0;
    addEventListener('mousemove', () => {
      document.body.classList.remove('idle');
      clearTimeout(idleT);
      idleT = setTimeout(() => { if (Clock.playing) document.body.classList.add('idle'); }, 2600);
    });
  }

  /* ------------------------------------------------------------ chapter grid */
  const ChapterGrid = {
    toggle() { const g = $('#chapter-grid'); g.classList.contains('on') ? this.hide() : this.show(); },
    show() {
      const g = $('#chapter-grid');
      if (!g.dataset.built) {
        const acts = { I: [], II: [], III: [], IV: [] };
        scenes.forEach((s, i) => acts[s.act].push({ s, i }));
        let html = '';
        for (const a of ['I', 'II', 'III', 'IV']) {
          html += `<div class="cg-act"><div class="cg-act-name">${actName(a)}</div>`;
          acts[a].forEach(({ s, i }) => {
            html += `<button class="cg-scene" data-i="${i}"><span class="cg-num">${String(i + 1).padStart(2, '0')}</span>${s.name}<span class="cg-dur">${s.dur}s</span></button>`;
          });
          html += '</div>';
        }
        g.querySelector('.cg-body').innerHTML = html;
        g.dataset.built = '1';
        g.addEventListener('click', (e) => {
          const b = e.target.closest('.cg-scene');
          if (b) { this.hide(); Film.goto(+b.dataset.i, true); }
        });
      }
      g.classList.add('on');
    },
    hide() { $('#chapter-grid').classList.remove('on'); },
  };
  function actName(a) {
    return { I: 'THE WAIT', II: 'THE SESSION', III: 'THE TWISTS', IV: 'THE RECONCILIATION' }[a];
  }

  /* ------------------------------------------------------------------- boot */
  addEventListener('DOMContentLoaded', () => {
    rail.el = $('#rail'); rail.witnessEl = $('#witness-n');
    rail.chipEl = $('#chip-time'); rail.actEl = $('#act-label');
    hand.init(); tl.build(); bindKeys();
    Act1.init(); Act2.init(); Act3.init(); Act4.init();
    FX.init();
    $('#btn-play').addEventListener('click', () => Film.start());
    $('#btn-replay') && $('#btn-replay').addEventListener('click', () => Film.start());
    document.body.classList.remove('loading');
    requestAnimationFrame(frame);
  });

  /* expose what the audio module needs without dragging Audio in early */
  let AudioBlip = Film._blip = {};
  function setAudioHooks(h) { AudioBlip = Film._blip = h; }

  window.Film = Film;
  window.FilmInternals = { define, caption, captionOff, hand, rail, $, $$, clamp, lerp, audit, setAudioHooks, Clock, after };
})();
