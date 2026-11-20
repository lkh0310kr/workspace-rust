//! gpui/헤드리스 공통: 키 입력을 CEF 로 정확히 "한 번" 전달하는 인코딩.
//! (실측: crates/spike-cef/src/suite.rs 의 keyexp 실험)
//!  - RAWKEYDOWN 단독(문자 없음)은 편집/이동 키가 2번 동작한다.
//!  - 편집/이동 키는 RAWKEYDOWN + macOS 기능 문자 + KEYUP 이 정확히 1번 동작한다.
//!  - 스페이스/엔터는 RAWKEYDOWN + CHAR + KEYUP 이어야 스크롤/제출이 된다.
//!  - Cmd+방향키/Cmd+Backspace 는 OSR 에서 전달되지 않아 Home/End 계열로 직접 매핑한다.

use cef::*;

pub const SHIFT: u32 = 2;
pub const CTRL: u32 = 4;
pub const ALT: u32 = 8;
pub const CMD: u32 = 128;

/// macOS 가상 키코드
pub fn native_of(vk: i32) -> i32 {
    match vk {
        8 => 51,
        9 => 48,
        13 => 36,
        27 => 53,
        32 => 49,
        33 => 116,
        34 => 121,
        35 => 119,
        36 => 115,
        37 => 123,
        38 => 126,
        39 => 124,
        40 => 125,
        46 => 117,
        _ => 0,
    }
}

/// 편집/이동 키의 macOS 기능 문자(NSEvent.characters)
fn function_char(vk: i32) -> Option<u16> {
    Some(match vk {
        8 => 0x7F,
        9 => 9,
        27 => 27,
        33 => 0xF72C,
        34 => 0xF72D,
        35 => 0xF72B,
        36 => 0xF729,
        37 => 0xF702,
        38 => 0xF700,
        39 => 0xF703,
        40 => 0xF701,
        46 => 0xF728,
        _ => return None,
    })
}

fn ev(vk: i32, mods: u32, t: KeyEventType, ch: u16) -> KeyEvent {
    KeyEvent {
        size: std::mem::size_of::<sys::cef_key_event_t>(),
        type_: t,
        modifiers: mods,
        windows_key_code: vk,
        native_key_code: native_of(vk),
        is_system_key: 0,
        character: ch,
        unmodified_character: ch,
        focus_on_editable_field: 0,
    }
}

fn send(h: &BrowserHost, vk: i32, mods: u32, ch: u16, with_char: bool) {
    h.send_key_event(Some(&ev(vk, mods, KeyEventType::RAWKEYDOWN, ch)));
    if with_char {
        h.send_key_event(Some(&ev(vk, mods, KeyEventType::CHAR, ch)));
    }
    h.send_key_event(Some(&ev(vk, mods, KeyEventType::KEYUP, ch)));
}

/// 특수/이동/편집 키 1회 입력. 일반 문자는 `ime_commit_text` 경로를 쓴다.
pub fn send_key(h: &BrowserHost, vk: i32, mods: u32) {
    // Cmd+방향키 / Cmd+Backspace 매핑(macOS 줄/문서 단위 이동)
    if mods & CMD != 0 {
        let rest = mods & !CMD;
        match vk {
            37 => return send_key(h, 36, rest),           // Cmd+← → Home
            39 => return send_key(h, 35, rest),           // Cmd+→ → End
            38 => return send_key(h, 36, rest | CTRL),    // Cmd+↑ → Ctrl+Home (문서 처음)
            40 => return send_key(h, 35, rest | CTRL),    // Cmd+↓ → Ctrl+End
            8 => {
                // Cmd+Backspace → 줄 처음까지 선택 후 삭제
                send_key(h, 36, rest | SHIFT);
                return send_key(h, 8, rest);
            }
            _ => {}
        }
    }
    match vk {
        32 => send(h, vk, mods, 32, true),
        13 => send(h, vk, mods, 13, true),
        _ => match function_char(vk) {
            Some(c) => send(h, vk, mods, c, false),
            None => send(h, vk, mods, 0, false),
        },
    }
}

/// 일반 문자 키의 keydown/keyup 만(텍스트 삽입은 IME commit 경로) — gpui 앱에서 JS keydown 을 보장
pub fn send_text_key_events(h: &BrowserHost, vk: i32, mods: u32, ch: u16) {
    h.send_key_event(Some(&ev(vk, mods, KeyEventType::RAWKEYDOWN, ch)));
    h.send_key_event(Some(&ev(vk, mods, KeyEventType::KEYUP, ch)));
}
