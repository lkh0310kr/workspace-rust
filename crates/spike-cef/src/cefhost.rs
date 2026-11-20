//! CEF(Chromium) off-screen rendering 호스트.
//! 모든 콜백은 메인(UI) 스레드에서 불린다(external_message_pump + do_message_loop_work) → thread_local 로 상태 공유.

use cef::{rc::Rc, *};
use std::cell::RefCell;
use std::time::Instant;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Which {
    Main = 0,
    Popup = 1,
}

#[derive(Default)]
pub struct Surf {
    pub data: Vec<u8>, // BGRA
    pub w: u32,
    pub h: u32,
    pub gen: u64,
}

pub struct St {
    pub main: Option<Browser>,
    pub popup: Option<Browser>,
    pub surf: [Surf; 3],
    /// <select> 등 popup widget (surf[2]) 의 위치/표시 상태 (논리 좌표, 메인 뷰 기준)
    pub widget: (i32, i32, i32, i32),
    pub widget_visible: bool,
    /// 논리 크기(DIP)
    pub size: [(f32, f32); 2],
    pub scale: f32,
    pub log: Vec<String>,
    pub popups_opened: u32,
    pub popups_closed: u32,
    pub ready: bool,
    pub paint_count: u64,
    pub paint_copy_us: u128,
    pub create_requested: bool,
    /// 0=arrow 1=hand 2=ibeam 3=cross (CEF cursor type → gpui CursorStyle)
    pub cursor: u8,
    /// 컨텍스트 메뉴 요청(논리 좌표, 메인 뷰 기준) + 링크 URL
    pub ctx: Option<(i32, i32, String)>,
    pub ui_gen: u64,
    /// 하니스가 쓰지 않는 입력 경로(조합 입력)로 들어온 횟수 = 사용자 간섭 감지용
    pub foreign_ime: u64,
    pub title: String,
    /// 입력 계측: gpui→CEF 로 전달한 이벤트 수 / 마지막 위치
    pub moves: u64,
    pub last_move: (i32, i32, u32),
    pub right_clicks: u64,
    pub downloads: Vec<String>,
    pub upload_path: Option<String>,
}

impl Default for St {
    fn default() -> Self {
        St {
            main: None,
            popup: None,
            surf: [Surf::default(), Surf::default(), Surf::default()],
            widget: (0, 0, 0, 0),
            widget_visible: false,
            size: [(800.0, 600.0), (480.0, 600.0)],
            scale: 2.0,
            log: vec![],
            popups_opened: 0,
            popups_closed: 0,
            ready: false,
            paint_count: 0,
            paint_copy_us: 0,
            create_requested: false,
            cursor: 0,
            ctx: None,
            ui_gen: 0,
            foreign_ime: 0,
            title: String::new(),
            moves: 0,
            last_move: (0, 0, 0),
            right_clicks: 0,
            downloads: vec![],
            upload_path: std::env::var("SPIKE_UPLOAD_FILE").ok(),
        }
    }
}

thread_local! {
    pub static ST: RefCell<St> = RefCell::new(St::default());
}

pub fn with<R>(f: impl FnOnce(&mut St) -> R) -> R {
    ST.with(|s| f(&mut s.borrow_mut()))
}

