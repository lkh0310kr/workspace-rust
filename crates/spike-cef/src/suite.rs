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

// ============================================================ 추가 묶음 (탭/프로필/크래시/웹 플랫폼/입력/UI)
fn rss_mb() -> f64 {
    let out = std::process::Command::new("ps").args(["-axo", "rss=,command="]).output().map(|o| String::from_utf8_lossy(&o.stdout).into_owned()).unwrap_or_default();
    out.lines()
        .filter(|l| l.contains("spike-cef.app"))
        .filter_map(|l| l.trim().split_whitespace().next().and_then(|n| n.parse::<f64>().ok()))
        .sum::<f64>()
        / 1024.0
}

fn wait_for(secs: u64, mut cond: impl FnMut() -> bool) -> bool {
    let t = Instant::now();
    while t.elapsed() < Duration::from_secs(secs) {
        pump(50);
        if cond() {
            return true;
        }
    }
    false
}

fn tab_paints() -> Vec<(i32, u64)> {
    with(|s| {
        let mut v: Vec<_> = s.tabs.iter().map(|t| (t.identifier(), *s.tab_paints.get(&t.identifier()).unwrap_or(&0))).collect();
        v.sort();
        v
    })
}

fn tab_load(i: usize, u: &str) {
    if let Some(f) = with(|s| s.tabs.get(i).cloned()).and_then(|b| b.main_frame()) {
        f.load_url(Some(&CefString::from(u)));
    }
}

fn close_all_tabs() {
    for t in with(|s| s.tabs.clone()) {
        if let Some(h) = t.host() {
            h.close_browser(1);
        }
    }
    wait_for(10, || with(|s| s.tabs.is_empty()));
}

fn cache_root() -> String {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
    std::env::var("SPIKE_CACHE").unwrap_or_else(|_| format!("{home}/Library/Application Support/spike-cef/cef-cache"))
}

