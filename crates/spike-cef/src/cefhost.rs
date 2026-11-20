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
    let wi = WindowInfo {
        windowless_rendering_enabled: 1,
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
