//! Radial Focus Tree — gpui 포팅. 로직은 `rte-engine`(원본 JS 와 수치 동일 검증됨), 이 파일은 그리기와 입력 연결.
//! 원본: crates/radial-tree/reference/index.html

mod shot;
mod text;

use gpui::{
    canvas, div, point, prelude::*, px, quad, size, App, Application, BorderStyle, Bounds, Context, CursorStyle, Edges, FocusHandle,
    Hsla, KeyDownEvent, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, PathBuilder, Pixels, Render, ScrollDelta,
    ScrollWheelEvent, Window, WindowAppearance, WindowBounds, WindowOptions, Corners, TitlebarOptions, transparent_black, DispatchPhase,
};
use rte_engine::{Engine, VIS};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Instant;
use text::TextCtx;

const BAR_PAD_Y: f64 = 14.0;
const BAR_PAD_X: f64 = 18.0;

#[derive(Clone, Copy)]
struct Theme {
    bg: Hsla,
    ink: Hsla,
    mut_: Hsla,
    acc: Hsla,
    rim: Hsla,
}

fn hex(c: u32) -> Hsla {
    gpui::rgb(c).into()
}

fn theme(dark: bool) -> Theme {
    if dark {
        Theme { bg: hex(0x101113), ink: hex(0xf4f4f4), mut_: hex(0x8d9096), acc: hex(0x3e6ae1), rim: hex(0x26282c) }
    } else {
        Theme { bg: hex(0xffffff), ink: hex(0x171a20), mut_: hex(0x5c5e62), acc: hex(0x3e6ae1), rim: hex(0xe3e4e6) }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Focus {
    None,
    Node(usize),
    Crumb(usize),
}

/// 크럼 항목 하나의 레이아웃(원본 CSS 를 직접 계산)
#[derive(Clone, Debug)]
struct CrumbItem {
    kind: CrumbKind,
    text: String,
    x: f64,
    w: f64,
    text_x: f64,
    shown: String,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CrumbKind {
    Sep,
    Button(usize),
    Last,
}

struct Shared {
    en: Engine,
    tc: TextCtx,
    start: Instant,
    dark_override: Option<bool>,
    dark_system: bool,
    hover: Option<usize>,
    hover_crumb: Option<usize>,
    focus: Focus,
    focus_visible: bool,
    down_target: Option<usize>,
    size: (f64, f64),
}

impl Shared {
    fn now(&self) -> f64 {
        1000.0 + self.start.elapsed().as_secs_f64() * 1000.0
    }
    fn theme(&self) -> Theme {
        theme(self.dark_override.unwrap_or(self.dark_system))
    }

    // ---- 크럼 레이아웃: `.bar` / `.crumbs` CSS 를 그대로 계산 ----
    fn crumb_layout(&self) -> Vec<CrumbItem> {
        let tc = &self.tc;
        let crumbs = self.en.crumbs();
        let avail = (self.size.0 - 2.0 * BAR_PAD_X + 8.0).max(0.0); // .crumbs margin-left:-8px → 사용 가능 폭 +8
        struct Raw {
            kind: CrumbKind,
            text: String,
            basis: f64,
            pad: f64,
            max: f64,
            weight: f64,
            ls: bool,
        }
        let mut items: Vec<Raw> = vec![];
        for (i, (last, name)) in crumbs.iter().enumerate() {
            if i > 0 {
                items.push(Raw { kind: CrumbKind::Sep, text: "›".into(), basis: 0.0, pad: 1.0, max: f64::MAX, weight: 500.0, ls: true });
            }
            let (kind, weight, ls) = if *last { (CrumbKind::Last, 500.0, true) } else { (CrumbKind::Button(i), 400.0, false) };
            items.push(Raw { kind, text: name.clone(), basis: 0.0, pad: 8.0, max: if *last { f64::MAX } else { 9.0 * 13.0 }, weight, ls });
        }
        for it in items.iter_mut() {
            let tw = tc.width(&it.text, 13.0, it.weight) + if it.ls { 13.0 * 0.01 * it.text.chars().count() as f64 } else { 0.0 };
            it.basis = (tw + 2.0 * it.pad).min(it.max);
        }
        // flex-shrink: 모든 항목 flex 0 1 auto(구분자는 none), min-width 0 — 넘치면 basis 비례로 줄임
        let total: f64 = items.iter().map(|i| i.basis).sum();
        let mut widths: Vec<f64> = items.iter().map(|i| i.basis).collect();
        if total > avail {
            let shrinkable: f64 = items.iter().filter(|i| i.kind != CrumbKind::Sep).map(|i| i.basis).sum();
            let over = total - avail;
            for (w, it) in widths.iter_mut().zip(&items) {
                if it.kind != CrumbKind::Sep && shrinkable > 0.0 {
                    *w = (it.basis - over * it.basis / shrinkable).max(0.0);
                }
            }
        }
        let mut x = BAR_PAD_X - 8.0;
        let mut out = vec![];
        for (it, w) in items.iter().zip(&widths) {
            let inner = (w - 2.0 * it.pad).max(0.0);
            let shown = tc.ellipsize(&it.text, 13.0, it.weight, inner, if it.ls { 0.01 } else { 0.0 });
            out.push(CrumbItem { kind: it.kind, text: it.text.clone(), x, w: *w, text_x: x + it.pad, shown });
            x += w;
        }
        out
    }

    /// 크럼 요소(버튼·마지막 span·구분자)의 사각형: x, y, w, h  (높이 28 = 16 + 패딩 6*2, 구분자는 16)
    fn crumb_rect(&self, c: &CrumbItem) -> (f64, f64, f64, f64) {
        match c.kind {
            CrumbKind::Sep => (c.x, BAR_PAD_Y + 6.0, c.w, 16.0),
            _ => (c.x, BAR_PAD_Y, c.w, 28.0),
        }
    }

    fn hit_crumb(&self, x: f64, y: f64) -> Option<(usize, CrumbKind)> {
        for (i, c) in self.crumb_layout().iter().enumerate() {
            let (rx, ry, rw, rh) = self.crumb_rect(c);
            if x >= rx && x < rx + rw && y >= ry && y < ry + rh {
                return Some((i, c.kind));
            }
        }
        None
    }

    /// svg 위에서의 최상단 노드(원본 DOM 역순). pointer-events:none(e.pe=false) 는 제외
    fn hit_node(&self, x: f64, y: f64) -> Option<usize> {
        for &id in self.en.dom.iter().rev() {
            let e = &self.en.e[&id];
            if !e.pe {
                continue;
            }
            let (ex, ey) = e.tf;
            let d = (x - ex).hypot(y - ey);
            if d <= e.r + 0.75 {
                return Some(id);
            }
            if let Some(r) = e.ring {
                if (d - r.r).abs() <= 1.25 {
                    return Some(id);
                }
            }
            if !e.t1.txt.is_empty() {
                if let (Some(tx), Some(ty), Some(an), Some(fs)) = (e.t1.x, e.t1.y, e.t1.anchor, e.t1.fs) {
                    let wt = if e.cls.as_deref().map(|c| c.starts_with("node g")).unwrap_or(false) { 400.0 } else { 500.0 };
                    let (x0, w, asc, desc) = self.tc.text_box(&e.t1.txt, fs, wt, an, ex + tx);
                    let base = ey + ty;
                    if x >= x0 && x <= x0 + w && y >= base - asc && y <= base + desc {
                        return Some(id);
                    }
                }
            }
        }
        None
    }

    fn tab_stops(&self) -> Vec<Focus> {
        let mut v: Vec<Focus> = self.en.dom.iter().filter(|id| self.en.e[id].tab == 0).map(|&id| Focus::Node(id)).collect();
        for (i, c) in self.crumb_layout().iter().enumerate() {
            if let CrumbKind::Button(_) = c.kind {
                v.push(Focus::Crumb(i));
            }
        }
        v
    }
}

struct Root {
    sh: Rc<RefCell<Shared>>,
    focus: FocusHandle,
    /// 개발용 캡처: RTE_SHOT=경로, RTE_SHOT_MS=지연(기본 2500), RTE_GO="30,31" 시작 시 이동할 노드 id
    shot: Option<(String, f64)>,
    shot_done: bool,
}

fn js_key(k: &str) -> String {
    match k {
        "escape" => "Escape".into(),
        "backspace" => "Backspace".into(),
        "down" => "ArrowDown".into(),
        "up" => "ArrowUp".into(),
        "left" => "ArrowLeft".into(),
        "right" => "ArrowRight".into(),
        "enter" => "Enter".into(),
        "space" => " ".into(),
        "tab" => "Tab".into(),
        o => o.into(),
    }
}

impl Root {
    fn sync_size(&self, window: &Window) {
        let vs = window.viewport_size();
        let (w, h) = (f32::from(vs.width) as f64, f32::from(vs.height) as f64);
        let mut sh = self.sh.borrow_mut();
        if sh.size != (w, h) {
            sh.size = (w, h);
            sh.en.resize(w, h);
        }
        sh.dark_system = matches!(window.appearance(), WindowAppearance::Dark | WindowAppearance::VibrantDark);
    }
}

impl Render for Root {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.sync_size(window);
        if let Some((path, delay)) = self.shot.clone() {
            let elapsed = self.sh.borrow().start.elapsed().as_secs_f64() * 1000.0;
            if !self.shot_done && elapsed > delay && !self.sh.borrow().en.raf {
                self.shot_done = true;
                match shot::capture(window, &path) {
                    Ok((w, h)) => println!("RTE_SHOT saved {path} {w}x{h}"),
                    Err(e) => println!("RTE_SHOT failed: {e}"),
                }
                cx.quit();
            } else if !self.shot_done {
                window.request_animation_frame();
            }
        }
        {
            let mut sh = self.sh.borrow_mut();
            let now = sh.now();
            sh.en.fire_timers(now);
            if sh.en.raf {
                sh.en.tick(now);
            }
            // 포커스된 노드 요소가 사라지면 DOM 처럼 포커스가 풀린다
            if let Focus::Node(id) = sh.focus {
                if !sh.en.e.contains_key(&id) {
                    sh.focus = Focus::None;
                }
            }
            if sh.en.raf {
                window.request_animation_frame();
            }
        }
        let sh_paint = self.sh.clone();
        let sh_cursor = self.sh.clone();
        let focus = self.focus.clone();
        let _ = &focus;
        let (cursor_style, theme_bg) = {
            let sh = sh_cursor.borrow();
            let mouse_cursor = {
                // 마지막으로 알려진 hover 기준 커서
                if sh.hover_crumb.is_some() {
                    CursorStyle::PointingHand
                } else if let Some(id) = sh.hover {
                    let is_center_root = sh.en.root_flag && sh.en.cur.get(&id).map(|c| c.role == rte_engine::Role::C).unwrap_or(false);
                    if is_center_root { CursorStyle::Arrow } else { CursorStyle::PointingHand }
                } else {
                    CursorStyle::Arrow
                }
            };
            (mouse_cursor, sh.theme().bg)
        };
        let sh_down = self.sh.clone();
        let sh_move = self.sh.clone();
        let sh_up = self.sh.clone();
        let sh_wheel = self.sh.clone();
        let sh_key = self.sh.clone();
        div()
            .size_full()
            .bg(theme_bg)
            .cursor(cursor_style)
            .track_focus(&self.focus)
            .on_mouse_down(MouseButton::Left, cx.listener(move |_, e: &MouseDownEvent, window, cx| {
                let mut sh = sh_down.borrow_mut();
                let (x, y) = (f32::from(e.position.x) as f64, f32::from(e.position.y) as f64);
                if let Some((i, kind)) = sh.hit_crumb(x, y) {
                    // 크럼 버튼: 포커스만(클릭은 mouseup 에서)
                    if let CrumbKind::Button(_) = kind {
                        sh.focus = Focus::Crumb(i);
                        sh.focus_visible = false;
                    }
                    sh.down_target = None;
                    drop(sh);
                    window.focus(&cx.focus_handle());
                    cx.notify();
                    return;
                }
                sh.down_target = sh.hit_node(x, y);
                if let Some(id) = sh.down_target {
                    sh.focus = Focus::Node(id);
                    sh.focus_visible = false;
                }
                sh.en.pointer_down(x, y);
                let now = sh.now();
                sh.en.fire_timers(now);
                cx.notify();
            }))
            .on_mouse_move(cx.listener(move |_, e: &MouseMoveEvent, _, cx| {
                let mut sh = sh_move.borrow_mut();
                let (x, y) = (f32::from(e.position.x) as f64, f32::from(e.position.y) as f64);
                sh.en.pointer_move(x, y);
                let c = sh.hit_crumb(x, y);
                sh.hover_crumb = c.and_then(|(i, k)| if let CrumbKind::Button(_) = k { Some(i) } else { None });
                sh.hover = if c.is_some() { None } else { sh.hit_node(x, y) };
                let now = sh.now();
                sh.en.fire_timers(now);
                cx.notify();
            }))
            .on_mouse_up(MouseButton::Left, cx.listener(move |_, e: &MouseUpEvent, _, cx| {
                let mut sh = sh_up.borrow_mut();
                let (x, y) = (f32::from(e.position.x) as f64, f32::from(e.position.y) as f64);
                let now = sh.now();
                if let Some((i, CrumbKind::Button(depth))) = sh.hit_crumb(x, y) {
                    // 크럼 버튼 click → go(n)
                    if sh.focus == Focus::Crumb(i) {
                        let id = sh.en.stack[depth];
                        sh.en.go(id, now);
                    }
                    cx.notify();
                    return;
                }
                sh.en.pointer_up(now);
                if let (Some(down), Some(up)) = (sh.down_target, sh.hit_node(x, y)) {
                    if down == up {
                        sh.en.click_node(down, now);
                    }
                }
                sh.down_target = None;
                sh.en.fire_timers(now);
                cx.notify();
            }))
            .on_scroll_wheel(cx.listener(move |_, e: &ScrollWheelEvent, _, cx| {
                let mut sh = sh_wheel.borrow_mut();
                let (x, y) = (f32::from(e.position.x) as f64, f32::from(e.position.y) as f64);
                // gpui 델타는 브라우저 deltaY 와 부호가 반대(아래로 스크롤 = deltaY 양수)
                let dy = match e.delta {
                    ScrollDelta::Pixels(p) => -(f32::from(p.y) as f64),
                    ScrollDelta::Lines(l) => -(l.y as f64) * 40.0,
                };
                let now = sh.now();
                sh.en.wheel(x, y, dy, now);
                sh.en.fire_timers(now);
                cx.notify();
            }))
            .on_key_down(cx.listener(move |_, e: &KeyDownEvent, _, cx| {
                let mut sh = sh_key.borrow_mut();
                let key = js_key(e.keystroke.key.as_str());
                let now = sh.now();
                match (sh.focus, key.as_str()) {
                    (Focus::Node(id), "Enter") | (Focus::Node(id), " ") if sh.en.e.contains_key(&id) => sh.en.node_key(id, &key, now),
                    (Focus::Crumb(i), "Enter") | (Focus::Crumb(i), " ") => {
                        sh.en.key(&key, now);
                        let items = sh.crumb_layout();
                        if let Some(CrumbItem { kind: CrumbKind::Button(depth), .. }) = items.get(i) {
                            let id = sh.en.stack[*depth];
                            sh.en.go(id, now);
                        }
                    }
                    _ => sh.en.key(&key, now),
                }
                if key == "Tab" {
                    let stops = sh.tab_stops();
                    if !stops.is_empty() {
                        let cur = stops.iter().position(|s| *s == sh.focus);
                        let n = stops.len();
                        let next = match (cur, e.keystroke.modifiers.shift) {
                            (Some(i), false) => (i + 1) % n,
                            (Some(i), true) => (i + n - 1) % n,
                            (None, false) => 0,
                            (None, true) => n - 1,
                        };
                        sh.focus = stops[next];
                        sh.focus_visible = true;
                    }
                }
                sh.en.fire_timers(now);
                cx.notify();
            }))
            .child(
                canvas(
                    |_, _, _| (),
                    move |bounds, _, window, cx| {
                        let sh = sh_paint.borrow();
                        paint(&sh, bounds, window, cx);
                    },
                )
                .size_full(),
            )
    }
}

fn with_alpha(c: Hsla, a: f64) -> Hsla {
    Hsla { a: (c.a as f64 * a.clamp(0.0, 1.0)) as f32, ..c }
}

fn circle(window: &mut Window, cx_: f64, cy: f64, r: f64, fill: Option<Hsla>, bw: f64, border: Hsla) {
    let b = Bounds { origin: point(px((cx_ - r) as f32), px((cy - r) as f32)), size: size(px((2.0 * r) as f32), px((2.0 * r) as f32)) };
    window.paint_quad(quad(b, Corners::all(px(r as f32)), fill.unwrap_or_else(transparent_black), Edges::all(px(bw as f32)), border, BorderStyle::Solid));
}

fn paint(sh: &Shared, _bounds: Bounds<Pixels>, window: &mut Window, cx: &mut App) {
    let th = sh.theme();
    let en = &sh.en;
    let tc = &sh.tc;
    let (cxm, cym, r1, r2) = (en.cx, en.cy, en.r1, en.r2);
    // .rim: 두 개의 원(stroke 1)
    for r in [r1, r2] {
        circle(window, cxm, cym, r + 0.5, None, 1.0, th.rim);
    }
    // 링크(line.link, stroke mut .75, opacity)
    for l in en.pool.iter().filter(|l| l.disp) {
        let mut pb = PathBuilder::stroke(px(0.75));
        pb.move_to(point(px(l.x1 as f32), px(l.y1 as f32)));
        pb.line_to(point(px(l.x2 as f32), px(l.y2 as f32)));
        if let Ok(p) = pb.build() {
            window.paint_path(p, with_alpha(th.mut_, l.op));
        }
    }
    // 노드: DOM 순서
    for &id in &en.dom {
        let e = &en.e[&id];
        let cls = e.cls.as_deref().unwrap_or("");
        let role = cls.split(' ').nth(1).unwrap_or("k");
        let hk = cls.ends_with(" hk");
        let (x, y) = e.tf;
        let hovered = sh.hover == Some(id) && role != "c" && e.pe;
        let focused = sh.focus == Focus::Node(id) && sh.focus_visible;
        let accent = hovered || focused;
        let (fill, stroke) = if role == "c" || accent {
            (th.acc, th.acc)
        } else if role == "g" {
            (if hk { th.mut_ } else { th.bg }, th.mut_)
        } else {
            (if hk { th.ink } else { th.bg }, th.ink)
        };
        let ga = e.gop;
        circle(window, x, y, e.r + 0.75, Some(with_alpha(fill, ga)), 1.5, with_alpha(stroke, ga));
        if let Some(rg) = e.ring {
            // .pt: 트랙 원(stroke rim 2)
            circle(window, x, y, rg.r + 1.0, None, 2.0, with_alpha(th.rim, ga));
            // .pr: 엄지 호(stroke acc 2.5, round cap). 경로는 위쪽에서 시작해 반시계, dash=THUMB 각도
            let c_len = 2.0 * std::f64::consts::PI * rg.r;
            let u0 = -rg.off / c_len;
            let u1 = u0 + rg.dash / c_len;
            let ang = |u: f64| (-90.0 - u * 360.0) * std::f64::consts::PI / 180.0;
            let (a0, a1) = (ang(u0), ang(u1));
            let p0 = point(px((x + rg.r * a0.cos()) as f32), px((y + rg.r * a0.sin()) as f32));
            let p1 = point(px((x + rg.r * a1.cos()) as f32), px((y + rg.r * a1.sin()) as f32));
            let mut pb = PathBuilder::stroke(px(2.5));
            pb.move_to(p0);
            pb.arc_to(point(px(rg.r as f32), px(rg.r as f32)), px(0.0), false, false, p1);
            if let Ok(p) = pb.build() {
                window.paint_path(p, with_alpha(th.acc, ga));
            }
            for a in [a0, a1] {
                circle(window, x + rg.r * a.cos(), y + rg.r * a.sin(), 1.25, Some(with_alpha(th.acc, ga)), 0.0, transparent_black());
            }
        }
        // 텍스트 (.t1): fill ink(g 는 mut, 400), 후광 stroke bg 3px, 불투명도 = gop * top
        if !e.t1.txt.is_empty() {
            if let (Some(tx), Some(ty), Some(an), Some(fs)) = (e.t1.x, e.t1.y, e.t1.anchor, e.t1.fs) {
                let g = role == "g";
                let fill = if g { th.mut_ } else { th.ink };
                let wt = if g { 400.0 } else { 500.0 };
                let a = e.gop * e.top;
                tc.paint(window, cx, &e.t1.txt, x + tx, y + ty, an, fs, wt, with_alpha(fill, a), Some(with_alpha(th.bg, a)), 0.01, focused);
            }
        }
    }
    // 크럼(.bar > .crumbs)
    let items = sh.crumb_layout();
    for (i, c) in items.iter().enumerate() {
        let base = BAR_PAD_Y + 6.0 + 13.0; // padding 6 + ascent 13
        match c.kind {
            CrumbKind::Sep => {
                tc.paint(window, cx, &c.text, c.text_x, base, "start", 13.0, 500.0, with_alpha(th.mut_, 0.5), None, 0.01, false);
            }
            CrumbKind::Last => {
                tc.paint(window, cx, &c.shown, c.text_x, base, "start", 13.0, 500.0, th.ink, None, 0.01, false);
            }
            CrumbKind::Button(_) => {
                let hov = sh.hover_crumb == Some(i);
                let col = if hov { th.ink } else { th.mut_ };
                tc.paint(window, cx, &c.shown, c.text_x, base, "start", 13.0, 400.0, col, None, 0.0, false);
                if sh.focus == Focus::Crumb(i) && sh.focus_visible {
                    let (rx, ry, rw, rh) = sh.crumb_rect(c);
                    // outline: 2px acc, offset 2px, border-radius 6 → 외곽 반지름 10
                    let b = Bounds { origin: point(px((rx - 4.0) as f32), px((ry - 4.0) as f32)), size: size(px((rw + 8.0) as f32), px((rh + 8.0) as f32)) };
                    window.paint_quad(quad(b, Corners::all(px(10.0)), transparent_black(), Edges::all(px(2.0)), th.acc, BorderStyle::Solid));
                }
            }
        }
    }
    // 힌트(.hint): 하단 중앙, 13px mut, 좌우 패딩 16, 좁으면 줄바꿈(아래 기준 쌓임)
    let hint = "노드 클릭: 이동 · 중앙 노드 클릭: 뒤로 · 링에서 스크롤·드래그: 회전";
    let lines = tc.wrap(hint, 13.0, 400.0, en.w - 32.0, 0.01);
    let n = lines.len() as f64;
    for (i, ln) in lines.iter().enumerate() {
        let top = en.h - 10.0 - 16.0 * (n - i as f64);
        tc.paint(window, cx, ln, en.w / 2.0, top + 13.0, "middle", 13.0, 400.0, th.mut_, None, 0.01, false);
    }
    let _ = VIS;
    let _ = Arc::strong_count(&sh.tc.ts);
}

fn main() {
    let sz = std::env::var("RTE_SIZE").ok().and_then(|v| v.split_once('x').map(|(a, b)| (a.parse::<f32>().unwrap_or(1100.0), b.parse::<f32>().unwrap_or(800.0)))).unwrap_or((1100.0, 800.0));
    let dark = std::env::var("RTE_THEME").ok().map(|t| t == "dark");
    Application::new().run(move |cx: &mut App| {
        text::register_fonts(&cx.text_system().clone());
        let data: serde_json::Value = serde_json::from_str(include_str!("../../rte-engine/data/data.json")).unwrap();
        let start = Instant::now();
        let bounds = Bounds::centered(None, size(px(sz.0), px(sz.1)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(TitlebarOptions { title: Some("Radial Focus Tree".into()), ..Default::default() }),
                ..Default::default()
            },
            move |window, cx| {
                // 텍스트 측정은 창 단위 텍스트 시스템이 필요하므로 엔진을 여기서 만든다
                let tc = TextCtx::new(window.text_system().clone());
                let measure_tc = tc.clone();
                let en = Engine::new(&data, sz.0 as f64, sz.1 as f64, 1000.0, Box::new(move |s, fs, wt| measure_tc.width(s, fs, wt as f64)));
                let sh = Rc::new(RefCell::new(Shared {
                    en,
                    tc,
                    start,
                    dark_override: dark,
                    dark_system: matches!(window.appearance(), WindowAppearance::Dark | WindowAppearance::VibrantDark),
                    hover: None,
                    hover_crumb: None,
                    focus: Focus::None,
                    focus_visible: false,
                    down_target: None,
                    size: (sz.0 as f64, sz.1 as f64),
                }));
                let focus = cx.focus_handle();
                window.focus(&focus);
                if let Ok(go) = std::env::var("RTE_GO") {
                    let mut s = sh.borrow_mut();
                    for id in go.split(',').filter_map(|v| v.trim().parse::<usize>().ok()) {
                        let now = s.now();
                        s.en.go(id, now);
                        // 전환이 끝나도록 시계를 앞으로 보낸다(캡처용 결정적 상태)
                    }
                }
                let shot = std::env::var("RTE_SHOT").ok().map(|p| (p, std::env::var("RTE_SHOT_MS").ok().and_then(|v| v.parse().ok()).unwrap_or(2500.0)));
                cx.new(|_| Root { sh, focus, shot, shot_done: false })
            },
        )
        .unwrap();
        cx.activate(true);
    });
    let _ = DispatchPhase::Bubble;
}
