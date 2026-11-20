//! gpui 의 NSApplication 서브클래스(`GPUIApplication`)가 CEF 가 요구하는 `CefAppProtocol` 을 만족하도록
//! 런타임에 메서드/프로토콜을 주입한다. (CEF 는 nested run loop 중 sendEvent 상태를 `isHandlingSendEvent` 로 질의)

use std::ffi::{c_char, c_void, CStr};
use std::sync::atomic::{AtomicBool, Ordering};

static HANDLING: AtomicBool = AtomicBool::new(false);

extern "C" {
    fn objc_getClass(name: *const c_char) -> *mut c_void;
    fn objc_getProtocol(name: *const c_char) -> *mut c_void;
    fn objc_allocateProtocol(name: *const c_char) -> *mut c_void;
    fn objc_registerProtocol(p: *mut c_void);
    fn protocol_addMethodDescription(p: *mut c_void, sel: *mut c_void, types: *const c_char, required: bool, instance: bool);
    fn protocol_addProtocol(p: *mut c_void, other: *mut c_void);
    fn sel_registerName(name: *const c_char) -> *mut c_void;
    fn class_addMethod(cls: *mut c_void, sel: *mut c_void, imp: *const c_void, types: *const c_char) -> bool;
    fn class_addProtocol(cls: *mut c_void, p: *mut c_void) -> bool;
}

extern "C" fn is_handling(_this: *mut c_void, _sel: *mut c_void) -> bool {
    HANDLING.load(Ordering::Relaxed)
}
extern "C" fn set_handling(_this: *mut c_void, _sel: *mut c_void, v: bool) {
    HANDLING.store(v, Ordering::Relaxed)
}

fn c(s: &'static [u8]) -> *const c_char {
    CStr::from_bytes_with_nul(s).unwrap().as_ptr()
}

/// 반환: 주입 결과 설명
pub unsafe fn patch_application(class_name: &'static [u8]) -> String {
    let cls = objc_getClass(c(class_name));
    if cls.is_null() {
        return "application class not found".into();
    }
    let s1 = sel_registerName(c(b"isHandlingSendEvent\0"));
    let s2 = sel_registerName(c(b"setHandlingSendEvent:\0"));
    let m1 = class_addMethod(cls, s1, is_handling as *const c_void, c(b"B@:\0"));
    let m2 = class_addMethod(cls, s2, set_handling as *const c_void, c(b"v@:B\0"));

    // 프로토콜: libcef 안에 이미 등록돼 있으면 그것을, 없으면 같은 이름으로 생성
    let get_or_make = |name: &'static [u8], methods: &[(*mut c_void, &'static [u8])], parent: Option<*mut c_void>| -> *mut c_void {
        let ex = objc_getProtocol(c(name));
        if !ex.is_null() {
            return ex;
        }
        let p = objc_allocateProtocol(c(name));
        for (sel, enc) in methods {
            protocol_addMethodDescription(p, *sel, c(enc), true, true);
        }
        if let Some(par) = parent {
            protocol_addProtocol(p, par);
        }
        objc_registerProtocol(p);
        p
    };
    let cr = get_or_make(b"CrAppProtocol\0", &[(s1, b"B@:\0")], None);
    let ctl = get_or_make(b"CrAppControlProtocol\0", &[(s2, b"v@:B\0")], Some(cr));
    let cef = get_or_make(b"CefAppProtocol\0", &[], Some(ctl));
    let a = class_addProtocol(cls, cr);
    let b = class_addProtocol(cls, ctl);
    let d = class_addProtocol(cls, cef);
    format!("methods={m1}/{m2} protocols={a}/{b}/{d}")
}