// ---------------------------------------------------------------- render handler
wrap_render_handler! {
    pub struct RH {
        which: Which,
    }
    impl RenderHandler {
        fn view_rect(&self, _b: Option<&mut Browser>, rect: Option<&mut Rect>) {
            if let Some(rect) = rect {
                let (w, h) = with(|s| s.size[self.which as usize]);
                rect.x = 0;
                rect.y = 0;
                rect.width = (w as i32).max(1);
                rect.height = (h as i32).max(1);
            }
        }

        fn on_popup_show(&self, _b: Option<&mut Browser>, show: ::std::os::raw::c_int) {
            with(|s| {
                s.widget_visible = show != 0;
                if show == 0 {
                    s.surf[2] = Surf::default();
                }
                s.log.push(format!("on_popup_show {show}"));
            });
        }

        fn on_popup_size(&self, _b: Option<&mut Browser>, rect: Option<&Rect>) {
            if let Some(r) = rect {
                with(|s| {
                    s.widget = (r.x, r.y, r.width, r.height);
                    s.log.push(format!("on_popup_size {},{} {}x{}", r.x, r.y, r.width, r.height));
                });
            }
        }

        fn screen_info(&self, _b: Option<&mut Browser>, info: Option<&mut ScreenInfo>) -> ::std::os::raw::c_int {
            if let Some(info) = info {
                info.device_scale_factor = with(|s| s.scale);
                return true as _;
            }
            false as _
        }

        fn on_paint(
            &self,
            _b: Option<&mut Browser>,
            type_: PaintElementType,
            _dirty: Option<&[Rect]>,
            buffer: *const u8,
            width: ::std::os::raw::c_int,
            height: ::std::os::raw::c_int,
        ) {
            if buffer.is_null() || width <= 0 || height <= 0 {
                return;
            }
            let is_widget = type_ == PaintElementType::POPUP;
            if !is_widget && type_ != PaintElementType::default() {
                return;
            }
            let t = Instant::now();
            let len = (width * height * 4) as usize;
            let src = unsafe { std::slice::from_raw_parts(buffer, len) };
            with(|s| {
                let f = &mut s.surf[if is_widget { 2 } else { self.which as usize }];
                f.data.clear();
                f.data.extend_from_slice(src);
                f.w = width as u32;
                f.h = height as u32;
                f.gen += 1;
                s.paint_count += 1;
                s.paint_copy_us += t.elapsed().as_micros();
            });
        }
    }
}

// ---------------------------------------------------------------- life span (popup 포함)
wrap_life_span_handler! {
    pub struct LSH {
        which: Which,
    }
    impl LifeSpanHandler {
        fn on_before_popup(
            &self,
            _browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            _popup_id: ::std::os::raw::c_int,
            target_url: Option<&CefString>,
            _target_frame_name: Option<&CefString>,
            _disposition: WindowOpenDisposition,
            _user_gesture: ::std::os::raw::c_int,
            features: Option<&PopupFeatures>,
            window_info: Option<&mut WindowInfo>,
            client: Option<&mut Option<Client>>,
            _settings: Option<&mut BrowserSettings>,
            _extra_info: Option<&mut Option<DictionaryValue>>,
            _no_js_access: Option<&mut ::std::os::raw::c_int>,
        ) -> ::std::os::raw::c_int {
            let url = target_url.map(|u| u.to_string()).unwrap_or_default();
            let (pw, ph) = features.map(|f| (f.width, f.height)).unwrap_or((0, 0));
            with(|s| {
                s.log.push(format!("on_before_popup url={url} features={pw}x{ph}"));
                s.popups_opened += 1;
                let (w, h) = if pw > 0 && ph > 0 { (pw as f32, ph as f32) } else { (480.0, 600.0) };
                s.size[1] = (w, h);
            });
            // 팝업도 windowless 로 만들어 gpui 가 그리게 한다. window.opener 관계는 CEF 가 유지한다.
            if let Some(wi) = window_info {
                wi.windowless_rendering_enabled = 1;
                match std::env::var("SPIKE_RUNTIME").as_deref() {
                    Ok("alloy") => wi.runtime_style = RuntimeStyle::ALLOY,
                    Ok("chrome") => wi.runtime_style = RuntimeStyle::CHROME,
                    _ => {}
                }
            }
            if let Some(c) = client {
                *c = Some(make_client(Which::Popup));
            }
            0 // 0 = 팝업 허용
        }

        fn on_after_created(&self, browser: Option<&mut Browser>) {
            let Some(b) = browser else { return };
            let is_popup = b.is_popup() == 1;
            let b = b.clone();
            with(|s| {
                if is_popup {
                    s.popup = Some(b);
                } else {
                    s.main = Some(b);
                }
            });
        }

        fn on_before_close(&self, browser: Option<&mut Browser>) {
            let Some(b) = browser else { return };
            if b.is_popup() == 1 {
                with(|s| {
                    s.popup = None;
                    s.popups_closed += 1;
                    s.surf[1] = Surf::default();
                    s.log.push("popup closed".into());
                });
            }
        }
    }
}

