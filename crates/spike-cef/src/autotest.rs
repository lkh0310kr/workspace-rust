//! 자동 검증 시나리오. 사람이 눈으로 보지 않고 (1) 스크린샷 픽셀 (2) 페이지가 보고한 값으로 판정한다.

use cef::*;
use crate::cefhost::{with, Which};
use crate::{native, popup_rect, url, Root, Sh, FRAME_PAD, OVERLAY, TOOLBAR_H, RADIUS};
use gpui::{App, AsyncApp, Entity};
use image::RgbaImage;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::msg_send;
use objc2_foundation::{NSRange, NSString};
use std::time::Duration;
use testsite::Reports;

#[derive(Default)]
struct Results(Vec<(String, &'static str, String)>);
impl Results {
    fn add(&mut self, id: &str, pass: Option<bool>, detail: impl Into<String>) {
        // 화면 픽셀 기반 항목: 백그라운드(투명) 모드에서는 합성 결과를 캡처할 수 없으므로 SKIP 으로 명시
        const PIXEL: [&str; 6] = ["Z0.", "Z1.", "Z4.", "Z5.", "U3b.", "L7."];
        if crate::native::background() && PIXEL.iter().any(|p| id.starts_with(p)) {
            println!("RESULT {id} SKIP pixel check needs a visible window (run with --visible)");
            self.0.push((id.to_string(), "SKIP", "requires visible window".into()));
            return;
        }
        let st = match pass {
            Some(true) => "PASS",
            Some(false) => "FAIL",
            None => "INFO",
        };
        let d = detail.into();
        println!("RESULT {id} {st} {d}");
        self.0.push((id.to_string(), st, d));
    }
}

async fn sleep(cx: &AsyncApp, ms: u64) {
    cx.background_executor().timer(Duration::from_millis(ms)).await;
}

fn shot(w: &objc2_app_kit::NSWindow, name: &str) -> (RgbaImage, native::Geom) {
    use core_graphics::geometry::{CGPoint, CGRect, CGSize};
    use core_graphics::window::{create_image, kCGWindowImageBestResolution, kCGWindowImageBoundsIgnoreFraming, kCGWindowListOptionIncludingWindow};
    native::activate(w); // 가려진 창은 macOS 가 디스플레이 링크를 멈춘다 → 캡처 전 전면화
    std::thread::sleep(Duration::from_millis(250));
    let g = native::geom(w);
    // CGRectNull = (inf, inf, 0, 0) → 창 전체
    let null = CGRect::new(&CGPoint::new(f64::INFINITY, f64::INFINITY), &CGSize::new(0.0, 0.0));
    let img = match create_image(
        null,
        kCGWindowListOptionIncludingWindow,
        g.window_number as u32,
        kCGWindowImageBoundsIgnoreFraming | kCGWindowImageBestResolution,
    ) {
        Some(cg) => {
            let (wd, ht, bpr) = (cg.width() as u32, cg.height() as u32, cg.bytes_per_row());
            let data = cg.data();
            let bytes = data.bytes();
            let mut out = RgbaImage::new(wd, ht);
            for y in 0..ht as usize {
                for x in 0..wd as usize {
                    let i = y * bpr + x * 4; // BGRA
                    out.put_pixel(x as u32, y as u32, image::Rgba([bytes[i + 2], bytes[i + 1], bytes[i], 255]));
                }
            }
            if native::background() {
                // 창 알파(STEALTH_ALPHA)가 곱해져 캡처되므로 보정
                for p in out.pixels_mut() {
                    for c in 0..3 { p.0[c] = ((p.0[c] as f64 / native::stealth_alpha()).round().min(255.0)) as u8; }
                }
            }
            let _ = std::fs::create_dir_all("target/shots");
            let _ = out.save(format!("target/shots/cef-{name}.png"));
            out
        }
        None => {
            eprintln!("CGWindowListCreateImage returned None");
            RgbaImage::new(1, 1)
        }
    };
    (img, g)
}

/// 콘텐츠 좌표(points, 좌상단 원점) 한 점의 색
fn px(img: &RgbaImage, g: &native::Geom, x: f64, y: f64) -> [u8; 3] {
    let (ix, iy) = (((x) * g.scale) as u32, ((g.titlebar + y) * g.scale) as u32);
    if ix >= img.width() || iy >= img.height() {
        return [0, 0, 0];
    }
    let p = img.get_pixel(ix, iy).0;
    [p[0], p[1], p[2]]
}
/// 색공간 변환(P3 등)에 둔감하도록 지배 채널로 판정
fn near(c: [u8; 3], t: [u8; 3]) -> bool {
    let (r, g, b) = (c[0] as i32, c[1] as i32, c[2] as i32);
    match t {
        [255, 0, 0] => r > 180 && g < 110 && b < 110,
        [0, 0, 255] => b > 180 && r < 110 && g < 140,
        [0, 170, 0] => g > 120 && r < 110 && b < 110,
        _ => false,
    }
}
const RED: [u8; 3] = [255, 0, 0];
const BLUE: [u8; 3] = [0, 0, 255];
const GREEN: [u8; 3] = [0, 170, 0];

fn responder(w: &objc2_app_kit::NSWindow) -> Retained<AnyObject> {
    let r = w.firstResponder().expect("first responder");
    unsafe { Retained::cast_unchecked(r) }
}
fn ime_set_marked(r: &AnyObject, s: &str) {
    let ns = NSString::from_str(s);
    let sel = NSRange::new(s.chars().count(), 0);
    let rep = NSRange::new(isize::MAX as usize, 0);
    unsafe { let _: () = msg_send![r, setMarkedText: &*ns, selectedRange: sel, replacementRange: rep]; }
}
fn ime_insert(r: &AnyObject, s: &str) {
    let ns = NSString::from_str(s);
    let rep = NSRange::new(isize::MAX as usize, 0);
    unsafe { let _: () = msg_send![r, insertText: &*ns, replacementRange: rep]; }
}

fn report(reports: &Reports, k: &str) -> Option<String> {
    reports.lock().unwrap().get(k).cloned()
}


fn exec(which: Which, js: &str) {
    let b = with(|s| if which == Which::Main { s.main.clone() } else { s.popup.clone() });
    if let Some(f) = b.and_then(|b| b.main_frame()) {
        f.execute_java_script(Some(&cef::CefString::from(js)), Some(&cef::CefString::from("spike://eval")), 0);
    }
}

async fn goto(cx: &AsyncApp, u: &str, wait: u64) {
    let g0 = with(|s| s.surf[0].gen);
    if let Some(f) = with(|s| s.main.clone()).and_then(|b| b.main_frame()) {
        f.load_url(Some(&cef::CefString::from(u)));
    }
    // OSR 은 변화가 있을 때만 프레임을 준다 → 새 프레임이 올 때까지 대기(최대 6s) 후 추가 안정화 대기
    for _ in 0..60 { sleep(cx, 100).await; if with(|s| s.surf[0].gen) > g0 { break; } }
    sleep(cx, wait).await;
}

/// 조건이 만족될 때까지 캡처를 반복(최대 ~6s). 마지막 캡처를 돌려준다.
async fn shot_until(cx: &AsyncApp, w: &objc2_app_kit::NSWindow, name: &str, ok: impl Fn(&RgbaImage, &native::Geom) -> bool) -> (RgbaImage, native::Geom) {
    let mut last = shot(w, name);
    for _ in 0..20 {
        if ok(&last.0, &last.1) { break; }
        sleep(cx, 300).await;
        last = shot(w, name);
    }
    last
}

/// 요소 중앙 좌표를 페이지가 보고하게 해서 가져온 뒤, 합성 클릭을 gpui 창에 주입 (실제 gpui hit-test 경로)
async fn click_sel(cx: &AsyncApp, sh: &Sh, reports: &Reports, w: &objc2_app_kit::NSWindow, which: Which, sel: &str) -> bool {
    reports.lock().unwrap().remove("q_rect");
    exec(which, &format!("(()=>{{const e=document.querySelector({sel:?});if(!e){{__report('q_rect','null');return}}const r=e.getBoundingClientRect();__report('q_rect',JSON.stringify([r.x+r.width/2,r.y+r.height/2]))}})()"));
    let mut got = None;
    for _ in 0..30 {
        sleep(cx, 100).await;
        if let Some(v) = reports.lock().unwrap().get("q_rect").cloned() { got = Some(v); break; }
    }
    let Some(s) = got else { return false };
    let xy: Vec<f64> = serde_json::from_str(&s).unwrap_or_default();
    if xy.len() != 2 { return false; }
    let b = sh.borrow().bounds.unwrap();
    let origin = if which == Which::Main { b.origin } else { popup_rect(b).origin };
    native::click(w, f32::from(origin.x) as f64 + xy[0], f32::from(origin.y) as f64 + xy[1]);
    true
}

/// 종료 직전 쿠키 DB flush. (Electron 은 내부적으로 처리하지만 CEF 임베딩에서는 앱이 책임진다)
async fn flush_cookies(cx: &AsyncApp) {
    if std::env::var("SPIKE_NO_FLUSH").is_ok() { return; }
    if let Some(m) = cef::cookie_manager_get_global_manager(None) {
        let r = m.flush_store(None);
        println!("cookie flush_store -> {r}");
    }
    sleep(cx, 1500).await;
}

/// #i 의 현재 value 를 페이지가 보고하게 해서 읽는다
async fn eval_value(cx: &AsyncApp, reports: &Reports) -> String {
    reports.lock().unwrap().remove("q_val");
    exec(Which::Main, "__report('q_val',document.getElementById('i').value)");
    for _ in 0..30 {
        sleep(cx, 100).await;
        if let Some(v) = reports.lock().unwrap().get("q_val").cloned() { return v; }
    }
    "TIMEOUT".into()
}

pub fn run(cx: &mut App, view: Entity<Root>, sh: Sh, reports: Reports) {
    cx.spawn(async move |cx| {
        let phase = std::env::var("SPIKE_PHASE").unwrap_or("1".into());
        let mut res = Results::default();
        // CEF 브라우저 생성 + 첫 프레임 대기
        for _ in 0..100 {
            sleep(cx, 100).await;
            if with(|s| s.main.is_some() && s.surf[0].gen > 0) { break; }
        }
        sleep(cx, 1500).await;
        let Some(bnd) = sh.borrow().bounds else {
            println!("RESULT SETUP FAIL no bounds");
            cx.update(|cx| cx.quit()).ok();
            return;
        };
        // 창 raw handle 은 gpui Window 에서 얻어야 한다
        let handle = cx.update(|cx| {
            let wh = cx.windows().into_iter().next().unwrap();
            let mut h = None;
            wh.update(cx, |_, window, _| { use raw_window_handle::HasWindowHandle; h = Some(window.window_handle().unwrap().as_raw()); }).ok();
            h
        }).ok().flatten().unwrap();
        let w = native::ns_window(handle);
        if native::background() {
            println!("occlusion before stealth={:?} visible={} onActiveSpace={}", w.occlusionState(), w.isVisible(), w.isOnActiveSpace());
            native::stealth(&w);
            for i in 0..4 { sleep(cx, 500).await; println!("occlusion t+{}ms={:?} visible={} onActiveSpace={} level={}", (i+1)*500, w.occlusionState(), w.isVisible(), w.isOnActiveSpace(), w.level()); }
        }
        let logs = with(|s| s.log.clone());
        res.add("SETUP", Some(with(|s| s.main.is_some())), format!("cef main browser created, frames={} log={logs:?}", with(|s| s.surf[0].gen)));

        if phase == "2" {
            goto(cx, &url("/me"), 2000).await;
            let me = report(&reports, "me").unwrap_or("NONE".into());
            let v: serde_json::Value = serde_json::from_str(&me).unwrap_or_default();
            res.add("L2.persistent_cookie", Some(v["psid"] == true), format!("after restart /me={me}"));
            res.add("L2.session_cookie_policy", None, format!("ssid kept after restart: {}", v["ssid"]));
            flush_cookies(cx).await;
            finish(cx, &res, "2");
            return;
        }

        let (bx, by, bw, bh) = (f32::from(bnd.origin.x) as f64, f32::from(bnd.origin.y) as f64, f32::from(bnd.size.width) as f64, f32::from(bnd.size.height) as f64);
        res.add("SETUP.bounds", None, format!("webview bounds=({bx},{by},{bw},{bh}) toolbar={TOOLBAR_H} pad={FRAME_PAD}"));

        // ---------------- Z1 ----------------
        goto(cx, &url("/bg?color=ff0000"), 2000).await;
        let (ox, oy, ow, oh) = (OVERLAY.0 as f64 + 200.0, OVERLAY.1 as f64, OVERLAY.2 as f64, OVERLAY.3 as f64);
        let (cxp, cyp) = (ox + ow / 2.0, oy + oh / 2.0);
        let (ctrl_x, ctrl_y) = (bx + bw - 40.0, by + bh - 40.0);
        let (img, g) = shot_until(cx, &w, "z0", |i, g| near(px(i, g, ctrl_x, ctrl_y), RED)).await;
        let ctrl = px(&img, &g, ctrl_x, ctrl_y);
        res.add("Z0.capture_sanity", Some(near(ctrl, RED)), format!("webview control pixel={ctrl:?} (red expected)"));
        view.update(cx, |r, cx| { r.overlay = true; cx.notify(); }).ok();
        sleep(cx, 800).await;
        let (img, g) = shot_until(cx, &w, "z1", |i, g| near(px(i, g, 150.0, cyp), BLUE)).await;
        let side = px(&img, &g, 150.0, cyp);
        res.add("Z1.control_overlay_rendered_over_sidebar", Some(near(side, BLUE)), format!("pixel over sidebar part={side:?}"));
        let c = px(&img, &g, cxp, cyp);
        res.add("Z1.overlay_above_webview", Some(near(c, BLUE)), format!("pixel at overlay center (over web content)={c:?}; blue=gpui on top, red=webview on top"));

        // ---------------- Z2 ----------------
        native::click(&w, ctrl_x, ctrl_y);
        sleep(cx, 700).await;
        let ctrl_clicks = report(&reports, "webview_clicks");
        res.add("Z2.control_click_reaches_webview", Some(ctrl_clicks.as_deref() == Some("1")), format!("webview_clicks={ctrl_clicks:?}"));
        native::click(&w, cxp, cyp);
        sleep(cx, 700).await;
        let (oc, wc) = (view.read_with(cx, |r, _| r.overlay_clicks).unwrap_or(99), report(&reports, "webview_clicks"));
        res.add("Z2.overlay_click_goes_to_gpui", Some(oc == 1 && wc.as_deref() == Some("1")), format!("gpui overlay_clicks={oc}, webview_clicks={wc:?} (gpui=1 & webview stays 1 expected)"));

        // ---------------- Z3 ----------------
        view.update(cx, |r, cx| { r.overlay = false; r.sidebar_w = 400.0; cx.notify(); }).ok();
        sleep(cx, 800).await;
        let (img, g) = shot_until(cx, &w, "z3", |i, g| {
            let y = by + 100.0;
            (0..g.frame_w as u32).find(|xi| near(px(i, g, *xi as f64, y), RED)).map(|l| (l as f64 - 408.0).abs() <= 2.0).unwrap_or(false)
        }).await;
        let y = by + 100.0;
        let mut left = None;
        for xi in 0..(g.frame_w as u32) {
            if near(px(&img, &g, xi as f64, y), RED) { left = Some(xi); break; }
        }
        let expect = 400.0 + FRAME_PAD as f64;
        let nb = sh.borrow().bounds.unwrap();
        let (new_w, new_h) = with(|s| s.size[0]);
        let z3_ok = if native::background() { (new_w as f64 - (bw - 200.0)).abs() <= 2.0 } else { left.map(|l| (l as f64 - expect).abs() <= 2.0).unwrap_or(false) };
        res.add("Z3.layout_sync", Some(z3_ok), format!("webview left edge x={left:?}, expected≈{expect}; browser logical size now {new_w}x{new_h} (canvas {:?}x{:?})", f32::from(nb.size.width), f32::from(nb.size.height)));

        // ---------------- Z4: 라운드 클립 (gpui paint_image corner_radii — 별도 우회 불필요) ----------------
        let (nx, ny) = (f32::from(nb.origin.x) as f64 + 1.0, f32::from(nb.origin.y) as f64 + 1.0);
        let corner = px(&img, &g, nx, ny);
        res.add("Z4.rounded_corner_clip", Some(!near(corner, RED)), format!("corner pixel={corner:?} (not red = clipped by gpui rounded mask)"));

        // ---------------- Z5 ----------------
        view.update(cx, |r, cx| { r.tab_hidden = true; cx.notify(); }).ok();
        sleep(cx, 600).await;
        let (img, g) = shot(&w, "z5a");
        let hid = px(&img, &g, ctrl_x - 150.0, ctrl_y - 100.0);
        view.update(cx, |r, cx| { r.tab_hidden = false; cx.notify(); }).ok();
        sleep(cx, 600).await;
        let (img, g) = shot(&w, "z5b");
        let shown = px(&img, &g, ctrl_x - 150.0, ctrl_y - 100.0);
        res.add("Z5.hide_show", Some(!near(hid, RED) && near(shown, RED)), format!("hidden px={hid:?}, shown px={shown:?}"));
        let _ = GREEN;
        let _ = RADIUS;

        // ---------------- L1 ----------------
        goto(cx, &url("/login"), 1500).await;
        exec(Which::Main, "document.querySelector('form').submit()");
        sleep(cx, 2000).await;
        let me = report(&reports, "me").unwrap_or("NONE".into());
        let v: serde_json::Value = serde_json::from_str(&me).unwrap_or_default();
        res.add("L1.form_login_cookie", Some(v["psid"] == true && v["ssid"] == true), format!("/me={me}"));

        // ---------------- L3: OAuth popup ----------------
        goto(cx, &url("/oauth/start"), 1500).await;
        let clicked = click_sel(cx, &sh, &reports, &w, Which::Main, "#go").await;
        let mut popup_seen = false;
        for _ in 0..40 { sleep(cx, 100).await; if with(|s| s.popup.is_some() && s.surf[1].gen > 0) { popup_seen = true; break; } }
        sleep(cx, 800).await;
        let (img, g) = shot(&w, "l3_popup");
        let _ = (img, g);
        let clicked2 = if popup_seen { click_sel(cx, &sh, &reports, &w, Which::Popup, "#ok").await } else { false };
        sleep(cx, 2000).await;
        let o = report(&reports, "oauth_popup");
        let closed = with(|s| s.popups_closed);
        res.add("L3.oauth_popup_postmessage", Some(o.as_deref().map_or(false, |s| s.starts_with("OAUTH_OK")) && closed >= 1), format!("clicked={clicked} popup_shown={popup_seen} popup_clicked={clicked2} report={o:?} popup_closed_count={closed}"));

        // ---------------- L4 ----------------
        let before = with(|s| s.popups_closed);
        goto(cx, &url("/blank"), 1500).await;
        let clicked = click_sel(cx, &sh, &reports, &w, Which::Main, "#lnk").await;
        sleep(cx, 2500).await;
        let t = report(&reports, "target_blank");
        res.add("L4.target_blank", Some(t.as_deref() == Some("OPENED")), format!("clicked={clicked} report={t:?}"));
        // 남은 팝업 정리
        if with(|s| s.popup.is_some()) { if let Some(h) = with(|s| s.popup.clone()).and_then(|b| b.host()) { h.close_browser(1); } }
        sleep(cx, 800).await;
        let _ = before;

        // ---------------- L5 ----------------
        goto(cx, &url("/tp?port=18080"), 3000).await;
        let tp = report(&reports, "third_party_cookie");
        res.add("L5.third_party_cookie", Some(tp.as_deref() == Some("TP:SENT")), format!("report={tp:?}"));

        // ---------------- L6 ----------------
        goto(cx, &url("/ua"), 2000).await;
        let ua = report(&reports, "ua").unwrap_or("NONE".into());
        let v: serde_json::Value = serde_json::from_str(&ua).unwrap_or_default();
        res.add("L6.ua_chrome_surface", Some(v["chrome"] == "object" && v["uaData"] == "object"), ua);

        // ---------------- I ----------------
        goto(cx, &url("/ime"), 2000).await;
        let _ = click_sel(cx, &sh, &reports, &w, Which::Main, "#i").await;
        sleep(cx, 600).await;
        let r = responder(&w);
        let rname = r.class().name().to_str().unwrap_or("?").to_string();
        ime_insert(&r, "abc");
        ime_set_marked(&r, "ㅎ");
        ime_set_marked(&r, "하");
        ime_set_marked(&r, "한");
        ime_insert(&r, "한");
        sleep(cx, 1000).await;
        let ime = report(&reports, "ime").unwrap_or("NONE".into());
        let v: serde_json::Value = serde_json::from_str(&ime).unwrap_or_default();
        let log = v["log"].to_string();
        let val = v["value"].as_str().unwrap_or("").to_string();
        res.add("I1.latin_text", Some(val.starts_with("abc")), format!("value={val:?}"));
        res.add("I2.korean_ime_composition", Some(log.contains("compositionstart") && log.contains("compositionupdate") && log.contains("compositionend") && val.ends_with("한")), format!("responder={rname} value={val:?} log={log}"));
        let m = report(&reports, "mouse").unwrap_or("NONE".into());
        res.add("I3.mouse_click", Some(m.contains("\"click\":1") || m.contains("\"click\":2")), format!("mouse report={m}"));

        // ---------------- U1: 휠 스크롤 ----------------
        goto(cx, &url("/scroll"), 1800).await;
        let (sx, sy) = (bx + bw / 2.0, by + bh / 2.0);
        for _ in 0..6 { native::scroll(&w, sx, sy, -60); sleep(cx, 60).await; }
        sleep(cx, 800).await;
        let down = report(&reports, "scroll");
        reports.lock().unwrap().remove("scroll");
        for _ in 0..3 { native::scroll(&w, sx, sy, 60); sleep(cx, 60).await; }
        sleep(cx, 800).await;
        let up = report(&reports, "scroll");
        let dv: f64 = down.as_deref().unwrap_or("0").parse().unwrap_or(0.0);
        let uv: f64 = up.as_deref().unwrap_or("0").parse().unwrap_or(0.0);
        res.add("U1.wheel_scroll", Some(dv > 50.0 && uv < dv), format!("after 6x(-60px) scrollY={down:?}; after 3x(+60px) scrollY={up:?}"));

        // ---------------- U2: 일반 문자 keydown / 단축키 라우팅(I4) ----------------
        goto(cx, &url("/ime"), 1800).await;
        let _ = click_sel(cx, &sh, &reports, &w, Which::Main, "#i").await;
        sleep(cx, 500).await;
        native::key(&w, "h", 4, 0);
        native::key(&w, "i", 34, 0);
        for _ in 0..30 { sleep(cx, 100).await; if with(|s| s.log.iter().any(|l| l.contains("key=\"i\""))) { break; } }
        sleep(cx, 1200).await;
        let ime = report(&reports, "ime").unwrap_or("NONE".into());
        let v: serde_json::Value = serde_json::from_str(&ime).unwrap_or_default();
        let log = v["log"].to_string();
        let val = v["value"].as_str().unwrap_or("").to_string();
        res.add("U2.plain_char_keydown_and_text", Some(log.contains("keydown:h") && log.contains("keydown:i") && val == "hi"), format!("value={val:?} log={log} handler_calls={:?}", with(|s| s.log.iter().filter(|l| l.starts_with("key_down")).cloned().collect::<Vec<_>>())));
        // Cmd+A, Cmd+C → 시스템 클립보드 (사용자 클립보드는 백업 후 마지막에 복원)
        let saved_clip = std::process::Command::new("pbpaste").output().map(|o| o.stdout).unwrap_or_default();
        let _ = std::process::Command::new("sh").args(["-c", "printf 'SENTINEL' | pbcopy"]).status();
        native::key(&w, "a", 0, 1 << 20);
        native::key(&w, "c", 8, 1 << 20);
        sleep(cx, 600).await;
        let clip = std::process::Command::new("pbpaste").output().map(|o| String::from_utf8_lossy(&o.stdout).into_owned()).unwrap_or_default();
        res.add("U2.cmd_a_cmd_c_clipboard", Some(clip == "hi"), format!("clipboard after Cmd+A,Cmd+C={clip:?} (sentinel was SENTINEL)"));
        // Cmd+V (선택 상태 → 대체)
        let _ = std::process::Command::new("sh").args(["-c", "printf 'PASTED' | pbcopy"]).status();
        native::key(&w, "v", 9, 1 << 20);
        sleep(cx, 800).await;
        let val2 = eval_value(cx, &reports).await;
        res.add("U2.cmd_v_paste", Some(val2 == "PASTED"), format!("input value after Cmd+V={val2:?}"));
        // gpui 키바인딩(Cmd+K)이 웹뷰 포커스 상태에서도 gpui 로 가는가
        let before = view.read_with(cx, |r, _| r.action_toggles).unwrap_or(99);
        reports.lock().unwrap().remove("ime");
        native::key(&w, "k", 40, 1 << 20);
        sleep(cx, 600).await;
        let after = view.read_with(cx, |r, _| r.action_toggles).unwrap_or(99);
        let ime = report(&reports, "ime").unwrap_or_default();
        res.add("I4.gpui_shortcut_while_web_focused", Some(after == before + 1 && !ime.contains("keydown:k")), format!("gpui action count {before}->{after}; page saw k keydown: {}", ime.contains("keydown:k")));
        view.update(cx, |r, cx| { r.overlay = false; cx.notify(); }).ok();
        {
            use std::io::Write;
            if let Ok(mut c) = std::process::Command::new("pbcopy").stdin(std::process::Stdio::piped()).spawn() {
                if let Some(mut i) = c.stdin.take() { let _ = i.write_all(&saved_clip); }
                let _ = c.wait();
            }
        }

        // ---------------- U3: <select> popup widget ----------------
        goto(cx, &url("/select"), 1800).await;
        let _ = click_sel(cx, &sh, &reports, &w, Which::Main, "#s").await;
        let mut shown = false;
        for _ in 0..30 { sleep(cx, 100).await; if with(|s| s.widget_visible && s.surf[2].gen > 0) { shown = true; break; } }
        sleep(cx, 400).await;
        let wr = with(|s| s.widget);
        let (img, g) = shot(&w, "u3_select");
        let bgpx = px(&img, &g, bx + 400.0, by + 400.0);
        res.add("U3b.default_background_opaque_white", Some(bgpx.iter().all(|c| *c > 235)), format!("page without CSS background pixel={bgpx:?} (white expected; green = transparent OSR bg leaks gpui)"));
        res.add("U3.select_popup_rendered", Some(shown && wr.2 > 0 && wr.3 > 0), format!("widget_visible={shown} rect={wr:?}"));
        if shown {
            // 두 번째 항목(대략 높이 3등분 중 가운데) 클릭
            let b = sh.borrow().bounds.unwrap();
            let tx = f32::from(b.origin.x) as f64 + wr.0 as f64 + wr.2 as f64 / 2.0;
            let ty = f32::from(b.origin.y) as f64 + wr.1 as f64 + wr.3 as f64 * 0.5;
            native::click(&w, tx, ty);
            sleep(cx, 900).await;
        }
        let sel = report(&reports, "select");
        let closed = !with(|s| s.widget_visible);
        res.add("U3.select_choose_option", Some(sel.as_deref() == Some("two") && closed), format!("change report={sel:?} popup_closed={closed}"));

        // ---------------- P: 성능 ----------------
        let (pc, pus) = with(|s| (s.paint_count, s.paint_copy_us));
        let (gp, gus) = { let s = sh.borrow(); (s.paints, s.paint_us) };
        res.add("P2.frame_pipeline_cost", None, format!("on_paint callbacks={pc} avg copy={:.2}ms; gpui paint closures={gp} avg RenderImage+paint_image build={:.2}ms (GPU upload excluded)", pus as f64 / pc.max(1) as f64 / 1000.0, gus as f64 / gp.max(1) as f64 / 1000.0));
        let logs = with(|s| s.log.clone());
        res.add("LOG", None, format!("{logs:?}"));

        if std::env::var("SPIKE_GOOGLE").is_ok() {
            goto(cx, "https://accounts.google.com/signin/v2/identifier?hl=en", 6000).await;
            let _ = shot(&w, "l7_google");
            res.add("L7.google_signin_page", None, "screenshot target/shots/cef-l7_google.png (manual review)");
        }
        flush_cookies(cx).await;
        finish(cx, &res, "1");
    })
    .detach();
}

fn finish(cx: &AsyncApp, res: &Results, phase: &str) {
    let j: Vec<_> = res
        .0
        .iter()
        .map(|(id, st, d)| serde_json::json!({"id": id, "status": st, "detail": d}))
        .collect();
    let dir = std::path::Path::new("target/results");
    let _ = std::fs::create_dir_all(dir);
    let _ = std::fs::write(dir.join(format!("cef-phase{phase}.json")), serde_json::to_string_pretty(&j).unwrap());
    println!("DONE phase {phase}");
    cx.update(|cx| cx.quit()).ok();
}
