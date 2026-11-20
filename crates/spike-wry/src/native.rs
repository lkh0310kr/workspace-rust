//! macOS 네이티브 보조: NSWindow 접근, 합성 마우스 이벤트, 창 기하, CALayer 라운딩.

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::msg_send;
use objc2_app_kit::{NSEvent, NSEventModifierFlags, NSEventType, NSView, NSWindow};
use objc2_foundation::{NSPoint, NSRect};
use raw_window_handle::RawWindowHandle;
use wry::WebViewExtMacOS;

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

/// 네이티브 WKWebView 레이어에 corner radius 적용 (gpui 는 네이티브 뷰를 clip 할 수 없다)
pub fn set_radius(wv: &wry::WebView, r: f64) {
    let v = wv.webview();
    unsafe {
        let _: () = msg_send![&*v, setWantsLayer: true];
        let layer: Option<Retained<AnyObject>> = msg_send![&*v, layer];
        if let Some(l) = layer {
            let _: () = msg_send![&*l, setCornerRadius: r];
            let _: () = msg_send![&*l, setMasksToBounds: true];
        }
    }
}

/// 창을 앞으로 + key 로 만든다 (사용자가 다른 앱을 만져도 합성 클릭이 첫 클릭으로 삼켜지지 않게)
pub fn activate(w: &NSWindow) {
    let mtm = objc2_foundation::MainThreadMarker::new().unwrap();
    objc2_app_kit::NSApplication::sharedApplication(mtm).activate();
    w.makeKeyAndOrderFront(None);
}
