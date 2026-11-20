//! CEF 서브프로세스(GPU/Renderer/Plugin/Alerts) 진입점. 번들 안의 Helper .app 으로 복사된다.
use cef::{args::Args, *};

fn main() {
    let args = Args::new();
    let _loader = {
        let loader = library_loader::LibraryLoader::new(&std::env::current_exe().unwrap(), true);
        assert!(loader.load());
        loader
    };
    let _ = api_hash(sys::CEF_API_VERSION_LAST, 0);
    execute_process(Some(args.as_main_args()), None::<&mut App>, std::ptr::null_mut());
}
