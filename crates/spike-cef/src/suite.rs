//! 헤드리스 검증 스위트 (창/포커스/사용자 입력과 무관). `SPIKE_SUITE=input|browser|all SPIKE_HEADLESS=1`.
//! 입력은 BrowserHost API 로 직접 보내고, 결과는 페이지 JS(document.title 경유)와 CEF 핸들러 상태로 판정한다.

use crate::cefhost::{self, with};
use crate::headless::{click, exec, host, pump, rect_of, step, title_query};
use cef::*;
use std::time::{Duration, Instant};

const BASE: &str = "http://127.0.0.1:18080";

struct Rs(Vec<(String, &'static str, String)>);
impl Rs {
    fn add(&mut self, id: &str, pass: Option<bool>, detail: impl Into<String>) {
        let st = match pass {
            Some(true) => "PASS",
            Some(false) => "FAIL",
            None => "INFO",
        };
        let d = detail.into();
        println!("RESULT {id} {st} {d}");
        use std::io::Write;
        let _ = std::io::stdout().flush();
        self.0.push((id.to_string(), st, d));
    }
}

fn url(p: &str) -> String {
    format!("{BASE}{p}")
}

fn main_browser() -> Option<Browser> {
    with(|s| s.main.clone())
}

/// 이동 후 로딩 완료까지 대기(최대 15s)
fn nav(u: &str) {
    let before = with(|s| s.load_events);
    if let Some(f) = main_browser().and_then(|b| b.main_frame()) {
        f.load_url(Some(&CefString::from(u)));
    }
    let t = Instant::now();
    while t.elapsed() < Duration::from_secs(15) {
        pump(50);
        let (loading, ev) = with(|s| (s.loading.0, s.load_events));
        if ev > before && !loading {
            break;
        }
    }
    pump(300);
}

fn ev(expr: &str) -> String {
    title_query(expr, "__E:").unwrap_or_else(|| "<timeout>".into())
}

fn evn(expr: &str) -> f64 {
    ev(expr).parse().unwrap_or(f64::NAN)
}

fn report_get(reports: &testsite::Reports, k: &str) -> Option<String> {
    reports.lock().unwrap().get(k).cloned()
}

use crate::keys::{native_of, ALT, CMD, SHIFT};

fn key(vk: i32, mods: u32) {
    if let Some(h) = host() {
        crate::keys::send_key(&h, vk, mods);
    }
    pump(70);
}

fn type_text(t: &str) {
    if let Some(h) = host() {
        let invalid = Range { from: u32::MAX, to: u32::MAX };
        h.ime_commit_text(Some(&CefString::from(t)), Some(&invalid), 0);
    }
    pump(100);
}

fn mouse(x: f64, y: f64, mods: u32, count: i32) {
    let Some(h) = host() else { return };
    let e = MouseEvent { x: x as i32, y: y as i32, modifiers: mods };
    h.set_focus(1);
    h.send_mouse_move_event(Some(&e), 0);
    for c in 1..=count {
        h.send_mouse_click_event(Some(&e), MouseButtonType::LEFT, 0, c);
        h.send_mouse_click_event(Some(&e), MouseButtonType::LEFT, 1, c);
    }
    pump(120);
}

fn click_sel(sel: &str) -> bool {
    match rect_of(&[sel]) {
        Some((x, y)) => {
            click(x, y);
            pump(150);
            true
        }
        None => false,
    }
}

fn close_popup() {
    if let Some(h) = with(|s| s.popup.clone()).and_then(|b| b.host()) {
        h.close_browser(1);
    }
    for _ in 0..40 {
        pump(50);
        if with(|s| s.popup.is_none()) {
            break;
        }
    }
}

// ============================================================ 입력 묶음
fn suite_input(r: &mut Rs, reports: &testsite::Reports) {
    // ---- K1: 키보드 스크롤 ----
    nav(&url("/scroll"));
    mouse(300.0, 300.0, 0, 1);
    let y0 = evn("Math.round(scrollY)");
    for _ in 0..3 {
        key(40, 0);
    }
    pump(400);
    let y_down = evn("Math.round(scrollY)");
    key(34, 0);
    pump(500);
    let y_pgdn = evn("Math.round(scrollY)");
    key(35, 0);
    pump(800);
    let y_end = evn("Math.round(scrollY)");
    key(36, 0);
    pump(800);
    let y_home = evn("Math.round(scrollY)");
    key(32, 0);
    pump(600);
    let y_space = evn("Math.round(scrollY)");
    r.add("K1.keyboard_scroll", Some(y_down > y0 && y_pgdn > y_down && y_end > y_pgdn && y_home < 5.0 && y_space > 100.0),
        format!("start={y0} down×3={y_down} pagedown={y_pgdn} end={y_end} home={y_home} space={y_space}"));

    // ---- K2: 텍스트 편집 키 ----
    nav(&url("/edit"));
    click_sel("#t");
    type_text("hello world");
    let v0 = ev("document.getElementById('t').value");
    key(8, 0);
    let v1 = ev("document.getElementById('t').value");
    key(37, 0);
    key(37, 0);
    key(46, 0);
    let v2 = ev("document.getElementById('t').value");
    key(36, SHIFT); // Shift+Home → 줄 시작까지 선택
    let sel = ev("(t=>t.value.slice(t.selectionStart,t.selectionEnd))(document.getElementById('t'))");
    key(35, 0);
    key(37, CMD); // Cmd+← 줄 처음
    let caret_cmd_left = evn("document.getElementById('t').selectionStart");
    key(39, ALT); // Opt+→ 단어 이동
    let caret_alt_right = evn("document.getElementById('t').selectionStart");
    r.add("K2a.type_backspace_delete", Some(v0 == "hello world" && v1 == "hello worl" && v2 == "hello wol"), format!("typed={v0:?} backspace={v1:?} left×2+delete={v2:?}"));
    r.add("K2b.shift_home_selection", Some(sel == "hello wo"), format!("selection after Shift+Home={sel:?}"));
    r.add("K2c.cmd_left_line_start", Some(caret_cmd_left == 0.0), format!("caret after End, Cmd+←={caret_cmd_left} (0 expected)"));
    r.add("K2d.opt_right_word_jump", Some(caret_alt_right == 5.0), format!("caret after Opt+→={caret_alt_right} (5 expected: end of 'hello')"));
    // undo / select all (OSR 에는 메뉴가 없어 frame API)
    if let Some(f) = main_browser().and_then(|b| b.main_frame()) {
        f.select_all();
        pump(150);
        let all = ev("(t=>t.value.slice(t.selectionStart,t.selectionEnd))(document.getElementById('t'))");
        f.undo();
        pump(200);
        let after_undo = ev("document.getElementById('t').value");
        r.add("K2e.select_all_and_undo_via_frame_api", Some(all == "hello wol" && after_undo != "hello wol"), format!("select_all={all:?} after undo={after_undo:?}"));
    }

    // ---- K3: Tab 포커스 이동 / Enter 제출 / Esc ----
    nav(&url("/forms"));
    click_sel("#a");
    let f0 = ev("document.activeElement.id");
    key(9, 0);
    let f1 = ev("document.activeElement.id");
    key(9, 0);
    let f2 = ev("document.activeElement.id");
    key(9, SHIFT);
    let f3 = ev("document.activeElement.id");
    r.add("K3a.tab_focus_order", Some(f0 == "a" && f1 == "b" && f2 == "c" && f3 == "b"), format!("a→{f1}→{f2}, shift+tab→{f3} (start {f0})"));
    // Enter 제출 / Esc: 서버 보고가 아닌 JS 상태로 확인
    exec("window.__sub=0;document.querySelector('form').addEventListener('submit',()=>{window.__sub++});window.__esc=0;addEventListener('keydown',e=>{if(e.key==='Escape')window.__esc++})");
    pump(150);
    click_sel("#b");
    key(13, 0);
    let sub = evn("window.__sub");
    key(27, 0);
    let esc = evn("window.__esc");
    r.add("K3b.enter_submits_form", Some(sub >= 1.0), format!("submit events={sub}"));
    r.add("K3c.escape_keydown_delivered", Some(esc >= 1.0), format!("Escape keydown events={esc}"));

    // ---- M1: 더블/트리플/Shift 클릭 선택 ----
    nav(&url("/edit"));
    if let Some((x, y)) = rect_of(&["#w"]) {
        mouse(x, y, 0, 2);
        let dbl = ev("getSelection().toString()");
        mouse(x, y, 0, 3);
        let tri = ev("getSelection().toString().trim()");
        r.add("M1a.double_click_selects_word", Some(dbl.trim() == "beta"), format!("double-click selection={dbl:?}"));
        r.add("M1b.triple_click_selects_paragraph", Some(tri == "alpha beta gamma delta"), format!("triple-click selection={tri:?}"));
    }
    nav(&url("/edit"));
    if let (Some((x0, y0)), Some((x1, _))) = (rect_of(&["#para"]), Some((0.0f64, 0.0f64))) {
        let _ = x1;
        // 문단 왼쪽 끝 클릭 → 오른쪽 부분 Shift+클릭
        let w = evn("document.getElementById('para').getBoundingClientRect().width");
        let left = x0 - w / 2.0 + 14.0; // 문단 margin(10px) 안쪽, 글자 시작 부근
        mouse(left, y0, 0, 1);
        pump(900); // 더블클릭 판정 시간 이후에 Shift+클릭(직전 클릭과 합쳐져 다중 클릭이 되는 것 방지)
        mouse(left + 220.0, y0, SHIFT, 1);
        let rng = ev("getSelection().toString()");
        r.add("M1c.shift_click_extends_selection", Some(rng.len() >= 5), format!("selection after click + shift-click={rng:?}"));
    }

    // ---- S3/B6: 줌 ----
    nav(&url("/zoom"));
    let h = host();
    if let Some(h) = h {
        let iw0 = evn("innerWidth");
        h.set_zoom_level(3.8); // 1.2^3.8 ≈ 2.0
        pump(600);
        let iw1 = evn("innerWidth");
        let lvl = h.zoom_level();
        let ratio = iw0 / iw1;
        r.add("S3.zoom_level_changes_layout", Some((1.8..2.2).contains(&ratio)), format!("level={lvl:.2} innerWidth {iw0}→{iw1} (ratio {ratio:.2}, ≈2 expected)"));
        // 줌 상태에서 클릭 정확도: CSS px 좌표 × 줌 배율 = 뷰 좌표
        reports.lock().unwrap().remove("zclick");
        if let Some((cx, cy)) = rect_of(&["#btn"]) {
            let f = 1.2f64.powf(lvl);
            mouse(cx * f, cy * f, 0, 1);
            pump(400);
            let hit_scaled = report_get(reports, "zclick").is_some();
            reports.lock().unwrap().remove("zclick");
            mouse(cx, cy, 0, 1); // 줌 배율을 무시한 좌표(잘못된 변환)
            pump(400);
            let hit_unscaled = report_get(reports, "zclick").is_some();
            r.add("B6.click_accuracy_when_zoomed", Some(hit_scaled && !hit_unscaled), format!("css center=({cx:.0},{cy:.0}), factor {f:.2}: click at css×factor hits={hit_scaled}, at raw css coords hits={hit_unscaled} → 앱이 줌 배율로 좌표를 변환해야 함"));
        }
        // 도메인별 줌 기억: 같은 오리진의 다른 페이지로 이동
        nav(&url("/bg?color=ffffff"));
        let lvl2 = h.zoom_level();
        r.add("S3b.zoom_persists_per_origin", Some((lvl2 - 3.8).abs() < 0.05), format!("level after navigating same-origin page={lvl2:.2}"));
        h.set_zoom_level(0.0);
        pump(300);
    }
}


// ============================================================ 브라우저 기능 묶음
fn cookie_url() -> CefString {
    CefString::from(format!("{BASE}/").as_str())
}

fn server_cookies() -> String {
    nav(&url("/cookies"));
    ev("document.getElementById('r').textContent")
}

/// 별도 스레드에서 요청하고 메인은 CEF 메시지 루프를 계속 돌린다(DevTools 서버는 같은 루프에서 처리됨)
fn http_get(port: u16, path: &str) -> Option<String> {
    let path = path.to_string();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(http_get_blocking(port, &path));
    });
    let t = Instant::now();
    while t.elapsed() < Duration::from_secs(8) {
        pump(30);
        if let Ok(v) = rx.try_recv() {
            return v;
        }
    }
    None
}

