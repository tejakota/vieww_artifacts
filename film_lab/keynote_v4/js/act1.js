/* ============================================================================
   keynote v4 — act1.js
   ACT I · KI (起) — THE WAIT
   act1 card · the wait (degraded-crafted terminal montage, 24-in-60 judder)
   · the question (glyphs ride a flow field, condense to a drop)
   · the genesis (the drop lands; viewwstudio springs up)
   All choreography is time-based — it freezes when the film pauses.
   ========================================================================== */
const Act1 = (() => {
  'use strict';
  const { define, caption, captionOff, hand, rail, $, $$, after, audit } = FilmInternals;

  /* ------------------------------------------------------------ act cards */
  function actCard(id, cardKey) {
    const a = ACTS[cardKey];
    const root = $('#sc-' + id);
    root.innerHTML = `
      <div class="actcard">
        <div class="ac-kanji">${a.kanji}</div>
        <div class="ac-rule"></div>
        <div class="ac-romaji">${a.romaji}</div>
        <div class="ac-name">${a.name}</div>
        <div class="ac-sub">${a.sub}</div>
      </div>`;
    define(id, {
      enter() { captionOff(); FX.setMood({ hue: 265, density: 0.25 }); },
    });
  }

  /* ---------------------------------------------------------------- THE WAIT */
  const WAIT_LINES = [
    { t: 'cargo build --release', cls: 'cmd', cps: 26 },
    { t: '  Compiling vieww-derive v0.1.0', cls: 'dim', cps: 130 },
    { t: '  Compiling … 438 crates', cls: 'dim', cps: 130 },
    { t: '  Compiling bindings v0.9 (8,412 lines of C++)', cls: 'warn', cps: 150 },
    { t: '  Bundling engine.wasm … 41.7 MB', cls: 'warn', cps: 150 },
    { t: '  node_modules — 41,203 files', cls: 'dim', cps: 150 },
    { t: 'warning: view tree truncated at depth 32 — silent', cls: 'err', cps: 120 },
    { t: 'warning: frame budget exceeded — dropping to 24 fps', cls: 'err', cps: 120 },
    { t: '  Linking …', cls: 'dim', cps: 90 },
  ];

  function theWait() {
    const root = $('#sc-wait');
    root.innerHTML = `
      <div class="wait-wrap">
        <div class="crt"></div>
        <div class="wait-terminal">
          <div class="wt-bar"><span class="wt-dot r"></span><span class="wt-dot y"></span><span class="wt-dot g"></span><span class="wt-title">build — zsh</span></div>
          <div class="wt-body"><div class="wt-lines"></div><div class="wt-caret">▊</div></div>
        </div>
        <div class="wait-hour"><span class="wh-label">waiting</span><span class="wh-clock">00:41:07</span></div>
        <div class="wait-frag frag-a">spinner ⠋ building…</div>
        <div class="wait-frag frag-b">░░░░░░▓▓▓▓ 62% ░░░</div>
        <div class="wait-frag frag-c">resync · resync · resync</div>
      </div>`;

    let sched = [], hourT = 0, lastRendered = -1;

    function buildSchedule() {
      sched = [];
      let t = 0.7;
      for (const L of WAIT_LINES) {
        sched.push({ ...L, start: t, end: t + L.t.length / L.cps });
        t += L.t.length / L.cps + 0.14 + Math.random() * 0.2;
      }
    }

    define('wait', {
      enter(fast) {
        Clock.judder = true;
        document.body.classList.add('degraded');
        buildSchedule();
        hourT = 2471; lastRendered = -1;
        $('.wt-lines', root).innerHTML = '';
        if (fast) {
          // seeked: render everything complete
          const box = $('.wt-lines', root);
          WAIT_LINES.forEach(L => {
            const el = document.createElement('div');
            el.className = 'wt-line ' + L.cls;
            el.textContent = L.t;
            box.appendChild(el);
          });
          hourT = 2690;
        } else {
          caption('the wait — the world as it is', { hold: 2.8 });
        }
      },
      exit() {
        Clock.judder = false;
        document.body.classList.remove('degraded');
      },
      tick(t, dt) {
        // hour counter, ticking up — the wait measured
        hourT += dt * 3.2;
        const m = 41 + Math.floor(hourT / 60), s = Math.floor(hourT % 60);
        $('.wh-clock', root).textContent = '00:' + String(m).padStart(2, '0') + ':' + String(s).padStart(2, '0');
        if (lastRendered === -2) return;               // fast mode, static
        // time-based line reveal
        const box = $('.wt-lines', root);
        for (let i = 0; i < sched.length; i++) {
          const S = sched[i];
          if (t < S.start) break;
          let el = box.children[i];
          if (!el) {
            el = document.createElement('div');
            el.className = 'wt-line ' + S.cls;
            box.appendChild(el);
          }
          const n = Math.min(S.t.length, Math.floor((t - S.start) * S.cps));
          const text = S.t.slice(0, n);
          if (el.textContent !== text) {
            el.textContent = text;
            if (n % 3 === 0) Audio.enabled && Audio.blip(2300 + Math.random() * 500, 0.012, 'square', 0.010);
          }
        }
      },
    });
  }

  /* ------------------------------------------------------------ THE QUESTION */
  const QUESTION = 'how long should it take to see what you built?';
  function theQuestion() {
    const root = $('#sc-question');
    root.innerHTML = `
      <canvas class="q-canvas"></canvas>
      <div class="q-type"></div>`;

    let cv, cx, W, H, dpr, parts = null, sprites = [], typed = 0, fontSize = 20;
    const TYPE_CPS = 22, HOLD = 1.7, FLY_T = 5.2;   // fly begins at TYPE_END+HOLD

    function resize() {
      dpr = Math.min(2, devicePixelRatio || 1);
      W = cv.width = innerWidth * dpr; H = cv.height = innerHeight * dpr;
    }

    define('question', {
      enter(fast) {
        cv = $('.q-canvas', root); cx = cv.getContext('2d'); resize();
        const typeEl = $('.q-type', root);
        typeEl.style.opacity = '1';
        parts = null; typed = 0;
        fontSize = Math.max(14, parseFloat(getComputedStyle(typeEl).fontSize));
        sprites = FX.glyphSprites(QUESTION, `${Math.round(fontSize * dpr)}px Geist Mono`, '#F8F4F2');
        typeEl.textContent = '';
        if (fast) { typed = QUESTION.length; typeEl.textContent = QUESTION; parts = []; }
      },
      tick(t, dt) {
        const typeEl = $('.q-type', root);
        const typeEnd = QUESTION.length / TYPE_CPS + 0.1;
        if (!parts) {
          // phase: typing → hold
          const n = Math.min(QUESTION.length, Math.floor(t * TYPE_CPS));
          if (n !== typed) {
            typed = n;
            typeEl.textContent = QUESTION.slice(0, n);
            Audio.enabled && Audio.blip(1900, 0.01, 'square', 0.009);
          }
          if (t > typeEnd + HOLD) {
            // lift-off: glyphs become particles
            typeEl.style.opacity = '0';
            parts = [];
            const n2 = QUESTION.length;
            const cw = W / n2;
            for (let i = 0; i < n2; i++) {
              parts.push({
                x: (i + 0.5) * cw, y: H * 0.48,
                vx: (Math.random() - 0.5) * 60, vy: (Math.random() - 0.5) * 60,
                s: sprites[i], seed: Math.random() * 10,
              });
            }
            flyT0 = t;
          } else return;
        }
        // phase: fly — ride the field, condense to a drop
        const ft = t - (typeEnd + HOLD);
        const prog = Math.min(1, ft / FLY_T);
        cx.clearRect(0, 0, W, H);
        const cxp = W / 2, cyp = H * 0.5;
        for (const p of parts) {
          const fx = Math.sin(p.y * 0.004 + p.seed + t * 0.9) * 260;
          const fy = Math.cos(p.x * 0.004 + p.seed * 1.3 - t * 0.7) * 260;
          p.vx += (fx * dt) * (1 - prog * 0.75);
          p.vy += (fy * dt) * (1 - prog * 0.75);
          const tx = cxp + Math.cos(p.seed * 7 + t * 2) * 34 * (1 - prog);
          const ty = cyp + Math.sin(p.seed * 5 + t * 2) * 34 * (1 - prog);
          p.vx += (tx - p.x) * 5.5 * prog * dt;
          p.vy += (ty - p.y) * 5.5 * prog * dt;
          p.vx *= 0.988; p.vy *= 0.988;
          p.x += p.vx * dt; p.y += p.vy * dt;
          const sc = 1 - prog * 0.55;
          cx.drawImage(p.s.c, p.x - (p.s.w * sc) / 2, p.y - (p.s.h * sc) / 2, p.s.w * sc, p.s.h * sc);
        }
        audit.shapes += parts.length;
        if (prog > 0.4) {
          cx.globalCompositeOperation = 'lighter';
          const g = cx.createRadialGradient(cxp, cyp, 0, cxp, cyp, 90 * dpr * prog);
          g.addColorStop(0, `rgba(180,145,255,${0.5 * prog})`);
          g.addColorStop(1, 'rgba(180,145,255,0)');
          cx.fillStyle = g;
          cx.fillRect(cxp - 100 * dpr, cyp - 100 * dpr, 200 * dpr, 200 * dpr);
          cx.globalCompositeOperation = 'source-over';
        }
      },
    });
    addEventListener('resize', () => { if (cv) resize(); });
  }

  /* ------------------------------------------------------------- THE GENESIS */
  function theGenesis() {
    const root = $('#sc-genesis');
    root.innerHTML = `
      <canvas class="g-canvas"></canvas>
      <div class="g-floor"></div>
      <div class="g-wordmark">
        <div class="g-line"><span class="g-chrome">vieww</span><span class="g-studio">studio</span></div>
        <div class="g-underline"></div>
        <div class="g-sub">the studio for UI in Rust — on the vieww engine</div>
        <div class="g-tag">RELEASE FILM</div>
      </div>`;

    let cv, cx, W, H, dpr;
    let dropY = 0, dropVy = 0, splash = [], landed = false;

    function resize() {
      dpr = Math.min(2, devicePixelRatio || 1);
      W = cv.width = innerWidth * dpr; H = cv.height = innerHeight * dpr;
    }

    define('genesis', {
      enter(fast) {
        cv = $('.g-canvas', root); cx = cv.getContext('2d'); resize();
        splash = []; landed = false;
        dropY = -0.06 * H; dropVy = 0;
        $('.g-wordmark', root).classList.remove('risen');
        if (fast) { landed = true; $('.g-wordmark', root).classList.add('risen'); }
        FX.setMood({ hue: 265, density: 0.3 });
      },
      tick(t, dt) {
        cx.clearRect(0, 0, W, H);
        const floorY = H * 0.62;
        if (!landed) {
          const g = 1.9 * H;                      // gravity, device px
          dropVy += g * dt;
          dropY += dropVy * dt;
          const x = W / 2;
          const tr = cx.createLinearGradient(x, dropY - 150 * dpr, x, dropY);
          tr.addColorStop(0, 'rgba(180,145,255,0)');
          tr.addColorStop(1, 'rgba(220,200,255,0.5)');
          cx.fillStyle = tr;
          cx.fillRect(x - 1.5 * dpr, dropY - 150 * dpr, 3 * dpr, 150 * dpr);
          cx.globalCompositeOperation = 'lighter';
          const gl = cx.createRadialGradient(x, dropY, 0, x, dropY, 26 * dpr);
          gl.addColorStop(0, 'rgba(248,244,242,0.95)');
          gl.addColorStop(0.3, 'rgba(180,145,255,0.7)');
          gl.addColorStop(1, 'rgba(180,145,255,0)');
          cx.fillStyle = gl;
          cx.beginPath(); cx.arc(x, dropY, 26 * dpr, 0, 7); cx.fill();
          cx.globalCompositeOperation = 'source-over';
          if (dropY >= floorY) {
            landed = true;
            for (let i = 0; i < 28; i++) {
              const a = (i / 28) * Math.PI * 2;
              splash.push({ x: W / 2, y: floorY, vx: Math.cos(a) * (260 + Math.random() * 320), vy: Math.sin(a) * (120 + Math.random() * 160) - 260, life: 1 });
            }
            $('.g-wordmark', root).classList.add('risen');
            Audio.enabled && Audio.blip(90, 0.6, 'sine', 0.4, 40);
          }
        } else {
          cx.globalCompositeOperation = 'lighter';
          for (const s of splash) {
            s.life -= dt * 1.05;
            if (s.life <= 0) continue;
            s.vy += 700 * dt;
            s.x += s.vx * dt; s.y += s.vy * dt;
            cx.globalAlpha = s.life;
            cx.fillStyle = 'rgba(200,170,255,0.9)';
            cx.beginPath(); cx.arc(s.x, s.y, (2.2 * s.life + 0.4) * dpr, 0, 7); cx.fill();
          }
          cx.globalAlpha = 1;
          cx.globalCompositeOperation = 'source-over';
        }
      },
    });
    addEventListener('resize', () => { if (cv) resize(); });
  }

  function init() {
    actCard('act1', 'ki');
    theWait(); theQuestion(); theGenesis();
  }
  return { init };
})();
