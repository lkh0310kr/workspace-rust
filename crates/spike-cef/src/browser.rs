use cef::{
    sys, BrowserHost, CefString, CompositionUnderline, CompositionUnderlineStyle, ImplBrowser,
    ImplBrowserHost, ImplFrame, MouseButtonType, MouseEvent, PaintElementType, Range,
};

use crate::{
    cefhost::{with, Which},
    keys, vkey,
};

pub struct SurfaceSnapshot {
    pub gen: u64,
    pub width: u32,
    pub height: u32,
    pub data: Option<Vec<u8>>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum BrowserTarget {
    Main,
    Popup,
}

impl BrowserTarget {
    fn which(self) -> Which {
        match self {
            Self::Main => Which::Main,
            Self::Popup => Which::Popup,
        }
    }
}

pub fn navigate(url: &str) {
    let browser = with(|s| s.main.clone());

    if let Some(browser) = browser {
        if let Some(frame) = browser.main_frame() {
            frame.load_url(Some(&CefString::from(url)));
        }
    }
}

fn host(which: Which) -> Option<BrowserHost> {
    with(|s| match which {
        Which::Main => s.main.clone(),
        Which::Popup => s.popup.clone(),
        Which::Tab => None,
    })
    .and_then(|browser| browser.host())
}

pub fn mouse_button(
    target: BrowserTarget,
    x: i32,
    y: i32,
    modifiers: u32,
    mouse_up: bool,
    right: bool,
    click_count: usize,
) {
    let Some(host) = host(target.which()) else {
        return;
    };

    if right {
        with(|s| s.right_clicks += 1);
    }

    let event = MouseEvent { x, y, modifiers };

    if !mouse_up {
        host.set_focus(1);
    }

    host.send_mouse_click_event(
        Some(&event),
        if right {
            MouseButtonType::RIGHT
        } else {
            MouseButtonType::LEFT
        },
        mouse_up as _,
        click_count.max(1) as _,
    );
}

pub fn mouse_move(target: BrowserTarget, x: i32, y: i32, modifiers: u32) {
    let Some(host) = host(target.which()) else {
        return;
    };

    with(|s| {
        s.moves += 1;
        s.last_move = (x, y, modifiers);
    });

    host.send_mouse_move_event(Some(&MouseEvent { x, y, modifiers }), 0);
}

pub fn mouse_wheel(
    target: BrowserTarget,
    x: i32,
    y: i32,
    modifiers: u32,
    delta_x: i32,
    delta_y: i32,
) {
    if let Some(host) = host(target.which()) {
        host.send_mouse_wheel_event(Some(&MouseEvent { x, y, modifiers }), delta_x, delta_y);
    }
}

pub fn key_down(
    key: &str,
    platform: bool,
    shift: bool,
    control: bool,
    modifiers: u32,
    allow_text_input: bool,
) {
    let target = if with(|s| s.popup.is_some()) {
        Which::Popup
    } else {
        Which::Main
    };

    let Some(h) = host(target) else {
        return;
    };

    let browser = with(|s| {
        if target == Which::Popup {
            s.popup.clone()
        } else {
            s.main.clone()
        }
    });

    if platform {
        if let Some(frame) = browser.and_then(|b| b.focused_frame().or_else(|| b.main_frame())) {
            match key {
                "a" => frame.select_all(),
                "c" => frame.copy(),
                "x" => frame.cut(),
                "v" => frame.paste(),
                "z" if shift => frame.redo(),
                "z" => frame.undo(),
                _ => {
                    if let Some(vk) = vkey(key) {
                        keys::send_key(&h, vk, modifiers);
                    }
                }
            }
        }

        return;
    }

    let special = matches!(
        key,
        "backspace"
            | "tab"
            | "enter"
            | "escape"
            | "space"
            | "pageup"
            | "pagedown"
            | "end"
            | "home"
            | "left"
            | "up"
            | "right"
            | "down"
            | "delete"
    );

    if special {
        if let Some(vk) = vkey(key) {
            keys::send_key(&h, vk, modifiers);
        }
    } else if allow_text_input && key.chars().count() == 1 && !control {
        let c = key.chars().next().unwrap();

        let vk = if c.is_ascii_alphabetic() {
            Some(c.to_ascii_uppercase() as i32)
        } else if c.is_ascii_digit() {
            Some(c as i32)
        } else {
            None
        };

        if let Some(vk) = vk {
            keys::send_text_key_events(&h, vk, modifiers, c as u16);
        }
    }
}

fn input_target() -> Which {
    if with(|s| s.popup.is_some()) {
        Which::Popup
    } else {
        Which::Main
    }
}

pub fn ime_finish() {
    if let Some(host) = host(Which::Main) {
        host.ime_finish_composing_text(0);
    }
}

pub fn ime_commit(text: &str) {
    let target = input_target();

    if let Some(host) = host(target) {
        let invalid = Range {
            from: u32::MAX,
            to: u32::MAX,
        };

        host.ime_commit_text(Some(&CefString::from(text)), Some(&invalid), 0);
    }
}

pub fn ime_set_composition(text: &str, selection: Option<(usize, usize)>) {
    let target = input_target();

    if let Some(host) = host(target) {
        let n = text.encode_utf16().count();

        let underline = CompositionUnderline {
            size: std::mem::size_of::<sys::cef_composition_underline_t>(),
            range: Range {
                from: 0,
                to: n as u32,
            },
            color: 0,
            background_color: 0,
            thick: 0,
            style: CompositionUnderlineStyle::default(),
        };

        let invalid = Range {
            from: u32::MAX,
            to: u32::MAX,
        };

        let selection = selection
            .map(|(start, end)| Range {
                from: start as u32,
                to: end as u32,
            })
            .unwrap_or(Range {
                from: n as u32,
                to: n as u32,
            });

        host.ime_set_composition(
            Some(&CefString::from(text)),
            Some(&[underline]),
            Some(&invalid),
            Some(&selection),
        );
    }
}

pub enum BrowserCommand {
    Copy,
    Paste,
    SelectAll,
    Reload,
}

pub fn execute(command: BrowserCommand) {
    let browser = with(|s| s.main.clone());

    let Some(browser) = browser else {
        return;
    };

    match command {
        BrowserCommand::Reload => browser.reload(),

        BrowserCommand::Copy => {
            if let Some(frame) = browser.focused_frame().or_else(|| browser.main_frame()) {
                frame.copy();
            }
        }

        BrowserCommand::Paste => {
            if let Some(frame) = browser.focused_frame().or_else(|| browser.main_frame()) {
                frame.paste();
            }
        }

        BrowserCommand::SelectAll => {
            if let Some(frame) = browser.focused_frame().or_else(|| browser.main_frame()) {
                frame.select_all();
            }
        }
    }
}

pub fn execute_context_command(command: BrowserCommand, label: &str) {
    execute(command);

    with(|s| {
        s.ctx = None;
        s.ui_gen += 1;
        s.log.push(format!("ctx_item {label}"));
    });
}

pub fn resize_main(width: f32, height: f32, scale: f32) {
    with(|s| {
        s.size[0] = (width, height);
        s.scale = scale;
    });

    if let Some(host) = host(Which::Main) {
        host.was_resized();
        host.invalidate(PaintElementType::default());
    }
}

pub fn has_popup() -> bool {
    with(|s| s.popup.is_some())
}

pub fn cursor() -> u8 {
    with(|s| s.cursor)
}

pub fn widget_bounds() -> (i32, i32, i32, i32) {
    with(|s| s.widget)
}

pub fn widget_visible() -> bool {
    with(|s| s.widget_visible)
}

pub fn popup_size() -> (f32, f32) {
    with(|s| s.size[1])
}

pub fn context_menu() -> Option<(i32, i32, String)> {
    with(|s| s.ctx.clone())
}

pub fn surface_snapshot(index: usize, known_gen: Option<u64>) -> Option<SurfaceSnapshot> {
    with(|s| {
        let surface = s.surf.get(index)?;

        if surface.gen == 0 {
            return Some(SurfaceSnapshot {
                gen: 0,
                width: 0,
                height: 0,
                data: None,
            });
        }

        let data = if known_gen == Some(surface.gen) {
            None
        } else {
            Some(surface.data.clone())
        };

        Some(SurfaceSnapshot {
            gen: surface.gen,
            width: surface.w,
            height: surface.h,
            data,
        })
    })
}

pub fn record_foreign_ime() {
    with(|s| s.foreign_ime += 1);
}

pub fn dismiss_context_menu() {
    with(|s| {
        if s.ctx.take().is_some() {
            s.ui_gen += 1;
        }
    });
}
