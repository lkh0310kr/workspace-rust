//! Spike A: gpui + wry(WKWebView) 를 child NSView 로 부착.
//! `SPIKE_AUTOTEST=1` 이면 자동 검증 시나리오를 돌리고 결과를 stdout/JSON 으로 남기고 종료한다.

mod native;
mod autotest;

use gpui::{
    canvas, div, point, prelude::*, px, rgb, size, App, Application, Bounds, Context, Entity,
    MouseButton, Window, WindowBounds, WindowOptions,
};
use raw_window_handle::{HasWindowHandle, RawWindowHandle, WindowHandle};
use std::cell::RefCell;
use std::rc::Rc;
use wry::{
    dpi::{LogicalPosition, LogicalSize},
    NewWindowResponse, Rect, WebView, WebViewBuilder,
};
#[allow(unused_imports)]
use wry::WebViewBuilderExtDarwin;

pub const PORT: u16 = 18080;
pub const TOOLBAR_H: f32 = 40.0;
pub const FRAME_PAD: f32 = 8.0;
pub const OVERLAY: (f32, f32, f32, f32) = (-100.0, 120.0, 300.0, 200.0); // dx from sidebar, y, w, h

/// 창 raw handle 을 복사해 두는 래퍼. (핸들러 콜백 안에서도 child webview 를 만들기 위함)
pub struct Handle(pub RawWindowHandle);
impl HasWindowHandle for Handle {
    fn window_handle(&self) -> Result<WindowHandle<'_>, raw_window_handle::HandleError> {
        Ok(unsafe { WindowHandle::borrow_raw(self.0) })
    }
}

#[derive(Default)]
pub struct Shared {
    pub wv: Option<WebView>,
    pub popup: Option<WebView>,
    pub bounds: Option<Bounds<gpui::Pixels>>,
    pub handle: Option<RawWindowHandle>,
    pub pending_url: Option<String>,
    pub radius: Option<f64>,
    pub hidden: bool,
}
pub type Sh = Rc<RefCell<Shared>>;

pub struct Root {
    pub sh: Sh,
    pub sidebar_w: f32,
    pub overlay: bool,
    pub overlay_clicks: u32,
    pub tab_hidden: bool,
}

fn rect(b: Bounds<gpui::Pixels>) -> Rect {
    Rect {
        position: LogicalPosition::new(f32::from(b.origin.x) as f64, f32::from(b.origin.y) as f64).into(),
        size: LogicalSize::new(f32::from(b.size.width) as f64, f32::from(b.size.height) as f64).into(),
    }
}

/// 로컬 테스트 사이트 base. (127.0.0.1)
pub fn url(path: &str) -> String {
    format!("http://127.0.0.1:{PORT}{path}")
}

fn build_webview(sh: &Sh, handle: RawWindowHandle, b: Bounds<gpui::Pixels>) -> WebView {
    let sh_for_popup = sh.clone();
    let h = Handle(handle);
    let ua = std::env::var("SPIKE_UA").ok();
    let mut builder = WebViewBuilder::new()
        .with_url(url("/bg?color=ff0000"))
        .with_bounds(rect(b))
        .with_accept_first_mouse(true)
        .with_data_store_identifier(*b"wry-spike-store!")
        .with_new_window_req_handler(move |u, features| {
            eprintln!("[wry] new_window_req url={u} size={:?}", features.size);
            // window.open / target=_blank → 같은 구성(configuration)의 popup webview 를 child 로 생성
            let _ = &sh_for_popup;
            NewWindowResponse::Allow
        });
    if let Some(ua) = ua {
        builder = builder.with_user_agent(ua);
    }
    let wv = builder.build_as_child(&h).expect("webview");
    wv
}

