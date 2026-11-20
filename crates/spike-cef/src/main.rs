//! Spike B: gpui + CEF(Chromium) off-screen rendering.
//! 웹 콘텐츠는 gpui 씬 안의 "이미지 한 장"으로 그려지므로 z-order/클립/오버레이는 gpui 규칙을 그대로 따른다.

mod autotest;
mod cefhost;
mod macapp;
mod native;

use cef::{args::Args, *};
use cefhost::{with, Which};
use gpui::{
    canvas, div, point, prelude::*, px, rgb, size, App, Application, Bounds, Context, Corners, Entity,
    EntityInputHandler, FocusHandle, KeyDownEvent, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent,
    Pixels, RenderImage, ScrollDelta, ScrollWheelEvent, UTF16Selection, Window, WindowBounds, WindowOptions,
};
use std::cell::RefCell;
use std::ops::Range as StdRange;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

gpui::actions!(spike, [ToggleOverlay]);

pub const PORT: u16 = 18080;
pub const CTX_W: f32 = 160.0;
pub const CTX_ITEM_H: f32 = 28.0;
pub const TOOLBAR_H: f32 = 40.0;
pub const FRAME_PAD: f32 = 8.0;
pub const RADIUS: f32 = 12.0;
pub const OVERLAY: (f32, f32, f32, f32) = (-100.0, 120.0, 300.0, 200.0);

pub fn url(path: &str) -> String {
    format!("http://127.0.0.1:{PORT}{path}")
}

#[derive(Default)]
pub struct Shared {
    pub bounds: Option<Bounds<Pixels>>,
    pub img: [Option<(u64, Arc<RenderImage>)>; 3],
    pub last_size: (f32, f32),
    pub paint_us: u128,
    pub paints: u64,
}
pub type Sh = Rc<RefCell<Shared>>;

pub struct Root {
    pub sh: Sh,
    pub sidebar_w: f32,
    pub overlay: bool,
    pub overlay_clicks: u32,
    pub tab_hidden: bool,
    pub focus: FocusHandle,
    pub marked: Option<StdRange<usize>>,
    pub action_toggles: u32,
}

/// 팝업(별도 Browser)이 그려지는 gpui 논리 사각형: 캔버스 중앙
pub fn popup_rect(canvas: Bounds<Pixels>) -> Bounds<Pixels> {
    let (w, h) = with(|s| s.size[1]);
    let (w, h) = (w.min(f32::from(canvas.size.width) - 40.0), h.min(f32::from(canvas.size.height) - 40.0));
    Bounds {
        origin: point(
            canvas.origin.x + (canvas.size.width - px(w)) / 2.0,
            canvas.origin.y + (canvas.size.height - px(h)) / 2.0,
        ),
        size: size(px(w), px(h)),
    }
}

fn host(which: Which) -> Option<BrowserHost> {
    with(|s| match which {
        Which::Main => s.main.clone(),
        Which::Popup => s.popup.clone(),
    })
    .and_then(|b| b.host())
}

fn mods(m: &gpui::Modifiers, left_down: bool) -> u32 {
    (if m.shift { 2 } else { 0 })
        | (if m.control { 4 } else { 0 })
        | (if m.alt { 8 } else { 0 })
        | (if m.platform { 128 } else { 0 })
        | (if left_down { 16 } else { 0 })
}

impl Root {
    /// 창 좌표 → (대상 브라우저, 대상 로컬 좌표)
    fn route(&self, pos: gpui::Point<Pixels>) -> Option<(Which, i32, i32)> {
        let b = self.sh.borrow().bounds?;
        let has_popup = with(|s| s.popup.is_some());
        if has_popup {
            let pr = popup_rect(b);
            if pr.contains(&pos) {
                return Some((Which::Popup, f32::from(pos.x - pr.origin.x) as i32, f32::from(pos.y - pr.origin.y) as i32));
            }
        }
        Some((Which::Main, f32::from(pos.x - b.origin.x) as i32, f32::from(pos.y - b.origin.y) as i32))
    }