fn http_get_blocking(port: u16, path: &str) -> Option<String> {
    use std::io::{Read, Write};
    let mut s = match std::net::TcpStream::connect(("127.0.0.1", port)) {
        Ok(s) => s,
        Err(e) => return Some(format!("CONNECT_ERR {e}")),
    };
    s.set_read_timeout(Some(Duration::from_secs(3))).ok()?;
    write!(s, "GET {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nAccept: */*\r\nUser-Agent: spike\r\nConnection: close\r\n\r\n").ok()?;
    // DevTools 서버는 연결을 바로 닫지 않으므로 타임아웃까지 읽은 바이트를 모두 모은다
    let mut buf = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        match s.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => {
                buf.extend_from_slice(&chunk[..n]);
                if String::from_utf8_lossy(&buf).contains("\r\n\r\n") && buf.len() > 200 && buf.ends_with(b"}\n") {
                    break;
                }
            }
            Err(_) => break,
        }
    }
    Some(String::from_utf8_lossy(&buf).into_owned())
}

fn suite_browser(r: &mut Rs, reports: &testsite::Reports) {
    // ---- B1: 쿠키 ----
    if let Some(m) = cookie_manager_get_global_manager(None) {
        m.delete_cookies(None, None, None);
        pump(300);
    }
    nav(&url("/setcookie?name=pers&value=1&max=3600"));
    nav(&url("/setcookie?name=sess&value=2"));
    nav(&url("/setcookie?name=hon&value=3&attrs=HttpOnly"));
    let dc = {
        nav(&url("/cookies"));
        ev("document.cookie")
    };
    let sv = ev("document.getElementById('r').textContent");
    r.add("B1a.set_cookie_js_vs_server_view", Some(dc.contains("pers=1") && dc.contains("sess=2") && !dc.contains("hon") && sv.contains("hon=3")), format!("document.cookie={dc:?} server saw={sv:?} (HttpOnly hidden from JS, visible to server)"));
    if let Some(m) = cookie_manager_get_global_manager(None) {
        let c = Cookie {
            size: std::mem::size_of::<sys::cef_cookie_t>(),
            name: CefString::from("mgr"),
            value: CefString::from("fromrust"),
            domain: CefString::from("127.0.0.1"),
            path: CefString::from("/"),
            ..Default::default()
        };
        let ok = m.set_cookie(Some(&cookie_url()), Some(&c), None);
        pump(400);
        let sv2 = server_cookies();
        r.add("B1b.set_cookie_from_rust", Some(sv2.contains("mgr=fromrust")), format!("set_cookie->{ok} server saw={sv2:?}"));
        m.delete_cookies(Some(&cookie_url()), Some(&CefString::from("mgr")), None);
        pump(400);
        let sv3 = server_cookies();
        r.add("B1c.delete_one_cookie", Some(!sv3.contains("mgr=") && sv3.contains("pers=1")), format!("after delete mgr: {sv3:?}"));
        m.delete_cookies(None, None, None);
        pump(500);
        let sv4 = server_cookies();
        r.add("B1d.delete_all_cookies", Some(sv4.trim().is_empty()), format!("after delete all: {sv4:?}"));
    }

    // ---- B2: 히스토리 ----
    nav(&url("/hist/1"));
    nav(&url("/hist/2"));
    nav(&url("/hist/3"));
    let (_, back0, fwd0) = with(|s| s.loading);
    let b = main_browser().unwrap();
    b.go_back();
    pump(800);
    let a1 = with(|s| s.address.clone());
    b.go_back();
    pump(800);
    let a2 = with(|s| s.address.clone());
    let (_, back2, fwd2) = with(|s| s.loading);
    b.go_forward();
    pump(800);
    b.go_forward();
    pump(800);
    let a3 = with(|s| s.address.clone());
    let le = with(|s| s.load_events);
    b.reload();
    pump(1200);
    let le2 = with(|s| s.load_events);
    r.add("B2a.back_forward_navigation", Some(a1.ends_with("/hist/2") && a2.ends_with("/hist/1") && a3.ends_with("/hist/3")), format!("back→{a1} back→{a2} fwd×2→{a3}"));
    r.add("B2b.can_go_back_forward_state", Some(back0 && !fwd0 && back2 && fwd2), format!("at /hist/3 back={back0} fwd={fwd0}; at /hist/1 back={back2} (이전 항목 존재) fwd={fwd2}"));
    r.add("B2c.reload", Some(le2 > le), format!("loading events {le}→{le2}"));
    nav(&url("/spa"));
    click_sel("#go");
    pump(500);
    let spa_addr = with(|s| s.address.clone());
    let spa_title = with(|s| s.title.clone());
    main_browser().unwrap().go_back();
    pump(700);
    let spa_back = with(|s| s.address.clone());
    r.add("B2d.spa_pushstate_and_back", Some(spa_addr.ends_with("/spa/x") && spa_back.ends_with("/spa")), format!("pushState→{spa_addr} (title {spa_title:?}) back→{spa_back}"));

    // ---- B5: 찾기 ----
    nav(&url("/find"));
    if let Some(h) = host() {
        let wait_find = |min_active: i32| {
            let t = Instant::now();
            while t.elapsed() < Duration::from_secs(3) {
                pump(50);
                let (_, act, fin) = with(|s| s.find);
                if fin && act >= min_active {
                    break;
                }
            }
        };
        with(|s| s.find = (0, 0, false));
        h.find(Some(&CefString::from("needle")), 1, 0, 0);
        wait_find(1);
        let (cnt, act, _) = with(|s| s.find);
        h.find(Some(&CefString::from("needle")), 1, 0, 1);
        wait_find(2);
        let (_, act2, _) = with(|s| s.find);
        with(|s| s.find = (0, 0, false));
        h.find(Some(&CefString::from("needle")), 1, 1, 0);
        wait_find(1);
        let (cnt_case, _, _) = with(|s| s.find);
        h.stop_finding(1);
        pump(200);
        r.add("B5.find_in_page", Some(cnt == 6 && act2 == act + 1 && cnt_case == 4), format!("case-insensitive count={cnt} initial active ordinal={act} (CEF 는 첫 검색에서 0); find-next → {act2}; case-sensitive count={cnt_case} (6 / +1 / 4 expected)"));
    }

    // ---- B10: JS 다이얼로그 ----
    nav(&url("/dialogs"));
    with(|s| s.dialogs.clear());
    exec("window.__r={a:(alert('A'),'ok'),c:confirm('C'),p:prompt('P','d')}");
    pump(600);
    let res = ev("JSON.stringify(window.__r||null)");
    let dl = with(|s| s.dialogs.clone());
    r.add("B10a.alert_confirm_prompt", Some(res.contains("\"c\":true") && res.contains("spike-input") && dl.len() == 3), format!("page result={res} handler log={dl:?}"));
    exec("window.onbeforeunload=e=>{e.preventDefault();return 'x'}");
    mouse(400.0, 300.0, 0, 1); // 사용자 활성화(beforeunload 는 활성화 필요)
    with(|s| s.dialogs.clear());
    nav(&url("/hist/1"));
    let dl2 = with(|s| s.dialogs.clone());
    r.add("B10b.beforeunload_dialog", Some(dl2.iter().any(|d| d.starts_with("beforeunload"))), format!("handler log after navigating away={dl2:?}"));

    // ---- B12: 로딩/메타 ----
    with(|s| {
        s.favicons.clear();
        s.load_errors.clear();
    });
    nav(&url("/titled"));
    pump(600);
    let title = with(|s| s.title.clone());
    let fav = with(|s| s.favicons.len());
    let addr = with(|s| s.address.clone());
    r.add("B12a.title_address_favicon", Some(title == "My Titled Page" && addr.ends_with("/titled") && fav >= 1), format!("title={title:?} address={addr} favicon events={fav}"));
    nav("http://127.0.0.1:54329/");
    let errs = with(|s| s.load_errors.clone());
    r.add("B12b.load_error_reported", Some(!errs.is_empty()), format!("load errors={errs:?}"));

    // ---- B4: 팝업 ----
    nav(&url("/popups"));
    with(|s| s.popup_log.clear());
    reports.lock().unwrap().remove("popchild");
    exec("window.open('/popchild','_blank','width=400,height=300')"); // 사용자 제스처 없음
    pump(2000);
    let blocked_default = with(|s| s.popup.is_none()) && with(|s| s.popup_log.is_empty());
    r.add("B4a.popup_without_gesture_blocked_by_default", Some(blocked_default), format!("popup opened={} on_before_popup calls={} (Chromium 이 기본 차단하므로 핸들러까지 오지 않음)", with(|s| s.popup.is_some()), with(|s| s.popup_log.len())));
    click_sel("#pb");
    for _ in 0..60 {
        pump(100);
        if with(|s| s.popup.is_some()) {
            break;
        }
    }
    pump(800);
    let pl2 = with(|s| s.popup_log.clone());
    let child = report_get(reports, "popchild");
    r.add("B4b.window_open_with_gesture_features_opener", Some(pl2.iter().any(|l| l.contains("gesture=1") && l.contains("400x300")) && child.as_deref() == Some("OPENER")), format!("log={pl2:?} child saw={child:?}"));
    close_popup();
    reports.lock().unwrap().remove("popchild");
    nav(&url("/popups"));
    click_sel("#pb2");
    for _ in 0..60 {
        pump(100);
        if with(|s| s.popup.is_some()) {
            break;
        }
    }
    pump(800);
    let child2 = report_get(reports, "popchild");
    r.add("B4c.noopener_popup_has_no_opener", Some(child2.as_deref() == Some("NO_OPENER")), format!("child saw={child2:?}"));
    close_popup();
    // 앱 정책 훅: 제스처 있는 팝업도 앱이 취소할 수 있는지 (block 플래그 사용)
    with(|s| s.block_no_gesture = true);
    r.add("B4d.popup_policy_hook_exists", None, "on_before_popup 에서 user_gesture/features/URL 로 허용·차단을 결정할 수 있음(차단 시 1 반환) — 코드 경로 존재, 제스처 있는 차단 시나리오는 미검증");
    with(|s| s.block_no_gesture = false);

    // ---- B7: 개발자 도구(원격 디버깅 포트) ----
    nav(&url("/hist/1"));
    let ver = http_get(9222, "/json/version").unwrap_or_default();
    let list = http_get(9222, "/json/list").unwrap_or_default();
    r.add("B7a.remote_debugging_endpoints", Some(ver.contains("Browser") && list.contains("webSocketDebuggerUrl") && list.contains("/hist/1")), format!("version ok={} list has page target={} (port 9222); version head={:?}", ver.contains("Browser"), list.contains("/hist/1"), ver.chars().take(160).collect::<String>()));
}