impl Render for Root {
    fn render(&mut self, _w: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let sh = self.sh.clone();
        let btn = |label: &'static str| {
            div()
                .px_3()
                .py_1()
                .rounded_md()
                .bg(rgb(0x444444))
                .text_color(rgb(0xffffff))
                .child(label)
        };
        let nav = |sh: Sh, path: &'static str| {
            move |_: &gpui::MouseDownEvent, _: &mut Window, _: &mut App| {
                if let Some(wv) = sh.borrow().wv.as_ref() {
                    let _ = wv.load_url(&url(path));
                }
            }
        };
        let (dx, oy, ow, oh) = OVERLAY;
        div()
            .relative()
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(0x1e1e1e))
            .child(
                div()
                    .h(px(TOOLBAR_H))
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_2()
                    .bg(rgb(0x333333))
                    .child(btn("overlay").on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|t, _, _, cx| {
                            t.overlay = !t.overlay;
                            cx.notify();
                        }),
                    ))
                    .child(btn("sidebar").on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|t, _, _, cx| {
                            t.sidebar_w = if t.sidebar_w < 300.0 { 400.0 } else { 200.0 };
                            cx.notify();
                        }),
                    ))
                    .child(btn("hide").on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|t, _, _, cx| {
                            t.tab_hidden = !t.tab_hidden;
                            cx.notify();
                        }),
                    ))
                    .child(btn("bg").on_mouse_down(MouseButton::Left, nav(sh.clone(), "/bg?color=ff0000")))
                    .child(btn("login").on_mouse_down(MouseButton::Left, nav(sh.clone(), "/login")))
                    .child(btn("oauth").on_mouse_down(MouseButton::Left, nav(sh.clone(), "/oauth/start")))
                    .child(btn("ua").on_mouse_down(MouseButton::Left, nav(sh.clone(), "/ua")))
                    .child(btn("ime").on_mouse_down(MouseButton::Left, nav(sh.clone(), "/ime")))
                    .child(div().text_color(rgb(0xffffff)).child(format!(
                        "gpui overlay clicks: {}",
                        self.overlay_clicks
                    ))),
            )
            .child(
                div()
                    .flex_1()
                    .flex()
                    .child(
                        div()
                            .w(px(self.sidebar_w))
                            .bg(rgb(0x252526))
                            .text_color(rgb(0xcccccc))
                            .p_2()
                            .child("gpui sidebar"),
                    )
                    .child(
                        // gpui 가 그리는 "프레임". 웹뷰는 이 안쪽 canvas 영역에 부착된다.
                        div().flex_1().p(px(FRAME_PAD)).bg(rgb(0x00aa00)).rounded(px(12.)).child(
                            canvas(
                                {
                                    let sh = sh.clone();
                                    let hidden = self.tab_hidden;
                                    move |bounds, window, _cx| {
                                        let mut s = sh.borrow_mut();
                                        let handle = window.window_handle().unwrap().as_raw();
                                        s.handle = Some(handle);
                                        s.bounds = Some(bounds);
                                        drop(s);
                                        if sh.borrow().wv.is_none() {
                                            let wv = build_webview(&sh, handle, bounds);
                                            sh.borrow_mut().wv = Some(wv);
                                        }
                                        let s = sh.borrow();
                                        let wv = s.wv.as_ref().unwrap();
                                        let _ = wv.set_bounds(rect(bounds));
                                        let _ = wv.set_visible(!hidden);
                                        if let Some(r) = s.radius {
                                            native::set_radius(wv, r);
                                        }
                                    }
                                },
                                |_, _, _, _| {},
                            )
                            .size_full(),
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
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|t, _, _, cx| {
                                t.overlay_clicks += 1;
                                cx.notify();
                            }),
                        )
                        .child("gpui overlay (dropdown/modal)"),
                )
            })
    }
}

fn main() {
    let autotest = std::env::var("SPIKE_AUTOTEST").is_ok();
    let reports = testsite::spawn(PORT);
    Application::new().run(move |cx: &mut App| {
        let sh: Sh = Rc::new(RefCell::new(Shared::default()));
        let bounds = Bounds::centered(None, size(px(1200.), px(800.)), cx);
        let sh2 = sh.clone();
        let view: Entity<Root> = cx
            .open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    ..Default::default()
                },
                move |_, cx| {
                    cx.new(|_| Root {
                        sh: sh2,
                        sidebar_w: 200.0,
                        overlay: false,
                        overlay_clicks: 0,
                        tab_hidden: false,
                    })
                },
            )
            .unwrap()
            .update(cx, |_, _, cx| cx.entity())
            .unwrap();
        cx.activate(true);
        let _ = point(px(0.), px(0.));
        if autotest {
            autotest::run(cx, view, sh, reports);
        }
    });
}
