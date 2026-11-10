//! Palette inspirée des menus de CarX Street : fond graphite, accent vif, texte clair.
//! (À ajuster d'après les captures d'écran du jeu.)

use gpui::{rgb, Rgba};

pub fn bg() -> Rgba { rgb(0x0E0F12) }
pub fn panel() -> Rgba { rgb(0x16181D) }
pub fn panel_hi() -> Rgba { rgb(0x1E2128) }
pub fn border() -> Rgba { rgb(0x2A2E37) }
pub fn text() -> Rgba { rgb(0xF2F3F5) }
pub fn text_dim() -> Rgba { rgb(0x8A909C) }
pub fn accent() -> Rgba { rgb(0xFFC400) }
pub fn accent_text() -> Rgba { rgb(0x111111) }
pub fn ok() -> Rgba { rgb(0x2BD67B) }
pub fn warn() -> Rgba { rgb(0xFF5A36) }
