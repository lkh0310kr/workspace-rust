//! Radial Focus Tree 엔진 — `crates/radial-tree/reference/index.html` 의 JS 를 같은 연산 순서로 옮긴 순수 로직.
//! UI 툴킷과 무관하다(gpui 는 이 상태를 그리기만 한다). 원본과의 동일성은 가짜 시계 기반 덤프 비교로 검증한다.
//!
//! 이식 규칙:
//!  - JS `for (const id in obj)` 는 정수 키를 오름차순으로 순회 → `BTreeMap` 사용.
//!  - JS `Math.round` 는 .5 를 +∞ 쪽으로 반올림 → `js_round`.
//!  - `performance.now()`/`setTimeout`/`requestAnimationFrame` 은 호출자가 주는 `now` 와 내부 타이머로 대체.
//!  - 원본의 각 함수(`geom`, `compute`, `sync`, `paint`, `tick`, 입력 핸들러)와 같은 이름·같은 순서의 연산을 유지한다.

pub mod scenario;

use serde_json::{json, Value};
use std::collections::BTreeMap;

pub const VIS: usize = 10;
pub const STEP: f64 = 30.0;
pub const D2R: f64 = std::f64::consts::PI / 180.0;
pub const MAXG: usize = 5;
pub const THUMB: f64 = 60.0;
pub const DUR: f64 = 800.0;