/// 키 이벤트 인코딩 변형 실험: 어떤 조합이 "정확히 한 번의 편집 동작"을 내는지 데이터로 확인
fn key_variant(vk: i32, mods: u32, variant: u32, ch: u16) {
    let Some(h) = host() else { return };
    let mk = |t: KeyEventType, c: u16| KeyEvent {
        size: std::mem::size_of::<sys::cef_key_event_t>(),
        type_: t,
        modifiers: mods,
        windows_key_code: vk,
        native_key_code: native_of(vk),
        is_system_key: 0,
        character: c,
        unmodified_character: c,
        focus_on_editable_field: 0,
    };
    match variant {
        1 => { h.send_key_event(Some(&mk(KeyEventType::RAWKEYDOWN, 0))); h.send_key_event(Some(&mk(KeyEventType::KEYUP, 0))); }
        2 => { h.send_key_event(Some(&mk(KeyEventType::RAWKEYDOWN, ch))); h.send_key_event(Some(&mk(KeyEventType::KEYUP, ch))); }
        3 => { h.send_key_event(Some(&mk(KeyEventType::KEYDOWN, ch))); h.send_key_event(Some(&mk(KeyEventType::KEYUP, ch))); }
        4 => { h.send_key_event(Some(&mk(KeyEventType::RAWKEYDOWN, ch))); h.send_key_event(Some(&mk(KeyEventType::CHAR, ch))); h.send_key_event(Some(&mk(KeyEventType::KEYUP, ch))); }
        _ => {}
    }
    pump(80);
}

