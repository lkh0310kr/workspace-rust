//! 두 스파이크(wry / cef)가 공유하는 로컬 테스트 사이트.
//! 페이지가 스스로 결과를 `POST /report?name=...`로 보고한다.
//! `127.0.0.1:PORT` 와 `localhost:PORT` 는 서로 다른 site 이므로 3rd-party 쿠키 테스트에 쓴다.

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};

pub type Reports = Arc<Mutex<BTreeMap<String, String>>>;

pub fn spawn(port: u16) -> Reports {
    let reports: Reports = Arc::new(Mutex::new(BTreeMap::new()));
    let listener = TcpListener::bind(("0.0.0.0", port)).expect("bind testsite");
    let r = reports.clone();
    std::thread::spawn(move || {
        for s in listener.incoming().flatten() {
            let r = r.clone();
            std::thread::spawn(move || {
                let _ = handle(s, r);
            });
        }
    });
    reports
}

struct Req {
    method: String,
    path: String,
    query: String,
    cookies: String,
    body: String,
}

fn handle(mut s: TcpStream, reports: Reports) -> std::io::Result<()> {
    let mut rd = BufReader::new(s.try_clone()?);
    let mut line = String::new();
    rd.read_line(&mut line)?;
    let mut it = line.split_whitespace();
    let method = it.next().unwrap_or("").to_string();
    let target = it.next().unwrap_or("/").to_string();
    let (path, query) = match target.split_once('?') {
        Some((p, q)) => (p.to_string(), q.to_string()),
        None => (target.clone(), String::new()),
    };
    let (mut clen, mut cookies) = (0usize, String::new());
    loop {
        let mut h = String::new();
        rd.read_line(&mut h)?;
        let h = h.trim_end();
        if h.is_empty() {
            break;
        }
        let low = h.to_ascii_lowercase();
        if let Some(v) = low.strip_prefix("content-length:") {
            clen = v.trim().parse().unwrap_or(0);
        }
        if low.starts_with("cookie:") {
            cookies = h[7..].trim().to_string();
        }
    }
    let mut body = vec![0u8; clen];
    rd.read_exact(&mut body)?;
    let req = Req {
        method,
        path,
        query,
        cookies,
        body: String::from_utf8_lossy(&body).into_owned(),
    };
    let (status, headers, html) = route(&req, &reports);
    let mut out = format!(
        "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\nCache-Control: no-store\r\n",
        html.len()
    );
    for h in headers {
        out.push_str(&h);
        out.push_str("\r\n");
    }
    out.push_str("\r\n");
    s.write_all(out.as_bytes())?;
    s.write_all(html.as_bytes())?;
    Ok(())
}

fn q(query: &str, key: &str) -> Option<String> {
    query.split('&').find_map(|kv| {
        let (k, v) = kv.split_once('=')?;
        (k == key).then(|| v.to_string())
    })
}

fn page(title: &str, body: &str) -> String {
    // 헬퍼는 본문보다 먼저 정의되어야 인라인 스크립트에서 쓸 수 있다.
    format!(
        "<!doctype html><meta charset=utf-8><title>{title}</title>\
<script>window.__report=(n,v)=>fetch('/report?name='+n,{{method:'POST',body:typeof v==='string'?v:JSON.stringify(v)}});</script>\
<style>body{{font:14px system-ui;margin:16px}}</style>{body}"
    )
}