fn clamp(v: f64, a: f64, b: f64) -> f64 {
    a.max(b.min(v))
}
/// JS Math.round: 소수 .5 는 +∞ 방향
fn js_round(x: f64) -> f64 {
    (x + 0.5).floor()
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Role {
    C,
    K,
    G,
}
impl Role {
    pub fn as_str(self) -> &'static str {
        match self {
            Role::C => "c",
            Role::K => "k",
            Role::G => "g",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Node {
    pub id: usize,
    pub p: Option<usize>,
    pub k: Vec<usize>,
    pub name: String,
}

#[derive(Clone, Copy, Debug)]
pub struct T {
    pub x: f64,
    pub y: f64,
    pub o: f64,
    pub r: f64,
    pub role: Role,
    pub th: f64,
}

#[derive(Clone, Debug)]
pub struct Cur {
    pub x: f64,
    pub y: f64,
    pub o: f64,
    pub r: f64,
    pub la: f64,
    pub gone: bool,
    pub ex: Option<(f64, f64)>,
    pub role: Role,
    pub fx: Option<f64>,
    pub fy: f64,
    pub fo: f64,
    pub fr: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    C,
    K(i8),
    Km(char),
    G(i8),
    G0,
}
impl Mode {
    pub fn as_string(&self) -> String {
        match self {
            Mode::C => "c".into(),
            Mode::K(d) => format!("k{d}"),
            Mode::Km(u) => format!("km{u}"),
            Mode::G(d) => format!("g{d}"),
            Mode::G0 => "g0".into(),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct TextAttr {
    pub x: Option<f64>,
    pub y: Option<f64>,
    pub anchor: Option<&'static str>,
    pub fs: Option<f64>,
    pub txt: String,
}

#[derive(Clone, Copy, Debug)]
pub struct Ring {
    pub r: f64,
    pub dash: f64,
    pub gap: f64,
    pub off: f64,
}

#[derive(Clone, Debug, Default)]
pub struct El {
    pub mode: Option<Mode>,
    pub cls: Option<String>,
    pub t1: TextAttr,
    pub tf: (f64, f64),
    pub r: f64,
    pub ring: Option<Ring>,
    pub gop: f64,
    pub top: f64,
    pub pe: bool,
    pub tab: i32,
}

#[derive(Clone, Copy, Debug)]
pub struct Line {
    pub x1: f64,
    pub y1: f64,
    pub x2: f64,
    pub y2: f64,
    pub op: f64,
    pub disp: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct Drag {
    pub x: f64,
    pub y: f64,
    pub ring: bool,
    pub la: f64,
    pub acc: f64,
    pub rot: f64,
    pub k: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TimerKind {
    MovedReset,
    WaccReset,
}
#[derive(Clone, Copy, Debug)]
struct Timer {
    id: u64,
    at: f64,
    kind: TimerKind,
}

/// 테스트용 결정적 텍스트 폭(폰트 비의존). 원본 하니스의 canvas.measureText 스텁과 같은 식.
pub fn stub_measure(s: &str, fs: f64, _wt: i32) -> f64 {
    s.chars().map(|c| if (c as u32) > 0x2e80 { fs } else { fs * 0.55 }).sum()
}

pub struct Engine {
    pub nodes: Vec<Node>,
    pub stack: Vec<usize>,
    pub rot_t: f64,
    pub rot_c: f64,
    pub w: f64,
    pub h: f64,
    pub cx: f64,
    pub cy: f64,
    pub r1: f64,
    pub r2: f64,
    pub t: BTreeMap<usize, T>,
    pub cur: BTreeMap<usize, Cur>,
    pub e: BTreeMap<usize, El>,
    /// DOM 순서(= 그리기 순서 = 히트 테스트 위에서부터의 역순). getEl 호출 순서
    pub dom: Vec<usize>,
    pub links: Vec<(usize, usize)>,
    pub raf: bool,
    pub pool: Vec<Line>,
    pub trans: bool,
    pub trans_until: f64,
    pub trans_start: f64,
    pub last: f64,
    pub bump: f64,
    pub bv: f64,
    pub bump_dir: f64,
    pub kick_ok: bool,
    pub wevt: f64,
    pub moved: bool,
    pub drag: Option<Drag>,
    pub wacc: f64,
    pub wlast: f64,
    wt: Option<u64>,
    timers: Vec<Timer>,
    timer_seq: u64,
    /// svg.dataset.root (스택 길이 < 2)
    pub root_flag: bool,
    measure: Box<dyn Fn(&str, f64, i32) -> f64>,
}

impl Engine {
    /// 원본 하단의 `geom();beginTrans();afterNav();` 와 같은 초기화
    pub fn new(data: &Value, w: f64, h: f64, now: f64, measure: Box<dyn Fn(&str, f64, i32) -> f64>) -> Engine {
        let mut nodes = Vec::new();
        fn mk(d: &Value, p: Option<usize>, nodes: &mut Vec<Node>) -> usize {
            let id = nodes.len();
            let name = match d {
                Value::String(s) => s.clone(),
                Value::Array(a) => a[0].as_str().unwrap().to_string(),
                _ => unreachable!(),
            };
            nodes.push(Node { id, p, k: vec![], name });
            if let Value::Array(a) = d {
                for c in &a[1..] {
                    let cid = mk(c, Some(id), nodes);
                    nodes[id].k.push(cid);
                }
            }
            id
        }
        let root = mk(data, None, &mut nodes);
        let mut en = Engine {
            nodes,
            stack: vec![root],
            rot_t: 0.0,
            rot_c: 0.0,
            w: 0.0,
            h: 0.0,
            cx: 0.0,
            cy: 0.0,
            r1: 0.0,
            r2: 0.0,
            t: BTreeMap::new(),
            cur: BTreeMap::new(),
            e: BTreeMap::new(),
            dom: vec![],
            links: vec![],
            raf: false,
            pool: vec![],
            trans: true,
            trans_until: 0.0,
            trans_start: 0.0,
            last: 0.0,
            bump: 0.0,
            bv: 0.0,
            bump_dir: 1.0,
            kick_ok: true,
            wevt: 0.0,
            moved: false,
            drag: None,
            wacc: 0.0,
            wlast: 0.0,
            wt: None,
            timers: vec![],
            timer_seq: 0,
            root_flag: true,
            measure,
        };
        en.geom(w, h);
        en.begin_trans(now);
        en.after_nav();
        en
    }

    pub fn foc(&self) -> usize {
        *self.stack.last().unwrap()
    }

    fn fit(&self, s: &str, w: f64, fs: f64, wt: i32) -> String {
        if w < 24.0 {
            return String::new();
        }
        if (self.measure)(s, fs, wt) <= w {
            return s.to_string();
        }
        let mut chars: Vec<char> = s.chars().collect();
        loop {
            let cand: String = chars.iter().collect::<String>() + "…";
            if !(chars.len() > 1 && (self.measure)(&cand, fs, wt) > w) {
                break;
            }
            chars.pop();
        }
        chars.iter().collect::<String>() + "…"
    }

    pub fn geom(&mut self, w: f64, h: f64) {
        self.w = w;
        self.h = h;
        self.cx = w / 2.0;
        self.cy = h * 0.54;
        let v = (self.cy - 66.0).min(h - self.cy - 26.0);
        self.r2 = (w / 2.0 - if w > 700.0 { 120.0 } else { 22.0 }).min(v);
        self.r1 = self.r2 * 0.56;
        for e in self.e.values_mut() {
            e.mode = None;
        }
    }

    /// addEventListener("resize") → geom(); run()
    pub fn resize(&mut self, w: f64, h: f64) {
        self.geom(w, h);
        self.run();
    }

    pub fn begin_trans(&mut self, now: f64) {
        self.rot_t = 0.0;
        self.rot_c = 0.0;
        self.bump = 0.0;
        self.bv = 0.0;
        self.trans = true;
        self.trans_start = now;
        self.trans_until = self.trans_start + DUR;
        for c in self.cur.values_mut() {
            c.fx = Some(c.x);
            c.fy = c.y;
            c.fo = c.o;
            c.fr = c.r;
        }
    }

    fn compute(&mut self) {
        let f = self.foc();
        let n = self.nodes[f].k.len();
        self.t.clear();
        self.links.clear();
        let (cx, cy, r1, r2) = (self.cx, self.cy, self.r1, self.r2);
        self.t.insert(f, T { x: cx, y: cy, o: 1.0, r: 14.0, role: Role::C, th: 0.0 });
        let vis = n.min(VIS);
        let sp = if n <= VIS { 360.0 / n as f64 } else { STEP };
        let kids = self.nodes[f].k.clone();
        for (i, &kid) in kids.iter().enumerate() {
            let p = if n <= VIS { i as f64 } else { i as f64 - (self.rot_c + self.bump) };
            if p >= vis as f64 || p <= -1.0 {
                continue;
            }
            let o = if n <= VIS { 1.0 } else { clamp((p + 1.0).min(vis as f64 - p), 0.0, 1.0) };
            if o <= 0.0 {
                continue;
            }
            let th = if n <= VIS {
                -90.0 + (p + 0.5) * sp
            } else {
                -90.0 + (360.0 - (VIS as f64 - 1.0) * STEP) / 2.0 + p * sp
            };
            let a = th * D2R;
            self.t.insert(kid, T { x: cx + r1 * a.cos(), y: cy + r1 * a.sin(), o, r: 7.0, role: Role::K, th });
            self.links.push((f, kid));
            let gs: Vec<usize> = self.nodes[kid].k.iter().take(MAXG).cloned().collect();
            let m = gs.len();
            let gst = (11.0f64).min(sp * 0.8 / (m.max(2) as f64));
            for (j, &g) in gs.iter().enumerate() {
                let ph = th + (j as f64 - (m as f64 - 1.0) / 2.0) * gst;
                let b = ph * D2R;
                self.t.insert(g, T { x: cx + r2 * b.cos(), y: cy + r2 * b.sin(), o: o * 0.9, r: 3.5, role: Role::G, th: ph });
                self.links.push((kid, g));
            }
        }
    }

    fn get_el(&mut self, id: usize) {
        if self.e.contains_key(&id) {
            return;
        }
        self.e.insert(id, El::default());
        self.dom.push(id);
    }

    fn sync(&mut self) {
        let ids: Vec<usize> = self.t.keys().cloned().collect();
        for id in ids {
            if let Some(c) = self.cur.get_mut(&id) {
                if c.gone {
                    c.gone = false;
                    c.ex = None;
                }
                continue;
            }
            let t = self.t[&id];
            let (mut x, mut y, mut r) = (t.x, t.y, t.r);
            if self.trans {
                let pp = self.nodes[id].p.and_then(|p| {
                    self.cur.get(&p).map(|c| (c.x, c.y)).or_else(|| self.t.get(&p).map(|t| (t.x, t.y)))
                });
                x = pp.map(|p| p.0).unwrap_or(self.cx);
                y = pp.map(|p| p.1).unwrap_or(self.cy);
                r = 3.0;
            }
            self.cur.insert(
                id,
                Cur { x, y, o: 0.0, r, la: 1.0, gone: false, ex: None, role: t.role, fx: if self.trans { Some(x) } else { None }, fy: y, fo: 0.0, fr: r },
            );
            self.get_el(id);
        }
        let (cx, cy, trans) = (self.cx, self.cy, self.trans);
        let tkeys: Vec<usize> = self.t.keys().cloned().collect();
        for (id, c) in self.cur.iter_mut() {
            if !tkeys.contains(id) && !c.gone {
                c.gone = true;
                if trans {
                    c.ex = Some((cx + (c.x - cx) * 0.5, cy + (c.y - cy) * 0.5));
                }
            }
        }
        self.trans = false;
    }

    fn mode_of(t: &T) -> Mode {
        if t.role == Role::C {
            return Mode::C;
        }
        let co = (t.th * D2R).cos();
        let sn = (t.th * D2R).sin();
        let d: i8 = if co > 0.0 { 1 } else { -1 };
        if t.role == Role::K {
            if co.abs() > 0.3 {
                Mode::K(d)
            } else {
                Mode::Km(if sn < 0.0 { 'u' } else { 'd' })
            }
        } else if co.abs() > 0.5 {
            Mode::G(d)
        } else {
            Mode::G0
        }
    }

    fn place(t1: &mut TextAttr, x: f64, anchor: &'static str, y: f64, fs: f64, txt: String) {
        t1.x = Some(x);
        t1.y = Some(y);
        t1.anchor = Some(anchor);
        t1.fs = Some(fs);
        t1.txt = txt;
    }

    fn paint(&mut self, id: usize, t: &T, m: Mode) {
        let d: f64 = match m {
            Mode::K(-1) | Mode::G(-1) => -1.0,
            _ => 1.0,
        };
        let name = self.nodes[id].name.clone();
        let hk = !self.nodes[id].k.is_empty();
        let (cx, w, r1, r2) = (self.cx, self.w, self.r1, self.r2);
        let fitted = |this: &Engine, wd: f64, fs: f64, wt: i32| this.fit(&name, wd, fs, wt);
        // fit 결과를 먼저 계산(차용 규칙) — 각 분기가 쓰는 폭/폰트는 원본과 동일
        let mut e = self.e.remove(&id).unwrap();
        e.cls = Some(format!("node {}{}", t.role.as_str(), if hk { " hk" } else { "" }));
        match m {
            Mode::C => {
                let s = fitted(self, r1 * 1.7, 22.0, 500);
                Self::place(&mut e.t1, 0.0, "middle", 48.0, 22.0, s);
            }
            Mode::K(_) => {
                let s = fitted(self, 60.0f64.max(r2 - r1 - 28.0), 14.0, 500);
                Self::place(&mut e.t1, d * 14.0, if d > 0.0 { "start" } else { "end" }, 5.0, 14.0, s);
            }
            Mode::Km(c) => {
                let s = fitted(self, 110.0, 14.0, 500);
                Self::place(&mut e.t1, 0.0, "middle", if c == 'u' { -14.0 } else { 26.0 }, 14.0, s);
            }
            Mode::G0 => {
                e.t1.txt = String::new();
            }
            Mode::G(_) => {
                let x = cx + r2 * (t.th * D2R).cos();
                let s = fitted(self, (if d > 0.0 { w - x } else { x }) - 20.0, 11.0, 400);
                Self::place(&mut e.t1, d * 9.0, if d > 0.0 { "start" } else { "end" }, 4.0, 11.0, s);
            }
        }
        self.e.insert(id, e);
    }

    pub fn after_nav(&mut self) {
        self.root_flag = self.stack.len() < 2;
        self.run();
    }

    pub fn go(&mut self, id: usize, now: f64) {
        let f = self.foc();
        if id == f {
            self.back(now);
            return;
        }
        if let Some(i) = self.stack.iter().position(|&s| s == id) {
            self.stack.truncate(i + 1);
        } else {
            let mut ch = vec![];
            let mut x = Some(id);
            while let Some(xi) = x {
                if xi == f {
                    break;
                }
                ch.insert(0, xi);
                x = self.nodes[xi].p;
            }
            if x != Some(f) {
                return;
            }
            self.stack.extend(ch);
        }
        self.begin_trans(now);
        self.after_nav();
    }

    pub fn back(&mut self, now: f64) {
        if self.stack.len() > 1 {
            self.stack.pop();
            self.begin_trans(now);
            self.after_nav();
        }
    }

    pub fn run(&mut self) {
        if !self.raf {
            self.raf = true;
        }
    }

    /// requestAnimationFrame 콜백. `now` = rAF 타임스탬프
    pub fn tick(&mut self, now: f64) {
        self.raf = false;
        let base = if self.last != 0.0 { self.last } else { now - 16.0 };
        let dt = 50.0f64.min(now - base);
        self.last = now;
        let mut busy = now < self.trans_until;
        self.rot_c += (self.rot_t - self.rot_c) * (1.0 - (-dt / 105.0).exp());
        if (self.rot_t - self.rot_c).abs() > 0.002 {
            busy = true;
        } else {
            self.rot_c = self.rot_t;
        }
        if self.bump != 0.0 || self.bv != 0.0 {
            let s = dt.min(32.0) / 1000.0;
            self.bv += (-260.0 * self.bump - 26.0 * self.bv) * s;
            self.bump += self.bv * s;
            if self.bump.abs() < 0.002 && self.bv.abs() < 0.02 {
                self.bump = 0.0;
                self.bv = 0.0;
            } else {
                busy = true;
            }
        }
        self.compute();
        self.sync();
        let nn = self.nodes[self.foc()].k.len();
        let in_t = now < self.trans_until;
        let ue = 1.0 - (1.0 - clamp((now - self.trans_start) / DUR, 0.0, 1.0)).powi(3);
        let kn = 1.0 - (-dt / 70.0).exp();
        let kl = 1.0 - (-dt / 140.0).exp();
        let ids: Vec<usize> = self.cur.keys().cloned().collect();
        for id in ids {
            let t = self.t.get(&id).copied();
            if let Some(t) = t {
                let c = self.cur.get_mut(&id).unwrap();
                if c.role != t.role {
                    c.role = t.role;
                    c.la = if c.o > 0.05 { 0.0 } else { 1.0 };
                }
                let m = Self::mode_of(&t);
                if self.e[&id].mode != Some(m) {
                    self.e.get_mut(&id).unwrap().mode = Some(m);
                    self.paint(id, &t, m);
                }
            }
            let c = self.cur.get_mut(&id).unwrap();
            let tx = match &t {
                Some(t) => t.x,
                None => c.ex.map(|e| e.0).unwrap_or(c.x),
            };
            let ty = match &t {
                Some(t) => t.y,
                None => c.ex.map(|e| e.1).unwrap_or(c.y),
            };
            let to = t.map(|t| t.o).unwrap_or(0.0);
            let tr = match &t {
                Some(t) => t.r,
                None => {
                    if c.ex.is_some() {
                        3.0
                    } else {
                        c.r
                    }
                }
            };
            if in_t && c.fx.is_some() && (t.is_some() || c.ex.is_some()) {
                let fx = c.fx.unwrap();
                c.x = fx + (tx - fx) * ue;
                c.y = c.fy + (ty - c.fy) * ue;
                c.o = c.fo + (to - c.fo) * ue;
                c.r = c.fr + (tr - c.fr) * ue;
            } else {
                let kk = if t.is_some() { 1.0 } else { kn };
                c.x += (tx - c.x) * kk;
                c.y += (ty - c.y) * kk;
                c.o += (to - c.o) * kk;
                c.r += (tr - c.r) * kk;
            }
            c.la += (1.0 - c.la) * kl;
            if (tx - c.x).abs() > 0.3 || (ty - c.y).abs() > 0.3 || (to - c.o).abs() > 0.01 || (tr - c.r).abs() > 0.05 || c.la < 0.99 {
                busy = true;
            }
            let (cxv, cyv, cov, crv, cla) = (c.x, c.y, c.o, c.r, c.la);
            let e = self.e.get_mut(&id).unwrap();
            e.tf = (cxv, cyv);
            e.r = crv;
            if t.map(|t| t.role == Role::C).unwrap_or(false) && nn > VIS {
                let rr = crv + 6.0;
                let cc = 2.0 * std::f64::consts::PI * rr;
                let f = clamp(self.rot_c / (nn - VIS) as f64, 0.0, 1.0);
                let l = THUMB / 360.0 * cc;
                e.ring = Some(Ring { r: rr, dash: l, gap: cc - l, off: -(f - THUMB / 720.0) * cc });
            } else {
                e.ring = None;
            }
            e.gop = cov;
            e.top = cla;
            e.pe = t.map(|t| t.o > 0.5).unwrap_or(false);
            e.tab = if t.is_some() { 0 } else { -1 };
            if t.is_none() && cov < 0.02 {
                self.e.remove(&id);
                self.dom.retain(|&d| d != id);
                self.cur.remove(&id);
            }
        }
        for (i, &(la, lb)) in self.links.clone().iter().enumerate() {
            let (a, b) = match (self.cur.get(&la), self.cur.get(&lb)) {
                (Some(a), Some(b)) => (a.clone(), b.clone()),
                _ => continue,
            };
            let ln = Line { x1: a.x, y1: a.y, x2: b.x, y2: b.y, op: 0.4 * a.o.min(b.o), disp: true };
            if i < self.pool.len() {
                self.pool[i] = ln;
            } else {
                assert_eq!(i, self.pool.len(), "sparse pool");
                self.pool.push(ln);
            }
        }
        for i in self.links.len()..self.pool.len() {
            self.pool[i].disp = false;
        }
        if busy {
            self.raf = true;
        }
    }

    // ------------------------------------------------------------ 입력 (원본 이벤트 핸들러)
    fn sp_n(&self) -> f64 {
        let n = self.nodes[self.foc()].k.len();
        if n <= VIS {
            360.0 / (n.max(1) as f64)
        } else {
            STEP
        }
    }
    fn ang(&self, x: f64, y: f64) -> f64 {
        (y - self.cy).atan2(x - self.cx) * 180.0 / std::f64::consts::PI
    }
    pub fn in_ring(&self, x: f64, y: f64) -> bool {
        (x - self.cx).hypot(y - self.cy) >= self.r1 * 0.7 && self.nodes[self.foc()].k.len() > 1
    }

    pub fn pointer_down(&mut self, x: f64, y: f64) {
        self.moved = false;
        self.drag = Some(Drag { x, y, ring: self.in_ring(x, y), la: self.ang(x, y), acc: 0.0, rot: self.rot_t, k: false });
    }

    pub fn pointer_move(&mut self, x: f64, y: f64) {
        let Some(mut d) = self.drag else { return };
        if (x - d.x).hypot(y - d.y) > 4.0 {
            self.moved = true;
        }
        if d.ring && self.moved {
            let a = self.ang(x, y);
            let mut dd = a - d.la;
            dd = ((dd + 540.0) % 360.0) - 180.0;
            d.acc += dd;
            d.la = a;
            let nn = self.nodes[self.foc()].k.len();
            let raw = d.rot - 1.8 * d.acc / self.sp_n();
            let mx = (nn as f64 - VIS as f64).max(0.0);
            self.rot_t = clamp(js_round(raw), 0.0, mx);
            if nn > VIS {
                if !d.k && (raw < -0.35 || raw > mx + 0.35) {
                    d.k = true;
                    self.kick(if raw < 0.0 { -1.0 } else { 1.0 });
                } else if raw >= -0.2 && raw <= mx + 0.2 {
                    d.k = false;
                }
            }
            self.drag = Some(d);
            self.run();
        } else {
            self.drag = Some(d);
        }
    }

    pub fn pointer_up(&mut self, now: f64) {
        self.drag = None;
        self.set_timeout(now, 0.0, TimerKind::MovedReset);
        self.run();
    }

    /// 노드 `g` 의 click 리스너: `if(!moved)go(n)`
    pub fn click_node(&mut self, id: usize, now: f64) {
        if !self.moved {
            self.go(id, now);
        }
    }

    pub fn step_dial(&mut self, d: f64) {
        let n = self.nodes[self.foc()].k.len();
        if n <= VIS {
            return;
        }
        let nt = clamp(js_round(self.rot_t) + d, 0.0, (n - VIS) as f64);
        if nt == js_round(self.rot_t) {
            if self.kick_ok {
                self.kick(d);
                self.kick_ok = false;
            }
            return;
        }
        if (nt - js_round(self.rot_c)).abs() <= 4.0 {
            self.rot_t = nt;
            self.run();
        }
    }

    pub fn kick(&mut self, d: f64) {
        self.bv = d * 5.0;
        self.bump_dir = d;
        self.run();
    }

    pub fn wheel(&mut self, x: f64, y: f64, dy: f64, now: f64) {
        if !self.in_ring(x, y) {
            return;
        }
        if now - self.wevt > 250.0 {
            self.kick_ok = true;
        }
        self.wevt = now;
        if let Some(id) = self.wt.take() {
            self.timers.retain(|t| t.id != id);
        }
        self.wt = Some(self.set_timeout(now, 200.0, TimerKind::WaccReset));
        if now - self.wlast < 45.0 {
            self.wacc = 0.0;
            return;
        }
        self.wacc += dy;
        if self.wacc.abs() >= 18.0 {
            self.step_dial(if self.wacc > 0.0 { 1.0 } else { -1.0 });
            self.wacc = 0.0;
            self.wlast = now;
        }
    }

    /// window keydown
    pub fn key(&mut self, key: &str, now: f64) {
        if now - self.wevt > 250.0 {
            self.kick_ok = true;
        }
        self.wevt = now;
        match key {
            "Escape" | "Backspace" => self.back(now),
            "ArrowDown" | "ArrowRight" => self.step_dial(1.0),
            "ArrowUp" | "ArrowLeft" => self.step_dial(-1.0),
            _ => {}
        }
    }

    /// 노드 요소의 keydown(Enter/Space → go) 후 window 로 버블링
    pub fn node_key(&mut self, id: usize, key: &str, now: f64) {
        if key == "Enter" || key == " " {
            self.go(id, now);
        }
        self.key(key, now);
    }

    // ------------------------------------------------------------ 타이머 (setTimeout 대체)
    fn set_timeout(&mut self, now: f64, ms: f64, kind: TimerKind) -> u64 {
        self.timer_seq += 1;
        let id = self.timer_seq;
        self.timers.push(Timer { id, at: now + ms, kind });
        id
    }

    /// `at <= now` 인 타이머를 (at, id) 순으로 실행
    pub fn fire_timers(&mut self, now: f64) {
        loop {
            let mut best: Option<usize> = None;
            for (i, t) in self.timers.iter().enumerate() {
                if t.at <= now && best.map(|b| (t.at, t.id) < (self.timers[b].at, self.timers[b].id)).unwrap_or(true) {
                    best = Some(i);
                }
            }
            let Some(i) = best else { break };
            let t = self.timers.remove(i);
            match t.kind {
                TimerKind::MovedReset => self.moved = false,
                TimerKind::WaccReset => {
                    self.wacc = 0.0;
                    if self.wt == Some(t.id) {
                        self.wt = None;
                    }
                }
            }
        }
    }

    pub fn crumbs(&self) -> Vec<(bool, String)> {
        let n = self.stack.len();
        self.stack.iter().enumerate().map(|(i, &id)| (i == n - 1, self.nodes[id].name.clone())).collect()
    }

    // ------------------------------------------------------------ 덤프 (JS 하니스와 같은 모양)
    pub fn dump(&self, label: &str, now: f64) -> Value {
        let mut cur = serde_json::Map::new();
        for (id, c) in &self.cur {
            cur.insert(
                id.to_string(),
                json!({"x":c.x,"y":c.y,"o":c.o,"r":c.r,"la":c.la,"gone":c.gone,
                       "ex": c.ex.map(|e| json!([e.0, e.1])).unwrap_or(Value::Null),
                       "role":c.role.as_str(),"fx":c.fx,"fy":c.fy,"fo":c.fo,"fr":c.fr}),
            );
        }
        let e: Vec<Value> = self
            .dom
            .iter()
            .map(|id| {
                let e = &self.e[id];
                json!({"id":id,"mode":e.mode.map(|m| m.as_string()),"cls":e.cls,"tf":[e.tf.0,e.tf.1],"r":e.r,
                       "t":{"x":e.t1.x,"y":e.t1.y,"anchor":e.t1.anchor,"fs":e.t1.fs,"txt":e.t1.txt},
                       "gop":e.gop,"top":e.top,"pe":e.pe,"tab":e.tab,
                       "ring": e.ring.map(|r| json!({"r":r.r,"dash":r.dash,"gap":r.gap,"off":r.off})).unwrap_or(Value::Null)})
            })
            .collect();
        let pool: Vec<Value> = self.pool.iter().map(|l| json!({"x1":l.x1,"y1":l.y1,"x2":l.x2,"y2":l.y2,"op":l.op,"disp":l.disp})).collect();
        json!({
            "label":label,"now":now,"raf":self.raf,
            "stack": self.stack, "rotT":self.rot_t,"rotC":self.rot_c,"bump":self.bump,"bv":self.bv,"bumpDir":self.bump_dir,
            "kickOK":self.kick_ok,"trans":self.trans,"transUntil":self.trans_until,"transStart":self.trans_start,"last":self.last,
            "moved":self.moved,"wacc":self.wacc,"wlast":self.wlast,"wevt":self.wevt,
            "geom":{"W":self.w,"H":self.h,"cx":self.cx,"cy":self.cy,"R1":self.r1,"R2":self.r2},
            "root":self.root_flag,
            "crumbs": self.crumbs().iter().map(|(last,n)| json!([if *last {"span"} else {"button"}, n])).collect::<Vec<_>>(),
            "cur":cur,"E":e,"links":self.links.iter().map(|l| json!([l.0,l.1])).collect::<Vec<_>>(),"pool":pool,
        })
    }
}