fn suite_keyexp(r: &mut Rs) {
    // (이름, vk, mods, mac 문자, 준비, 관측식)
    let cases: Vec<(&str, i32, u32, u16)> = vec![
        ("Backspace", 8, 0, 0x7F),
        ("Left", 37, 0, 0xF702),
        ("Cmd+Left", 37, CMD, 0xF702),
        ("Opt+Left", 37, ALT, 0xF702),
        ("Shift+Home", 36, SHIFT, 0xF729),
        ("Delete(fwd)", 46, 0, 0xF728),
    ];
    for (name, vk, mods, ch) in cases {
        for variant in 1..=4u32 {
            nav(&url("/edit"));
            click_sel("#t");
            type_text("hello world");
            // 중간 위치로 이동(Delete/Left 관측용): 캐럿 8
            for _ in 0..3 { key_variant(37, 0, 1, 0xF702); }
            let before = ev("(t=>t.selectionStart+'|'+t.value)(document.getElementById('t'))");
            key_variant(vk, mods, variant, ch);
            let after = ev("(t=>t.selectionStart+'-'+t.selectionEnd+'|'+t.value)(document.getElementById('t'))");
            r.add(&format!("KEYEXP.{name}.v{variant}"), None, format!("before={before} after={after}"));
        }
    }
    // Tab / Space
    for variant in 1..=4u32 {
        nav(&url("/forms"));
        click_sel("#a");
        key_variant(9, 0, variant, 9);
        let f = ev("document.activeElement.id");
        r.add(&format!("KEYEXP.Tab.v{variant}"), None, format!("a → {f} (b expected)"));
    }
    for variant in 1..=4u32 {
        nav(&url("/scroll"));
        mouse(300.0, 300.0, 0, 1);
        key_variant(32, 0, variant, 32);
        pump(500);
        let y = ev("Math.round(scrollY)");
        r.add(&format!("KEYEXP.Space.v{variant}"), None, format!("scrollY={y} (>0 expected)"));
    }
}