// ---------------------------------------------------------------- display (콘솔 로그)
wrap_display_handler! {
    pub struct DH {
        which: Which,
    }
    impl DisplayHandler {
        fn on_console_message(
            &self,
            _b: Option<&mut Browser>,
            _level: LogSeverity,
            message: Option<&CefString>,
            source: Option<&CefString>,
            line: ::std::os::raw::c_int,
        ) -> ::std::os::raw::c_int {
            let m = message.map(|m| m.to_string()).unwrap_or_default();
            let s = source.map(|m| m.to_string()).unwrap_or_default();
            with(|st| st.log.push(format!("[console:{:?}] {m} ({s}:{line})", self.which)));
            0
        }

        fn on_title_change(&self, _b: Option<&mut Browser>, title: Option<&CefString>) {
            let t = title.map(|t| t.to_string()).unwrap_or_default();
            with(|s| s.title = t);
        }

        fn on_cursor_change(
            &self,
            _b: Option<&mut Browser>,
            _cursor: *mut u8,
            type_: CursorType,
            _info: Option<&CursorInfo>,
        ) -> ::std::os::raw::c_int {
            let k = if type_ == CursorType::HAND { 1 } else if type_ == CursorType::IBEAM { 2 } else if type_ == CursorType::CROSS { 3 } else { 0 };
            with(|st| {
                st.log.push(format!("cursor_change {:?} -> {k}", type_));
                if st.cursor != k {
                    st.cursor = k;
                    st.ui_gen += 1;
                }
            });
            0
        }
    }
}

// ---------------------------------------------------------------- context menu: 네이티브 메뉴 대신 gpui 가 그린다
wrap_context_menu_handler! {
    pub struct CMH {}
    impl ContextMenuHandler {
        fn on_before_context_menu(
            &self,
            _b: Option<&mut Browser>,
            _f: Option<&mut Frame>,
            params: Option<&mut ContextMenuParams>,
            model: Option<&mut MenuModel>,
        ) {
            if let Some(m) = model {
                m.clear(); // CEF 기본 메뉴 항목 제거
            }
            if let Some(p) = params {
                let link = CefString::from(&p.link_url()).to_string();
                let (x, y) = (p.xcoord(), p.ycoord());
                with(|s| {
                    s.ctx = Some((x, y, link));
                    s.ui_gen += 1;
                    s.log.push(format!("context_menu at {x},{y}"));
                });
            }
        }
    }
}

// ---------------------------------------------------------------- download
wrap_download_handler! {
    pub struct DLH {}
    impl DownloadHandler {
        fn can_download(&self, _b: Option<&mut Browser>, _url: Option<&CefString>, _m: Option<&CefString>) -> ::std::os::raw::c_int {
            1
        }

        fn on_before_download(
            &self,
            _b: Option<&mut Browser>,
            _item: Option<&mut DownloadItem>,
            suggested_name: Option<&CefString>,
            callback: Option<&mut BeforeDownloadCallback>,
        ) -> ::std::os::raw::c_int {
            let name = suggested_name.map(|n| n.to_string()).unwrap_or_else(|| "download".into());
            let dir = std::env::current_dir().unwrap().join("target/downloads");
            let _ = std::fs::create_dir_all(&dir);
            let path = dir.join(&name);
            with(|s| s.downloads.push(format!("start {name}")));
            if let Some(cb) = callback {
                // show_dialog=0: 앱이 경로를 정한다 (실제 앱에서는 저장 대화상자로 대체)
                cb.cont(Some(&CefString::from(path.to_string_lossy().as_ref())), 0);
            }
            1
        }

        fn on_download_updated(&self, _b: Option<&mut Browser>, item: Option<&mut DownloadItem>, _cb: Option<&mut DownloadItemCallback>) {
            if let Some(i) = item {
                if i.is_complete() == 1 {
                    let n = i.received_bytes();
                    with(|s| s.downloads.push(format!("complete {n} bytes")));
                }
            }
        }
    }
}

