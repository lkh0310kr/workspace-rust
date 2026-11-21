//! 웹 ↔ Rust 통신 방식 비교용 구현(브라우저 프로세스 쪽).
//!  - 커스텀 스킴: `workspace://rpc/blob?n=<bytes>` → Rust ResourceHandler 가 n 바이트 응답
//!  - 프로세스 메시지: 렌더러 V8 함수 → `ws_invoke` 메시지 → Rust → `ws_reply` 메시지(렌더러가 JS 콜백 호출)
//! 측정 목적의 echo 서버 수준이며 인증/오리진 검사는 하지 않는다(실제 이식 시 필수, docs/ipc-comparison.md 참고).

use cef::{rc::Rc, *};
use std::sync::{Arc, Mutex};

pub const SCHEME: &str = "workspace";
pub const HOST: &str = "rpc";

pub fn payload(n: usize) -> Vec<u8> {
    vec![b'a'; n]
}

fn parse_n(url: &str) -> usize {
    url.split("n=")
        .nth(1)
        .and_then(|s| s.split('&').next())
        .and_then(|s| s.parse().ok())
        .unwrap_or(0)
}

wrap_resource_handler! {
    pub struct BlobHandler {
        data: Arc<Mutex<(Vec<u8>, usize)>>,
    }
    impl ResourceHandler {
        fn open(&self, request: Option<&mut Request>, handle_request: Option<&mut ::std::os::raw::c_int>, _cb: Option<&mut Callback>) -> ::std::os::raw::c_int {
            let url = request.map(|r| CefString::from(&r.url()).to_string()).unwrap_or_default();
            let n = parse_n(&url);
            *self.data.lock().unwrap() = (payload(n), 0);
            if let Some(h) = handle_request {
                *h = 1; // 즉시 처리
            }
            1
        }

        fn response_headers(&self, response: Option<&mut Response>, response_length: Option<&mut i64>, _redirect: Option<&mut CefString>) {
            let len = self.data.lock().unwrap().0.len() as i64;
            if let Some(r) = response {
                r.set_status(200);
                r.set_mime_type(Some(&CefString::from("text/plain")));
                r.set_header_by_name(Some(&CefString::from("Access-Control-Allow-Origin")), Some(&CefString::from("*")), 1);
            }
            if let Some(l) = response_length {
                *l = len;
            }
        }

        fn read(&self, data_out: *mut u8, bytes_to_read: ::std::os::raw::c_int, bytes_read: Option<&mut ::std::os::raw::c_int>, _cb: Option<&mut ResourceReadCallback>) -> ::std::os::raw::c_int {
            let mut g = self.data.lock().unwrap();
            let (buf, pos) = &mut *g;
            let remain = buf.len() - *pos;
            let n = remain.min(bytes_to_read.max(0) as usize);
            if n > 0 {
                unsafe { std::ptr::copy_nonoverlapping(buf.as_ptr().add(*pos), data_out, n) };
                *pos += n;
            }
            if let Some(b) = bytes_read {
                *b = n as i32;
            }
            (n > 0) as i32 // 0 = 완료
        }
    }
}

wrap_scheme_handler_factory! {
    pub struct SchemeFactory {}
    impl SchemeHandlerFactory {
        fn create(&self, _b: Option<&mut Browser>, _f: Option<&mut Frame>, _scheme: Option<&CefString>, _req: Option<&mut Request>) -> Option<ResourceHandler> {
            Some(BlobHandler::new(Arc::new(Mutex::new((vec![], 0)))))
        }
    }
}

pub fn register_factory() -> i32 {
    let mut f = SchemeFactory::new();
    register_scheme_handler_factory(
        Some(&CefString::from(SCHEME)),
        Some(&CefString::from(HOST)),
        Some(&mut f),
    )
}

/// 렌더러가 보낸 `ws_invoke`("<id>|<n>") 에 `ws_reply`(id, payload) 로 응답
pub fn handle_process_message(frame: &Frame, msg: &ProcessMessage) -> bool {
    let name = CefString::from(&msg.name()).to_string();
    if name != "ws_invoke" {
        return false;
    }
    let Some(args) = msg.argument_list() else {
        return true;
    };
    let a0 = CefString::from(&args.string(0)).to_string();
    let (id, n) = a0
        .split_once('|')
        .map(|(i, n)| (i.to_string(), n.parse::<usize>().unwrap_or(0)))
        .unwrap_or_default();
    if let Some(mut reply) = process_message_create(Some(&CefString::from("ws_reply"))) {
        if let Some(rargs) = reply.argument_list() {
            rargs.set_string(0, Some(&CefString::from(id.as_str())));
            rargs.set_string(
                1,
                Some(&CefString::from(
                    String::from_utf8(payload(n)).unwrap().as_str(),
                )),
            );
        }
        frame.send_process_message(ProcessId::RENDERER, Some(&mut reply));
    }
    true
}
