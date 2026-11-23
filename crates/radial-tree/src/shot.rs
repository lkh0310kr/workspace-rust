//! 창 캡처(개발용): 자기 프로세스의 창을 CGWindowListCreateImage 로 찍어 PNG 로 저장 (화면 녹화 권한 불필요).
//! 타이틀바를 잘라 콘텐츠 영역만 저장한다.

#[cfg(target_os = "macos")]
use core_graphics::geometry::{CGPoint, CGRect, CGSize};
#[cfg(target_os = "macos")]
use core_graphics::window::{
    create_image, kCGWindowImageBestResolution, kCGWindowImageBoundsIgnoreFraming,
    kCGWindowListOptionIncludingWindow,
};
#[cfg(target_os = "macos")]
use objc2::rc::Retained;
#[cfg(target_os = "macos")]
use objc2_app_kit::NSView;
#[cfg(target_os = "macos")]
use raw_window_handle::{HasWindowHandle, RawWindowHandle};

#[cfg(target_os = "macos")]
pub fn capture(window: &gpui::Window, path: &str) -> Result<(u32, u32), String> {
    let h = HasWindowHandle::window_handle(window)
        .map_err(|e| format!("{e:?}"))?
        .as_raw();
    let RawWindowHandle::AppKit(a) = h else {
        return Err("not appkit".into());
    };
    let view: Retained<NSView> =
        unsafe { Retained::retain(a.ns_view.as_ptr() as *mut NSView).unwrap() };
    let w = view.window().ok_or("no window")?;
    let frame = w.frame();
    let content = w.contentRectForFrameRect(frame);
    let titlebar = frame.size.height - content.size.height;
    let scale = w.backingScaleFactor();
    let null = CGRect::new(
        &CGPoint::new(f64::INFINITY, f64::INFINITY),
        &CGSize::new(0.0, 0.0),
    );
    let cg = create_image(
        null,
        kCGWindowListOptionIncludingWindow,
        w.windowNumber() as u32,
        kCGWindowImageBoundsIgnoreFraming | kCGWindowImageBestResolution,
    )
    .ok_or("capture failed")?;
    let (wd, ht, bpr) = (cg.width() as u32, cg.height() as u32, cg.bytes_per_row());
    let data = cg.data();
    let bytes = data.bytes();
    let top = (titlebar * scale).round() as u32;
    let (cw, ch) = (wd, ht.saturating_sub(top));
    let mut out = image::RgbaImage::new(cw, ch);
    for y in 0..ch as usize {
        for x in 0..cw as usize {
            let i = (y + top as usize) * bpr + x * 4;
            out.put_pixel(
                x as u32,
                y as u32,
                image::Rgba([bytes[i + 2], bytes[i + 1], bytes[i], 255]),
            );
        }
    }
    out.save(path).map_err(|e| e.to_string())?;
    Ok((cw, ch))
}

#[cfg(not(target_os = "macos"))]
pub fn capture(_window: &gpui::Window, _path: &str) -> Result<(u32, u32), String> {
    Err("window capture is only available on macOS".into())
}
