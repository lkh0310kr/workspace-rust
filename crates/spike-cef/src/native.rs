//! macOS 네이티브 보조: NSWindow 접근, 합성 마우스 이벤트, 창 기하, CALayer 라운딩.

use objc2::rc::Retained;
use objc2_app_kit::{NSEvent, NSEventModifierFlags, NSEventType, NSView, NSWindow};
use objc2_foundation::{NSPoint, NSRect};
use raw_window_handle::RawWindowHandle;

pub fn ns_window(h: RawWindowHandle) -> Retained<NSWindow> {
    let RawWindowHandle::AppKit(a) = h else { panic!("not appkit") };
    let view: Retained<NSView> = unsafe { Retained::retain(a.ns_view.as_ptr() as *mut NSView).unwrap() };
    view.window().expect("no window")
}

pub struct Geom {
    pub window_number: isize,
    /// 화면 좌상단 기준 윈도우 프레임 (points)
    pub frame_x: f64,
    pub frame_y: f64,
    pub frame_w: f64,
    pub frame_h: f64,
    /// 프레임 상단에서 콘텐츠 영역 상단까지(타이틀바 높이, points)
    pub titlebar: f64,
    pub content_h: f64,
    pub scale: f64,
}

pub fn geom(w: &NSWindow) -> Geom {
    let frame: NSRect = w.frame();
    let content: NSRect = w.contentRectForFrameRect(frame);
    let mtm = objc2_foundation::MainThreadMarker::new().unwrap();
    let screens = objc2_app_kit::NSScreen::screens(mtm);
    let primary_h = screens.objectAtIndex(0).frame().size.height;
    Geom {
        window_number: w.windowNumber(),
        frame_x: frame.origin.x,
        frame_y: primary_h - (frame.origin.y + frame.size.height),
        frame_w: frame.size.width,
        frame_h: frame.size.height,
        titlebar: frame.size.height - content.size.height,
        content_h: content.size.height,
        scale: w.backingScaleFactor(),
    }
}

/// 콘텐츠 영역 좌상단 기준 (x, y_top) 좌표(points)에 좌클릭(down+up)을 합성해 `NSWindow.sendEvent`로 주입.
/// 실제 hit-test 경로(contentView → 가장 위 subview)를 그대로 탄다.
pub fn click(w: &NSWindow, x: f64, y_top: f64) {
    let g = geom(w);
    let loc = NSPoint::new(x, g.content_h - y_top);
    for (ty, pressure) in [(NSEventType::LeftMouseDown, 1.0f32), (NSEventType::LeftMouseUp, 0.0)] {
        let ev = NSEvent::mouseEventWithType_location_modifierFlags_timestamp_windowNumber_context_eventNumber_clickCount_pressure(
            ty,
            loc,
            NSEventModifierFlags::empty(),
            0.0,
            g.window_number,
            None,
            0,
            1,
            pressure,
        )
        .expect("event");
        w.sendEvent(&ev);
    }
}

/// 창을 앞으로 + key 로 만든다 (사용자가 다른 앱을 만져도 합성 클릭이 첫 클릭으로 삼켜지지 않게)
pub fn background() -> bool {
    std::env::var("SPIKE_BG").is_ok()
}

pub fn activate(w: &NSWindow) {
    if background() {
        return; // 사용자 작업 방해 금지: 포커스/전면화 안 함
    }
    let mtm = objc2_foundation::MainThreadMarker::new().unwrap();
    objc2_app_kit::NSApplication::sharedApplication(mtm).activate();
    w.makeKeyAndOrderFront(None);
    w.orderFrontRegardless();
}

extern "C" {
    fn CGEventCreateScrollWheelEvent2(source: *const std::ffi::c_void, units: u32, count: u32, w1: i32, w2: i32, w3: i32) -> *mut std::ffi::c_void;
    fn CGEventSetLocation(e: *mut std::ffi::c_void, p: objc2_foundation::NSPoint);
    fn CFRelease(p: *const std::ffi::c_void);
}

