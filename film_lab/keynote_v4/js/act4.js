/* ============================================================================
   keynote v4 — act4.js
   ACT IV · KETSU (結) — THE RECONCILIATION
   the fold-back (planes → the studio, one thing) · built on vieww (36
   crates, one renderer) · the green ledger (90 plates, tests green, 0
   open) · the end card (install · repo · the live manifest) · the loop.
   ========================================================================== */
const Act4 = (() => {
  'use strict';
  const { define, caption, captionOff, hand, rail, $, $$, audit, after } = FilmInternals;

  /* ------------------------------------------------------------ act card */
  function actCard4() {
    const a = ACTS.ketsu;
    $('#sc-act4').innerHTML = `
      <div class="actcard">
        <div class="ac-kanji">${a.kanji}</div><div class="ac-rule"></div>
        <div class="ac-romaji">${a.romaji}</div><div class="ac-name">${a.name}</div>
        <div class="ac-sub">${a.sub}</div>
      </div>`;
    define('act4', { enter() { captionOff(); FX.setMood({ hue: 265, density: 0.3 }); } });
  }

  /* ---------------------------------------------------------- THE FOLD-BACK */
  function sceneFoldback() {
    const root = $('#sc-foldback');
    root.innerHTML = `
      <div class="fb-planes">
        <div class="fb-plane w"></div>
        <div class="fb-plane e"></div>
        <div class="fb-plane r"></div>
      </div>`;
    define('foldback', { studio: true,
      enter(fast) {
        root.classList.remove('go'); void root.offsetWidth; root.classList.add('go');
        const S = $('#studio-app');
        Studio.reset(); Studio.showFinal(); Studio.setCount(7, false);
        S.classList.add('show', 'flattening');
        if (!fast) {
          caption('the fold-back — fast, spring-eased', { hold: 2.2 });
          after(2.6, () => caption('everything you watched was one thing', { hold: 2.8 }));
        }
      },
      exit() { $('#studio-app').classList.remove('flattening'); },
    });
  }

  /* -------------------------------------------------------- BUILT ON VIEWW */
  function sceneStack() {
    const root = $('#sc-stack');
    root.innerHTML = `
      <div class="stack-wrap">
        <div class="st-stack">
          <div class="stk-layer l1"><span class="stk-name">your app</span><span class="stk-note">screen() — one tree, every frame</span></div>
          <div class="stk-layer l2"><span class="stk-name">viewwstudio</span><span class="stk-note">buffers · say · live preview · devices · devtools</span></div>
          <div class="stk-layer l3">
            <div class="stk-l3-head"><span class="stk-name">vieww</span><span class="stk-note">36 crates — one renderer, all the way down</span></div>
            <div class="stk-crates" id="stk-crates"></div>
          </div>
          <div class="stk-caps">
            <div class="stk-cap"><b>4×</b> supersampled rasterizer · <b>28</b> blend modes · <b>16</b>-stop gradients</div>
            <div class="stk-cap"><b>springs</b> · <b>signals</b> · <b>damage tracking</b> — at 60</div>
            <div class="stk-cap"><b>desktop · android · web</b> — one codebase</div>
          </div>
        </div>
      </div>`;
    const crateEl = $('#stk-crates');
    CRATES.forEach((c, i) => {
      const d = document.createElement('span');
      d.className = 'crate';
      d.textContent = c.replace('vieww-', '').replace('vieww', 'vieww ·core');
      d.style.setProperty('--i', i);
      crateEl.appendChild(d);
    });
    define('stack', {
      enter(fast) {
        $('#studio-app').classList.remove('show');
        root.classList.remove('go'); void root.offsetWidth; root.classList.add('go');
        if (!fast) {
          FX.setMood({ hue: 265, density: 0.4 });
          caption('the studio stands on the engine — and shows it to you', { hold: 3.2 });
          after(3.4, () => caption('vieww — 36 crates, from widget to metal', { measured: 'vieww_base/crates — counted, not rounded', hold: 3.2 }));
          after(7.0, () => caption('its own rasterizer — no platform controls passed the buck', { hold: 3.2 }));
          after(0.4, () => Audio.enabled && Audio.blip(120, 1.2, 'sine', 0.15, 240));
        }
      },
    });
  }

  /* -------------------------------------------------------- THE GREEN LEDGER */
  function sceneLedger() {
    const root = $('#sc-ledger');
    const plates = [
      ['eclipse', 'eclipse — totality as a light sequence'],
      ['ink', 'ink — one drop becomes a nebula'],
      ['quantum', 'quantum — Born-rule fringes, measured'],
      ['lorenz', 'lorenz — λ = 0.878, beside 0.906'],
      ['tesseract', 'tesseract — 4D through 3D to your 2D'],
      ['bubble', 'bubble — 48-wavelength interference'],
      ['galton', 'galton — σ at −0.5% vs theory'],
      ['forest', 'forest — four seasons, one law'],
    ];
    root.innerHTML = `
      <div class="lg-wrap">
        <div class="lg-left">
          <div class="lg-h">the green ledger</div>
          <div class="lg-rows">
            <div class="lg-row"><span class="lg-v" id="lg-plates">0</span><span class="lg-l">plates rendered — every one ships sheet · gif · metrics</span></div>
            <div class="lg-row"><span class="lg-v tests">518<span class="lg-plus">+237</span><span class="lg-plus">+13</span></span><span class="lg-l">tests green across the workspace</span></div>
            <div class="lg-row"><span class="lg-v zero">0</span><span class="lg-l">upgrades open — U-01 … U-23, all closed</span></div>
            <div class="lg-row"><span class="lg-v">4</span><span class="lg-l">real apps shipped — three · vavlt · upxcale · snapsearch</span></div>
          </div>
          <div class="lg-note">no number on this wall was typed by a human —<br>each arrived as a receipt</div>
        </div>
        <div class="lg-mosaic">
          ${plates.map(([p, cap]) => `
            <figure class="lg-cell" style="--d:${(Math.random() * 0.9).toFixed(2)}s">
              <img src="assets/plates/${p}.gif" alt="${cap}">
              <figcaption>${cap}</figcaption>
            </figure>`).join('')}
        </div>
      </div>`;
    define('ledger', {
      enter(fast) {
        root.classList.remove('go'); void root.offsetWidth; root.classList.add('go');
        if (!fast) {
          caption('before this film — the bench did its homework', { hold: 3 });
          after(1.6, () => {
            FX.countUp($('#lg-plates'), 90, 1.8);
            Audio.enabled && Audio.blip(220, 0.8, 'sine', 0.12, 440);
          });
          after(4.2, () => caption('receipts over claims — 90 plates, all deterministic', { measured: 'film_lab/renders — the three-artifact set', hold: 3.4 }));
          after(8.6, () => caption('the upgrades ledger: 23 found, 23 fixed, 0 open', { measured: 'todo-upgrades.md — re-audited after round 11', hold: 3.4 }));
          after(12.4, () => caption('four apps already live on it', { hold: 2.4 }));
        }
      },
    });
  }

  /* ------------------------------------------------------------ THE END CARD */
  function sceneEndcard() {
    const root = $('#sc-endcard');
    root.innerHTML = `
      <div class="ec-wrap">
        <div class="ec-mark"><span class="ec-chrome">vieww</span><span class="ec-studio">studio</span></div>
        <div class="ec-reflect" aria-hidden="true"><span class="ec-chrome">vieww</span><span class="ec-studio">studio</span></div>
        <div class="ec-install">
          <div class="ec-cmd"><span class="ec-p">$</span> curl -fsSL https://vieww.dev | sh</div>
          <div class="ec-cmd win"><span class="ec-p ps">ps></span> irm https://vieww.dev/install.ps1 | iex</div>
        </div>
        <div class="ec-repo" id="ec-repo">${RECEIPTS.repo}</div>
        <div class="ec-manifest" id="ec-manifest">
          <div class="ecm-h">this film, audited live in your browser</div>
          <div class="ecm-bars">
            <div class="ecm-bar"><span class="ecm-l">scenes</span><span class="ecm-track"><i id="bar-scenes"></i></span><span class="ecm-v" id="val-scenes">0</span></div>
            <div class="ecm-bar"><span class="ecm-l">frames drawn</span><span class="ecm-track"><i id="bar-frames"></i></span><span class="ecm-v" id="val-frames">0</span></div>
            <div class="ecm-bar"><span class="ecm-l">canvas shapes</span><span class="ecm-track"><i id="bar-shapes"></i></span><span class="ecm-v" id="val-shapes">0</span></div>
            <div class="ecm-bar"><span class="ecm-l">witness taps</span><span class="ecm-track"><i id="bar-taps"></i></span><span class="ecm-v" id="val-taps">0</span></div>
          </div>
        </div>
        <div class="ec-sting" id="ec-sting">every plate in this film was rendered by vieww —<br>this page simply cut them together.</div>
      </div>`;
    define('endcard', {
      enter(fast) {
        root.classList.remove('go'); void root.offsetWidth; root.classList.add('go');
        $('#ec-sting').classList.remove('on');
        if (!fast) {
          FX.setMood({ hue: 265, density: 0.22 });
          after(1.8, () => {
            const a = audit;
            $('#val-scenes').textContent = Film.scenes.length;
            $('#val-frames').textContent = a.frames.toLocaleString();
            $('#val-shapes').textContent = a.shapes.toLocaleString();
            $('#val-taps').textContent = a.taps;
            const max = { frames: 16000, shapes: 400000, taps: 8, scenes: Film.scenes.length };
            $('#bar-scenes').style.width = '100%';
            $('#bar-frames').style.width = Math.min(100, a.frames / max.frames * 100) + '%';
            $('#bar-shapes').style.width = Math.min(100, a.shapes / max.shapes * 100) + '%';
            $('#bar-taps').style.width = Math.min(100, a.taps / max.taps * 100) + '%';
          });
          after(5.2, () => { $('#ec-repo').classList.add('lit'); Audio.enabled && Audio.blip(180, 0.7, 'sine', 0.14, 360); });
          after(10.0, () => $('#ec-sting').classList.add('on'));
        } else {
          $('#ec-repo').classList.add('lit'); $('#ec-sting').classList.add('on');
        }
      },
    });
  }

  /* --------------------------------------------------------------- THE LOOP */
  function sceneLoop() {
    const root = $('#sc-loop');
    root.innerHTML = `
      <div class="lp-wrap">
        <div class="lp-studio">
          <div class="lp-titlebar">
            <span class="st-dots"><i class="r"></i><i class="y"></i><i class="g"></i></span>
            <span class="lp-title">viewwstudio — <em>endcard</em></span>
            <span class="st-chip">session <b id="lp-chip">04:32</b></span>
          </div>
          <div class="lp-body">
            <div class="lp-side">
              <div class="side-h">buffers</div>
              <div class="side-item say on"><span class="lang-pill">say</span>endcard.say</div>
              <div class="side-item rust"><span class="lang-pill rs">rs</span>endcard.rs</div>
            </div>
            <div class="lp-preview">
              <div class="lp-pv-chrome">preview</div>
              <div class="lp-pv-body">
                <div class="lp-mark"><span class="ec-chrome">vieww</span><span class="ec-studio">studio</span></div>
                <div class="lp-repo">${RECEIPTS.repo}</div>
              </div>
            </div>
          </div>
          <div class="lp-status"><span>say · 14 lines · remounts 26</span><span>build 0.81 ms · raster 2.02 ms</span></div>
        </div>
      </div>`;
    define('loop', {
      enter(fast) {
        root.classList.remove('go'); void root.offsetWidth; root.classList.add('go');
        const e = Film.state.sessionStart ? (performance.now() - Film.state.sessionStart) / 1000 : 0;
        const m = Math.floor(e / 60), s = Math.floor(e % 60);
        $('#lp-chip').textContent = String(m).padStart(2, '0') + ':' + String(s).padStart(2, '0');
        if (!fast) {
          caption('the end card was one more buffer — the session never ended', { hold: 4 });
          after(4.4, () => caption('it rendered the film', { hold: 2.6 }));
          FX.setMood({ hue: 265, density: 0.12 });
        }
      },
    });
  }

  function init() {
    actCard4(); sceneFoldback(); sceneStack(); sceneLedger(); sceneEndcard(); sceneLoop();
  }
  return { init };
})();
