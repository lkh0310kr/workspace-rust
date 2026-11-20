//! 자동 검증 시나리오. 사람이 눈으로 보지 않고 (1) 스크린샷 픽셀 (2) 페이지가 보고한 값으로 판정한다.

use crate::{native, url, Root, Sh, FRAME_PAD, OVERLAY, TOOLBAR_H};
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
            let _ = std::fs::create_dir_all("target/shots");
            let _ = out.save(format!("target/shots/wry-{name}.png"));
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

fn eval(sh: &Sh, js: &str) {
    if let Some(wv) = sh.borrow().wv.as_ref() {
        let _ = wv.evaluate_script(js);
    }
}

/// JS 값을 받아온다 (wry evaluate_script_with_callback)
async fn eval_cb(cx: &AsyncApp, sh: &Sh, js: &str) -> String {
    let slot = std::sync::Arc::new(std::sync::Mutex::new(None::<String>));
    let s2 = slot.clone();
    if let Some(wv) = sh.borrow().wv.as_ref() {
        let _ = wv.evaluate_script_with_callback(js, move |r| *s2.lock().unwrap() = Some(r));
    }
    for _ in 0..50 {
        if let Some(v) = slot.lock().unwrap().clone() {
            return v;
        }
        sleep(cx, 100).await;
    }
    "TIMEOUT".into()
}

async fn goto(cx: &AsyncApp, sh: &Sh, u: &str, wait: u64) {
    if let Some(wv) = sh.borrow().wv.as_ref() {
        let _ = wv.load_url(u);
    }
    sleep(cx, wait).await;
}

/// 셀렉터 요소 중앙을 네이티브 합성 클릭 (사용자 제스처로 인정되도록)
async fn click_sel(cx: &AsyncApp, sh: &Sh, w: &objc2_app_kit::NSWindow, sel: &str) -> bool {
    let r = eval_cb(
        cx,
        sh,
        &format!("(()=>{{const e=document.querySelector({sel:?});if(!e)return null;const r=e.getBoundingClientRect();return JSON.stringify([r.x+r.width/2,r.y+r.height/2])}})()"),
    )
    .await;
    let v: serde_json::Value = serde_json::from_str(&r).unwrap_or_default();
    let Some(s) = v.as_str() else { return false };
    let xy: Vec<f64> = serde_json::from_str(s).unwrap_or_default();
    if xy.len() != 2 {
        return false;
    }
    let b = sh.borrow().bounds.unwrap();
    native::click(
        w,
        f32::from(b.origin.x) as f64 + xy[0],
        f32::from(b.origin.y) as f64 + xy[1],
    );
    true
}

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