/// Shift+클릭 범위 선택 변형 실험
fn suite_shiftclick(r: &mut Rs) {
    nav(&url("/edit"));
    let Some((x0, y0)) = rect_of(&["#para"]) else { return };
    let w = evn("document.getElementById('para').getBoundingClientRect().width");
    let left = x0 - w / 2.0 + 14.0;
    let Some(h) = host() else { return };
    for variant in 1..=3 {
        nav(&url("/edit"));
        h.set_focus(1);
        let e0 = MouseEvent { x: left as i32, y: y0 as i32, modifiers: 0 };
        h.send_mouse_move_event(Some(&e0), 0);
        h.send_mouse_click_event(Some(&e0), MouseButtonType::LEFT, 0, 1);
        h.send_mouse_click_event(Some(&e0), MouseButtonType::LEFT, 1, 1);
        pump(900); // 더블클릭 판정 시간 이후
        let first = ev("(s=>s.anchorOffset+'/'+s.focusOffset+' type='+s.type)(getSelection())");
        let e1 = MouseEvent { x: (left + 220.0) as i32, y: y0 as i32, modifiers: SHIFT };
        match variant {
            1 => { // 이동·클릭 모두 Shift
                h.send_mouse_move_event(Some(&e1), 0);
                h.send_mouse_click_event(Some(&e1), MouseButtonType::LEFT, 0, 1);
                h.send_mouse_click_event(Some(&e1), MouseButtonType::LEFT, 1, 1);
            }
            2 => { // 클릭에만 Shift
                let plain = MouseEvent { x: e1.x, y: e1.y, modifiers: 0 };
                h.send_mouse_move_event(Some(&plain), 0);
                h.send_mouse_click_event(Some(&e1), MouseButtonType::LEFT, 0, 1);
                h.send_mouse_click_event(Some(&e1), MouseButtonType::LEFT, 1, 1);
            }
            _ => { // Shift 키 down 이벤트를 먼저 보낸 뒤 클릭
                h.send_key_event(Some(&KeyEvent { size: std::mem::size_of::<sys::cef_key_event_t>(), type_: KeyEventType::RAWKEYDOWN, modifiers: SHIFT, windows_key_code: 16, native_key_code: 56, is_system_key: 0, character: 0, unmodified_character: 0, focus_on_editable_field: 0 }));
                h.send_mouse_move_event(Some(&e1), 0);
                h.send_mouse_click_event(Some(&e1), MouseButtonType::LEFT, 0, 1);
                h.send_mouse_click_event(Some(&e1), MouseButtonType::LEFT, 1, 1);
                h.send_key_event(Some(&KeyEvent { size: std::mem::size_of::<sys::cef_key_event_t>(), type_: KeyEventType::KEYUP, modifiers: 0, windows_key_code: 16, native_key_code: 56, is_system_key: 0, character: 0, unmodified_character: 0, focus_on_editable_field: 0 }));
            }
        }
        pump(300);
        let sel = ev("getSelection().toString()");
        r.add(&format!("SHIFTCLICK.v{variant}"), None, format!("after first click: {first}; after shift-click selection={sel:?}"));
    }
}

