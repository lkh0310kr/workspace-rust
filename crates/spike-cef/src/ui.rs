use std::ops::Range as StdRange;
use std::{cell::RefCell, rc::Rc, sync::Arc};

use gpui::prelude::FluentBuilder;
use gpui::{
    canvas, div, point, px, rgb, size, App, Bounds, Context, Corners, EntityInputHandler,
    FocusHandle, InteractiveElement, IntoElement, KeyDownEvent, MouseButton, MouseDownEvent,
    MouseMoveEvent, MouseUpEvent, ParentElement, Pixels, Render, RenderImage, ScrollDelta,
    ScrollWheelEvent, Styled, UTF16Selection, Window,
};

use crate::application::{Application, BrowserCommand, BrowserTarget, CursorIcon, SurfaceTarget};

gpui::actions!(spike, [ToggleOverlay]);

pub const PORT: u16 = 18080;
const CTX_W: f32 = 160.0;
const CTX_ITEM_H: f32 = 28.0;
const TOOLBAR_H: f32 = 40.0;
const FRAME_PAD: f32 = 8.0;
const RADIUS: f32 = 12.0;
const OVERLAY: (f32, f32, f32, f32) = (-100.0, 120.0, 300.0, 200.0);

/// 눈에 띄는 색(빨강/파랑/초록)은 픽셀 검증용. 기본은 흰색.
fn colorful() -> bool {
    std::env::var("SPIKE_COLORFUL").is_ok()
}

pub fn url(path: &str) -> String {
    format!("http://127.0.0.1:{PORT}{path}")
}

fn mods(m: &gpui::Modifiers, left_down: bool) -> u32 {
    (if m.shift { 2 } else { 0 })
        | (if m.control { 4 } else { 0 })
        | (if m.alt { 8 } else { 0 })
        | (if m.platform { 128 } else { 0 })
        | (if left_down { 16 } else { 0 })
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
    pub app: Rc<Application>,
    pub sidebar_w: f32,
    pub overlay: bool,
    pub overlay_clicks: u32,
    pub tab_hidden: bool,
    pub focus: FocusHandle,
    pub marked: Option<StdRange<usize>>,
    pub action_toggles: u32,
}

impl Root {
    /// 창 좌표 → (대상 브라우저, 대상 로컬 좌표)
    fn route(&self, pos: gpui::Point<Pixels>) -> Option<(BrowserTarget, i32, i32)> {
        let b = self.sh.borrow().bounds?;
        Some(self.app.route_pointer(
            f32::from(pos.x - b.origin.x),
            f32::from(pos.y - b.origin.y),
            f32::from(b.size.width),
            f32::from(b.size.height),
        ))
    }
    fn mouse_button(
        &mut self,
        ev_pos: gpui::Point<Pixels>,
        m: &gpui::Modifiers,
        click_count: usize,
        up: bool,
        right: bool,
    ) {
        let Some((w, x, y)) = self.route(ev_pos) else {
            return;
        };

        self.app
            .mouse_button(w, x, y, mods(m, !up && !right), up, right, click_count);
    }
}

impl EntityInputHandler for Root {
    fn text_for_range(
        &mut self,
        _r: StdRange<usize>,
        _a: &mut Option<StdRange<usize>>,
        _w: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<String> {
        None
    }
    fn selected_text_range(
        &mut self,
        _i: bool,
        _w: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: 0..0,
            reversed: false,
        })
    }
    fn marked_text_range(
        &self,
        _w: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<StdRange<usize>> {
        self.marked.clone()
    }
    fn unmark_text(&mut self, _w: &mut Window, _cx: &mut Context<Self>) {
        self.marked = None;
        self.app.ime_finish();
    }
    fn replace_text_in_range(
        &mut self,
        _r: Option<StdRange<usize>>,
        text: &str,
        _w: &mut Window,
        _cx: &mut Context<Self>,
    ) {
        self.marked = None;
        self.app.ime_commit(text);
    }
    fn replace_and_mark_text_in_range(
        &mut self,
        _r: Option<StdRange<usize>>,
        text: &str,
        sel: Option<StdRange<usize>>,
        _w: &mut Window,
        _cx: &mut Context<Self>,
    ) {
        let n = text.encode_utf16().count();

        self.app.record_foreign_ime();
        self.marked = Some(0..n);

        self.app
            .ime_set_composition(text, sel.map(|s| (s.start, s.end)));
    }
    fn bounds_for_range(
        &mut self,
        _r: StdRange<usize>,
        element_bounds: Bounds<Pixels>,
        _w: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        Some(Bounds {
            origin: element_bounds.origin,
            size: size(px(2.), px(20.)),
        })
    }
    fn character_index_for_point(
        &mut self,
        _p: gpui::Point<Pixels>,
        _w: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        None
    }
}

