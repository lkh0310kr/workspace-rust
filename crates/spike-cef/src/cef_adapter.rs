use cef::{
    api_hash, args::Args, cookie_manager_get_global_manager, do_message_loop_work, execute_process,
    initialize as cef_initialize, shutdown, sys, App, BrowserHost, CefString, CompositionUnderline,
    CompositionUnderlineStyle, ImplBrowser, ImplBrowserHost, ImplCommandLine, ImplCookieManager,
    ImplFrame, LogSeverity, MouseButtonType, MouseEvent, PaintElementType, Range, Settings,
};

#[path = "cefhost.rs"]
pub(crate) mod host;
#[path = "ipc_browser.rs"]
pub(crate) mod ipc_browser;
#[path = "keys.rs"]
pub(crate) mod keys;

use crate::application::{
    BrowserCommand, BrowserEngine, BrowserTarget, CursorIcon, SurfaceSnapshot, SurfaceTarget,
    UiState,
};
use host::{with, Which};

fn vkey(key: &str) -> Option<i32> {
    Some(match key {
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

impl BrowserTarget {
    fn which(self) -> Which {
        match self {
            Self::Main => Which::Main,
            Self::Popup => Which::Popup,
        }
    }
}

impl SurfaceTarget {
    fn index(self) -> usize {
        match self {
            Self::Main => 0,
            Self::Popup => 1,
            Self::Widget => 2,
        }
    }
}

#[cfg(target_os = "macos")]
pub fn load_framework() -> library_loader::LibraryLoader {
    let loader = library_loader::LibraryLoader::new(&std::env::current_exe().unwrap(), false);
    assert!(
        loader.load(),
        "CEF framework load failed (run from the bundled .app)"
    );
    loader
}

pub fn args() -> Args {
    Args::new()
}

pub fn make_app() -> App {
    host::make_app()
}

pub fn verify_api_version() {
    let _ = api_hash(sys::CEF_API_VERSION_LAST, 0);
}

pub fn run_subprocess(args: &Args, app: &mut App) -> (bool, i32) {
    let command_line = args.as_cmd_line().unwrap();
    let is_browser = command_line.has_switch(Some(&CefString::from("type"))) != 1;
    let result = execute_process(Some(args.as_main_args()), Some(app), std::ptr::null_mut());
    (is_browser, result)
}

pub fn initialize(args: &Args, app: &mut App) {
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
    let initialized = cef_initialize(
        Some(args.as_main_args()),
        Some(&settings),
        Some(app),
        std::ptr::null_mut(),
    );
    println!("cef initialize -> {initialized}");
    assert_eq!(initialized, 1);
}

pub fn message_loop_work() {
    do_message_loop_work();
}

pub fn flush_cookies() {
    if let Some(manager) = cookie_manager_get_global_manager(None) {
        manager.flush_store(None);
    }
}

pub fn shutdown_runtime() {
    shutdown();
}

pub fn ensure_main_browser(initial_url: &str) {
    let should_create = with(|state| {
        if state.ready && !state.create_requested {
            state.create_requested = true;
            true
        } else {
            false
        }
    });

    if should_create {
        host::create_main_browser(initial_url);
    }
}

pub fn revision() -> u64 {
    with(|state| {
        state.surf[0].gen
            + state.surf[1].gen
            + state.surf[2].gen
            + state.widget_visible as u64
            + state.ui_gen
    })
}

pub fn has_popup() -> bool {
    with(|state| state.popup.is_some())
}

pub fn popup_size() -> (f32, f32) {
    with(|state| state.size[1])
}

pub fn navigate(url: &str) {
    let browser = with(|s| s.main.clone());

    if let Some(browser) = browser {
        if let Some(frame) = browser.main_frame() {
            frame.load_url(Some(&CefString::from(url)));
        }
    }
}

fn host(which: Which) -> Option<BrowserHost> {
    with(|s| match which {
        Which::Main => s.main.clone(),
        Which::Popup => s.popup.clone(),
        Which::Tab => None,
    })
    .and_then(|browser| browser.host())
}

pub fn mouse_button(
    target: BrowserTarget,
    x: i32,
    y: i32,
    modifiers: u32,
    mouse_up: bool,
    right: bool,
    click_count: usize,
) {
    let Some(host) = host(target.which()) else {
        return;
    };

    if right {
        with(|s| s.right_clicks += 1);
    }

    let event = MouseEvent { x, y, modifiers };

    if !mouse_up {
        host.set_focus(1);
    }

    host.send_mouse_click_event(
        Some(&event),
        if right {
            MouseButtonType::RIGHT
        } else {
            MouseButtonType::LEFT
        },
        mouse_up as _,
        click_count.max(1) as _,
    );
}

pub fn mouse_move(target: BrowserTarget, x: i32, y: i32, modifiers: u32) {
    let Some(host) = host(target.which()) else {
        return;
    };

    with(|s| {
        s.moves += 1;
        s.last_move = (x, y, modifiers);
    });

    host.send_mouse_move_event(Some(&MouseEvent { x, y, modifiers }), 0);
}

pub fn mouse_wheel(
    target: BrowserTarget,
    x: i32,
    y: i32,
    modifiers: u32,
    delta_x: i32,
    delta_y: i32,
) {
    if let Some(host) = host(target.which()) {
        host.send_mouse_wheel_event(Some(&MouseEvent { x, y, modifiers }), delta_x, delta_y);
    }
}

pub fn key_down(
    target: BrowserTarget,
    key: &str,
    platform: bool,
    shift: bool,
    control: bool,
    modifiers: u32,
    allow_text_input: bool,
) {
    let target = target.which();

    let Some(h) = host(target) else {
        return;
    };

    let browser = with(|s| {
        if target == Which::Popup {
            s.popup.clone()
        } else {
            s.main.clone()
        }
    });

    if platform {
        if let Some(frame) = browser.and_then(|b| b.focused_frame().or_else(|| b.main_frame())) {
            match key {
                "a" => frame.select_all(),
                "c" => frame.copy(),
                "x" => frame.cut(),
                "v" => frame.paste(),
                "z" if shift => frame.redo(),
                "z" => frame.undo(),
                _ => {
                    if let Some(vk) = vkey(key) {
                        keys::send_key(&h, vk, modifiers);
                    }
                }
            }
        }

        return;
    }

    let special = matches!(
        key,
        "backspace"
            | "tab"
            | "enter"
            | "escape"
            | "space"
            | "pageup"
            | "pagedown"
            | "end"
            | "home"
            | "left"
            | "up"
            | "right"
            | "down"
            | "delete"
    );

    if special {
        if let Some(vk) = vkey(key) {
            keys::send_key(&h, vk, modifiers);
        }
    } else if allow_text_input && key.chars().count() == 1 && !control {
        let c = key.chars().next().unwrap();

        let vk = if c.is_ascii_alphabetic() {
            Some(c.to_ascii_uppercase() as i32)
        } else if c.is_ascii_digit() {
            Some(c as i32)
        } else {
            None
        };

        if let Some(vk) = vk {
            keys::send_text_key_events(&h, vk, modifiers, c as u16);
        }
    }
}

pub fn ime_finish() {
    if let Some(host) = host(Which::Main) {
        host.ime_finish_composing_text(0);
    }
}

pub fn ime_commit(target: BrowserTarget, text: &str) {
    let target = target.which();

    if let Some(host) = host(target) {
        let invalid = Range {
            from: u32::MAX,
            to: u32::MAX,
        };

        host.ime_commit_text(Some(&CefString::from(text)), Some(&invalid), 0);
    }
}

pub fn ime_set_composition(target: BrowserTarget, text: &str, selection: Option<(usize, usize)>) {
    let target = target.which();

    if let Some(host) = host(target) {
        let n = text.encode_utf16().count();

        let underline = CompositionUnderline {
            size: std::mem::size_of::<sys::cef_composition_underline_t>(),
            range: Range {
                from: 0,
                to: n as u32,
            },
            color: 0,
            background_color: 0,
            thick: 0,
            style: CompositionUnderlineStyle::default(),
        };

        let invalid = Range {
            from: u32::MAX,
            to: u32::MAX,
        };

        let selection = selection
            .map(|(start, end)| Range {
                from: start as u32,
                to: end as u32,
            })
            .unwrap_or(Range {
                from: n as u32,
                to: n as u32,
            });

        host.ime_set_composition(
            Some(&CefString::from(text)),
            Some(&[underline]),
            Some(&invalid),
            Some(&selection),
        );
    }
}

pub fn execute(command: BrowserCommand) {
    let browser = with(|s| s.main.clone());

    let Some(browser) = browser else {
        return;
    };

    match command {
        BrowserCommand::Reload => browser.reload(),

        BrowserCommand::Copy => {
            if let Some(frame) = browser.focused_frame().or_else(|| browser.main_frame()) {
                frame.copy();
            }
        }

        BrowserCommand::Paste => {
            if let Some(frame) = browser.focused_frame().or_else(|| browser.main_frame()) {
                frame.paste();
            }
        }

        BrowserCommand::SelectAll => {
            if let Some(frame) = browser.focused_frame().or_else(|| browser.main_frame()) {
                frame.select_all();
            }
        }
    }
}

pub fn record_context_command(label: &str) {
    with(|s| s.log.push(format!("ctx_item {label}")));
}

pub fn resize_main(width: f32, height: f32, scale: f32) {
    with(|s| {
        s.size[0] = (width, height);
        s.scale = scale;
    });

    if let Some(host) = host(Which::Main) {
        host.was_resized();
        host.invalidate(PaintElementType::default());
    }
}

pub fn ui_state() -> UiState {
    with(|s| UiState {
        cursor: match s.cursor {
            1 => CursorIcon::Hand,
            2 => CursorIcon::Text,
            3 => CursorIcon::Crosshair,
            _ => CursorIcon::Arrow,
        },
        widget_bounds: s.widget,
        widget_visible: s.widget_visible,
        context_menu: s.ctx.clone(),
    })
}

pub fn surface_snapshot(target: SurfaceTarget, known_gen: Option<u64>) -> Option<SurfaceSnapshot> {
    with(|s| {
        let surface = s.surf.get(target.index())?;

        if surface.gen == 0 {
            return Some(SurfaceSnapshot {
                gen: 0,
                width: 0,
                height: 0,
                data: None,
            });
        }

        let data = if known_gen == Some(surface.gen) {
            None
        } else {
            Some(surface.data.clone())
        };

        Some(SurfaceSnapshot {
            gen: surface.gen,
            width: surface.w,
            height: surface.h,
            data,
        })
    })
}

pub fn record_foreign_ime() {
    with(|s| s.foreign_ime += 1);
}

pub fn dismiss_context_menu() {
    with(|s| {
        if s.ctx.take().is_some() {
            s.ui_gen += 1;
        }
    });
}

pub struct CefAdapter;

impl BrowserEngine for CefAdapter {
    fn ensure_main_browser(&self, initial_url: &str) {
        crate::cef_adapter::ensure_main_browser(initial_url);
    }
    fn revision(&self) -> u64 {
        crate::cef_adapter::revision()
    }
    fn has_popup(&self) -> bool {
        crate::cef_adapter::has_popup()
    }
    fn popup_size(&self) -> (f32, f32) {
        crate::cef_adapter::popup_size()
    }
    fn navigate(&self, url: &str) {
        crate::cef_adapter::navigate(url);
    }
    fn mouse_button(
        &self,
        target: BrowserTarget,
        x: i32,
        y: i32,
        modifiers: u32,
        mouse_up: bool,
        right: bool,
        click_count: usize,
    ) {
        crate::cef_adapter::mouse_button(target, x, y, modifiers, mouse_up, right, click_count);
    }
    fn mouse_move(&self, target: BrowserTarget, x: i32, y: i32, modifiers: u32) {
        crate::cef_adapter::mouse_move(target, x, y, modifiers);
    }
    fn mouse_wheel(
        &self,
        target: BrowserTarget,
        x: i32,
        y: i32,
        modifiers: u32,
        delta_x: i32,
        delta_y: i32,
    ) {
        crate::cef_adapter::mouse_wheel(target, x, y, modifiers, delta_x, delta_y);
    }
    fn key_down(
        &self,
        target: BrowserTarget,
        key: &str,
        platform: bool,
        shift: bool,
        control: bool,
        modifiers: u32,
        allow_text_input: bool,
    ) {
        crate::cef_adapter::key_down(
            target,
            key,
            platform,
            shift,
            control,
            modifiers,
            allow_text_input,
        );
    }
    fn ime_finish(&self) {
        crate::cef_adapter::ime_finish();
    }
    fn ime_commit(&self, target: BrowserTarget, text: &str) {
        crate::cef_adapter::ime_commit(target, text);
    }
    fn ime_set_composition(
        &self,
        target: BrowserTarget,
        text: &str,
        selection: Option<(usize, usize)>,
    ) {
        crate::cef_adapter::ime_set_composition(target, text, selection);
    }
    fn execute(&self, command: BrowserCommand) {
        crate::cef_adapter::execute(command);
    }
    fn record_context_command(&self, label: &str) {
        crate::cef_adapter::record_context_command(label);
    }
    fn resize_main(&self, width: f32, height: f32, scale: f32) {
        crate::cef_adapter::resize_main(width, height, scale);
    }
    fn ui_state(&self) -> UiState {
        crate::cef_adapter::ui_state()
    }
    fn surface_snapshot(
        &self,
        target: SurfaceTarget,
        known_gen: Option<u64>,
    ) -> Option<SurfaceSnapshot> {
        crate::cef_adapter::surface_snapshot(target, known_gen)
    }
    fn record_foreign_ime(&self) {
        crate::cef_adapter::record_foreign_ime();
    }
    fn dismiss_context_menu(&self) {
        crate::cef_adapter::dismiss_context_menu();
    }
}