fn route(r: &Req, reports: &Reports) -> (&'static str, Vec<String>, String) {
    match (r.method.as_str(), r.path.as_str()) {
        ("POST", "/report") => {
            let name = q(&r.query, "name").unwrap_or_default();
            reports.lock().unwrap().insert(name, r.body.clone());
            ("200 OK", vec![], "ok".into())
        }
        ("GET", "/reports") => {
            let g = reports.lock().unwrap();
            let j: Vec<String> = g.iter().map(|(k, v)| format!("{k:?}:{v:?}")).collect();
            ("200 OK", vec![], format!("{{{}}}", j.join(",")))
        }
        // ---- Z: 배경 단색 페이지. 클릭하면 report(webview_click) ----
        ("GET", "/bg") => {
            let c = q(&r.query, "color").unwrap_or("ff0000".into());
            (
                "200 OK",
                vec![],
                page(
                    "bg",
                    &format!(
                        "<style>html,body{{margin:0;height:100%;background:#{c}}}</style>\
<script>let n=0;addEventListener('click',()=>{{n++;__report('webview_clicks',''+n)}})</script>"
                    ),
                ),
            )
        }
        // ---- L1/L2: 폼 로그인 ----
        ("GET", "/login") => (
            "200 OK",
            vec![],
            page(
                "login",
                "<form method=post action=/login><input name=u value=demo><button id=go>login</button></form>",
            ),
        ),
        ("POST", "/login") => (
            "302 Found",
            vec![
                "Location: /me".into(),
                // 영속 쿠키 + 세션 쿠키 + 서버 전용 HttpOnly
                "Set-Cookie: psid=persist123; Max-Age=86400; Path=/; HttpOnly; SameSite=Lax".into(),
                "Set-Cookie: ssid=session456; Path=/; SameSite=Lax".into(),
            ],
            String::new(),
        ),
        ("GET", "/me") => {
            let has_p = r.cookies.contains("psid=persist123");
            let has_s = r.cookies.contains("ssid=session456");
            let rep = format!("{{\"psid\":{has_p},\"ssid\":{has_s}}}");
            (
                "200 OK",
                vec![],
                page(
                    "me",
                    &format!(
                        "<pre id=result>{rep}</pre><script>__report('me',{rep:?})</script>"
                    ),
                ),
            )
        }
        // ---- L3: OAuth 팝업 ----
        ("GET", "/oauth/start") => (
            "200 OK",
            vec![],
            page(
                "oauth",
                "<button id=go onclick=\"window.open('/oauth/provider','oauth','width=480,height=600')\">sign in</button>\
<pre id=result>waiting</pre>\
<script>addEventListener('message',e=>{if(e.data&&e.data.type==='oauth'){document.getElementById('result').textContent='OAUTH_OK:'+e.data.code;__report('oauth_popup','OAUTH_OK:'+e.data.code)}})</script>",
            ),
        ),
        ("GET", "/oauth/provider") => (
            "200 OK",
            vec![],
            page(
                "provider",
                "<h3>provider</h3><button id=ok>approve</button>\
<script>const go=()=>{if(window.opener){window.opener.postMessage({type:'oauth',code:'abc123'},'*');window.close()}else{__report('oauth_popup','NO_OPENER')}};\
document.getElementById('ok').onclick=go;if(location.search.includes('auto'))go();</script>",
            ),
        ),
        // ---- L4: target=_blank ----
        ("GET", "/blank") => (
            "200 OK",
            vec![],
            page("blank", "<a id=lnk target=_blank href=/blank/target>open</a>"),
        ),
        ("GET", "/blank/target") => (
            "200 OK",
            vec![],
            page("target", "<script>__report('target_blank','OPENED')</script><h3>opened</h3>"),
        ),
        // ---- L5: 3rd-party 쿠키 ----
        // 최상위 127.0.0.1 → iframe localhost. iframe 안에서 쿠키 set → 재요청 시 전송되는지.
        ("GET", "/tp") => (
            "200 OK",
            vec![],
            page(
                "tp",
                "<iframe id=f src='http://localhost:PORT/tp/frame' width=300 height=60></iframe><pre id=result>waiting</pre>\
<script>addEventListener('message',e=>{if(e.data&&e.data.tp!==undefined){document.getElementById('result').textContent='TP:'+e.data.tp;__report('third_party_cookie','TP:'+e.data.tp)}})</script>"
                    .replace("PORT", &q(&r.query, "port").unwrap_or("18080".into())).as_str(),
            ),
        ),
        ("GET", "/tp/frame") => {
            let seen = r.cookies.contains("tp=1");
            (
                "200 OK",
                vec!["Set-Cookie: tp=1; Max-Age=3600; Path=/; SameSite=None; Secure".into()],
                page(
                    "tpframe",
                    // 첫 로드에서 set, 같은 iframe을 fetch로 다시 요청해 쿠키가 실려오는지 확인
                    &format!(
                        "<script>fetch('/tp/seen',{{credentials:'include'}}).then(r=>r.text()).then(t=>parent.postMessage({{tp:t}},'*'));void {seen}</script>"
                    ),
                ),
            )
        }
        ("GET", "/tp/seen") => ("200 OK", vec![], if r.cookies.contains("tp=1") { "SENT" } else { "BLOCKED" }.into()),
        // ---- L6: UA / 자동화 표면 ----
        ("GET", "/ua") => (
            "200 OK",
            vec![],
            page(
                "ua",
                "<pre id=result></pre><script>const o={ua:navigator.userAgent,webdriver:navigator.webdriver,chrome:typeof window.chrome,uaData:typeof navigator.userAgentData,brands:navigator.userAgentData?navigator.userAgentData.brands.map(b=>b.brand):null,vendor:navigator.vendor,platform:navigator.platform,lang:navigator.language};\
document.getElementById('result').textContent=JSON.stringify(o,null,1);__report('ua',o)</script>",
            ),
        ),
        // ---- I: 입력 ----
        ("GET", "/ime") => (
            "200 OK",
            vec![],
            page(
                "ime",
                "<input id=i autofocus style='width:300px;font-size:20px'><br><textarea id=t></textarea><pre id=log></pre>\
<script>const log=[];const L=(e)=>{log.push(e.type+(e.data!==undefined&&e.data!==null?':'+e.data:(e.key?':'+e.key:'')));document.getElementById('log').textContent=log.slice(-12).join('\\n');__report('ime',{log:log,value:document.getElementById('i').value})};\
for(const t of ['compositionstart','compositionupdate','compositionend','keydown','input'])document.getElementById('i').addEventListener(t,L);\
let c=0,w=0,m=0;addEventListener('click',()=>{c++;__report('mouse',{click:c,wheel:w,move:m})});addEventListener('wheel',()=>{w++;__report('mouse',{click:c,wheel:w,move:m})},{passive:true});addEventListener('mousemove',()=>{m++},{passive:true});</script>\
<div style='height:2000px'>scroll</div>",
            ),
        ),
        // ---- U1: 스크롤 ----
        ("GET", "/scroll") => (
            "200 OK",
            vec![],
            page(
                "scroll",
                "<style>body{margin:0}</style><div style='height:5000px;background:linear-gradient(#fff,#000)'>tall</div>\
<script>let t;addEventListener('scroll',()=>{clearTimeout(t);t=setTimeout(()=>__report('scroll',''+Math.round(scrollY)),100)},{passive:true})</script>",
            ),
        ),
        // ---- U3: <select> popup widget ----
        ("GET", "/select") => (
            "200 OK",
            vec![],
            page(
                "select",
                "<select id=s style='font-size:20px;margin:20px'><option>one<option>two<option>three</select>\
<script>document.getElementById('s').addEventListener('change',e=>__report('select',e.target.value))</script>",
            ),
        ),
        (_, "/") => (
            "200 OK",
            vec![],
            page("index", "<h3>testsite</h3><a href=/login>login</a> <a href=/ua>ua</a> <a href=/ime>ime</a>"),
        ),
        _ => ("404 Not Found", vec![], "not found".into()),
    }
}
