//! 창 없는(헤드리스) CEF OSR 재현 모드.
//! gpui/NSWindow 를 만들지 않고, Accessory 앱(Dock/포커스/창 없음)으로 CEF 만 돌린다.
//! 입력은 CEF BrowserHost API 로 직접 보내고, 화면은 OnPaint 버퍼를 PNG 로 저장해 확인한다.
//! 사용자 키보드/마우스/포커스와 완전히 독립적이다.

use crate::cefhost::{self, with};
use cef::{args::Args, *};
use std::time::{Duration, Instant};

pub(crate) fn host() -> Option<BrowserHost> {
    with(|s| s.main.clone()).and_then(|b| b.host())
}

pub(crate) fn pump(ms: u64) {
    let t = Instant::now();
    while t.elapsed() < Duration::from_millis(ms) {
        do_message_loop_work();
        std::thread::sleep(Duration::from_millis(4));
    }
}

pub(crate) fn step(s: &str) {
    println!("STEP {s}");
    use std::io::Write;
    let _ = std::io::stdout().flush();
}

pub(crate) fn exec(js: &str) {
    if let Some(f) = with(|s| s.main.clone()).and_then(|b| b.main_frame()) {
        f.execute_java_script(Some(&CefString::from(js)), Some(&CefString::from("spike://eval")), 0);
    }
}

/// JS 결과를 document.title 로 받는다 (교차 출처 페이지에서도 동작)
pub(crate) fn title_query(js_expr: &str, prefix: &str) -> Option<String> {
    // 같은 값이 연속되면 title 이 안 바뀌어 이벤트가 없으므로, 호출마다 고유 번호를 붙인다.
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed) + 1;
    let tag = format!("{prefix}{n}|");
    with(|s| s.title.clear());
    exec(&format!("document.title='{tag}'+({js_expr})"));
    for _ in 0..40 {
        pump(100);
        let t = with(|s| s.title.clone());
        if let Some(v) = t.strip_prefix(&tag) {
            return Some(v.to_string());
        }
    }
    None
}

pub(crate) fn click(x: f64, y: f64) {
    let Some(h) = host() else { return };
    let e = MouseEvent { x: x as i32, y: y as i32, modifiers: 0 };
    h.set_focus(1);
    h.send_mouse_move_event(Some(&e), 0);
    h.send_mouse_click_event(Some(&e), MouseButtonType::LEFT, 0, 1);
    std::thread::sleep(Duration::from_millis(60));
    h.send_mouse_click_event(Some(&e), MouseButtonType::LEFT, 1, 1);
}

pub(crate) fn save_png(name: &str) {
    let (w, h, data) = with(|s| (s.surf[0].w, s.surf[0].h, s.surf[0].data.clone()));
    if w == 0 || h == 0 {
        return;
    }
    let mut rgba = data;
    for p in rgba.chunks_mut(4) {
        p.swap(0, 2); // BGRA → RGBA
    }
    if let Some(img) = image::RgbaImage::from_raw(w, h, rgba) {
        let _ = std::fs::create_dir_all("target/shots");
        let _ = img.save(format!("target/shots/headless-{name}.png"));
    }
}

pub(crate) fn rect_of(selectors: &[&str]) -> Option<(f64, f64)> {
    let list = selectors.iter().map(|s| format!("{s:?}")).collect::<Vec<_>>().join(",");
    let js = format!("(()=>{{let e=null;for(const s of [{list}]){{e=document.querySelector(s);if(e)break}}if(!e)return 'none';const r=e.getBoundingClientRect();return JSON.stringify([r.x+r.width/2,r.y+r.height/2])}})()");
    let v = title_query(&js, "__R:")?;
    let xy: Vec<f64> = serde_json::from_str(&v).ok()?;
    (xy.len() == 2).then(|| (xy[0], xy[1]))
}

