/* ============================================================================
   keynote v4 — act3.js
   ACT III · TEN (転) — THE TWISTS
   the mirror (the IDE is a vieww app) · the machine room (emergent
   performance — live boids + the fourier choir + the ceilings wall)
   · the receipts (a 2019 phone) · the unfold (three trees + the signal).
   ========================================================================== */
const Act3 = (() => {
  'use strict';
  const { define, caption, captionOff, hand, rail, $, $$, after, audit } = FilmInternals;

  /* ------------------------------------------------------------ act card */
  function actCard3() {
    const a = ACTS.ten;
    $('#sc-act3').innerHTML = `
      <div class="actcard">
        <div class="ac-kanji">${a.kanji}</div><div class="ac-rule"></div>
        <div class="ac-romaji">${a.romaji}</div><div class="ac-name">${a.name}</div>
        <div class="ac-sub">${a.sub}</div>
      </div>`;
    define('act3', { enter() { captionOff(); FX.setMood({ hue: 30, density: 0.55 }); } });
  }

  /* ------------------------------------------------------------- THE MIRROR */
  function sceneMirror() {
    let built = false;
    define('mirror', { studio: true,
      enter(fast) {
        const S = $('#studio-app');
        Studio.reset(); Studio.showFinal(); Studio.setCount(7, false);
        S.classList.add('show', 'mirror');
        if (!built) buildDevtools(); built = true;
        $('#devtools').classList.add('on');
        if (!fast) {
          caption('devtools — turn it on the studio itself', { hold: 3 });
          after(5.2, () => {
            $('#devtools').classList.add('lit');
            Audio.enabled && Audio.blip(440, 0.5, 'sine', 0.12, 660);
            caption('the IDE you’ve been watching — is a vieww app', { hold: 3.6 });
          });
          after(9.2, () => caption('its own damage, its own trees, live', { measured: 'vieww-devtools · the inspector', hold: 2.6 }));
        } else $('#devtools').classList.add('lit');
      },
      exit() {
        $('#studio-app').classList.remove('show', 'mirror');
        const d = $('#devtools'); if (d) { d.classList.remove('on', 'lit'); }
      },
    });
  }
  function buildDevtools() {
    const d = document.createElement('div');
    d.id = 'devtools';
    d.innerHTML = `
      <div class="dt-h"><span class="dt-dot"></span>vieww-devtools — inspecting <b>viewwstudio</b></div>
      <div class="dt-body">
        <div class="dt-col">
          <div class="dt-col-h">widget tree</div>
          <div class="dt-rows">
            <div class="dt-row depth0"><i class="pill w">W</i>Studio</div>
            <div class="dt-row depth1"><i class="pill w">W</i>WindowChrome</div>
            <div class="dt-row depth2"><i class="pill w">W</i>EditorPane</div>
            <div class="dt-row depth3"><i class="pill w">W</i>PreviewPane</div>
            <div class="dt-row depth4 lit"><i class="pill e">E</i>AppCard <span class="dt-badge">build 2</span></div>
            <div class="dt-row depth5"><i class="pill w">W</i>Label “Tapped 7 times”</div>
          </div>
        </div>
        <div class="dt-col">
          <div class="dt-col-h">identity + state</div>
          <div class="dt-rows">
            <div class="dt-row depth2"><i class="pill e">E</i>editor — state held</div>
            <div class="dt-row depth3"><i class="pill e">E</i>preview — remount ×12</div>
            <div class="dt-row depth4"><i class="pill s">S</i>count → <b>7</b></div>
            <div class="dt-row depth4"><i class="pill s">S</i>one signal, outside every tree</div>
          </div>
        </div>
      </div>
      <div class="dt-damage"><span></span>studio damage: 1 rect · 2,310 px</div>`;
    document.getElementById('stage').appendChild(d);
  }

  /* ------------------------------------------------------- THE MACHINE ROOM */
  function sceneMachine() {
    const root = $('#sc-machine');
    root.innerHTML = `
      <div class="mr-wrap">
        <div class="mr-live" id="mr-live">
          <canvas id="boids-canvas"></canvas>
          <canvas id="fourier-canvas" class="hidden"></canvas>
          <div class="mr-hud">
            <div class="mr-hud-live"><span class="live-dot"></span>LIVE · IN THIS BROWSER</div>
            <div class="mr-hud-n" id="mr-hud-n">2,400 boids</div>
            <div class="mr-hud-sub" id="mr-hud-sub">the swarm plate — build and raster split, cleanly</div>
          </div>
        </div>
        <div class="mr-receipts">
          <div class="mr-card on" id="mr-card-swarm">
            <div class="mrc-gif"><img src="assets/plates/swarm.gif" alt="swarm — a real vieww render"></div>
            <div class="mrc-meta">
              <div class="mrc-name">swarm · plate receipt</div>
              <div class="mrc-line">2,400 boids · <b>2,511 shapes / frame</b></div>
              <div class="mrc-line">build <b>66.4 ms</b> · raster <b>214.1 ms</b> — split, counted</div>
            </div>
          </div>
          <div class="mr-card" id="mr-card-fourier">
            <div class="mrc-gif"><img src="assets/plates/fourier.gif" alt="fourier — a real vieww render"></div>
            <div class="mrc-meta">
              <div class="mrc-name">fourier · plate receipt</div>
              <div class="mrc-line">the avatar line, re-drawn by its own choir</div>
              <div class="mrc-line">2,004 shapes · 32 frames — every circle a term</div>
            </div>
          </div>
        </div>
      </div>
      <div class="mr-ceilings" id="mr-ceilings">
        <div class="ceil-card"><img src="assets/plates/galaxy.gif"><div class="ceil-meta"><b>galaxy</b><span>60,527 shapes / frame</span><span>101 ms — the record</span></div></div>
        <div class="ceil-card"><img src="assets/plates/mandel.gif"><div class="ceil-meta"><b>mandel</b><span>57,602 rects / frame</span><span>69 ms</span></div></div>
        <div class="ceil-card"><img src="assets/plates/megapath.gif"><div class="ceil-meta"><b>megapath</b><span>one line · 40,000 segments</span><span>one stroke verb</span></div></div>
        <div class="ceil-card"><img src="assets/plates/hero4k.gif"><div class="ceil-meta"><b>hero4k</b><span>3840 × 2160</span><span>440–489 ms / frame</span></div></div>
        <div class="ceil-card"><img src="assets/plates/longplay.gif"><div class="ceil-meta"><b>longplay</b><span>256 frames · RSS flat</span><span>38.3 → 39.0 MiB</span></div></div>
      </div>`;

    /* ---- boids: 2,400, direction-bucketed sprites, additive trails ---- */
    const N = 2400;
    let boids = [], grid = null, bcv, bcx, W, H, dpr, sprites = [], live = false;
    const CELL = 46;

    function makeDirSprites() {
      sprites = [];
      for (let k = 0; k < 16; k++) {
        const c = document.createElement('canvas'); c.width = c.height = 22;
        const g = c.getContext('2d');
        const a = (k / 16) * Math.PI * 2;
        g.translate(11, 11); g.rotate(a);
        const grad = g.createLinearGradient(-9, 0, 9, 0);
        grad.addColorStop(0, 'rgba(126,92,232,0)');
        grad.addColorStop(1, 'rgba(190,160,255,0.9)');
        g.fillStyle = grad;
        g.beginPath();
        g.moveTo(9, 0); g.lineTo(-4, -2.6); g.lineTo(-4, 2.6); g.closePath(); g.fill();
        sprites.push(c);
      }
    }
    function seedBoids() {
      boids = [];
      for (let i = 0; i < N; i++) {
        boids.push({
          x: Math.random() * W, y: Math.random() * H,
          vx: (Math.random() - 0.5) * 160, vy: (Math.random() - 0.5) * 160,
        });
      }
    }
    function stepBoids(dt) {
      // spatial hash
      const cols = Math.max(1, Math.ceil(W / CELL)), rows = Math.max(1, Math.ceil(H / CELL));
      grid = new Array(cols * rows);
      for (const b of boids) {
        const gx = clampI(b.x / CELL, 0, cols - 1), gy = clampI(b.y / CELL, 0, rows - 1);
        const gi = gy * cols + gx;
        (grid[gi] || (grid[gi] = [])).push(b);
      }
      for (const b of boids) {
        let ax = 0, ay = 0, cx = 0, cy = 0, nx = 0, ny = 0, n = 0;
        const gx = clampI(b.x / CELL, 0, cols - 1), gy = clampI(b.y / CELL, 0, rows - 1);
        for (let oy = -1; oy <= 1; oy++) for (let ox = -1; ox <= 1; ox++) {
          const cell = grid[(gy + oy) * cols + (gx + ox)];
          if (!cell) continue;
          for (const o of cell) {
            if (o === b) continue;
            const dx = o.x - b.x, dy = o.y - b.y, d2 = dx * dx + dy * dy;
            if (d2 > 3600 || d2 < 0.01) continue;
            n++; cx += o.x; cy += o.y;
            if (d2 < 400) { ax -= dx * 0.06; ay -= dy * 0.06; }         // separation
            ax += dx * 0.008; ay += dy * 0.008;                          // cohesion
            ax += o.vx * 0.02 - 0; ay += o.vy * 0.02;                    // alignment pull
          }
        }
        if (n > 0) { ax += (cx / n - b.x) * 0.35; ay += (cy / n - b.y) * 0.35; }
        // gentle swirl + walls
        ax += Math.sin(b.y * 0.004) * 14; ay += Math.cos(b.x * 0.004) * 14;
        if (b.x < 60) ax += 200; if (b.x > W - 60) ax -= 200;
        if (b.y < 60) ay += 200; if (b.y > H - 60) ay -= 200;
        b.vx += ax * dt; b.vy += ay * dt;
        const sp = Math.hypot(b.vx, b.vy), max = 190, min = 70;
        if (sp > max) { b.vx *= max / sp; b.vy *= max / sp; }
        if (sp < min && sp > 0.01) { b.vx *= min / sp; b.vy *= min / sp; }
        b.x += b.vx * dt; b.y += b.vy * dt;
      }
    }
    const clampI = (v, a, b) => Math.max(a, Math.min(b, Math.floor(v)));

    function drawBoids() {
      // trail fade instead of clear — the ghosts economy
      bcx.globalCompositeOperation = 'source-over';
      bcx.fillStyle = 'rgba(8,7,10,0.16)';
      bcx.fillRect(0, 0, W, H);
      bcx.globalCompositeOperation = 'lighter';
      for (const b of boids) {
        const ang = Math.atan2(b.vy, b.vx);
        const k = ((Math.round(ang / (Math.PI * 2) * 16) % 16) + 16) % 16;
        bcx.drawImage(sprites[k], b.x - 8, b.y - 8, 18, 18);
      }
      bcx.globalCompositeOperation = 'source-over';
    }

    /* ---- fourier: the choir redraws a seven ---- */
    let fcv, fcx, coeffs = [], trace = [], fT = 0;
    function buildFourier() {
      // a chunky "7" as a closed path (the witness number)
      const pts = [];
      const P = [
        [18, 16], [82, 16], [84, 26], [46, 92], [34, 92], [66, 32], [18, 30],
      ];
      for (let i = 0; i < P.length; i++) {
        const a = P[i], b = P[(i + 1) % P.length];
        for (let t = 0; t < 1; t += 0.09) pts.push([a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]);
      }
      const n = pts.length;
      coeffs = [];
      for (let k = 0; k < Math.min(28, n); k++) {
        let re = 0, im = 0;
        for (let t = 0; t < n; t++) {
          const ang = -2 * Math.PI * k * t / n;
          re += pts[t][0] * Math.cos(ang) - pts[t][1] * Math.sin(ang);
          im += pts[t][0] * Math.sin(ang) + pts[t][1] * Math.cos(ang);
        }
        re /= n; im /= n;
        coeffs.push({ re, im, freq: k });
      }
    }
    function drawFourier(dt) {
      fcx.clearRect(0, 0, W, H);
      const cxp = W / 2, cyp = H / 2, scale = Math.min(W, H) / 130;
      fT += dt * 0.55;
      const twoPiT = fT % 1;
      let x = 0, y = 0;
      fcx.lineWidth = 1.1 * dpr;
      for (let i = 0; i < coeffs.length; i++) {
        const c = coeffs[i];
        const ang = twoPiT * c.freq * Math.PI * 2;
        const r = Math.hypot(c.re, c.im);
        const px = x, py = y;
        x += c.re * Math.cos(ang) - c.im * Math.sin(ang);
        y += c.re * Math.sin(ang) + c.im * Math.cos(ang);
        if (i > 0) {
          fcx.strokeStyle = 'rgba(126,92,232,0.22)';
          fcx.beginPath(); fcx.arc(cxp + px * scale, cyp + py * scale, r * scale, 0, 7); fcx.stroke();
        }
        fcx.strokeStyle = 'rgba(180,145,255,0.30)';
        fcx.beginPath();
        fcx.moveTo(cxp + px * scale, cyp + py * scale);
        fcx.lineTo(cxp + x * scale, cyp + y * scale);
        fcx.stroke();
      }
      trace.push([cxp + x * scale, cyp + y * scale]);
      if (trace.length > 620) trace.shift();
      fcx.strokeStyle = '#F8F4F2';
      fcx.lineWidth = 2.2 * dpr;
      fcx.shadowColor = 'rgba(180,145,255,0.8)'; fcx.shadowBlur = 12 * dpr;
      fcx.beginPath();
      trace.forEach((p, i) => i ? fcx.lineTo(p[0], p[1]) : fcx.moveTo(p[0], p[1]));
      fcx.stroke();
      fcx.shadowBlur = 0;
    }

    define('machine', {
      enter(fast) {
        live = true;
        bcv = $('#boids-canvas'); bcx = bcv.getContext('2d');
        fcv = $('#fourier-canvas'); fcx = fcv.getContext('2d');
        const rs = () => {
          dpr = Math.min(2, devicePixelRatio || 1);
          const r = $('#mr-live').getBoundingClientRect();
          W = bcv.width = fcv.width = Math.floor(r.width * dpr);
          H = bcv.height = fcv.height = Math.floor(r.height * dpr);
        };
        rs(); addEventListener('resize', rs);
        makeDirSprites(); seedBoids(); buildFourier(); trace = []; fT = 0;
        FX.setMood({ hue: 265, density: 0.35 });

        const ph = fast ? () => { enterPhase(2); enterPhase(3); } : null;
        if (fast) { ph(); return; }
        caption('2,400 boids — live, in this browser', { hold: 3.2 });
        after(3.4, () => caption('the plate beside it is the real render — same rules, other bench', { measured: 'renders/swarm/metrics.txt', hold: 3.2 }));
        after(8.0, () => enterPhase(2));       // fourier
        after(14.2, () => enterPhase(3));      // ceilings
      },
      exit() { live = false; },
      tick(t, dt) {
        if (!live) return;
        const dtc = Math.min(dt, 0.033);
        if (!$('#fourier-canvas').classList.contains('hidden')) { drawFourier(dtc); audit.shapes += 30; }
        else { stepBoids(dtc); drawBoids(); audit.shapes += 2400; }
      },
    });

    function enterPhase(p) {
      if (p === 2) {
        $('#boids-canvas').classList.add('hidden');
        const f = $('#fourier-canvas'); f.classList.remove('hidden');
        $('#mr-hud-n').textContent = 'the fourier choir';
        $('#mr-hud-sub').textContent = '28 epicycles — drawing your seven, live';
        $('#mr-card-swarm').classList.remove('on');
        $('#mr-card-fourier').classList.add('on');
        caption('a line re-drawn by its own choir — of terms, not pixels', { measured: 'renders/fourier/metrics.txt', hold: 3 });
      }
      if (p === 3) {
        $('#mr-wrap').classList.add('lifted');
        $('#mr-ceilings').classList.add('on');
        caption('the ceilings — where the renderer stops complaining', { hold: 3 });
        after(3.2, () => caption('nothing in this film is limited by the renderer', { hold: 3.2 }));
      }
    }
  }

  /* ----------------------------------------------------------- THE RECEIPTS */
  function sceneBench() {
    const root = $('#sc-bench');
    root.innerHTML = `
      <div class="bench-wrap">
        <div class="bench-card" id="bench-card">
          <div class="bc-h">device suite · CI artifact</div>
          <div class="bc-row">
            <div class="bc-metric"><span class="bc-v" id="bench-fps">0.0</span><span class="bc-u">fps</span><span class="bc-l">median</span></div>
            <div class="bc-metric"><span class="bc-v" id="bench-ms">0.00</span><span class="bc-u">ms</span><span class="bc-l">frame · median</span></div>
          </div>
          <div class="bc-device">Redmi Note 7 Pro <span>(2019)</span> · Android · vieww on winit</div>
          <svg class="bc-ci" viewBox="0 0 520 64">
            <path id="ci-path" d="M 8 46 H 60 L 74 46 L 88 22 L 102 46 H 150 L 164 46 L 178 22 L 192 46 H 250 L 264 46 L 278 22 L 292 46 H 350 L 364 46 L 378 22 L 392 46 H 450 L 464 46 L 478 22 L 492 46 H 512" />
            <g id="ci-ticks"></g>
          </svg>
          <div class="bc-ci-label">the device-suite timeline — every gate green</div>
        </div>
      </div>`;
    define('bench', {
      enter(fast) {
        $('#bench-card').classList.remove('on'); void $('#bench-card').offsetWidth;
        $('#bench-card').classList.add('on');
        const ticks = $('#ci-ticks');
        ticks.innerHTML = '';
        if (!fast) {
          caption('the numbers arrived as files — CI artifacts, untouched', { hold: 3.2 });
          const path = $('#ci-path');
          const len = path.getTotalLength();
          path.style.strokeDasharray = len; path.style.strokeDashoffset = len;
          path.getBoundingClientRect();
          path.style.transition = 'stroke-dashoffset 2.6s cubic-bezier(.22,1,.36,1)';
          path.style.strokeDashoffset = 0;
          after(2.2, () => {
            FX.countUp($('#bench-fps'), 59.3, 1.6, v => v.toFixed(1));
            FX.countUp($('#bench-ms'), 4.09, 1.6, v => v.toFixed(2));
            Audio.enabled && Audio.blip(220, 0.6, 'sine', 0.14, 440);
            for (let i = 0; i < 12; i++) {
              after(i * 0.13, () => {
                const c = document.createElementNS('http://www.w3.org/2000/svg', 'circle');
                c.setAttribute('cx', 20 + i * 43); c.setAttribute('cy', 46); c.setAttribute('r', 3);
                ticks.appendChild(c);
                Audio.enabled && Audio.blip(900 + i * 60, 0.04, 'sine', 0.03);
              });
            }
            caption('that’s a 2019 phone.', { measured: 'ci/mobile device-suite', hold: 3.4 });
          });
        } else {
          $('#bench-fps').textContent = '59.3'; $('#bench-ms').textContent = '4.09';
          $('#ci-path').style.strokeDashoffset = 0;
          for (let i = 0; i < 12; i++) {
            const c = document.createElementNS('http://www.w3.org/2000/svg', 'circle');
            c.setAttribute('cx', 20 + i * 43); c.setAttribute('cy', 46); c.setAttribute('r', 3);
            ticks.appendChild(c);
          }
        }
      },
    });
  }

  /* ------------------------------------------------------------- THE UNFOLD */
  function sceneUnfold() {
    const root = $('#sc-unfold');
    root.innerHTML = `
      <div class="uf-floor"></div>
      <div class="uf-stage">
        <div class="uf-app" id="uf-app">
          <div class="app-heading">Counter</div>
          <div class="app-label">Tapped 7 times</div>
          <button class="app-btn">Add one</button>
        </div>
        <div class="uf-planes" id="uf-planes">
          <div class="uf-plane pw">
            <div class="ufp-h"><i class="pill w">W</i>widget</div>
            <div class="ufp-sub">description — one frame</div>
            <div class="ufp-tree">
              <div class="trow d0">column</div>
              <div class="trow d1">heading “Counter”</div>
              <div class="trow d1">label “Tapped 7 times”</div>
              <div class="trow d1">button “Add one”</div>
            </div>
          </div>
          <div class="uf-plane pe">
            <div class="ufp-h"><i class="pill e">E</i>element</div>
            <div class="ufp-sub">identity + state — across frames</div>
            <div class="ufp-tree">
              <div class="trow d0">column · build 2</div>
              <div class="trow d1">heading · build 2</div>
              <div class="trow d1 lit">label · build 12</div>
              <div class="trow d1">button · build 5</div>
            </div>
          </div>
          <div class="uf-plane pr">
            <div class="ufp-h"><i class="pill r">R</i>render object</div>
            <div class="ufp-sub">geometry — to the sub-pixel</div>
            <div class="ufp-tree">
              <div class="trow d0">paragraph · 1 glyph run</div>
              <div class="trow d1">rect 214 × 28</div>
              <div class="trow d1">rect 132 × 44 · radius 12</div>
              <div class="trow d1">raster 1,543 px</div>
            </div>
          </div>
        </div>
        <div class="uf-signal" id="uf-signal">
          <div class="ufs-node">7</div>
          <div class="ufs-label">signal · count</div>
          <svg class="ufs-tether" viewBox="0 0 200 40"><path d="M 4 20 H 196" /></svg>
        </div>
      </div>`;
    define('unfold', {
      enter(fast) {
        root.classList.remove('ph2', 'ph3', 'ph4');
        if (!fast) {
          FX.setMood({ hue: 265, density: 0.5 });
          caption('what you watched — from outside', { hold: 2.6 });
          after(2.6, () => { root.classList.add('ph2'); caption('depth opens', { hold: 1.6 }); });
          after(4.4, () => {
            root.classList.add('ph3');
            Audio.enabled && Audio.blip(160, 0.8, 'sine', 0.16, 320);
            caption('description · identity · geometry — three trees, one truth', { hold: 3.6 });
          });
          after(10.6, () => {
            root.classList.add('ph4');
            Audio.enabled && Audio.blip(520, 0.9, 'sine', 0.14, 260);
            caption('state lives outside the tree — the signal, reading seven', { measured: 'runtime().signal(v) — the read is the subscription', hold: 4 });
          });
          after(15.2, () => caption('this is the whole framework, in one picture', { hold: 2.4 }));
        } else root.classList.add('ph2', 'ph3', 'ph4');
      },
      exit() { root.classList.remove('ph2', 'ph3', 'ph4'); },
    });
  }

  function init() {
    actCard3(); sceneMirror(); sceneMachine(); sceneBench(); sceneUnfold();
  }
  return { init };
})();
