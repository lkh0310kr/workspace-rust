//! CEF 서브프로세스(GPU/Renderer/Plugin/Alerts) 진입점. 번들 안의 Helper .app 으로 복사된다.
//! 렌더러 프로세스에서는 페이지 스크립트 실행 전에 WebAuthn API 를 제거할 수 있다(SPIKE_DISABLE_WEBAUTHN=1).
//! 임베디드 OSR 환경에서는 passkey 인증 UI/인증기가 없어 브라우저 프로세스가 크래시하는 문제의 회피책.
use cef::{args::Args, rc::Rc, *};

/// 신뢰하는 오리진에만 window.workspace 를 주입한다(Electron 의 preload/contextBridge 에 해당).
const TRUSTED: &str = "http://127.0.0.1:18080";

wrap_v8_handler! {
    struct WsSend {}
    impl V8Handler {
        fn execute(&self, _name: Option<&CefString>, _obj: Option<&mut V8Value>, arguments: Option<&[Option<V8Value>]>, _ret: Option<&mut Option<V8Value>>, _exc: Option<&mut CefString>) -> ::std::os::raw::c_int {
            let Some(args) = arguments else { return 0 };
            let Some(Some(a0)) = args.first() else { return 0 };
            let s = CefString::from(&a0.string_value()).to_string();
            if let Some(ctx) = v8_context_get_current_context() {
                if let Some(frame) = ctx.frame() {
                    if let Some(mut m) = process_message_create(Some(&CefString::from("ws_invoke"))) {
                        if let Some(l) = m.argument_list() {
                            l.set_string(0, Some(&CefString::from(s.as_str())));
                        }
                        frame.send_process_message(ProcessId::BROWSER, Some(&mut m));
                    }
                }
            }
            1
        }
    }
}

wrap_render_process_handler! {
    struct RPH {}
    impl RenderProcessHandler {
        fn on_context_created(&self, _b: Option<&mut Browser>, frame: Option<&mut Frame>, context: Option<&mut V8Context>) {
            let Some(ctx) = context else { return };
            if std::env::var("SPIKE_DISABLE_WEBAUTHN").is_ok() {
                let code = CefString::from("try{delete window.PublicKeyCredential;Object.defineProperty(navigator,'credentials',{get(){return undefined},configurable:true})}catch(e){}");
                let (mut ret, mut exc) = (None, None);
                ctx.eval(Some(&code), Some(&CefString::from("spike://disable-webauthn")), 0, Some(&mut ret), Some(&mut exc));
            }
            // 오리진 게이팅: 신뢰 오리진에만 브리지 노출
            let url = frame.map(|f| CefString::from(&f.url()).to_string()).unwrap_or_default();
            if !url.starts_with(TRUSTED) {
                return;
            }
            if let Some(g) = ctx.global() {
                let mut handler = WsSend::new();
                if let Some(mut f) = v8_value_create_function(Some(&CefString::from("__ws_send")), Some(&mut handler)) {
                    g.set_value_bykey(Some(&CefString::from("__ws_send")), Some(&mut f), V8Propertyattribute::default());
                }
            }
            let code = CefString::from("(()=>{let id=0;const cb={};window.__ws_reply=(i,p)=>{const f=cb[i];delete cb[i];f(p)};window.workspace={invoke:(n)=>new Promise(res=>{const i=String(++id);cb[i]=res;__ws_send(i+'|'+n)})}})()");
            let (mut ret, mut exc) = (None, None);
            ctx.eval(Some(&code), Some(&CefString::from("spike://workspace-bridge")), 0, Some(&mut ret), Some(&mut exc));
        }

        fn on_process_message_received(&self, _b: Option<&mut Browser>, frame: Option<&mut Frame>, _src: ProcessId, message: Option<&mut ProcessMessage>) -> ::std::os::raw::c_int {
            let (Some(frame), Some(msg)) = (frame, message) else { return 0 };
            if CefString::from(&msg.name()).to_string() != "ws_reply" {
                return 0;
            }
            let Some(args) = msg.argument_list() else { return 1 };
            let id = CefString::from(&args.string(0)).to_string();
            let payload = CefString::from(&args.string(1)).to_string();
            if let Some(ctx) = frame.v8_context() {
                ctx.enter();
                if let Some(g) = ctx.global() {
                    if let Some(mut p) = v8_value_create_string(Some(&CefString::from(payload.as_str()))) {
                        g.set_value_bykey(Some(&CefString::from("__ws_p")), Some(&mut p), V8Propertyattribute::default());
                    }
                }
                let code = CefString::from(format!("window.__ws_reply({id:?},window.__ws_p)").as_str());
                let (mut ret, mut exc) = (None, None);
                ctx.eval(Some(&code), Some(&CefString::from("spike://ws-reply")), 0, Some(&mut ret), Some(&mut exc));
                ctx.exit();
            }
            1
        }
    }
}

wrap_app! {
    struct HelperApp {}
    impl App {
        fn on_register_custom_schemes(&self, registrar: Option<&mut SchemeRegistrar>) {
            if let Some(r) = registrar {
                use cef::sys::cef_scheme_options_t as O;
                let opts = O::CEF_SCHEME_OPTION_STANDARD as i32 | O::CEF_SCHEME_OPTION_SECURE as i32 | O::CEF_SCHEME_OPTION_CORS_ENABLED as i32 | O::CEF_SCHEME_OPTION_FETCH_ENABLED as i32;
                r.add_custom_scheme(Some(&CefString::from("workspace")), opts);
            }
        }
        fn render_process_handler(&self) -> Option<RenderProcessHandler> {
            Some(RPH::new())
        }
    }
}

fn main() {
    let args = Args::new();
    #[cfg(target_os = "macos")]
    let _loader = {
        let loader = library_loader::LibraryLoader::new(&std::env::current_exe().unwrap(), true);
        assert!(loader.load());
        loader
    };
    let _ = api_hash(sys::CEF_API_VERSION_LAST, 0);
    let mut app = HelperApp::new();
    execute_process(
        Some(args.as_main_args()),
        Some(&mut app),
        std::ptr::null_mut(),
    );
}
