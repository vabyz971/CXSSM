//! Charte graphique (d'après `docs/design/`) : couleurs, polices et espacements.
//! Toutes les valeurs visuelles de l'interface viennent d'ici.

use gpui::{px, Pixels, Rgba};

/// `0xRRGGBB` → couleur opaque (utilisable dans une constante).
const fn hex(c: u32) -> Rgba {
    hexa((c << 8) | 0xFF)
}

/// `0xRRGGBBAA` → couleur avec transparence.
const fn hexa(c: u32) -> Rgba {
    Rgba {
        r: ((c >> 24) & 0xFF) as f32 / 255.,
        g: ((c >> 16) & 0xFF) as f32 / 255.,
        b: ((c >> 8) & 0xFF) as f32 / 255.,
        a: (c & 0xFF) as f32 / 255.,
    }
}

// ───────────────────────────── couleurs ─────────────────────────────

/// Fond de la fenêtre.
pub const BG: Rgba = hex(0x0B0C0E);
/// Cartes (simulateur, liste des sons), boîte de dialogue.
pub const PANEL: Rgba = hex(0x121316);
/// Colonnes latérales (arborescence, panneau de détails, navigation des préférences).
pub const SIDEBAR: Rgba = hex(0x101114);
/// Barres d'application et d'état.
pub const BAR: Rgba = hex(0x0F1013);
/// Champs (recherche, chemins).
pub const FIELD: Rgba = hex(0x0B0C0E);
/// Fond des pistes de spectre.
pub const STRIP: Rgba = hex(0x0D0E11);
/// Groupes segmentés, cases.
pub const INSET: Rgba = hex(0x15171B);
/// Boutons secondaires.
pub const BUTTON: Rgba = hex(0x181A1E);
pub const BUTTON_HOVER: Rgba = hex(0x21242A);
/// Survol des lignes.
pub const HOVER: Rgba = hex(0x1B1E23);
/// Pistes des jauges.
pub const TRACK: Rgba = hex(0x1D2025);

pub const DIVIDER: Rgba = hex(0x23262C);
pub const BORDER: Rgba = hex(0x2A2E35);
pub const BORDER_STRONG: Rgba = hex(0x333840);
pub const BORDER_HOVER: Rgba = hex(0x3A3F48);

pub const TEXT: Rgba = hex(0xECEEF1);
pub const TEXT_BRIGHT: Rgba = hex(0xFFFFFF);
pub const TEXT_SOFT: Rgba = hex(0xD7DBE1);
pub const TEXT_ROW: Rgba = hex(0xC4C9D1);
pub const TEXT_ICON: Rgba = hex(0xB6BBC4);
pub const TEXT_DIM: Rgba = hex(0x9AA0AA);
pub const TEXT_FAINT: Rgba = hex(0x8B919B);
pub const TEXT_DISABLED: Rgba = hex(0x6F7580);

pub const ACCENT: Rgba = hex(0xF5B800);
/// Accent à ~12 % (sélection, fonds teintés).
pub const ACCENT_TINT: Rgba = hexa(0xF5B8001F);
/// Accent à ~14 % (bande de régime, section active).
pub const ACCENT_BAND: Rgba = hexa(0xF5B80024);
/// Accent à ~15 % (voiture sélectionnée dans l'arborescence).
pub const ACCENT_SELECT: Rgba = hexa(0xF5B80026);
/// Texte posé sur l'accent.
pub const ON_ACCENT: Rgba = hex(0x111111);

/// Spectre d'un son au repos : remplissage et contour.
pub const SPECTRUM_IDLE: Rgba = hexa(0x3A3F48C0);
pub const SPECTRUM_LINE: Rgba = hex(0x6F7580);
/// Spectre d'un son entendu (accent translucide, contour accent).
pub const SPECTRUM_LIT: Rgba = hexa(0xF5B80059);
/// Bande de régime d'un son au repos.
pub const ACCENT_DIM: Rgba = hexa(0xF5B80066);
/// Bordure d'un son entendu dans le simulateur.
pub const ACCENT_EDGE: Rgba = hexa(0xF5B80080);
/// Fond d'un son entendu dans le simulateur.
pub const ROW_LIVE: Rgba = hexa(0xF5B80014);
/// Interrupteur éteint.
pub const SWITCH_OFF: Rgba = hex(0x3A3F48);
pub const DANGER: Rgba = hex(0xFF6B4A);
pub const BACKDROP: Rgba = hexa(0x000000A0);

// ───────────────────────────── polices ─────────────────────────────

/// Police de l'interface (intégrée à l'exécutable, cf. `assets/fonts`).
pub const FONT_UI: &str = "Inter";
/// Police des nombres et des chemins.
pub const FONT_MONO: &str = "JetBrains Mono NL";

pub const TEXT_XS: Pixels = px(11.);
pub const TEXT_SM: Pixels = px(12.);
pub const TEXT_MD: Pixels = px(13.);
pub const TEXT_LG: Pixels = px(16.);
pub const TEXT_TITLE: Pixels = px(18.);
pub const TEXT_H1: Pixels = px(22.);
pub const TEXT_RPM: Pixels = px(52.);

// ───────────────────────────── espacements ─────────────────────────────

pub const APPBAR_PAD_X: Pixels = px(20.);
pub const APPBAR_PAD_Y: Pixels = px(10.);
pub const STATUSBAR_PAD_Y: Pixels = px(7.);
pub const TREE_W: Pixels = px(272.);
pub const SIDE_W: Pixels = px(344.);
pub const RAIL_W: Pixels = px(52.);
pub const MAIN_PAD_X: Pixels = px(20.);
pub const MAIN_PAD_Y: Pixels = px(18.);
pub const GAP_XS: Pixels = px(4.);
pub const GAP_SM: Pixels = px(8.);
pub const GAP_MD: Pixels = px(12.);
pub const GAP_LG: Pixels = px(16.);
pub const GAP_XL: Pixels = px(26.);

pub const CONTROL_H: Pixels = px(34.);
pub const BUTTON_H: Pixels = px(36.);
pub const ROW_H: Pixels = px(32.);
pub const TAB_H: Pixels = px(46.);
pub const SOUND_ROW_H: Pixels = px(64.);
pub const STRIP_H: Pixels = px(46.);

pub const RADIUS_SM: Pixels = px(6.);
pub const RADIUS: Pixels = px(8.);
pub const RADIUS_MD: Pixels = px(9.);
pub const RADIUS_LG: Pixels = px(12.);
pub const RADIUS_XL: Pixels = px(14.);
