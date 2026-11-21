//! Spike B: gpui + CEF(Chromium) off-screen rendering.
//! 웹 콘텐츠는 gpui 씬 안의 "이미지 한 장"으로 그려지므로 z-order/클립/오버레이는 gpui 규칙을 그대로 따른다.

#[cfg(target_os = "macos")]
mod autotest;
mod cefhost;
#[cfg(target_os = "macos")]
mod headless;
mod ipc_browser;
mod keys;
#[cfg(target_os = "macos")]
mod macapp;
#[cfg(target_os = "macos")]
mod native;
mod root_view;
#[cfg(target_os = "macos")]
mod suite;

use cef::{
    api_hash, args::Args, cookie_manager_get_global_manager, do_message_loop_work, execute_process,
    initialize, sys, CefString, ImplCommandLine, ImplCookieManager, LogSeverity, Settings,
};
use cefhost::with;
use gpui::{px, size, App, AppContext, Application, Bounds, Entity, WindowBounds, WindowOptions};
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use crate::root_view::{url, Root, Sh, Shared, ToggleOverlay, PORT};

/// gpui keystroke → Windows virtual key code (특수키만; 일반 문자는 IME/commit 경로)
fn vkey(k: &str) -> Option<i32> {
    Some(match k {
        "backspace" => 8,
        "tab" => 9,
        "enter" => 13,
        "escape" => 27,
        "space" => 32,
        "pageup" => 33,
        "pagedown" => 34,
        "end" => 35,
        "home" => 36,
        "left" => 37,
        "up" => 38,
        "right" => 39,
        "down" => 40,
        "delete" => 46,
        _ => return None,
    })
}

fn main() {
    #[cfg(target_os = "macos")]
    let _loader = {
        let loader = library_loader::LibraryLoader::new(&std::env::current_exe().unwrap(), false);
        assert!(
            loader.load(),
            "CEF framework load failed (번들 .app 으로 실행해야 함)"
        );
        loader
    };
    let _ = api_hash(sys::CEF_API_VERSION_LAST, 0);

    let args = Args::new();
    let cmd = args.as_cmd_line().unwrap();
    let is_browser = cmd.has_switch(Some(&CefString::from("type"))) != 1;
    let mut app = cefhost::make_app();
    let ret = execute_process(
        Some(args.as_main_args()),
        Some(&mut app),
        std::ptr::null_mut(),
    );
    if !is_browser {
        std::process::exit(ret.max(0));
    }

    // 창 없는(헤드리스) 재현 모드: gpui 를 띄우지 않는다 (사용자 작업 방해 없음)
    #[cfg(target_os = "macos")]
    if std::env::var("SPIKE_HEADLESS").is_ok() {
        headless::run(&args, &mut app);
        return;
    }

    #[cfg(target_os = "macos")]
    {
        let patched = unsafe { macapp::patch_application(b"GPUIApplication\0") };
        println!("macapp: {patched}");
    }

    let autotest = std::env::var("SPIKE_AUTOTEST").is_ok();
    // todo: port
    let reports = testsite::spawn(PORT);

    Application::new().run(move |cx: &mut App| {
        let sh: Sh = Rc::new(RefCell::new(Shared::default()));
        let bg = {
            #[cfg(target_os = "macos")]
            native::background();

            #[cfg(not(target_os = "macos"))]
            false
        };
        // 백그라운드 모드: 포커스를 가져오지 않고, 창은 autotest 에서 stealth() 처리(거의 투명, 마우스 투과)
        let bounds = Bounds::centered(None, size(px(1200.), px(800.)), cx);
        let sh2 = sh.clone();
        let win = cx
            .open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    focus: !bg,
                    ..Default::default()
                },
                move |_, cx| {
                    cx.new(|cx| Root {
                        sh: sh2,
                        sidebar_w: 200.0,
                        overlay: false,
                        overlay_clicks: 0,
                        tab_hidden: false,
                        focus: cx.focus_handle(),
                        marked: None,
                        action_toggles: 0,
                    })
                },
            )
            .unwrap();
        let view: Entity<Root> = win.update(cx, |_, _, cx| cx.entity()).unwrap();
        cx.bind_keys([gpui::KeyBinding::new("cmd-k", ToggleOverlay, None)]);

        #[cfg(target_os = "macos")]
        {
            if bg {
                native::set_accessory();
                let nap = native::disable_app_nap();
                std::mem::forget(_nap);
            } else {
                cx.activate(true);
            };
        }

        #[cfg(not(target_os = "macos"))]
        {
            cx.activate(true);
        }

        // CEF 초기화 (gpui 의 NSApplication 이 만들어진 뒤)
        // 전용 캐시/로그 폴더 (open 으로 실행하면 cwd 가 / 라서 상대경로가 무효 → 시스템 Chromium 프로필로 폴백되는 문제 방지)
        let data_dir = {
            #[cfg(target_os = "windows")]
            {
                std::path::PathBuf::from(
                    std::env::var_os("LOCALAPPDATA").expect("LOCALAPPDATA not set"),
                )
                .join("spike-cef")
            }

            #[cfg(target_os = "macos")]
            {
                std::path::PathBuf::from(std::env::var_os("HOME").expect("HOME not set"))
                    .join("Library/Application Support/spike-cef")
            }
        };

        let cache = std::env::var_os("SPIKE_CACHE")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| data_dir.join("cef-cache"));

        let log_dir = data_dir.join("logs");

        std::fs::create_dir_all(&cache).unwrap();
        std::fs::create_dir_all(&log_dir).unwrap();

        let cache = cache.to_string_lossy().into_owned();
        let log_file = log_dir.join("cef.log");
        let log_file = log_file.to_string_lossy().into_owned();
        let settings = Settings {
            windowless_rendering_enabled: true as _,
            external_message_pump: true as _,
            no_sandbox: true as _,
            root_cache_path: CefString::from(cache.as_str()),
            cache_path: CefString::from(cache.as_str()),
            log_file: CefString::from(log_file.as_str()),
            log_severity: LogSeverity::INFO,
            ..Default::default()
        };
        let ok = initialize(
            Some(args.as_main_args()),
            Some(&settings),
            Some(&mut app),
            std::ptr::null_mut(),
        );
        println!("cef initialize -> {ok}");
        assert_eq!(ok, 1);

        // 메시지 펌프 + 프레임 갱신 알림
        let v2 = view.clone();
        cx.spawn(async move |cx| {
            let mut last = 0u64;
            let mut ticks = 0u64;
            loop {
                do_message_loop_work();
                // 수동 사용 시 로그인 세션이 종료 직전에 유실되지 않도록 쿠키 DB 를 주기적으로 flush (약 5초)
                ticks += 1;
                if ticks % 1250 == 0 {
                    if let Some(m) = cookie_manager_get_global_manager(None) {
                        m.flush_store(None);
                    }
                }
                if with(|s| s.ready && !s.create_requested) {
                    with(|s| s.create_requested = true);
                    cefhost::create_main_browser(&url("/bg?color=ff0000"));
                }
                let g = with(|s| {
                    s.surf[0].gen
                        + s.surf[1].gen
                        + s.surf[2].gen
                        + s.widget_visible as u64
                        + s.ui_gen
                });
                if g != last {
                    last = g;
                    v2.update(cx, |_, cx| cx.notify()).ok();
                }
                cx.background_executor()
                    .timer(Duration::from_millis(4))
                    .await;
            }
        })
        .detach();

        if autotest {
            #[cfg(target_os = "macos")]
            autotest::run(cx, view, sh, reports);
        }
    });
    cef::shutdown();
}