    fn mouse_button(&mut self, ev_pos: gpui::Point<Pixels>, m: &gpui::Modifiers, click_count: usize, up: bool, right: bool) {
        let Some((w, x, y)) = self.route(ev_pos) else { return };
        if let Some(h) = host(w) {
            let e = MouseEvent { x, y, modifiers: mods(m, !up && !right) };
            if !up {
                h.set_focus(1);
            }
            h.send_mouse_click_event(Some(&e), if right { MouseButtonType::RIGHT } else { MouseButtonType::LEFT }, up as _, click_count.max(1) as _);
        }
    }
}

impl EntityInputHandler for Root {
    fn text_for_range(&mut self, _r: StdRange<usize>, _a: &mut Option<StdRange<usize>>, _w: &mut Window, _cx: &mut Context<Self>) -> Option<String> {
        None
    }
    fn selected_text_range(&mut self, _i: bool, _w: &mut Window, _cx: &mut Context<Self>) -> Option<UTF16Selection> {
        Some(UTF16Selection { range: 0..0, reversed: false })
    }
    fn marked_text_range(&self, _w: &mut Window, _cx: &mut Context<Self>) -> Option<StdRange<usize>> {
        self.marked.clone()
    }
    fn unmark_text(&mut self, _w: &mut Window, _cx: &mut Context<Self>) {
        self.marked = None;
        if let Some(h) = host(Which::Main) {
            h.ime_finish_composing_text(0);
        }
    }
    fn replace_text_in_range(&mut self, _r: Option<StdRange<usize>>, text: &str, _w: &mut Window, _cx: &mut Context<Self>) {
        self.marked = None;
        let target = if with(|s| s.popup.is_some()) { Which::Popup } else { Which::Main };
        if let Some(h) = host(target) {
            let invalid = Range { from: u32::MAX, to: u32::MAX };
            h.ime_commit_text(Some(&CefString::from(text)), Some(&invalid), 0);
        }
    }
    fn replace_and_mark_text_in_range(&mut self, _r: Option<StdRange<usize>>, text: &str, sel: Option<StdRange<usize>>, _w: &mut Window, _cx: &mut Context<Self>) {
        let n = text.encode_utf16().count();
        self.marked = Some(0..n);
        let target = if with(|s| s.popup.is_some()) { Which::Popup } else { Which::Main };
        if let Some(h) = host(target) {
            let ul = CompositionUnderline {
                size: std::mem::size_of::<sys::cef_composition_underline_t>(),
                range: Range { from: 0, to: n as u32 },
                color: 0,
                background_color: 0,
                thick: 0,
                style: CompositionUnderlineStyle::default(),
            };
            let invalid = Range { from: u32::MAX, to: u32::MAX };
            let sel = sel.map(|s| Range { from: s.start as u32, to: s.end as u32 }).unwrap_or(Range { from: n as u32, to: n as u32 });
            h.ime_set_composition(Some(&CefString::from(text)), Some(&[ul]), Some(&invalid), Some(&sel));
        }
    }
    fn bounds_for_range(&mut self, _r: StdRange<usize>, element_bounds: Bounds<Pixels>, _w: &mut Window, _cx: &mut Context<Self>) -> Option<Bounds<Pixels>> {
        Some(Bounds { origin: element_bounds.origin, size: size(px(2.), px(20.)) })
    }
    fn character_index_for_point(&mut self, _p: gpui::Point<Pixels>, _w: &mut Window, _cx: &mut Context<Self>) -> Option<usize> {
        None
    }
}

/// gpui keystroke → Windows virtual key code (특수키만; 일반 문자는 IME/commit 경로)
fn vkey(k: &str) -> Option<i32> {
    Some(match k {
        "backspace" => 8,
        "tab" => 9,
        "enter" => 13,
        "escape" => 27,
        "space" => 32,
        "pageup" => 33,
        "pagedown" => 34,
        "end" => 35,
        "home" => 36,
        "left" => 37,
        "up" => 38,
        "right" => 39,
        "down" => 40,
        "delete" => 46,
        _ => return None,
    })
}