pub fn run(cx: &mut App, view: Entity<Root>, sh: Sh, reports: Reports) {
    cx.spawn(async move |cx| {
        let phase = std::env::var("SPIKE_PHASE").unwrap_or("1".into());
        let mut res = Results::default();
        sleep(cx, 2500).await;
        let Some(handle) = sh.borrow().handle else {
            println!("RESULT SETUP FAIL no window handle");
            cx.update(|cx| cx.quit()).ok();
            return;
        };
        let w = native::ns_window(handle);

        if phase == "2" {
            // 재시작 후: 영속 쿠키가 남아 있는가
            goto(cx, &sh, &url("/me"), 1500).await;
            let me = report(&reports, "me").unwrap_or("NONE".into());
            let v: serde_json::Value = serde_json::from_str(&me).unwrap_or_default();
            res.add("L2.persistent_cookie", Some(v["psid"] == true), format!("after restart /me={me}"));
            res.add("L2.session_cookie_policy", None, format!("ssid kept after restart: {}", v["ssid"]));
            finish(cx, &res, "2");
            return;
        }

        let b = sh.borrow().bounds.unwrap();
        let (bx, by, bw, bh) = (
            f32::from(b.origin.x) as f64,
            f32::from(b.origin.y) as f64,
            f32::from(b.size.width) as f64,
            f32::from(b.size.height) as f64,
        );
        res.add("SETUP", None, format!("webview bounds=({bx},{by},{bw},{bh}) toolbar={TOOLBAR_H} pad={FRAME_PAD}"));

        // ---------------- Z1: gpui overlay vs 네이티브 웹뷰 ----------------
        goto(cx, &sh, &url("/bg?color=ff0000"), 1500).await;
        let (_, ox, oy, ow, oh) = (0, OVERLAY.0 as f64 + 200.0, OVERLAY.1 as f64, OVERLAY.2 as f64, OVERLAY.3 as f64);
        let (cxp, cyp) = (ox + ow / 2.0, oy + oh / 2.0);
        let (ctrl_x, ctrl_y) = (bx + bw - 40.0, by + bh - 40.0);
        let (img, g) = shot(&w, "z0");
        let ctrl = px(&img, &g, ctrl_x, ctrl_y);
        res.add("Z0.capture_sanity", Some(near(ctrl, RED)), format!("webview control pixel={ctrl:?} (red expected)"));
        view.update(cx, |r, cx| { r.overlay = true; cx.notify(); }).ok();
        sleep(cx, 800).await;
        let (img, g) = shot(&w, "z1");
        let c = px(&img, &g, cxp, cyp);
        let side = px(&img, &g, 150.0, cyp);
        res.add("Z1.control_overlay_rendered_over_sidebar", Some(near(side, BLUE)), format!("pixel over sidebar part={side:?} (blue expected: proves overlay is drawn)"));
        res.add(
            "Z1.overlay_above_webview",
            Some(near(c, BLUE)),
            format!("pixel at overlay center={c:?}; blue=gpui on top, red=native webview on top"),
        );

        // ---------------- Z2: 클릭 라우팅 ----------------
        native::click(&w, ctrl_x, ctrl_y);
        sleep(cx, 500).await;
        let ctrl_clicks = report(&reports, "webview_clicks");
        res.add("Z2.control_click_reaches_webview", Some(ctrl_clicks.as_deref() == Some("1")), format!("webview_clicks={ctrl_clicks:?}"));
        native::click(&w, cxp, cyp);
        sleep(cx, 500).await;
        let (oc, wc) = (view.read_with(cx, |r, _| r.overlay_clicks).unwrap_or(99), report(&reports, "webview_clicks"));
        res.add(
            "Z2.overlay_click_goes_to_gpui",
            Some(oc == 1 && wc.as_deref() == Some("1")),
            format!("gpui overlay_clicks={oc}, webview_clicks={wc:?} (gpui=1 & webview stays 1 expected)"),
        );

        // ---------------- Z3: 레이아웃 동기화 ----------------
        view.update(cx, |r, cx| { r.overlay = false; r.sidebar_w = 400.0; cx.notify(); }).ok();
        sleep(cx, 800).await;
        let (img, g) = shot(&w, "z3");
        let y = by + 100.0;
        let mut left = None;
        for xi in 0..(g.frame_w as u32) {
            if near(px(&img, &g, xi as f64, y), RED) {
                left = Some(xi);
                break;
            }
        }
        let expect = 400.0 + FRAME_PAD as f64;
        res.add(
            "Z3.layout_sync",
            Some(left.map(|l| (l as f64 - expect).abs() <= 2.0).unwrap_or(false)),
            format!("webview left edge x={left:?}, expected≈{expect}"),
        );

        // ---------------- Z4: 라운드 클립 (네이티브 CALayer 우회) ----------------
        let nb = sh.borrow().bounds.unwrap();
        let (nx, ny) = (f32::from(nb.origin.x) as f64 + 1.0, f32::from(nb.origin.y) as f64 + 1.0);
        let (img, g) = shot(&w, "z4a");
        let before = px(&img, &g, nx, ny);
        sh.borrow_mut().radius = Some(12.0);
        view.update(cx, |_, cx| cx.notify()).ok();
        sleep(cx, 800).await;
        let (img, g) = shot(&w, "z4b");
        let after = px(&img, &g, nx, ny);
        res.add("Z4.rounded_corner_native_layer", Some(near(before, RED) && !near(after, RED)),
            format!("corner pixel before radius={before:?} (red = squared), after CALayer.cornerRadius={after:?}"));

        // ---------------- Z5: hide/show ----------------
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

        // ---------------- L1: 폼 로그인 ----------------
        goto(cx, &sh, &url("/login"), 1200).await;
        eval(&sh, "document.querySelector('form').submit()");
        sleep(cx, 1500).await;
        let me = report(&reports, "me").unwrap_or("NONE".into());
        let v: serde_json::Value = serde_json::from_str(&me).unwrap_or_default();
        res.add("L1.form_login_cookie", Some(v["psid"] == true && v["ssid"] == true), format!("/me={me}"));

        // ---------------- L3: OAuth popup ----------------
        goto(cx, &sh, &url("/oauth/start"), 1200).await;
        let clicked = click_sel(cx, &sh, &w, "#go").await;
        sleep(cx, 2500).await;
        let o = report(&reports, "oauth_popup");
        res.add("L3.oauth_popup_postmessage", Some(o.as_deref().map_or(false, |s| s.starts_with("OAUTH_OK"))), format!("clicked={clicked} report={o:?}"));

        // ---------------- L4: target=_blank ----------------
        goto(cx, &sh, &url("/blank"), 1200).await;
        let clicked = click_sel(cx, &sh, &w, "#lnk").await;
        sleep(cx, 2000).await;
        let t = report(&reports, "target_blank");
        res.add("L4.target_blank", Some(t.as_deref() == Some("OPENED")), format!("clicked={clicked} report={t:?}"));

        // ---------------- L5: 3rd-party cookie ----------------
        goto(cx, &sh, &url("/tp?port=18080"), 2500).await;
        let tp = report(&reports, "third_party_cookie");
        res.add("L5.third_party_cookie", Some(tp.as_deref() == Some("TP:SENT")), format!("report={tp:?} (Chromium allows; WebKit ITP typically blocks)"));

        // ---------------- L6: UA / 자동화 표면 ----------------
        goto(cx, &sh, &url("/ua"), 1500).await;
        let ua = report(&reports, "ua").unwrap_or("NONE".into());
        let v: serde_json::Value = serde_json::from_str(&ua).unwrap_or_default();
        let chromeish = v["chrome"] == "object";
        res.add("L6.ua_chrome_surface", Some(chromeish), format!("{ua}"));

        // ---------------- I: 입력 ----------------
        goto(cx, &sh, &url("/ime"), 1500).await;
        let _ = click_sel(cx, &sh, &w, "#i").await;
        sleep(cx, 500).await;
        let r = responder(&w);
        ime_insert(&r, "abc");
        ime_set_marked(&r, "ㅎ");
        ime_set_marked(&r, "하");
        ime_set_marked(&r, "한");
        ime_insert(&r, "한");
        sleep(cx, 800).await;
        let ime = report(&reports, "ime").unwrap_or("NONE".into());
        let v: serde_json::Value = serde_json::from_str(&ime).unwrap_or_default();
        let log = v["log"].to_string();
        let val = v["value"].as_str().unwrap_or("").to_string();
        res.add("I1.latin_text", Some(val.starts_with("abc")), format!("value={val:?}"));
        res.add(
            "I2.korean_ime_composition",
            Some(log.contains("compositionstart") && log.contains("compositionupdate") && log.contains("compositionend") && val.ends_with("한")),
            format!("responder={} value={val:?} log={log}", r.class().name().to_str().unwrap_or("?")),
        );

        // ---------------- L7: 실제 Google 로그인 페이지 ----------------
        if std::env::var("SPIKE_GOOGLE").is_ok() {
            goto(cx, &sh, "https://accounts.google.com/signin/v2/identifier?hl=en", 5000).await;
            let t = eval_cb(cx, &sh, "document.body.innerText.slice(0,300)").await;
            let u = eval_cb(cx, &sh, "location.href").await;
            res.add("L7.google_signin_page", Some(!t.to_lowercase().contains("not secure")), format!("url={u} text={t}"));
        }

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
    let _ = std::fs::write(dir.join(format!("wry-phase{phase}.json")), serde_json::to_string_pretty(&j).unwrap());
    println!("DONE phase {phase}");
    cx.update(|cx| cx.quit()).ok();
}
