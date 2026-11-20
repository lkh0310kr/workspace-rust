//! CEF 서브프로세스(GPU/Renderer/Plugin/Alerts) 진입점. 번들 안의 Helper .app 으로 복사된다.
//! 렌더러 프로세스에서는 페이지 스크립트 실행 전에 WebAuthn API 를 제거할 수 있다(SPIKE_DISABLE_WEBAUTHN=1).
//! 임베디드 OSR 환경에서는 passkey 인증 UI/인증기가 없어 브라우저 프로세스가 크래시하는 문제의 회피책.
use cef::{args::Args, rc::Rc, *};

wrap_render_process_handler! {
    struct RPH {}
    impl RenderProcessHandler {
        fn on_context_created(&self, _b: Option<&mut Browser>, _f: Option<&mut Frame>, context: Option<&mut V8Context>) {
            if std::env::var("SPIKE_DISABLE_WEBAUTHN").is_err() {
                return;
            }
            let Some(ctx) = context else { return };
            let code = CefString::from(
                "try{delete window.PublicKeyCredential;Object.defineProperty(navigator,'credentials',{get(){return undefined},configurable:true})}catch(e){}",
            );
            let url = CefString::from("spike://disable-webauthn");
            let mut ret = None;
            let mut exc = None;
            ctx.eval(Some(&code), Some(&url), 0, Some(&mut ret), Some(&mut exc));
        }
    }
}

wrap_app! {
    struct HelperApp {}
    impl App {
        fn render_process_handler(&self) -> Option<RenderProcessHandler> {
            Some(RPH::new())
        }
    }
}

fn main() {
    let args = Args::new();
    let _loader = {
        let loader = library_loader::LibraryLoader::new(&std::env::current_exe().unwrap(), true);
        assert!(loader.load());
        loader
    };
    let _ = api_hash(sys::CEF_API_VERSION_LAST, 0);
    let mut app = HelperApp::new();
    execute_process(Some(args.as_main_args()), Some(&mut app), std::ptr::null_mut());
}