impl Render for Root {
    fn render(&mut self, _w: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let sh = self.sh.clone();
        let btn = |label: &'static str| {
            div().px_3().py_1().rounded_md().bg(rgb(0x444444)).text_color(rgb(0xffffff)).child(label)
        };
        let nav = |path: &'static str| {
            move |_: &MouseDownEvent, _: &mut Window, _: &mut App| {
                if let Some(b) = with(|s| s.main.clone()) {
                    if let Some(f) = b.main_frame() {
                        f.load_url(Some(&CefString::from(url(path).as_str())));
                    }
                }
            }
        };
        let (dx, oy, ow, oh) = OVERLAY;
        let entity = cx.entity();
        let focus = self.focus.clone();
                let hidden = self.tab_hidden;
        div()
            .relative()
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(0x1e1e1e))
            .on_action(cx.listener(|t, _: &ToggleOverlay, _, cx| {
                t.action_toggles += 1;
                t.overlay = !t.overlay;
                cx.notify();
            }))
            .child(
                div()
                    .h(px(TOOLBAR_H))
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_2()
                    .bg(rgb(0x333333))
                    .child(btn("overlay").on_mouse_down(MouseButton::Left, cx.listener(|t, _, _, cx| { t.overlay = !t.overlay; cx.notify(); })))
                    .child(btn("sidebar").on_mouse_down(MouseButton::Left, cx.listener(|t, _, _, cx| { t.sidebar_w = if t.sidebar_w < 300.0 { 400.0 } else { 200.0 }; cx.notify(); })))
                    .child(btn("hide").on_mouse_down(MouseButton::Left, cx.listener(|t, _, _, cx| { t.tab_hidden = !t.tab_hidden; cx.notify(); })))
                    .child(btn("bg").on_mouse_down(MouseButton::Left, nav("/bg?color=ff0000")))
                    .child(btn("login").on_mouse_down(MouseButton::Left, nav("/login")))
                    .child(btn("oauth").on_mouse_down(MouseButton::Left, nav("/oauth/start")))
                    .child(btn("ua").on_mouse_down(MouseButton::Left, nav("/ua")))
                    .child(btn("ime").on_mouse_down(MouseButton::Left, nav("/ime")))
                    .child(div().text_color(rgb(0xffffff)).child(format!("gpui overlay clicks: {}", self.overlay_clicks))),
            )
            .child(
                div()
                    .flex_1()
                    .flex()
                    .child(div().w(px(self.sidebar_w)).bg(rgb(0x252526)).text_color(rgb(0xcccccc)).p_2().child("gpui sidebar"))
                    .child(
                        div()
                            .flex_1()
                            .p(px(FRAME_PAD))
                            .bg(rgb(0x00aa00))
                            .rounded(px(RADIUS))
                            .child(
                                div()
                                    .size_full()
                                    .cursor(match with(|s| s.cursor) {
                                        1 => gpui::CursorStyle::PointingHand,
                                        2 => gpui::CursorStyle::IBeam,
                                        3 => gpui::CursorStyle::Crosshair,
                                        _ => gpui::CursorStyle::Arrow,
                                    })
                                    .track_focus(&self.focus)
                                    .on_mouse_down(MouseButton::Left, cx.listener(|t, e: &MouseDownEvent, w, _| {
                                        w.focus(&t.focus);
                                        with(|s| if s.ctx.take().is_some() { s.ui_gen += 1 });
                                        t.mouse_button(e.position, &e.modifiers, e.click_count, false, false)
                                    }))
                                    .on_mouse_down(MouseButton::Right, cx.listener(|t, e: &MouseDownEvent, w, _| {
                                        w.focus(&t.focus);
                                        t.mouse_button(e.position, &e.modifiers, 1, false, true)
                                    }))
                                    .on_mouse_up(MouseButton::Right, cx.listener(|t, e: &MouseUpEvent, _, _| {
                                        t.mouse_button(e.position, &e.modifiers, 1, true, true)
                                    }))
                                    .on_mouse_up(MouseButton::Left, cx.listener(|t, e: &MouseUpEvent, _, _| {
                                        t.mouse_button(e.position, &e.modifiers, e.click_count, true, false)
                                    }))
                                    .on_mouse_move(cx.listener(|t, e: &MouseMoveEvent, _, _| {
                                        if let Some((w, x, y)) = t.route(e.position) {
                                            if let Some(h) = host(w) {
                                                h.send_mouse_move_event(Some(&MouseEvent { x, y, modifiers: mods(&e.modifiers, e.pressed_button.is_some()) }), 0);
                                            }
                                        }
                                    }))
                                    .on_scroll_wheel(cx.listener(|t, e: &ScrollWheelEvent, _, _| {
                                        if let Some((w, x, y)) = t.route(e.position) {
                                            if let Some(h) = host(w) {
                                                let d = match e.delta {
                                                    ScrollDelta::Pixels(p) => (f32::from(p.x) as i32, f32::from(p.y) as i32),
                                                    ScrollDelta::Lines(p) => ((p.x * 40.0) as i32, (p.y * 40.0) as i32),
                                                };
                                                h.send_mouse_wheel_event(Some(&MouseEvent { x, y, modifiers: mods(&e.modifiers, false) }), d.0, d.1);
                                            }
                                        }
                                    }))
                                    .on_key_down(cx.listener(|t, e: &KeyDownEvent, _, _| {
                                        let target = if with(|s| s.popup.is_some()) { Which::Popup } else { Which::Main };
                                        let Some(h) = host(target) else { return };
                                        let k = &e.keystroke;
                                        let b = with(|s| if target == Which::Popup { s.popup.clone() } else { s.main.clone() });
                                        if k.modifiers.platform {
                                            // OSR 에는 메뉴가 없으므로 편집 단축키는 frame API 로 직접 보낸다
                                            if let Some(f) = b.and_then(|b| b.focused_frame().or_else(|| b.main_frame())) {
                                                match k.key.as_str() {
                                                    "a" => f.select_all(),
                                                    "c" => f.copy(),
                                                    "x" => f.cut(),
                                                    "v" => f.paste(),
                                                    "z" => if k.modifiers.shift { f.redo() } else { f.undo() },
                                                    _ => {}
                                                }
                                            }
                                            return;
                                        }
                                        // 일반 문자: 텍스트 삽입은 IME commit 경로(중복 방지)이므로 keydown/keyup 만 JS 에 전달. 조합 중이면 생략.
                                        let printable = k.key.chars().count() == 1 && !k.modifiers.control && t.marked.is_none();
                                        let vk_opt = vkey(&k.key).or_else(|| {
                                            if !printable { return None; }
                                            let c = k.key.chars().next().unwrap();
                                            if c.is_ascii_alphabetic() { Some(c.to_ascii_uppercase() as i32) } else if c.is_ascii_digit() { Some(c as i32) } else { None }
                                        });
                                        if let Some(vk) = vk_opt {
                                            let m = mods(&k.modifiers, false);
                                            let mut ev = KeyEvent {
                                                size: std::mem::size_of::<sys::cef_key_event_t>(),
                                                type_: KeyEventType::RAWKEYDOWN,
                                                modifiers: m,
                                                windows_key_code: vk,
                                                native_key_code: 0,
                                                is_system_key: 0,
                                                character: if printable { k.key.chars().next().map(|c| c as u16).unwrap_or(0) } else { 0 },
                                                unmodified_character: if printable { k.key.chars().next().map(|c| c as u16).unwrap_or(0) } else { 0 },
                                                focus_on_editable_field: 0,
                                            };
                                            with(|s| s.log.push(format!("key_down handler key={:?} vk={vk}", k.key)));
                                            h.send_key_event(Some(&ev));
                                            if vk == 13 {
                                                ev.type_ = KeyEventType::CHAR;
                                                ev.character = 13;
                                                ev.unmodified_character = 13;
                                                h.send_key_event(Some(&ev));
                                            }
                                            ev.type_ = KeyEventType::KEYUP;
                                            ev.character = 0;
                                            ev.unmodified_character = 0;
                                            h.send_key_event(Some(&ev));
                                        }
                                        let _ = t;
                                    }))
                                    .child(
                                        canvas(
                                            {
                                                let sh = sh.clone();
                                                move |bounds, window, _cx| {
                                                    sh.borrow_mut().bounds = Some(bounds);
                                                    let sz = (f32::from(bounds.size.width), f32::from(bounds.size.height));
                                                    let scale = window.scale_factor();
                                                    let changed = {
                                                        let mut s = sh.borrow_mut();
                                                        let c = s.last_size != sz;
                                                        s.last_size = sz;
                                                        c
                                                    };
                                                    if changed {
                                                        with(|s| {
                                                            s.size[0] = sz;
                                                            s.scale = scale;
                                                        });
                                                        if let Some(h) = host(Which::Main) {
                                                            h.was_resized();
                                                        }
                                                    }
                                                    bounds
                                                }
                                            },
                                            {
                                                let sh = sh.clone();
                                                move |_, bounds, window, cx| {
                                                    window.handle_input(&focus, gpui::ElementInputHandler::new(bounds, entity.clone()), cx);
                                                    if hidden {
                                                        return;
                                                    }
                                                    let t = std::time::Instant::now();
                                                    let wr = with(|s| s.widget);
                                                    let widget_b = Bounds { origin: point(bounds.origin.x + px(wr.0 as f32), bounds.origin.y + px(wr.1 as f32)), size: size(px(wr.2 as f32), px(wr.3 as f32)) };
                                                    for (i, b) in [(0usize, bounds), (1usize, popup_rect(bounds)), (2usize, widget_b)] {
                                                        if i == 1 && with(|s| s.popup.is_none()) {
                                                            continue;
                                                        }
                                                        if i == 2 && !with(|s| s.widget_visible) {
                                                            continue;
                                                        }
                                                        let (gen, w, h, data) = with(|s| {
                                                            let f = &s.surf[i];
                                                            (f.gen, f.w, f.h, if sh.borrow().img[i].as_ref().map(|x| x.0) == Some(f.gen) { None } else { Some(f.data.clone()) })
                                                        });
                                                        if gen == 0 {
                                                            continue;
                                                        }
                                                        if let Some(data) = data {
                                                            // BGRA 바이트를 그대로 RGBA 컨테이너에 담는다 (gpui RenderImage 는 BGRA 레이아웃)
                                                            let buf = image::RgbaImage::from_raw(w, h, data).unwrap();
                                                            let ri = Arc::new(RenderImage::new(smallvec::smallvec![image::Frame::new(buf)]));
                                                            if let Some((_, old)) = sh.borrow_mut().img[i].replace((gen, ri)) {
                                                                let _ = window.drop_image(old);
                                                            }
                                                        }
                                                        if let Some((_, ri)) = sh.borrow().img[i].clone() {
                                                            let radii = if i == 0 { Corners::all(px(RADIUS - FRAME_PAD / 2.0)) } else { Corners::all(px(8.)) };
                                                            let _ = window.paint_image(b, radii, ri, 0, false);
                                                        }
                                                    }
                                                    let mut s = sh.borrow_mut();
                                                    s.paint_us += t.elapsed().as_micros();
                                                    s.paints += 1;
                                                }
                                            },
                                        )
                                        .size_full(),
                                    ),
                            ),
                    ),
            )
            .when(self.overlay, |d| {
                d.child(
                    div()
                        .absolute()
                        .left(px(self.sidebar_w + dx))
                        .top(px(oy))
                        .w(px(ow))
                        .h(px(oh))
                        .bg(rgb(0x0000ff))
                        .text_color(rgb(0xffffff))
                        .occlude()
                        .on_mouse_down(MouseButton::Left, cx.listener(|t, _, _, cx| { t.overlay_clicks += 1; cx.notify(); }))
                        .child("gpui overlay (dropdown/modal)"),
                )
            })
            .when_some(with(|s| s.ctx.clone()), |d, (x, y, link)| {
                let o = self.sh.borrow().bounds.map(|b| b.origin).unwrap_or_default();
                let items = ["Copy", "Paste", "Select all", "Reload"];
                d.child(
                    div()
                        .absolute()
                        .left(o.x + px(x as f32))
                        .top(o.y + px(y as f32))
                        .w(px(CTX_W))
                        .bg(rgb(0x2d2d2d))
                        .text_color(rgb(0xffffff))
                        .rounded(px(6.))
                        .occlude()
                        .children(items.into_iter().map(|label| {
                            div()
                                .h(px(CTX_ITEM_H))
                                .px_2()
                                .flex()
                                .items_center()
                                .child(if link.is_empty() { label.to_string() } else { format!("{label}") })
                                .on_mouse_down(MouseButton::Left, cx.listener(move |_, _, _, cx| {
                                    let b = with(|s| s.main.clone());
                                    if let Some(b) = b {
                                        let f = b.focused_frame().or_else(|| b.main_frame());
                                        match (label, f) {
                                            ("Copy", Some(f)) => f.copy(),
                                            ("Paste", Some(f)) => f.paste(),
                                            ("Select all", Some(f)) => f.select_all(),
                                            ("Reload", _) => b.reload(),
                                            _ => {}
                                        }
                                    }
                                    with(|s| { s.ctx = None; s.ui_gen += 1; s.log.push(format!("ctx_item {label}")); });
                                    cx.notify();
                                }))
                        })),
                )
            })
    }
}

