/* ============================================================================
   keynote v4 — act2.js
   ACT II · SHŌ (承) — THE SESSION
   The hero at work: one studio, one buffer, one session across eight
   scenes — first paint → descent → live compose → receipts → carry →
   damage → the world tour. The witness counter climbs 1…7.
   All choreography runs on film time (after()/tick) — pause-proof.
   ========================================================================== */
const Act2 = (() => {
  'use strict';
  const { define, caption, captionOff, hand, rail, $, $$, after } = FilmInternals;

  /* --------------------------------------------------------------- highlight */
  const esc = s => s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');

  function hlSay(line) {
    if (line.startsWith('--')) return `<span class="c-com">${esc(line)}</span>`;
    let out = '', i = 0;
    const kw = new Set(['keep', 'screen', 'a', 'an', 'which', 'when', 'tapped:', 'add', 'to', 'from', 'starting', 'at', 'called', 'children', 'aligned', 'spaced']);
    while (i < line.length) {
      const ch = line[i];
      if (ch === '"') {
        let j = i + 1; while (j < line.length && line[j] !== '"') j++;
        out += `<span class="c-str">${esc(line.slice(i, j + 1))}</span>`; i = j + 1;
      } else if (ch === '\\') {
        let j = i + 1; while (j < line.length && line[j] !== ')') j++;
        out += `<span class="c-sig">${esc(line.slice(i, j + 1))}</span>`; i = j + 1;
      } else if (/[0-9]/.test(ch)) {
        let j = i; while (j < line.length && /[0-9]/.test(line[j])) j++;
        out += `<span class="c-num">${esc(line.slice(i, j))}</span>`; i = j;
      } else if (/[A-Za-z_]/.test(ch)) {
        let j = i; while (j < line.length && /[A-Za-z_\\]/.test(line[j])) j++;
        const w = line.slice(i, j);
        out += kw.has(w) ? `<span class="c-kw">${esc(w)}</span>` : esc(w);
        i = j;
      } else { out += esc(ch); i++; }
    }
    return out;
  }

  function hlRust(line) {
    if (line.trim().startsWith('//')) return `<span class="c-com">${esc(line)}</span>`;
    let out = '', i = 0;
    const kw = new Set(['use', 'fn', 'let', 'impl', 'move', 'return', 'pub', 'struct', 'mut', 'new', 'main', 'Result', 'Box']);
    while (i < line.length) {
      const ch = line[i];
      if (ch === '/' && line[i + 1] === '/') { out += `<span class="c-com">${esc(line.slice(i))}</span>`; break; }
      if (ch === '"') {
        let j = i + 1; while (j < line.length && line[j] !== '"') j++;
        out += `<span class="c-str">${esc(line.slice(i, j + 1))}</span>`; i = j + 1;
      } else if (/[0-9]/.test(ch)) {
        let j = i; while (j < line.length && /[0-9.]/.test(line[j])) j++;
        out += `<span class="c-num">${esc(line.slice(i, j))}</span>`; i = j;
      } else if (/[A-Za-z_]/.test(ch)) {
        let j = i; while (j < line.length && /[A-Za-z_0-9]/.test(line[j])) j++;
        const w = line.slice(i, j);
        if (kw.has(w)) out += `<span class="c-kw">${esc(w)}</span>`;
        else if (/^[A-Z]/.test(w)) out += `<span class="c-type">${esc(w)}</span>`;
        else if (line[j] === '(' || line[j] === '!') out += `<span class="c-fn">${esc(w)}</span>`;
        else out += esc(w);
        i = j;
      } else { out += esc(ch); i++; }
    }
    return out;
  }

  /* --------------------------------------------------- the typer (film time) */
  let typer = null;
  function startTyper(pane, lines, hl, opts) {
    if (!opts.append) pane.innerHTML = '';
    typer = {
      pane, lines, hl, cps: opts.cps || 40, lineDelay: opts.lineDelay || 0.12,
      append: !!opts.append,
      baseRows: opts.append ? pane.querySelectorAll('.code-row').length : 0,
      budget: 0, li: 0, ci: 0, done: false,
      onDone: opts.onDone || null, lastAudio: 0,
    };
  }
  function stopTyper() { typer = null; }
  function tickTyper(dt) {
    const ty = typer;
    if (!ty || ty.done) return;
    ty.budget += dt;
    let guard = 0;
    while (ty.budget > 0 && !ty.done && guard++ < 60) {
      const line = ty.lines[ty.li];
      if (line === undefined) { ty.done = true; ty.onDone && ty.onDone(); break; }
      if (line === '') {
        appendRow(ty, '');
        ty.li++; ty.ci = 0;
        ty.budget -= ty.lineDelay * 0.35;
        continue;
      }
      if (ty.ci >= line.length) {
        ty.li++; ty.ci = 0;
        ty.budget -= ty.lineDelay;
        continue;
      }
      const take = Math.min(line.length - ty.ci, ty.budget * ty.cps);
      ty.ci += take;
      ty.budget -= take / ty.cps;
      const row = ensureRow(ty);
      const shown = line.slice(0, Math.floor(ty.ci));
      row.querySelector('.lc').innerHTML = ty.hl(shown);
      if (Math.floor(ty.ci) > ty.lastAudio) {
        ty.lastAudio = Math.floor(ty.ci);
        Audio.enabled && Audio.blip(2300 + Math.random() * 400, 0.008, 'square', 0.007);
      }
    }
  }
  function ensureRow(ty) {
    const rows = ty.pane.querySelectorAll('.code-row');
    const want = ty.baseRows + ty.li + 1;
    if (rows.length < want) return appendRow(ty, ty.lines[ty.li] || '');
    return rows[rows.length - 1];
  }
  function appendRow(ty, text) {
    const row = document.createElement('div');
    row.className = 'code-row' + (ty.append ? ' new' : '');
    const n = ty.pane.querySelectorAll('.code-row').length + 1;
    row.innerHTML = `<span class="ln">${n}</span><span class="lc">${text === '' ? '' : ty.hl(text)}</span>`;
    ty.pane.appendChild(row);
    return row;
  }

  function renderCode(pane, lines, hl) {
    pane.innerHTML = lines.map((l, i) =>
      `<div class="code-row"><span class="ln">${i + 1}</span><span class="lc">${l === '' ? '' : hl(l)}</span></div>`).join('');
  }

  /* ------------------------------------------------------------- the studio */
  let S = null;
  const app = { count: 0, dial: false, dialV: 7, err: false };

  function buildStudio() {
    S = document.createElement('div');
    S.id = 'studio-app';
    S.innerHTML = `
      <div class="st-glow"></div>
      <div class="st-window">
        <div class="st-titlebar">
          <span class="st-dots"><i class="r"></i><i class="y"></i><i class="g"></i></span>
          <span class="st-title">viewwstudio — <em>counter</em></span>
          <span class="st-chip">session <b id="st-chip-time">00:00</b></span>
        </div>
        <div class="st-body">
          <div class="st-side">
            <div class="side-h">buffers</div>
            <div class="side-item say on" id="tab-say"><span class="lang-pill">say</span>counter.say</div>
            <div class="side-item rust" id="tab-rust"><span class="lang-pill rs">rs</span>counter.rs</div>
            <div class="side-h">assets</div>
            <div class="side-item dim">fonts/</div>
            <div class="side-item dim">icons/</div>
          </div>
          <div class="st-main">
            <div class="st-tabs">
              <div class="tab on" id="tabbar-say"><span class="lang-pill">say</span>counter.say<i class="tab-x">×</i></div>
              <div class="tab" id="tabbar-rust"><span class="lang-pill rs">rs</span>counter.rs<i class="tab-x">×</i></div>
            </div>
            <div class="st-editor" id="editor">
              <div class="code-scroll">
                <div class="code-pane" id="pane-say"></div>
                <div class="code-pane rust" id="pane-rust" style="display:none"></div>
                <div class="scanline" id="scanline"></div>
              </div>
            </div>
            <div class="st-problems" id="problems">
              <div class="pr-h">problems · 1</div>
              <div class="pr-row"><span class="pr-dot"></span>unknown widget: <b>diall</b> — counter.say:10 <span class="pr-fix">the language reserved “a dial” — one keystroke fixes it</span></div>
            </div>
          </div>
          <div class="st-preview">
            <div class="pv-chrome">
              <span class="pv-title">preview</span>
              <span class="pv-dev" id="pv-dev">desktop · live</span>
            </div>
            <div class="pv-stage">
              <div class="pv-frame">
                <div class="app-card" id="app-card">
                  <div class="app-heading">Counter</div>
                  <div class="app-label" id="app-label">Tapped 0 times</div>
                  <div class="app-dial" id="app-dial">
                    <svg viewBox="0 0 100 62">
                      <path class="dial-track" d="M 10 56 A 44 44 0 0 1 90 56" />
                      <path class="dial-fill" id="dial-fill" d="M 10 56 A 44 44 0 0 1 90 56" />
                      <line class="dial-needle" id="dial-needle" x1="50" y1="56" x2="50" y2="18" />
                      <circle class="dial-pivot" cx="50" cy="56" r="3.4" />
                    </svg>
                    <div class="dial-val" id="dial-val">7</div>
                  </div>
                  <button class="app-btn" id="app-btn">Add one</button>
                </div>
                <div class="app-error" id="app-error">
                  <div class="ae-h">error boundary</div>
                  <div class="ae-b">unknown widget: <b>diall</b></div>
                  <div class="ae-f">counter.say:10 · everything else kept rendering</div>
                </div>
                <div class="dmg-rect" id="dmg-rect"></div>
                <div class="pv-badges" id="pv-badges"></div>
              </div>
              <div class="sigma-hud" id="sigma-hud">
                <div>editor σ <b id="sig-ed">0.0</b></div>
                <div>preview σ <b id="sig-pv">0.0</b></div>
              </div>
              <div id="coalescer">
                <div class="coal-funnel"></div>
                <div class="coal-out"></div>
                <div id="coal-count"></div>
              </div>
            </div>
          </div>
        </div>
        <div class="st-statusbar">
          <span id="sb-left">say · 9 lines · remounts 1</span>
          <span id="sb-right">build 0.87 ms · raster 2.31 ms</span>
        </div>
      </div>`;
    document.getElementById('stage').appendChild(S);
  }

  function setCount(n, fromTap) {
    app.count = n;
    $('#app-label').textContent = `Tapped ${n} times`;
    $$('#devices .app-label').forEach(el => el.textContent = `Tapped ${n} times`);
    if (fromTap) {
      const b = $('#app-btn');
      b.classList.remove('pressed'); void b.offsetWidth; b.classList.add('pressed');
    }
  }
  function badge(text, x, y) {
    const el = document.createElement('div');
    el.className = 'pv-badge';
    el.textContent = text;
    el.style.left = x + '%'; el.style.top = y + '%';
    $('#pv-badges').appendChild(el);
    setTimeout(() => el.classList.add('on'), 20);
  }
  function clearBadges() { $('#pv-badges').innerHTML = ''; }

  function showStudio(v) { S.classList.toggle('show', v); }
  function studioMode(cls, on) { S.classList.toggle(cls, !!on); }

  function setDial(v, spring) {
    app.dialV = v;
    const frac = v / 10;
    const ang = -90 + frac * 180;           // 0 → 9 o'clock, 10 → 3 o'clock
    const needle = $('#dial-needle');
    needle.style.transformOrigin = '50px 56px';
    needle.style.transform = `rotate(${ang}deg)`;
    if (spring) needle.classList.add('springy'); else needle.classList.remove('springy');
    $('#dial-fill').style.strokeDasharray = `${138 * frac} 400`;
    $('#dial-val').textContent = v;
  }

  /* the session's later scenes all show the same final say pane */
  function showFinalSayPane() {
    $('#pane-say').style.display = '';
    $('#pane-rust').style.display = 'none';
    $('#tabbar-say').classList.add('on');
    $('#tabbar-rust').classList.remove('on');
    renderCode($('#pane-say'), [...SAY_CODE, SAY_DIAL], hlSay);
    $('#app-dial').classList.add('on');
    setDial(7, false);
  }

  function resetStudioUI() {
    $('#problems').classList.remove('on');
    $('#app-error').classList.remove('on');
    $('#app-dial').classList.remove('dimmed');
    $('#dmg-rect').classList.remove('on');
    $('#sigma-hud').classList.remove('on');
    clearBadges();
    studioMode('rack-preview', false);
    studioMode('coalescer', false);
    const c = $('#coalescer');
    c.classList.remove('fired');
    c.querySelectorAll('.coal-tick').forEach(t => t.remove());
    $('#coal-count').textContent = '';
    $('#pane-say').style.display = '';
    $('#pane-rust').style.display = 'none';
    $('#tabbar-say').classList.add('on');
    $('#tabbar-rust').classList.remove('on');
    $('#scanline').classList.remove('sweep');
    $('#sb-left').textContent = 'say · 9 lines · remounts 1';
  }

  /* ============================================================ the scenes */

  function actCard2() {
    const a = ACTS.sho;
    $('#sc-act2').innerHTML = `
      <div class="actcard">
        <div class="ac-kanji">${a.kanji}</div><div class="ac-rule"></div>
        <div class="ac-romaji">${a.romaji}</div><div class="ac-name">${a.name}</div>
        <div class="ac-sub">${a.sub}</div>
      </div>`;
    define('act2', { enter() { captionOff(); showStudio(false); FX.setMood({ hue: 265, density: 0.45 }); } });
  }

  /* --- studio opens ------------------------------------------------------- */
  function sceneStudio() {
    define('studio', { studio: true,
      enter(fast) {
        resetStudioUI();
        showStudio(true); studioMode('assemble', !fast);
        renderCode($('#pane-say'), ['-- say-language: 1'], hlSay);
        setCount(0, false);
        $('#app-card').classList.remove('alive');
        $('#app-dial').classList.remove('on');
        if (!fast) {
          caption('viewwstudio — one buffer, one preview, always alive', { hold: 4 });
          after(5.4, () => caption('the hero: an IDE that is itself a vieww app', { hold: 3.4 }));
        }
      },
      exit() { studioMode('assemble', false); },
      tick(t, dt) { tickTyper(dt); },
    });
  }

  /* --- first paint -------------------------------------------------------- */
  function scenePaint() {
    define('paint', { studio: true,
      enter(fast) {
        resetStudioUI();
        showStudio(true);
        setCount(0, false);
        $('#app-card').classList.remove('alive');
        $('#app-dial').classList.remove('on');
        if (fast) {
          renderCode($('#pane-say'), SAY_CODE, hlSay);
          $('#app-card').classList.add('alive');
          setCount(1, false);
          return;
        }
        caption('three lines of say — English, not Rust', { hold: 4.5 });
        startTyper($('#pane-say'), SAY_CODE, hlSay, {
          cps: 44, lineDelay: 0.14,
          onDone() {
            $('#app-card').classList.add('alive');
            Audio.enabled && Audio.blip(520, 0.4, 'sine', 0.18, 780);
            caption('alive in 0.246 s', { measured: 'census3 · alive probe, this bench', hold: 3.4 });
            badge('build 1', 88, 6);
            after(3.6, () => caption('the first code of a Rust framework — is English', { hold: 3.2 }));
          },
        });
        after(9.0, () => hand.moveTo(63.5, 64));
        after(11.2, () => {
          hand.tap(); setCount(1, true); rail.setWitness(1);
          badge('build 2', 60, 86);
          caption('one — the witness counter opens its ledger', { hold: 3 });
        });
      },
      exit() { stopTyper(); hand.hide(); },
      tick(t, dt) { tickTyper(dt); },
    });
  }

  /* --- the descent -------------------------------------------------------- */
  const RUST_SHOW = RUST_CODE.filter((l, i) =>
    [0, 2, 3, 4, 5, 6, 7, 9, 10, 11, 12, 13, 14, 16, 17, 18, 19, 20, 21, 23, 24].includes(i));

  function sceneDescent() {
    define('descent', { studio: true,
      enter(fast) {
        resetStudioUI();
        showStudio(true);
        setCount(1, false);
        $('#app-dial').classList.remove('on');
        if (fast) {
          $('#tabbar-say').classList.remove('on'); $('#tabbar-rust').classList.add('on');
          $('#pane-say').style.display = 'none';
          $('#pane-rust').style.display = '';
          renderCode($('#pane-rust'), RUST_SHOW, hlRust);
          setCount(2, false);
          return;
        }
        caption('the same buffer — as ordinary Rust', { hold: 3.4 });
        after(0.5, () => {
          $('#scanline').classList.add('sweep');
          Audio.enabled && Audio.blip(300, 0.7, 'sine', 0.1, 900);
        });
        after(1.4, () => {
          $('#tabbar-say').classList.remove('on');
          $('#tabbar-rust').classList.add('on');
          $('#pane-say').style.display = 'none';
          $('#pane-rust').style.display = '';
          renderCode($('#pane-rust'), [], hlRust);
          startTyper($('#pane-rust'), RUST_SHOW, hlRust, {
            cps: 110, lineDelay: 0.03,
            onDone() { caption('state held the door — the counter never reset', { hold: 3 }); },
          });
        });
        after(6.8, () => hand.moveTo(63.5, 64));
        after(8.3, () => {
          hand.tap(); setCount(2, true); rail.setWitness(2);
          caption('two — rust, and still one session', { hold: 2.6 });
        });
      },
      exit() { stopTyper(); hand.hide(); },
      tick(t, dt) { tickTyper(dt); },
    });
  }

  /* --- compose live -------------------------------------------------------- */
  function sceneCompose() {
    define('compose', { studio: true,
      enter(fast) {
        resetStudioUI();
        showStudio(true);
        setCount(2, false);
        setDial(7, false);
        $('#app-dial').classList.remove('on');
        if (fast) {
          renderCode($('#pane-say'), [...SAY_CODE, SAY_DIAL], hlSay);
          $('#app-dial').classList.add('on');
          setDial(7, false);
          setCount(3, false);
          return;
        }
        caption('compose — per keystroke', { hold: 2.6 });
        after(1.0, () => {
          // the say pane returns, showing the counter code + room to grow
          renderCode($('#pane-say'), SAY_CODE, hlSay);
          startTyper($('#pane-say'), [SAY_DIAL], hlSay, {
            cps: 30, lineDelay: 0.1, append: true,
            onDone() {
              $('#app-dial').classList.add('on');
              setDial(7, true);
              badge('build 3', 26, 44);
              caption('a dial grows — the needle is a real spring', { hold: 3 });
            },
          });
        });
        after(6.9, () => {
          const rows = $('#pane-say').querySelectorAll('.code-row');
          const last = rows[rows.length - 1];
          last.classList.add('err-line');
          last.querySelector('.lc').innerHTML = hlSay(SAY_DIAL_ERR);
          $('#app-error').classList.add('on');
          $('#app-dial').classList.add('dimmed');
          $('#problems').classList.add('on');
          Audio.enabled && Audio.blip(220, 0.25, 'triangle', 0.12, 180);
          caption('a mistake is a boundary — not a crash', { hold: 3.4 });
        });
        after(9.4, () => hand.moveTo(40, 52));
        after(11.2, () => {
          hand.tap();
          const rows = $('#pane-say').querySelectorAll('.code-row');
          const last = rows[rows.length - 1];
          last.classList.remove('err-line');
          last.classList.add('fix-pop');
          last.querySelector('.lc').innerHTML = hlSay(SAY_DIAL);
          $('#app-error').classList.remove('on');
          $('#app-dial').classList.remove('dimmed');
          $('#problems').classList.remove('on');
          setDial(7, true);
          Audio.enabled && Audio.blip(660, 0.2, 'sine', 0.1, 880);
          caption('one keystroke — everything else kept rendering', { hold: 2.8 });
        });
        after(13.0, () => hand.moveTo(63.5, 64));
        after(14.3, () => {
          hand.tap(); setCount(3, true); rail.setWitness(3);
          caption('three — composed, live', { hold: 2.2 });
        });
      },
      exit() { stopTyper(); hand.hide(); },
      tick(t, dt) { tickTyper(dt); },
    });
  }

  /* --- the receipt ---------------------------------------------------------- */
  function sceneReceipt() {
    define('receipt', { studio: true,
      enter(fast) {
        resetStudioUI();
        showStudio(true);
        setCount(3, false);
        showFinalSayPane();
        if (fast) {
          $('#sigma-hud').classList.add('on');
          $('#sig-ed').textContent = '14.2'; $('#sig-pv').textContent = '0.0';
          studioMode('rack-preview', true);
          studioMode('coalescer', true);
          $('#coalescer').classList.add('fired');
          $('#coal-count').textContent = '10 writes → 1 rebuild';
          return;
        }
        caption('rack focus — the receipt shot', { hold: 2.4 });
        $('#sigma-hud').classList.add('on');
        // focus pull animated on film time in tick()
        after(1.8, () => caption('both σ values, printed live — no post focus', { measured: 'Filtered::blur — the rackfocus plate', hold: 3 }));
        after(5.6, () => {
          badge('build 4', 88, 8); badge('build 2', 12, 30); badge('build 3', 26, 46); badge('build 2', 60, 84);
          caption('build counts — the element tree’s own account', { hold: 2.8 });
        });
        after(8.8, () => {
          studioMode('coalescer', true);
          caption('10 writes → 1 rebuild — the scheduler coalesces', { measured: 'the scrub plate, counted live', hold: 4 });
        });
      },
      exit() {
        $('#sigma-hud').classList.remove('on');
        studioMode('rack-preview', false);
        studioMode('coalescer', false);
      },
      tick(t, dt) {
        // rack focus 0→1 over the first 1.6s
        if (t < 1.6) {
          const e = Math.min(14.2, t / 1.6 * 14.2);
          $('#sig-ed').textContent = e.toFixed(1);
          $('#sig-pv').textContent = Math.max(0, 14.2 - e).toFixed(1);
          studioMode('rack-preview', t > 0.8);
        } else {
          $('#sig-ed').textContent = '14.2'; $('#sig-pv').textContent = '0.0';
          studioMode('rack-preview', true);
        }
        // the coalescer: ten ticks land, one rebuild fires
        if (t >= 8.8 && t < 10.6) {
          const w = Math.min(10, Math.floor((t - 8.8) / 0.16) + 1);
          const box = $('#coalescer');
          const have = box.querySelectorAll('.coal-tick').length;
          if (w > have) {
            for (let i = have; i < w; i++) {
              const tick = document.createElement('div');
              tick.className = 'coal-tick';
              tick.style.setProperty('--d', '0s');
              box.appendChild(tick);
              Audio.enabled && Audio.blip(1400, 0.02, 'square', 0.02);
            }
          }
        }
        if (t >= 10.6 && !$('#coalescer').classList.contains('fired')) {
          $('#coalescer').classList.add('fired');
          $('#coal-count').textContent = '10 writes → 1 rebuild';
          Audio.enabled && Audio.blip(140, 0.3, 'sine', 0.2, 90);
        }
      },
    });
  }

  /* --- state survives -------------------------------------------------------- */
  function sceneCarry() {
    define('carry', { studio: true,
      enter(fast) {
        resetStudioUI();
        showStudio(true);
        showFinalSayPane();
        if (fast) { setCount(4, false); return; }
        setCount(3, false);
        caption('edited mid-flight — the spring keeps swinging', { hold: 3.2 });
        after(3.4, () => caption('keep survives the recompile — the carry is the shot', { hold: 3.2 }));
        after(6.2, () => hand.moveTo(63.5, 64));
        after(7.6, () => {
          hand.tap(); setCount(4, true); rail.setWitness(4);
          caption('four — carried', { hold: 2 });
        });
      },
      tick(t, dt) {
        // the needle swings on its own spring while the buffer churns
        const ang = Math.sin(t * 2.6) * 26 * Math.exp(-t * 0.06);
        const frac = app.dialV / 10;
        const needle = $('#dial-needle');
        if (needle) needle.style.transform = `rotate(${-90 + frac * 180 + ang}deg)`;
        $('#sb-left').textContent = `say · 10 lines · remounts ${12 + Math.floor(t * 1.7)}`;
      },
      exit() { hand.hide(); },
    });
  }

  /* --- damage in pixels -------------------------------------------------------- */
  function sceneDamage() {
    define('damage', { studio: true,
      enter(fast) {
        resetStudioUI();
        showStudio(true);
        showFinalSayPane();
        setCount(4, false);
        if (fast) { $('#dmg-rect').classList.add('on'); setCount(5, false); return; }
        caption('FrameStats — measured, not typed', { hold: 2.4 });
        after(2.6, () => {
          $('#dmg-rect').classList.add('on');
          Audio.enabled && Audio.blip(320, 0.18, 'triangle', 0.1, 260);
          caption('one write lights one region — 1,543 px', { measured: 'damage tracking · the damage plate', hold: 3.4 });
        });
        after(7.4, () => hand.moveTo(63.5, 64));
        after(9.0, () => {
          hand.tap(); setCount(5, true); rail.setWitness(5);
          caption('five — the write counted', { hold: 2 });
        });
      },
      exit() { $('#dmg-rect').classList.remove('on'); hand.hide(); },
    });
  }

  /* --- the world tour -------------------------------------------------------- */
  function sceneWorld() {
    let built = false;
    define('world', { studio: true,
      enter(fast) {
        resetStudioUI();
        showStudio(true); studioMode('shrink', true);
        showFinalSayPane();
        if (!built) buildDevices(); built = true;
        $('#devices').classList.add('on');
        if (fast) { setCount(7, false); return; }
        setCount(5, false);
        caption('the session line forks ×3 — native · web · phone', { hold: 3.4 });
        after(3.6, () => caption('one session · every surface', { hold: 2.6 }));
        after(6.0, () => hand.moveTo(24, 60));
        after(7.4, () => {
          hand.tap(); setCount(6, true);
          caption('six — from the desktop', { hold: 2 });
        });
        after(9.6, () => hand.moveTo(76, 56));
        after(11.4, () => {
          hand.tap(); setCount(7, true); rail.setWitness(7);
          caption('seven — everywhere at once. the phone felt it.', { hold: 3.2 });
        });
      },
      exit() {
        studioMode('shrink', false);
        $('#devices').classList.remove('on');
        hand.hide();
      },
    });
  }

  function buildDevices() {
    const d = document.createElement('div');
    d.id = 'devices';
    d.innerHTML = `
      <svg id="fork-svg" viewBox="0 0 1000 240" preserveAspectRatio="none">
        <path id="fork-path" d="M 500 230 C 500 150, 240 160, 240 60" />
        <path id="fork-path2" d="M 500 230 C 500 150, 500 150, 500 60" />
        <path id="fork-path3" d="M 500 230 C 500 150, 760 160, 760 60" />
      </svg>
      <div class="dev browser" style="left:13vw">
        <div class="bw-bar"><span class="bw-dots">●●●</span><span class="bw-url">localhost:5173</span><span class="bw-r">↻</span></div>
        <div class="bw-body dev-body"></div>
      </div>
      <div class="dev phone" style="left:66vw">
        <div class="ph-notch"></div>
        <div class="dev-body"></div>
      </div>
      <div class="dev-label l1">web-dom</div>
      <div class="dev-label l2">android</div>`;
    document.getElementById('stage').appendChild(d);
    $$('#devices .dev-body').forEach(b => {
      const clone = $('#app-card').cloneNode(true);
      clone.id = ''; clone.classList.add('clone');
      b.appendChild(clone);
    });
  }

  /* ------------------------------------------------------------------ init */
  function init() {
    buildStudio();
    actCard2(); sceneStudio(); scenePaint(); sceneDescent();
    sceneCompose(); sceneReceipt(); sceneCarry(); sceneDamage(); sceneWorld();
    // other acts re-show the studio — this is the state API they need
    window.Studio = { reset: resetStudioUI, showFinal: showFinalSayPane, setCount, show: showStudio };
  }
  return { init };
})();