/// IPC 방식 비교 측정(창 없음): http / 커스텀 스킴 / 프로세스 메시지 / Rust→페이지 푸시 / 오리진 게이팅
fn ipc_bench() {
    let reports = testsite::spawn(18080);
    let f = crate::ipc_browser::register_factory();
    println!("scheme factory registered -> {f}");
    let load = |u: &str| {
        reports.lock().unwrap().remove("ipcbench");
        if let Some(fr) = with(|s| s.main.clone()).and_then(|b| b.main_frame()) {
            fr.load_url(Some(&CefString::from(u)));
        }
    };
    for (label, u) in [("trusted origin http://127.0.0.1", "http://127.0.0.1:18080/ipcbench"), ("UNtrusted origin http://localhost", "http://localhost:18080/ipcbench")] {
        step(&format!("bench: {label}"));
        load(u);
        let t = Instant::now();
        let mut got = None;
        while t.elapsed() < Duration::from_secs(150) {
            pump(200);
            if let Some(v) = reports.lock().unwrap().get("ipcbench").cloned() {
                got = Some(v);
                break;
            }
        }
        println!("IPCBENCH {label}: {}", got.unwrap_or_else(|| "TIMEOUT".into()));
    }
    // Rust → 페이지 푸시: execute_java_script N 회 후 페이지 카운터가 N 이 될 때까지의 시간
    step("bench: rust->page push");
    load("http://127.0.0.1:18080/");
    pump(1500);
    exec("window.__c=0");
    pump(200);
    let n = 2000;
    let t = Instant::now();
    for _ in 0..n {
        exec("window.__c++");
    }
    let sent = t.elapsed();
    let mut done = None;
    while t.elapsed() < Duration::from_secs(30) {
        pump(5);
        let v = title_query("window.__c", "__C:").unwrap_or_default();
        if v == n.to_string() {
            done = Some(t.elapsed());
            break;
        }
    }
    println!("IPCBENCH push: {n} execute_java_script calls queued in {:.1}ms, all applied after {:?}", sent.as_secs_f64() * 1000.0, done.map(|d| format!("{:.1}ms", d.as_secs_f64() * 1000.0)));
}

