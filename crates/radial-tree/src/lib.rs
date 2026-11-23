mod shot;
mod text;

pub mod tree_view;

pub use tree_view::{RadialTree, ThemeMode, TreeViewOptions};

/// Register the font used by `RadialTree` with the host application's text system.
pub fn register_fonts(text_system: &gpui::TextSystem) {
    text::register_fonts(text_system);
}