/// 합성 스크롤휠(픽셀 단위)을 `NSWindow.sendEvent` 로 주입. dy>0 = 위로 스크롤(콘텐츠가 아래로).
/// window 가 없는 CGEvent 기반 NSEvent 의 locationInWindow 는 화면 좌표(좌하단 원점)로 들어오므로
/// gpui 가 창 좌표로 해석하게끔 원하는 창 내부 좌표를 그대로 넣는다.
pub fn scroll(w: &NSWindow, x: f64, y_top: f64, dy: i32) -> bool {
    activate(w);
    let g = geom(w);
    let mtm = objc2_foundation::MainThreadMarker::new().unwrap();
    let primary_h = objc2_app_kit::NSScreen::screens(mtm).objectAtIndex(0).frame().size.height;
    let want_y_ns = g.content_h - y_top; // 창 좌표(좌하단 원점)
    unsafe {
        let cg = CGEventCreateScrollWheelEvent2(std::ptr::null(), 0, 1, dy, 0, 0);
        if cg.is_null() { return false; }
        CGEventSetLocation(cg, NSPoint::new(x, primary_h - want_y_ns));
        let ev: Option<Retained<NSEvent>> = objc2::msg_send![objc2::class!(NSEvent), eventWithCGEvent: cg];
        CFRelease(cg);
        let Some(ev) = ev else { return false };
        w.sendEvent(&ev);
    }
    true
}

/// 합성 키 입력(down+up). `flags`: cmd=1<<20, shift=1<<17, ctrl=1<<18, alt=1<<19
pub fn key(w: &NSWindow, chars: &str, keycode: u16, flags: usize) {
    activate(w);
    let g = geom(w);
    let ns = objc2_foundation::NSString::from_str(chars);
    for ty in [NSEventType::KeyDown, NSEventType::KeyUp] {
        let ev = NSEvent::keyEventWithType_location_modifierFlags_timestamp_windowNumber_context_characters_charactersIgnoringModifiers_isARepeat_keyCode(
            ty,
            NSPoint::new(0.0, 0.0),
            NSEventModifierFlags::from_bits_retain(flags),
            0.0,
            g.window_number,
            None,
            &ns,
            &ns,
            false,
            keycode,
        )
        .expect("key event");
        w.sendEvent(&ev);
    }
}

/// Dock 아이콘/포커스 이동 없이 동작하도록 Accessory 정책으로 전환
pub fn set_accessory() {
    let mtm = objc2_foundation::MainThreadMarker::new().unwrap();
    objc2_app_kit::NSApplication::sharedApplication(mtm).setActivationPolicy(objc2_app_kit::NSApplicationActivationPolicy::Accessory);
}

pub fn stealth_alpha() -> f64 {
    std::env::var("SPIKE_ALPHA").ok().and_then(|v| v.parse().ok()).unwrap_or(0.03)
}

extern "C" {
    fn object_getClass(o: *const std::ffi::c_void) -> *mut std::ffi::c_void;
    fn sel_registerName(n: *const std::ffi::c_char) -> *mut std::ffi::c_void;
    fn class_replaceMethod(cls: *mut std::ffi::c_void, sel: *mut std::ffi::c_void, imp: *const std::ffi::c_void, types: *const std::ffi::c_char) -> *mut std::ffi::c_void;
}
extern "C" fn yes(_this: *mut std::ffi::c_void, _sel: *mut std::ffi::c_void) -> bool {
    true
}