pub fn run(which: &str) {
    let reports = testsite::spawn(18080);
    let _ = cefhost::apply_prefs_off; // 사용 안 함 경고 방지
    step("suite: waiting for browser");
    // 브라우저 생성
    for _ in 0..500 {
        pump(10);
        if with(|s| s.ready) {
            break;
        }
    }
    cefhost::create_main_browser("about:blank");
    for _ in 0..300 {
        pump(50);
        if with(|s| s.main.is_some()) {
            break;
        }
    }
    let _ = crate::ipc_browser::register_factory();
    pump(500);
    let mut rs = Rs(vec![]);
    if which == "hold" {
        nav(&url("/hist/1"));
        println!("HOLD ready");
        pump(60000);
    }
    if which == "shiftclick" {
        suite_shiftclick(&mut rs);
    }
    if which == "keyexp" {
        suite_keyexp(&mut rs);
    }
    if which == "input" || which == "all" {
        suite_input(&mut rs, &reports);
    }
    if which == "browser" || which == "all" {
        suite_browser(&mut rs, &reports);
    }
    let pass = rs.0.iter().filter(|r| r.1 == "PASS").count();
    let fail = rs.0.iter().filter(|r| r.1 == "FAIL").count();
    println!("SUITE {which}: {pass} PASS / {fail} FAIL");
    let j: Vec<_> = rs.0.iter().map(|(id, st, d)| serde_json::json!({"id": id, "status": st, "detail": d})).collect();
    let _ = std::fs::create_dir_all("target/results");
    let _ = std::fs::write(format!("target/results/cef-headless-{which}.json"), serde_json::to_string_pretty(&j).unwrap());
    if let Some(m) = cookie_manager_get_global_manager(None) {
        m.flush_store(None);
    }
    pump(800);
}