// ---------------------------------------------------------------- file chooser (<input type=file>)
wrap_dialog_handler! {
    pub struct DGH {}
    impl DialogHandler {
        fn on_file_dialog(
            &self,
            _b: Option<&mut Browser>,
            _mode: FileDialogMode,
            _title: Option<&CefString>,
            _default: Option<&CefString>,
            _filters: Option<&mut CefStringList>,
            _exts: Option<&mut CefStringList>,
            _descs: Option<&mut CefStringList>,
            callback: Option<&mut FileDialogCallback>,
        ) -> ::std::os::raw::c_int {
            let path = with(|s| {
                s.log.push("on_file_dialog".into());
                s.upload_path.clone()
            });
            let Some(cb) = callback else { return 0 };
            match path {
                Some(p) => {
                    let mut l = CefStringList::new();
                    l.append(&p);
                    cb.cont(Some(&mut l));
                }
                None => cb.cancel(), // 실제 앱: 네이티브 열기 대화상자로 대체
            }
            1
        }
    }
}

wrap_client! {
    pub struct CB {
        which: Which,
    }
    impl Client {
        fn render_handler(&self) -> Option<RenderHandler> {
            Some(RH::new(self.which))
        }
        fn life_span_handler(&self) -> Option<LifeSpanHandler> {
            Some(LSH::new(self.which))
        }
        fn display_handler(&self) -> Option<DisplayHandler> {
            Some(DH::new(self.which))
        }
        fn context_menu_handler(&self) -> Option<ContextMenuHandler> {
            Some(CMH::new())
        }
        fn download_handler(&self) -> Option<DownloadHandler> {
            Some(DLH::new())
        }
        fn dialog_handler(&self) -> Option<DialogHandler> {
            Some(DGH::new())
        }
        fn on_process_message_received(
            &self,
            _b: Option<&mut Browser>,
            frame: Option<&mut Frame>,
            _src: ProcessId,
            message: Option<&mut ProcessMessage>,
        ) -> ::std::os::raw::c_int {
            match (frame, message) {
                (Some(f), Some(m)) => crate::ipc_browser::handle_process_message(f, m) as i32,
                _ => 0,
            }
        }
    }
}

pub fn make_client(which: Which) -> Client {
    CB::new(which)
}

// ---------------------------------------------------------------- app / browser process handler
wrap_browser_process_handler! {
    pub struct BPH {}
    impl BrowserProcessHandler {
        fn on_context_initialized(&self) {
            with(|s| s.ready = true);
        }

        // 자식(GPU/Renderer 등) 프로세스에 전달되는 커맨드라인 기록: 내가 넣은 disable-features 가 실제 반영됐는지 확인용
        fn on_before_child_process_launch(&self, command_line: Option<&mut CommandLine>) {
            if let Some(cl) = command_line {
                let line = CefString::from(&cl.command_line_string()).to_string();
                let home = std::env::var("HOME").unwrap_or_default();
                let _ = std::fs::OpenOptions::new().create(true).append(true)
                    .open(format!("{home}/Library/Logs/spike-cef/child-cmdline.log"))
                    .and_then(|mut f| { use std::io::Write; writeln!(f, "{line}\n") });
            }
        }
    }
}