impl Render for Root {
    fn render(&mut self, _w: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let sh = self.sh.clone();
        let ui = self.app.ui_state();
        let btn = |label: &'static str| {
            div()
                .px_3()
                .py_1()
                .rounded_md()
                .bg(rgb(0x444444))
                .text_color(rgb(0xffffff))
                .child(label)
        };
        let app = self.app.clone();
        let nav = |path: &'static str| {
            let app = app.clone();
            move |_: &MouseDownEvent, _: &mut Window, _: &mut App| {
                let url = if path.starts_with("http") {
                    path.to_string()
                } else {
                    url(path)
                };

                app.navigate(&url);
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
                    .child(btn("bg").on_mouse_down(MouseButton::Left, nav("/bg?color=ff0000")))
                    .child(btn("login").on_mouse_down(MouseButton::Left, nav("/login")))
                    .child(btn("oauth").on_mouse_down(MouseButton::Left, nav("/oauth/start")))
                    .child(btn("ua").on_mouse_down(MouseButton::Left, nav("/ua")))
                    .child(btn("ime").on_mouse_down(MouseButton::Left, nav("/ime")))
                    .child(
                        btn("google")
                            .on_mouse_down(MouseButton::Left, nav("https://accounts.google.com/")),
                    )
                    .child(
                        div()
                            .text_color(rgb(0xffffff))
                            .child(format!("gpui overlay clicks: {}", self.overlay_clicks)),
                    ),
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
                        div()
                            .flex_1()
                            .p(px(FRAME_PAD))
                            .bg(rgb(if colorful() { 0x00aa00 } else { 0xffffff }))
                            .rounded(px(RADIUS))
                            .child(
                                div()
                                    .size_full()
                                    .cursor(match ui.cursor {
                                        CursorIcon::Hand => gpui::CursorStyle::PointingHand,
                                        CursorIcon::Text => gpui::CursorStyle::IBeam,
                                        CursorIcon::Crosshair => gpui::CursorStyle::Crosshair,
                                        CursorIcon::Arrow => gpui::CursorStyle::Arrow,
                                    })
                                    .track_focus(&self.focus)
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(|t, e: &MouseDownEvent, w, _| {
                                            w.focus(&t.focus);
                                            t.app.dismiss_context_menu();
                                            t.mouse_button(
                                                e.position,
                                                &e.modifiers,
                                                e.click_count,
                                                false,
                                                false,
                                            )
                                        }),
                                    )
                                    .on_mouse_down(
                                        MouseButton::Right,
                                        cx.listener(|t, e: &MouseDownEvent, w, _| {
                                            w.focus(&t.focus);
                                            t.mouse_button(e.position, &e.modifiers, 1, false, true)
                                        }),
                                    )
                                    .on_mouse_up(
                                        MouseButton::Right,
                                        cx.listener(|t, e: &MouseUpEvent, _, _| {
                                            t.mouse_button(e.position, &e.modifiers, 1, true, true)
                                        }),
                                    )
                                    .on_mouse_up(
                                        MouseButton::Left,
                                        cx.listener(|t, e: &MouseUpEvent, _, _| {
                                            t.mouse_button(
                                                e.position,
                                                &e.modifiers,
                                                e.click_count,
                                                true,
                                                false,
                                            )
                                        }),
                                    )
                                    .on_mouse_move(cx.listener(|t, e: &MouseMoveEvent, _, _| {
                                        if let Some((w, x, y)) = t.route(e.position) {
                                            t.app.mouse_move(
                                                w,
                                                x,
                                                y,
                                                mods(&e.modifiers, e.pressed_button.is_some()),
                                            );
                                        }
                                    }))
                                    .on_scroll_wheel(cx.listener(
                                        |t, e: &ScrollWheelEvent, _, _| {
                                            if let Some((w, x, y)) = t.route(e.position) {
                                                let (dx, dy) = match e.delta {
                                                    ScrollDelta::Pixels(p) => (
                                                        f32::from(p.x) as i32,
                                                        f32::from(p.y) as i32,
                                                    ),
                                                    ScrollDelta::Lines(p) => {
                                                        ((p.x * 40.0) as i32, (p.y * 40.0) as i32)
                                                    }
                                                };

                                                t.app.mouse_wheel(
                                                    w,
                                                    x,
                                                    y,
                                                    mods(&e.modifiers, false),
                                                    dx,
                                                    dy,
                                                );
                                            }
                                        },
                                    ))
                                    .on_key_down(cx.listener(|t, e: &KeyDownEvent, _, _| {
                                        let k = &e.keystroke;

                                        t.app.key_down(
                                            &k.key,
                                            k.modifiers.platform,
                                            k.modifiers.shift,
                                            k.modifiers.control,
                                            mods(&k.modifiers, false),
                                            t.marked.is_none(),
                                        );
                                    }))
                                    .child(
                                        canvas(
                                            {
                                                let sh = sh.clone();
                                                let app = self.app.clone();
                                                move |bounds, window, _cx| {
                                                    sh.borrow_mut().bounds = Some(bounds);
                                                    let sz = (
                                                        f32::from(bounds.size.width),
                                                        f32::from(bounds.size.height),
                                                    );
                                                    let scale = window.scale_factor();
                                                    let changed = {
                                                        let mut s = sh.borrow_mut();
                                                        let c = s.last_size != sz;
                                                        s.last_size = sz;
                                                        c
                                                    };
                                                    if changed {
                                                        app.resize_main(sz.0, sz.1, scale);
                                                    }
                                                    bounds
                                                }
                                            },
                                            {
                                                let sh = sh.clone();
                                                let app = self.app.clone();
                                                move |_, bounds, window, cx| {
                                                    window.handle_input(
                                                        &focus,
                                                        gpui::ElementInputHandler::new(
                                                            bounds,
                                                            entity.clone(),
                                                        ),
                                                        cx,
                                                    );
                                                    if hidden {
                                                        return;
                                                    }
                                                    let t = std::time::Instant::now();
                                                    let wr = ui.widget_bounds;
                                                    let widget_b = Bounds {
                                                        origin: point(
                                                            bounds.origin.x + px(wr.0 as f32),
                                                            bounds.origin.y + px(wr.1 as f32),
                                                        ),
                                                        size: size(
                                                            px(wr.2 as f32),
                                                            px(wr.3 as f32),
                                                        ),
                                                    };
                                                    let popup_b = app
                                                        .popup_bounds(
                                                            f32::from(bounds.size.width),
                                                            f32::from(bounds.size.height),
                                                        )
                                                        .map(|popup| Bounds {
                                                            origin: point(
                                                                bounds.origin.x + px(popup.x),
                                                                bounds.origin.y + px(popup.y),
                                                            ),
                                                            size: size(
                                                                px(popup.width),
                                                                px(popup.height),
                                                            ),
                                                        });
                                                    for (i, target, maybe_bounds) in [
                                                        (0usize, SurfaceTarget::Main, Some(bounds)),
                                                        (1usize, SurfaceTarget::Popup, popup_b),
                                                        (
                                                            2usize,
                                                            SurfaceTarget::Widget,
                                                            ui.widget_visible.then_some(widget_b),
                                                        ),
                                                    ] {
                                                        let Some(b) = maybe_bounds else {
                                                            continue;
                                                        };
                                                        let known_gen = sh.borrow().img[i]
                                                            .as_ref()
                                                            .map(|x| x.0);

                                                        let Some(snapshot) =
                                                            app.surface_snapshot(target, known_gen)
                                                        else {
                                                            continue;
                                                        };

                                                        if snapshot.gen == 0 {
                                                            continue;
                                                        }

                                                        if let Some(data) = snapshot.data {
                                                            let buf = image::RgbaImage::from_raw(
                                                                snapshot.width,
                                                                snapshot.height,
                                                                data,
                                                            )
                                                            .unwrap();

                                                            let ri = Arc::new(RenderImage::new(
                                                                smallvec::smallvec![
                                                                    image::Frame::new(buf)
                                                                ],
                                                            ));

                                                            if let Some((_, old)) =
                                                                sh.borrow_mut().img[i]
                                                                    .replace((snapshot.gen, ri))
                                                            {
                                                                let _ = window.drop_image(old);
                                                            }
                                                        }

                                                        if let Some((_, ri)) =
                                                            sh.borrow().img[i].clone()
                                                        {
                                                            let radii = if i == 0 {
                                                                Corners::all(px(
                                                                    RADIUS - FRAME_PAD / 2.0
                                                                ))
                                                            } else {
                                                                Corners::all(px(8.))
                                                            };

                                                            let _ = window.paint_image(
                                                                b, radii, ri, 0, false,
                                                            );
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
                        .bg(rgb(if colorful() { 0x0000ff } else { 0xffffff }))
                        .text_color(rgb(if colorful() { 0xffffff } else { 0x333333 }))
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
            .when_some(ui.context_menu, |d, (x, y, link)| {
                let o = self
                    .sh
                    .borrow()
                    .bounds
                    .map(|b| b.origin)
                    .unwrap_or_default();
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
                                .child(if link.is_empty() {
                                    label.to_string()
                                } else {
                                    format!("{label}")
                                })
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(move |t, _, _, cx| {
                                        let command = match label {
                                            "Copy" => Some(BrowserCommand::Copy),
                                            "Paste" => Some(BrowserCommand::Paste),
                                            "Select all" => {
                                                Some(BrowserCommand::SelectAll)
                                            }
                                            "Reload" => {
                                                Some(BrowserCommand::Reload)
                                            }
                                            _ => None,
                                        };

                                        if let Some(command) = command {
                                            t.app.execute_context_command(command, label);
                                        }
                                        cx.notify();
                                    }),
                                )
                        })),
                )
            })
    }
}
