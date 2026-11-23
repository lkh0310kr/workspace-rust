pub struct SurfaceSnapshot {
    pub gen: u64,
    pub width: u32,
    pub height: u32,
    pub data: Option<Vec<u8>>,
}

pub struct UiState {
    pub cursor: CursorIcon,
    pub widget_bounds: (i32, i32, i32, i32),
    pub widget_visible: bool,
    pub context_menu: Option<(i32, i32, String)>,
}

pub struct BrowserRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

pub enum CursorIcon {
    Arrow,
    Hand,
    Text,
    Crosshair,
}

#[derive(Clone, Copy)]
pub enum SurfaceTarget {
    Main,
    Popup,
    Widget,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum BrowserTarget {
    Main,
    Popup,
}

pub enum BrowserCommand {
    Copy,
    Paste,
    SelectAll,
    Reload,
}

/// Engine-specific browser operations. Implementations run on the application thread.
pub trait BrowserEngine {
    fn ensure_main_browser(&self, initial_url: &str);
    fn revision(&self) -> u64;
    fn has_popup(&self) -> bool;
    fn popup_size(&self) -> (f32, f32);
    fn navigate(&self, url: &str);
    fn mouse_button(
        &self,
        target: BrowserTarget,
        x: i32,
        y: i32,
        modifiers: u32,
        mouse_up: bool,
        right: bool,
        click_count: usize,
    );
    fn mouse_move(&self, target: BrowserTarget, x: i32, y: i32, modifiers: u32);
    fn mouse_wheel(
        &self,
        target: BrowserTarget,
        x: i32,
        y: i32,
        modifiers: u32,
        delta_x: i32,
        delta_y: i32,
    );
    fn key_down(
        &self,
        target: BrowserTarget,
        key: &str,
        platform: bool,
        shift: bool,
        control: bool,
        modifiers: u32,
        allow_text_input: bool,
    );
    fn ime_finish(&self);
    fn ime_commit(&self, target: BrowserTarget, text: &str);
    fn ime_set_composition(
        &self,
        target: BrowserTarget,
        text: &str,
        selection: Option<(usize, usize)>,
    );
    fn execute(&self, command: BrowserCommand);
    fn record_context_command(&self, label: &str);
    fn resize_main(&self, width: f32, height: f32, scale: f32);
    fn ui_state(&self) -> UiState;
    fn surface_snapshot(
        &self,
        target: SurfaceTarget,
        known_gen: Option<u64>,
    ) -> Option<SurfaceSnapshot>;
    fn record_foreign_ime(&self);
    fn dismiss_context_menu(&self);
}

pub struct Application {
    engine: Box<dyn BrowserEngine>,
}

impl Application {
    pub fn new(engine: impl BrowserEngine + 'static) -> Self {
        Self {
            engine: Box::new(engine),
        }
    }

    pub fn ensure_main_browser(&self, initial_url: &str) {
        self.engine.ensure_main_browser(initial_url);
    }
    pub fn revision(&self) -> u64 {
        self.engine.revision()
    }
    fn input_target(&self) -> BrowserTarget {
        if self.engine.has_popup() {
            BrowserTarget::Popup
        } else {
            BrowserTarget::Main
        }
    }
    pub fn popup_bounds(&self, viewport_width: f32, viewport_height: f32) -> Option<BrowserRect> {
        if !self.engine.has_popup() {
            return None;
        }

        let popup_size = self.engine.popup_size();
        let width = popup_size.0.min(viewport_width - 40.0);
        let height = popup_size.1.min(viewport_height - 40.0);
        Some(BrowserRect {
            x: (viewport_width - width) / 2.0,
            y: (viewport_height - height) / 2.0,
            width,
            height,
        })
    }
    pub fn route_pointer(
        &self,
        x: f32,
        y: f32,
        viewport_width: f32,
        viewport_height: f32,
    ) -> (BrowserTarget, i32, i32) {
        if let Some(popup) = self.popup_bounds(viewport_width, viewport_height) {
            if x >= popup.x
                && y >= popup.y
                && x <= popup.x + popup.width
                && y <= popup.y + popup.height
            {
                return (
                    BrowserTarget::Popup,
                    (x - popup.x) as i32,
                    (y - popup.y) as i32,
                );
            }
        }
        (BrowserTarget::Main, x as i32, y as i32)
    }
    pub fn navigate(&self, url: &str) {
        self.engine.navigate(url);
    }
    pub fn mouse_button(
        &self,
        target: BrowserTarget,
        x: i32,
        y: i32,
        modifiers: u32,
        mouse_up: bool,
        right: bool,
        click_count: usize,
    ) {
        self.engine
            .mouse_button(target, x, y, modifiers, mouse_up, right, click_count);
    }
    pub fn mouse_move(&self, target: BrowserTarget, x: i32, y: i32, modifiers: u32) {
        self.engine.mouse_move(target, x, y, modifiers);
    }
    pub fn mouse_wheel(
        &self,
        target: BrowserTarget,
        x: i32,
        y: i32,
        modifiers: u32,
        delta_x: i32,
        delta_y: i32,
    ) {
        self.engine
            .mouse_wheel(target, x, y, modifiers, delta_x, delta_y);
    }
    pub fn key_down(
        &self,
        key: &str,
        platform: bool,
        shift: bool,
        control: bool,
        modifiers: u32,
        allow_text_input: bool,
    ) {
        self.engine.key_down(
            self.input_target(),
            key,
            platform,
            shift,
            control,
            modifiers,
            allow_text_input,
        );
    }
    pub fn ime_finish(&self) {
        self.engine.ime_finish();
    }
    pub fn ime_commit(&self, text: &str) {
        self.engine.ime_commit(self.input_target(), text);
    }
    pub fn ime_set_composition(&self, text: &str, selection: Option<(usize, usize)>) {
        self.engine
            .ime_set_composition(self.input_target(), text, selection);
    }
    pub fn execute_context_command(&self, command: BrowserCommand, label: &str) {
        self.engine.execute(command);
        self.engine.dismiss_context_menu();
        self.engine.record_context_command(label);
    }
    pub fn resize_main(&self, width: f32, height: f32, scale: f32) {
        self.engine.resize_main(width, height, scale);
    }
    pub fn ui_state(&self) -> UiState {
        self.engine.ui_state()
    }
    pub fn surface_snapshot(
        &self,
        target: SurfaceTarget,
        known_gen: Option<u64>,
    ) -> Option<SurfaceSnapshot> {
        self.engine.surface_snapshot(target, known_gen)
    }
    pub fn record_foreign_ime(&self) {
        self.engine.record_foreign_ime();
    }
    pub fn dismiss_context_menu(&self) {
        self.engine.dismiss_context_menu();
    }
}