wrap_app! {
    pub struct AppB {}
    impl App {
        fn on_before_command_line_processing(
            &self,
            _process_type: Option<&CefString>,
            command_line: Option<&mut CommandLine>,
        ) {
            let Some(cl) = command_line else { return };
            // 개발 중 키체인 프롬프트 방지 (Electron 개발 모드와 동일한 관행)
            cl.append_switch(Some(&"use-mock-keychain".into()));
            cl.append_switch(Some(&"no-startup-window".into()));
            // Google 로그인(비밀번호 단계)에서 passkey/iCloud 키체인 인증이 시작되며 OSR(네이티브 anchor 없음)에서 크래시하는 문제의 가설 검증:
            // iCloud 키체인 passkey 경로를 끈다. 되돌려 재현하려면 SPIKE_ENABLE_ICLOUD_PASSKEY=1
            if std::env::var("SPIKE_ENABLE_ICLOUD_PASSKEY").is_err() {
                cl.append_switch_with_value(
                    Some(&"disable-features".into()),
                    Some(&"WebAuthenticationICloudKeychainForActiveWithDrive,WebAuthenticationICloudKeychainForActiveWithoutDrive,WebAuthenticationICloudKeychainForInactiveWithDrive,WebAuthenticationICloudKeychainForInactiveWithoutDrive,WebAuthenticationICloudKeychainForGoogle".into()),
                );
            }
            // 로그인 크래시 조사용: WebAuthn/passkey/키체인 관련 상세 로그
            cl.append_switch(Some(&"enable-logging".into()));
            cl.append_switch_with_value(Some(&"vmodule".into()), Some(&"*webauthn*=2,*authenticator*=2,*touch_id*=2,*keychain*=2,*fido*=2".into()));
        }
        fn on_register_custom_schemes(&self, registrar: Option<&mut SchemeRegistrar>) {
            if let Some(r) = registrar {
                use cef::sys::cef_scheme_options_t as O;
                let opts = O::CEF_SCHEME_OPTION_STANDARD as i32 | O::CEF_SCHEME_OPTION_SECURE as i32 | O::CEF_SCHEME_OPTION_CORS_ENABLED as i32 | O::CEF_SCHEME_OPTION_FETCH_ENABLED as i32;
                r.add_custom_scheme(Some(&CefString::from(crate::ipc_browser::SCHEME)), opts);
            }
        }
        fn browser_process_handler(&self) -> Option<BrowserProcessHandler> {
            Some(BPH::new())
        }
    }
}

pub fn make_app() -> App {
    AppB::new()
}

pub fn create_main_browser(url: &str) {
    // 런타임 스타일: 기본(Chrome)은 네이티브 Browser 창을 전제로 하는 UI 경로가 많다. SPIKE_RUNTIME=alloy|chrome 로 비교.
    let style = match std::env::var("SPIKE_RUNTIME").as_deref() {
        Ok("alloy") => RuntimeStyle::ALLOY,
        Ok("chrome") => RuntimeStyle::CHROME,
        _ => RuntimeStyle::DEFAULT,
    };
    with(|s| s.log.push(format!("runtime_style={:?}", std::env::var("SPIKE_RUNTIME"))));
    let wi = WindowInfo {
        windowless_rendering_enabled: 1,
        runtime_style: style,
        ..Default::default()
    };
    let mut client = make_client(Which::Main);
    let bs = BrowserSettings {
        windowless_frame_rate: 60,
        // OSR 기본 배경은 투명 → 배경 미지정 페이지에서 gpui 뒷면이 비친다. 불투명 흰색으로 지정.
        background_color: 0xFFFF_FFFF,
        ..Default::default()
    };
    let r = browser_host_create_browser(
        Some(&wi),
        Some(&mut client),
        Some(&CefString::from(url)),
        Some(&bs),
        None,
        None,
    );
    with(|s| s.log.push(format!("browser_host_create_browser -> {r}")));
}

/// 프로필 prefs 로 비밀번호 관리자/자동완성을 끈다. (passkey 조건부 요청은 이 기능들이 입력칸의 autocomplete=webauthn 을 보고 시작한다)
/// SPIKE_PREFS=off 일 때 적용. 반드시 UI 스레드에서, 컨텍스트 초기화 이후에 호출.
pub fn apply_prefs_off() -> Vec<String> {
    let mut out = vec![];
    let Some(ctx) = request_context_get_global_context() else {
        return vec!["no global context".into()];
    };
    for name in [
        "credentials_enable_service",
        "credentials_enable_autosignin",
        "profile.password_manager_enabled",
        "autofill.profile_enabled",
        "autofill.credit_card_enabled",
        "autofill.enabled",
    ] {
        let n = CefString::from(name);
        if ctx.has_preference(Some(&n)) != 1 {
            out.push(format!("{name}: not registered"));
            continue;
        }
        let Some(mut v) = value_create() else { continue };
        v.set_bool(0);
        let mut err = CefString::default();
        let ok = ctx.set_preference(Some(&n), Some(&mut v), Some(&mut err));
        out.push(format!("{name}: set false -> {ok} {}", err.to_string()));
    }
    out
}