pub fn run(args: &Args, app: &mut cef::App) {
    let gid = std::env::var("SPIKE_GOOGLE_LOGIN").unwrap_or_else(|_| "lkh0310kr".into());
    let _ = &gid;
    // NSApplication: 창/Dock/포커스 없는 Accessory 정책. CEF 가 요구하는 CefAppProtocol 을 NSApplication 에 주입.
    let mtm = objc2_foundation::MainThreadMarker::new().expect("main thread");
    let nsapp = objc2_app_kit::NSApplication::sharedApplication(mtm);
    let patched = unsafe { crate::macapp::patch_application(b"NSApplication\0") };
    nsapp.setActivationPolicy(objc2_app_kit::NSApplicationActivationPolicy::Accessory);
    println!("headless: macapp {patched}");

    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
    let cache = std::env::var("SPIKE_CACHE").unwrap_or_else(|_| format!("{home}/Library/Application Support/spike-cef/cef-cache"));
    let log_dir = format!("{home}/Library/Logs/spike-cef");
    let _ = std::fs::create_dir_all(&log_dir);
    let settings = Settings {
        windowless_rendering_enabled: true as _,
        external_message_pump: true as _,
        no_sandbox: true as _,
        root_cache_path: CefString::from(cache.as_str()),
        cache_path: CefString::from(cache.as_str()),
        log_file: CefString::from(format!("{log_dir}/cef.log").as_str()),
        remote_debugging_port: if std::env::var("SPIKE_SUITE").is_ok() { 9222 } else { 0 },
        log_severity: if std::env::var("SPIKE_VERBOSE").is_ok() { LogSeverity::VERBOSE } else { LogSeverity::INFO },
        ..Default::default()
    };
    let ok = initialize(Some(args.as_main_args()), Some(&settings), Some(app), std::ptr::null_mut());
    println!("cef initialize -> {ok}");
    assert_eq!(ok, 1);

    // 가상 뷰: 1100x800 @1x
    let (vw, vh) = std::env::var("SPIKE_VIEW").ok().and_then(|v| v.split_once('x').map(|(a, b)| (a.parse::<f32>().unwrap_or(1100.0), b.parse::<f32>().unwrap_or(800.0)))).unwrap_or((1100.0, 800.0));
    with(|s| {
        s.size[0] = (vw, vh);
        s.scale = 1.0;
    });

    for _ in 0..500 {
        do_message_loop_work();
        if with(|s| s.ready) {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    step("context ready");
    if std::env::var("SPIKE_PREFS").as_deref() == Ok("off") {
        for l in cefhost::apply_prefs_off() { step(&format!("pref {l}")); }
    }
    cefhost::create_main_browser("about:blank");
    for _ in 0..300 {
        pump(50);
        if with(|s| s.main.is_some()) {
            break;
        }
    }
    step("browser created");
    if let Some(h) = host() { step(&format!("actual runtime_style={:?}", h.runtime_style())); }

    if let Ok(which) = std::env::var("SPIKE_SUITE") {
        crate::suite::run(&which);
        std::process::exit(0);
    }
    if std::env::var("SPIKE_IPCBENCH").is_ok() {
        ipc_bench();
        std::process::exit(0);
    }
    if let Some(f) = with(|s| s.main.clone()).and_then(|b| b.main_frame()) {
        f.load_url(Some(&CefString::from("https://accounts.google.com/signin/v2/identifier?hl=en")));
    }
    pump(6000);
    save_png("1_identifier");

    let Some((ex, ey)) = rect_of(&["input[name=identifier]", "#identifierId", "input[type=email]", "input[type=text]"]) else {
        println!("RESULT HL.google_login INVALID email input not found");
        return;
    };
    step(&format!("email field at ({ex:.0},{ey:.0})"));

    // 입력 + 검증 (재시도 3회)
    let mut typed = false;
    for attempt in 1..=3 {
        click(ex, ey);
        pump(500);
        if let Some(h) = host() {
            let invalid = Range { from: u32::MAX, to: u32::MAX };
            h.ime_commit_text(Some(&CefString::from(gid.as_str())), Some(&invalid), 0);
        }
        pump(700);
        let v = title_query("(document.querySelector('input[name=identifier],#identifierId,input[type=email],input[type=text]')||{value:'<none>'}).value", "__V:").unwrap_or_default();
        step(&format!("type id attempt {attempt}: value={v:?}"));
        if v == gid {
            typed = true;
            break;
        }
    }
    save_png("2_typed");
    if !typed {
        println!("RESULT HL.google_login INVALID id was not typed");
        return;
    }

    let Some((nx, ny)) = rect_of(&["#identifierNext button", "#identifierNext"]).or_else(|| {
        let js = "(()=>{const b=[...document.querySelectorAll('button')].find(b=>/^\\s*next\\s*$/i.test(b.textContent));if(!b)return 'none';const r=b.getBoundingClientRect();return JSON.stringify([r.x+r.width/2,r.y+r.height/2])})()";
        let v = title_query(js, "__R:")?;
        let xy: Vec<f64> = serde_json::from_str(&v).ok()?;
        (xy.len() == 2).then(|| (xy[0], xy[1]))
    }) else {
        println!("RESULT HL.google_login INVALID Next button not found");
        return;
    };
    step(&format!("click Next at ({nx:.0},{ny:.0})"));
    click(nx, ny);
    for i in 1..=14 {
        pump(1000);
        step(&format!("wait {i}s after Next"));
        if i == 4 || i == 8 || i == 12 {
            save_png(&format!("3_after_next_{i}"));
        }
    }
    step("SURVIVED (no crash) with id verified");
    println!("RESULT HL.google_login PASS survived 14s after Next");
    let _ = std::fs::create_dir_all("target/results");
    // 쿠키 flush 후 종료
    if let Some(m) = cookie_manager_get_global_manager(None) {
        m.flush_store(None);
    }
    pump(1500);
    std::process::exit(0);
}
