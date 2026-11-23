//! Spike B: gpui + CEF(Chromium) off-screen rendering.
//! 웹 콘텐츠는 gpui 씬 안의 "이미지 한 장"으로 그려지므로 z-order/클립/오버레이는 gpui 규칙을 그대로 따른다.

#[cfg(target_os = "macos")]
mod autotest;
mod application;
mod cef_adapter;
#[cfg(target_os = "macos")]
mod headless;
#[cfg(target_os = "macos")]
mod macapp;
#[cfg(target_os = "macos")]
mod native;
#[cfg(target_os = "macos")]
mod suite;
mod ui;

use gpui::{px, size, App, AppContext, Application, Bounds, Entity, WindowBounds, WindowOptions};
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use crate::ui::{url, Root, Sh, Shared, ToggleOverlay, PORT};

fn main() {
    #[cfg(target_os = "macos")]
    let _loader = cef_adapter::load_framework();
    cef_adapter::verify_api_version();
    let args = cef_adapter::args();
    let mut app = cef_adapter::make_app();
    let (is_browser, ret) = cef_adapter::run_subprocess(&args, &mut app);
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
        let app_logic = Rc::new(application::Application::new(cef_adapter::CefAdapter));
        let bounds = Bounds::centered(None, size(px(1200.), px(800.)), cx);
        let sh2 = sh.clone();
        let app_logic2 = app_logic.clone();
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
                        app: app_logic2,
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
        cef_adapter::initialize(&args, &mut app);

        // 메시지 펌프 + 프레임 갱신 알림
        let v2 = view.clone();
        let app_for_loop = app_logic.clone();
        cx.spawn(async move |cx| {
            let mut last = 0u64;
            let mut ticks = 0u64;
            loop {
                cef_adapter::message_loop_work();
                // 수동 사용 시 로그인 세션이 종료 직전에 유실되지 않도록 쿠키 DB 를 주기적으로 flush (약 5초)
                ticks += 1;
                if ticks % 1250 == 0 {
                    cef_adapter::flush_cookies();
                }
                app_for_loop.ensure_main_browser(&url("/bg?color=ff0000"));
                let g = app_for_loop.revision();
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
    cef_adapter::shutdown_runtime();
}
