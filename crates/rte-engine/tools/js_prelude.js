// 원본 index.html 의 <head> 에 주입되는 하니스. 원본 스크립트보다 먼저 실행되어 시계/타이머/rAF 를 가짜로 바꾼다.
// 원본 파일 자체는 수정하지 않는다.
(function () {
  const FAKE = (window.FAKE = { now: 1000, q: null, timers: [], tid: 0 });
  performance.now = function () { return FAKE.now; };
  window.requestAnimationFrame = function (cb) { FAKE.q = cb; return 1; };
  window.cancelAnimationFrame = function () {};
  window.setTimeout = function (fn, ms) { const id = ++FAKE.tid; FAKE.timers.push({ id, at: FAKE.now + (ms || 0), fn }); return id; };
  window.clearTimeout = function (id) { FAKE.timers = FAKE.timers.filter(t => t.id !== id); };
  FAKE.fire = function () {
    for (;;) {
      let best = null;
      for (const t of FAKE.timers) if (t.at <= FAKE.now && (!best || t.at < best.at || (t.at === best.at && t.id < best.id))) best = t;
      if (!best) break;
      FAKE.timers = FAKE.timers.filter(t => t !== best);
      best.fn();
    }
  };
  FAKE.step = function (ms, n) {
    for (let i = 0; i < n; i++) {
      FAKE.now += ms; FAKE.fire();
      const q = FAKE.q; if (q) { FAKE.q = null; q(FAKE.now); }
    }
  };
  // 결정적 텍스트 폭(폰트 비의존): Rust stub_measure 와 같은 식
  CanvasRenderingContext2D.prototype.measureText = function (s) {
    const fs = parseFloat(/(\d+(?:\.\d+)?)px/.exec(this.font)[1]);
    let w = 0; for (const ch of s) w += ch.codePointAt(0) > 0x2e80 ? fs : fs * 0.55;
    return { width: w };
  };
  window.__report = (n, v) => fetch('/report?name=' + n, { method: 'POST', body: typeof v === 'string' ? v : JSON.stringify(v) });

  window.__dump = function (label) {
    const o = { label, now: FAKE.now, raf: !!raf, stack: stack.map(n => n.id), rotT, rotC, bump, bv, bumpDir, kickOK, trans, transUntil, transStart, last, moved, wacc, wlast, wevt,
      geom: { W, H, cx, cy, R1, R2 }, root: svg.dataset.root === 'true',
      crumbs: [...crumbEl.children].filter(c => c.tagName.toLowerCase() !== 'i').map(c => [c.tagName.toLowerCase(), c.textContent]),
      cur: {}, E: [], links: links.map(l => [l[0], l[1]]), pool: [] };
    for (const id in cur) { const c = cur[id]; o.cur[id] = { x: c.x, y: c.y, o: c.o, r: c.r, la: c.la, gone: c.gone, ex: c.ex ? [c.ex.x, c.ex.y] : null, role: c.role, fx: c.fx, fy: c.fy, fo: c.fo, fr: c.fr }; }
    const rev = new Map(); for (const id in E) rev.set(E[id].g, +id);
    const num = a => a === null ? null : parseFloat(a);
    for (const g of layer.children) {
      const id = rev.get(g), e = E[id];
      const tf = /translate\(([^,]+),([^)]+)\)/.exec(g.getAttribute('transform') || '');
      const shown = e.pr.style.display !== 'none';
      const da = (e.pr.getAttribute('stroke-dasharray') || '0 0').split(' ');
      o.E.push({ id, mode: e.mode, cls: g.getAttribute('class'), tf: tf ? [+tf[1], +tf[2]] : [0, 0], r: num(e.c.getAttribute('r')),
        t: { x: num(e.t1.getAttribute('x')), y: num(e.t1.getAttribute('y')), anchor: e.t1.getAttribute('text-anchor'), fs: num(e.t1.getAttribute('font-size')), txt: e.t1.textContent },
        gop: parseFloat(g.style.opacity), top: parseFloat(e.t1.style.opacity), pe: g.style.pointerEvents === 'auto', tab: parseInt(g.getAttribute('tabindex')),
        ring: shown ? { r: num(e.pr.getAttribute('r')), dash: parseFloat(da[0]), gap: parseFloat(da[1]), off: parseFloat(e.pr.getAttribute('stroke-dashoffset')) } : null });
    }
    for (const l of linkL.children) o.pool.push({ x1: +l.getAttribute('x1'), y1: +l.getAttribute('y1'), x2: +l.getAttribute('x2'), y2: +l.getAttribute('y2'), op: parseFloat(l.style.opacity), disp: l.style.display !== 'none' });
    return o;
  };

  window.__run = function (actions) {
    const dumps = [];
    const fire = () => FAKE.fire();
    const pdown = (x, y) => { svg.dispatchEvent(new PointerEvent('pointerdown', { clientX: x, clientY: y, bubbles: true, pointerId: 1 })); fire(); };
    const pup = () => window.dispatchEvent(new PointerEvent('pointerup', { bubbles: true, pointerId: 1 }));
    for (const a of actions) {
      switch (a.a) {
        case 'step': FAKE.step(a.ms, a.n); break;
        case 'down': pdown(a.x, a.y); break;
        case 'move': window.dispatchEvent(new PointerEvent('pointermove', { clientX: a.x, clientY: a.y, bubbles: true, pointerId: 1 })); fire(); break;
        case 'up': pup(); if (a.click != null && E[a.click]) E[a.click].g.dispatchEvent(new MouseEvent('click', { bubbles: true })); fire(); break;
        case 'node': { const e = E[a.id]; if (!e) break; const c = cur[a.id]; pdown(c.x, c.y); pup(); e.g.dispatchEvent(new MouseEvent('click', { bubbles: true })); fire(); break; }
        case 'wheel': svg.dispatchEvent(new WheelEvent('wheel', { clientX: a.x, clientY: a.y, deltaY: a.dy, bubbles: true, cancelable: true })); fire(); break;
        case 'key': window.dispatchEvent(new KeyboardEvent('keydown', { key: a.key, bubbles: true, cancelable: true })); fire(); break;
        case 'nodekey': { const e = E[a.id]; if (!e) break; e.g.dispatchEvent(new KeyboardEvent('keydown', { key: a.key, bubbles: true, cancelable: true })); fire(); break; }
        case 'dump': dumps.push(window.__dump(a.label)); break;
        default: throw new Error('unsupported action ' + a.a);
      }
    }
    window.__report('rte_dump', JSON.stringify(dumps));
  };
})();