fn suite_more(r: &mut Rs, reports: &testsite::Reports) {
    // ---------------- 탭 ----------------
    nav(&url("/hist/1"));
    let rss_base = rss_mb();
    for n in ["t1", "t2", "t3"] {
        cefhost::create_tab(&url(&format!("/tabpage?name={n}")), None);
    }
    let ok = wait_for(20, || with(|s| s.tabs.len() == 3) && ["t1", "t2", "t3"].iter().all(|n| report_get(reports, &format!("tab_{n}")).is_some()));
    let urls: Vec<_> = ["t1", "t2", "t3"].iter().map(|n| report_get(reports, &format!("tab_{n}")).unwrap_or_default()).collect();
    r.add("T1.diag_browser_creation_log", None, format!("{:?}", with(|s| s.log.iter().filter(|l| l.starts_with("created browser")).cloned().collect::<Vec<_>>())));
    r.add("T1.tabs_create_distinct", Some(ok && urls.iter().collect::<std::collections::HashSet<_>>().len() == 3), format!("tabs={} reported urls={urls:?}", with(|s| s.tabs.len())));

    // 숨긴 탭은 렌더가 멈추는가(was_hidden)
    for i in 0..3 {
        tab_load(i, &url("/anim2"));
    }
    pump(2500);
    let p0 = tab_paints();
    pump(2000);
    let p1 = tab_paints();
    let vis: Vec<u64> = p0.iter().zip(&p1).map(|(a, b)| b.1 - a.1).collect();
    let tabs = with(|s| s.tabs.clone());
    for t in &tabs[1..] {
        if let Some(h) = t.host() {
            h.was_hidden(1);
        }
    }
    pump(600);
    let h0 = tab_paints();
    pump(2000);
    let h1 = tab_paints();
    let hid: Vec<u64> = h0.iter().zip(&h1).map(|(a, b)| b.1 - a.1).collect();
    for t in &tabs[1..] {
        if let Some(h) = t.host() {
            h.was_hidden(0);
        }
    }
    pump(800);
    let s0 = tab_paints();
    pump(2000);
    let s1 = tab_paints();
    let shown: Vec<u64> = s0.iter().zip(&s1).map(|(a, b)| b.1 - a.1).collect();
    r.add("T2.hidden_tabs_stop_rendering", Some(vis.iter().all(|v| *v > 20) && hid[0] > 20 && hid[1] <= 3 && hid[2] <= 3 && shown.iter().all(|v| *v > 20)),
        format!("2s paint counts — all visible={vis:?}; tab0 visible & tab1,2 hidden={hid:?}; after show={shown:?}"));

    // 메모리: 10탭
    for i in 0..7 {
        cefhost::create_tab(&url(&format!("/tabpage?name=m{i}")), None);
    }
    wait_for(30, || with(|s| s.tabs.len() == 10));
    pump(2500);
    let rss_10 = rss_mb();
    close_all_tabs();
    let closed = with(|s| s.tab_closed);
    pump(3000);
    let rss_after = rss_mb();
    r.add("T3.tabs_close_cleanup", Some(with(|s| s.tabs.is_empty()) && closed >= 10), format!("tabs left={} closed events={closed}", with(|s| s.tabs.len())));
    r.add("T4.tabs_memory", None, format!("process-tree RSS: 1 tab {rss_base:.0}MB → 10 tabs {rss_10:.0}MB (+{:.0}MB, 탭당 약 {:.0}MB) → 닫은 뒤 {rss_after:.0}MB", rss_10 - rss_base, (rss_10 - rss_base) / 9.0));

    // ---------------- 프로필 격리 ----------------
    let root = cache_root();
    let (da, db) = (format!("{root}/ctx-A"), format!("{root}/ctx-B"));
    let mut ca = cefhost::make_context(&da);
    let mut cb = cefhost::make_context(&db);
    let mut cm = cefhost::make_context("");
    cefhost::create_tab(&url("/setcookie?name=iso&value=A&max=3600"), ca.as_mut());
    pump(1500);
    cefhost::create_tab(&url("/cookies?tag=B"), cb.as_mut());
    cefhost::create_tab(&url("/cookies?tag=A2"), ca.as_mut());
    wait_for(15, || report_get(reports, "cookies_B").is_some() && report_get(reports, "cookies_A2").is_some());
    nav(&url("/cookies?tag=D"));
    let (cb_s, ca_s, cd_s) = (report_get(reports, "cookies_B").unwrap_or_default(), report_get(reports, "cookies_A2").unwrap_or_default(), report_get(reports, "cookies_D").unwrap_or_default());
    r.add("PR1.profile_isolation", Some(ca_s.contains("iso=A") && !cb_s.contains("iso") && !cd_s.contains("iso")), format!("ctxA sees {ca_s:?}; ctxB sees {cb_s:?}; default context sees {cd_s:?}"));
    // 메모리 전용(시크릿)
    cefhost::create_tab(&url("/setcookie?name=iso&value=M&max=3600"), cm.as_mut());
    pump(1500);
    cefhost::create_tab(&url("/cookies?tag=M2"), cm.as_mut());
    wait_for(15, || report_get(reports, "cookies_M2").is_some());
    let m2 = report_get(reports, "cookies_M2").unwrap_or_default();
    // 디스크 프로필: 컨텍스트를 다시 만들어도 쿠키가 남는가 / 메모리 전용은 사라지는가
    if let Some(mgr) = ca.as_ref().and_then(|c| c.cookie_manager(None)) {
        mgr.flush_store(None);
    }
    pump(800);
    close_all_tabs();
    drop(ca);
    drop(cm);
    pump(800);
    let mut ca2 = cefhost::make_context(&da);
    let mut cm2 = cefhost::make_context("");
    cefhost::create_tab(&url("/cookies?tag=A3"), ca2.as_mut());
    cefhost::create_tab(&url("/cookies?tag=M3"), cm2.as_mut());
    wait_for(15, || report_get(reports, "cookies_A3").is_some() && report_get(reports, "cookies_M3").is_some());
    let (a3, m3) = (report_get(reports, "cookies_A3").unwrap_or_default(), report_get(reports, "cookies_M3").unwrap_or_default());
    r.add("PR2.disk_profile_persists_across_context_recreate", Some(a3.contains("iso=A")), format!("recreated ctxA sees {a3:?}"));
    r.add("PR3.incognito_context_not_persisted", Some(m2.contains("iso=M") && !m3.contains("iso")), format!("in-memory ctx sees {m2:?} while alive; recreated in-memory ctx sees {m3:?}"));
    close_all_tabs();

    // ---------------- 렌더러 크래시 복구 ----------------
    nav(&url("/hist/1"));
    let t0 = with(|s| s.terminated);
    if let Some(f) = main_browser().and_then(|b| b.main_frame()) {
        f.load_url(Some(&CefString::from("chrome://crash")));
    }
    let mut via = "chrome://crash";
    if !wait_for(8, || with(|s| s.terminated) > t0) {
        via = "kill -9 renderer";
        let _ = std::process::Command::new("sh").args(["-c", "pkill -9 -f 'spike-cef.*--type=renderer'"]).status();
        wait_for(8, || with(|s| s.terminated) > t0);
    }
    let terminated = with(|s| s.terminated) - t0;
    nav(&url("/hist/2"));
    let title = ev("document.title");
    r.add("ST1.renderer_crash_recovery", Some(terminated >= 1 && title == "hist 2"), format!("crash via {via}: on_render_process_terminated x{terminated}; after navigating again title={title:?}; log={:?}", with(|s| s.log.iter().filter(|l| l.starts_with("render_process_terminated")).cloned().collect::<Vec<_>>())));

    // ---------------- DevTools 프론트엔드 ----------------
    nav(&url("/hist/1"));
    let list = http_get(9222, "/json/list").unwrap_or_default();
    let page_id = list.lines().find(|l| l.contains("\"id\"")).and_then(|l| l.split('"').nth(3)).unwrap_or("").to_string();
    let insp = http_get(9222, "/devtools/inspector.html").unwrap_or_default();
    let status = insp.lines().next().unwrap_or("").to_string();
    r.add("D1.devtools_frontend_assets_served_locally", Some(status.contains("200")), format!("GET /devtools/inspector.html → {status:?} (404 면 최소 CEF 번들에 DevTools UI 리소스가 없음)"));
    with(|s| s.load_errors.clear()); // 앞선 B12b 의 의도적 연결 거부 오류가 섞이지 않게
    cefhost::create_tab(&format!("http://127.0.0.1:9222/devtools/inspector.html?ws=127.0.0.1:9222/devtools/page/{page_id}"), None);
    wait_for(15, || with(|s| s.tabs.len() == 1));
    pump(4000);
    let painted = tab_paints().first().map(|t| t.1).unwrap_or(0);
    let errs = with(|s| s.load_errors.clone());
    r.add("D2.devtools_in_second_windowless_browser", Some(status.contains("200") && painted > 5 && errs.is_empty()), format!("frontend status={status:?} tab paints={painted} load errors={errs:?}"));
    close_all_tabs();

    // ---------------- 웹 플랫폼 ----------------
    reports.lock().unwrap().remove("web");
    nav(&url("/web"));
    wait_for(30, || report_get(reports, "web").is_some());
    let w: serde_json::Value = serde_json::from_str(&report_get(reports, "web").unwrap_or_default()).unwrap_or_default();
    r.add("W1.codec_support", None, format!("canPlayType {}", w["codecs"]));
    r.add("W2.webgl2_webgpu", Some(w["webgl2"] == true), format!("webgl2={} renderer={} navigator.gpu={}", w["webgl2"], w["renderer"], w["webgpu"]));
    r.add("W3.localstorage_indexeddb", Some(w["ls"] == "v1" && w["idb"] == "v2"), format!("localStorage={} indexedDB={}", w["ls"], w["idb"]));
    r.add("W4.service_worker", Some(w["sw"] == "active"), format!("service worker={}", w["sw"]));
    r.add("W5.webrtc_offer", Some(w["rtc"] == "offer"), format!("RTCPeerConnection.createOffer={}", w["rtc"]));
    // 오디오: 제스처 없이(기본 자동재생 정책) / 사용자 클릭 후
    reports.lock().unwrap().remove("audio");
    exec("window.__audio()");
    wait_for(8, || report_get(reports, "audio").is_some());
    let a_no: serde_json::Value = serde_json::from_str(&report_get(reports, "audio").unwrap_or_default()).unwrap_or_default();
    reports.lock().unwrap().remove("audio");
    mouse(300.0, 300.0, 0, 1);
    exec("window.__audio()");
    wait_for(8, || report_get(reports, "audio").is_some());
    let a_yes: serde_json::Value = serde_json::from_str(&report_get(reports, "audio").unwrap_or_default()).unwrap_or_default();
    r.add("W6.audio_playback_pipeline", Some(a_yes["audio_time"].as_f64().unwrap_or(0.0) > 0.3), format!("muted <audio> without user gesture → {a_no}; after user click → {a_yes} (play: resolved|rejected|pending)"));

    // ---------------- 입력: hover / 키 반복 ----------------
    nav(&url("/hover"));
    if let (Some(h), Some((bx, by))) = (host(), rect_of(&["#box"])) {
        let out = MouseEvent { x: 600, y: 400, modifiers: 0 };
        let inn = MouseEvent { x: bx as i32, y: by as i32, modifiers: 0 };
        h.send_mouse_move_event(Some(&out), 0);
        pump(150);
        h.send_mouse_move_event(Some(&inn), 0);
        pump(200);
        let e1 = ev("JSON.stringify(__h)");
        h.send_mouse_move_event(Some(&out), 0);
        pump(200);
        let e2 = ev("JSON.stringify(__h)");
        h.send_mouse_move_event(Some(&inn), 0);
        pump(150);
        h.send_mouse_move_event(Some(&inn), 1); // 패널 밖으로 나감(mouse_leave)
        pump(250);
        let e3 = ev("JSON.stringify(__h)");
        r.add("H1.hover_enter_leave_and_panel_exit", Some(e1.contains("\"enter\":1") && e2.contains("\"leave\":1") && e3.contains("\"leave\":2")), format!("after enter={e1} after move-out={e2} after panel-leave={e3}"));
    }
    nav(&url("/hover"));
    if let Some(h) = host() {
        mouse(300.0, 300.0, 0, 1);
        let mk = |t: KeyEventType, mods: u32| KeyEvent { size: std::mem::size_of::<sys::cef_key_event_t>(), type_: t, modifiers: mods, windows_key_code: 65, native_key_code: 0, is_system_key: 0, character: 97, unmodified_character: 97, focus_on_editable_field: 0 };
        h.send_key_event(Some(&mk(KeyEventType::RAWKEYDOWN, 0)));
        h.send_key_event(Some(&mk(KeyEventType::RAWKEYDOWN, 1 << 13))); // EVENTFLAG_IS_REPEAT
        h.send_key_event(Some(&mk(KeyEventType::RAWKEYDOWN, 1 << 13)));
        h.send_key_event(Some(&mk(KeyEventType::KEYUP, 0)));
        pump(300);
        let k = ev("JSON.stringify(__k)");
        r.add("H2.key_repeat_flag", Some(k == "[\"d\",\"R\",\"R\"]"), format!("keydown sequence (d=first, R=e.repeat) = {k}"));
    }

    // ---------------- UI: HTTP 인증 / 클립보드 / 로딩 진행률 ----------------
    *cefhost::AUTH_CREDENTIALS.lock().unwrap() = None;
    cefhost::AUTH_REQUESTS.lock().unwrap().clear();
    nav(&url("/auth"));
    let no_cred = cefhost::AUTH_REQUESTS.lock().unwrap().len();
    let body0 = ev("document.body.innerText");
    let errs0 = with(|s| s.load_errors.clone());
    *cefhost::AUTH_CREDENTIALS.lock().unwrap() = Some(("u".into(), "p".into()));
    reports.lock().unwrap().remove("auth_ok");
    nav(&url("/auth"));
    wait_for(5, || report_get(reports, "auth_ok").is_some());
    let a = report_get(reports, "auth_ok").unwrap_or_default();
    r.add("A1.http_basic_auth_prompt_handled", Some(a.contains("dTpw") && no_cred >= 1), format!("auth requests without credentials={no_cred}; with credentials server saw {a:?} (dTpw = base64 'u:p'); requests={:?}; page text without credentials={body0:?} load errors={errs0:?}", cefhost::AUTH_REQUESTS.lock().unwrap().clone()));
    // 자격 증명을 URL 에 넣는 방식(콜백 대신)
    reports.lock().unwrap().remove("auth_ok");
    nav("http://u:p@127.0.0.1:18080/auth");
    wait_for(5, || report_get(reports, "auth_ok").is_some());
    let a2 = report_get(reports, "auth_ok").unwrap_or_default();
    r.add("A2.http_auth_via_url_credentials", Some(a2.contains("dTpw")), format!("http://u:p@host/ → server saw Authorization {a2:?}"));
    let saved = std::process::Command::new("pbpaste").output().map(|o| o.stdout).unwrap_or_default();
    reports.lock().unwrap().remove("clip_write");
    nav(&url("/clip"));
    click_sel("#b");
    wait_for(5, || report_get(reports, "clip_write").is_some());
    pump(300);
    let cw = report_get(reports, "clip_write").unwrap_or_default();
    let sys = std::process::Command::new("pbpaste").output().map(|o| String::from_utf8_lossy(&o.stdout).into_owned()).unwrap_or_default();
    {
        use std::io::Write;
        if let Ok(mut c) = std::process::Command::new("pbcopy").stdin(std::process::Stdio::piped()).spawn() {
            if let Some(mut i) = c.stdin.take() {
                let _ = i.write_all(&saved);
            }
            let _ = c.wait();
        }
    }
    r.add("C1.clipboard_write_api", Some(cw == "OK" && sys == "spike-clip-ok"), format!("navigator.clipboard.writeText → {cw:?}; system clipboard={sys:?} (사용자 클립보드는 복원함)"));
    with(|s| s.progress = 0.0);
    nav(&url("/hist/3"));
    let prog = with(|s| s.progress);
    r.add("N1.loading_progress_reaches_1", Some((prog - 1.0).abs() < 1e-6), format!("final on_loading_progress_change={prog}"));
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
    if with(|s| s.main.is_none()) {
        cefhost::create_main_browser("about:blank");
    }
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
    if which == "more" || which == "all" {
        suite_more(&mut rs, &reports);
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