/// 백그라운드 검증용(하니스 전용): 거의 투명 + 마우스 투과 + 모든 Space 에 상주. 시스템 포커스는 가져오지 않는다.
/// - 완전히 가려지거나 화면 밖이면 macOS 가 디스플레이 링크를 멈춰 gpui 가 그리지 않으므로 "보이는 상태"는 유지.
/// - gpui 는 `isKeyWindow` 로 활성 여부를 판단하므로, 이 프로세스 안에서만 YES 를 돌려주게 하고
///   delegate 에 windowDidBecomeKey 를 직접 전달해 gpui 가 활성 창으로 인식하게 한다.
pub fn stealth(w: &NSWindow) {
    println!("stealth alpha={} occlusion(before)={:?}", stealth_alpha(), w.occlusionState());
    w.setAlphaValue(stealth_alpha());
    w.setIgnoresMouseEvents(true);
    w.setHasShadow(false);
    unsafe {
        let _: () = objc2::msg_send![w, setCollectionBehavior: (1usize << 0) | (1 << 4) | (1 << 6)];
        let cls = object_getClass(w as *const NSWindow as *const std::ffi::c_void);
        if std::env::var("SPIKE_KEYHACK").is_ok() { for sel in [b"isKeyWindow\0".as_slice(), b"isMainWindow\0".as_slice()] {
            class_replaceMethod(cls, sel_registerName(sel.as_ptr() as _), yes as *const std::ffi::c_void, b"B@:\0".as_ptr() as _);
        } }
        let delegate: Option<Retained<objc2::runtime::AnyObject>> = objc2::msg_send![w, delegate];
        if let (Some(d), true) = (delegate, std::env::var("SPIKE_KEYHACK").is_ok()) {
            let name = objc2_foundation::NSString::from_str("NSWindowDidBecomeKeyNotification");
            let n: Retained<objc2::runtime::AnyObject> = objc2::msg_send![objc2::class!(NSNotification), notificationWithName: &*name, object: w];
            let _: () = objc2::msg_send![&*d, windowDidBecomeKey: &*n];
        }
    }
    w.orderFrontRegardless();
}

/// App Nap / 타이머 억제 방지 (포커스 없는 백그라운드 실행에서 gpui 프레임이 멈추는 것을 막기 위함)
pub fn disable_app_nap() -> Retained<objc2::runtime::AnyObject> {
    unsafe {
        let pi: Retained<objc2::runtime::AnyObject> = objc2::msg_send![objc2::class!(NSProcessInfo), processInfo];
        let reason = objc2_foundation::NSString::from_str("spike-cef harness");
        // NSActivityUserInitiated(0x00FFFFFF) | NSActivityLatencyCritical(0xFF00000000)
        let tok: Retained<objc2::runtime::AnyObject> = objc2::msg_send![&*pi, beginActivityWithOptions: 0xFF_00FF_FFFFu64, reason: &*reason];
        tok
    }
}

fn mouse_ev(w: &NSWindow, ty: NSEventType, x: f64, y_top: f64, click: isize, pressure: f32) {
    let g = geom(w);
    let ev = NSEvent::mouseEventWithType_location_modifierFlags_timestamp_windowNumber_context_eventNumber_clickCount_pressure(
        ty, NSPoint::new(x, g.content_h - y_top), NSEventModifierFlags::empty(), 0.0, g.window_number, None, 0, click, pressure,
    )
    .expect("mouse event");
    w.sendEvent(&ev);
}

/// 마우스 이동 (hover / 커서 모양 검증용)
pub fn mouse_move(w: &NSWindow, x: f64, y_top: f64) {
    activate(w);
    mouse_ev(w, NSEventType::MouseMoved, x, y_top, 0, 0.0);
}

pub fn right_click(w: &NSWindow, x: f64, y_top: f64) {
    activate(w);
    mouse_ev(w, NSEventType::RightMouseDown, x, y_top, 1, 1.0);
    mouse_ev(w, NSEventType::RightMouseUp, x, y_top, 1, 0.0);
}

/// 좌클릭 드래그 (down → 여러 dragged → up)
pub fn drag(w: &NSWindow, x1: f64, y1: f64, x2: f64, y2: f64) {
    activate(w);
    mouse_ev(w, NSEventType::LeftMouseDown, x1, y1, 1, 1.0);
    for i in 1..=8 {
        let t = i as f64 / 8.0;
        mouse_ev(w, NSEventType::LeftMouseDragged, x1 + (x2 - x1) * t, y1 + (y2 - y1) * t, 1, 1.0);
    }
    mouse_ev(w, NSEventType::LeftMouseUp, x2, y2, 1, 0.0);
}