fn main() {
    let _loader = {
        let loader = library_loader::LibraryLoader::new(&std::env::current_exe().unwrap(), false);
        assert!(loader.load(), "CEF framework load failed (번들 .app 으로 실행해야 함)");
        loader
    };
    let _ = api_hash(sys::CEF_API_VERSION_LAST, 0);

    let args = Args::new();
    let cmd = args.as_cmd_line().unwrap();
    let is_browser = cmd.has_switch(Some(&CefString::from("type"))) != 1;
    let mut app = cefhost::make_app();
    let ret = execute_process(Some(args.as_main_args()), Some(&mut app), std::ptr::null_mut());
    if !is_browser {
        std::process::exit(ret.max(0));
    }

    let patched = unsafe { macapp::patch_application() };
    println!("macapp: {patched}");

    let autotest = std::env::var("SPIKE_AUTOTEST").is_ok();
    let reports = testsite::spawn(PORT);

    Application::new().run(move |cx: &mut App| {
        let sh: Sh = Rc::new(RefCell::new(Shared::default()));
        let bg = native::background();
        // 백그라운드 모드: 포커스를 가져오지 않고, 창은 autotest 에서 stealth() 처리(거의 투명, 마우스 투과)
        let bounds = Bounds::centered(None, size(px(1200.), px(800.)), cx);
        let sh2 = sh.clone();
        let win = cx
            .open_window(
                WindowOptions { window_bounds: Some(WindowBounds::Windowed(bounds)), focus: !bg, ..Default::default() },
                move |_, cx| {
                    cx.new(|cx| Root {
                        sh: sh2,
                        sidebar_w: 200.0,
                        overlay: false,
                        overlay_clicks: 0,
                        tab_hidden: false,
                        focus: cx.focus_handle(),
                        marked: None,
                        action_toggles: 0,
                    })
                },
            )
            .unwrap();
        let view: Entity<Root> = win.update(cx, |_, _, cx| cx.entity()).unwrap();
        cx.bind_keys([gpui::KeyBinding::new("cmd-k", ToggleOverlay, None)]);
        let _nap = if bg { native::set_accessory(); Some(native::disable_app_nap()) } else { cx.activate(true); None };
        std::mem::forget(_nap);

        // CEF 초기화 (gpui 의 NSApplication 이 만들어진 뒤)
        let cache = std::env::var("SPIKE_CACHE").unwrap_or_else(|_| {
            std::env::current_dir().unwrap().join("target/cef-cache").to_string_lossy().into_owned()
        });
        let settings = Settings {
            windowless_rendering_enabled: true as _,
            external_message_pump: true as _,
            no_sandbox: true as _,
            root_cache_path: CefString::from(cache.as_str()),
            cache_path: CefString::from(cache.as_str()),
            ..Default::default()
        };
        let ok = initialize(Some(args.as_main_args()), Some(&settings), Some(&mut app), std::ptr::null_mut());
        println!("cef initialize -> {ok}");
        assert_eq!(ok, 1);

        // 메시지 펌프 + 프레임 갱신 알림
        let v2 = view.clone();
        cx.spawn(async move |cx| {
            let mut last = 0u64;
            loop {
                do_message_loop_work();
                if with(|s| s.ready && !s.create_requested) {
                    with(|s| s.create_requested = true);
                    cefhost::create_main_browser(&url("/bg?color=ff0000"));
                }
                let g = with(|s| s.surf[0].gen + s.surf[1].gen + s.surf[2].gen + s.widget_visible as u64 + s.ui_gen);
                if g != last {
                    last = g;
                    v2.update(cx, |_, cx| cx.notify()).ok();
                }
                cx.background_executor().timer(Duration::from_millis(4)).await;
            }
        })
        .detach();

        if autotest {
            autotest::run(cx, view, sh, reports);
        }
    });
    cef::shutdown();
}
