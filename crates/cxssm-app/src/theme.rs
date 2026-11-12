//! Charte graphique (d'après `docs/design/`) : couleurs, polices et espacements.
//! Toutes les valeurs visuelles de l'interface viennent d'ici ; les couleurs suivent le
//! thème et la couleur d'accentuation des préférences ([`apply`]).

use std::sync::RwLock;

use cxssm_core::ThemeMode;
use gpui::{px, Pixels, Rgba};

/// `0xRRGGBB` → couleur opaque (utilisable dans une constante).
pub const fn hex(c: u32) -> Rgba {
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
//
// Les couleurs dépendent du thème (clair / sombre) et de la couleur d'accentuation
// choisis dans les préférences : elles sont lues dans la palette active, via une
// fonction par couleur (`theme::accent()`, `theme::text_dim()`…).

/// Couleurs d'accentuation proposées : (couleur, nom français, nom anglais).
pub const ACCENTS: [(u32, &str, &str); 8] = [
    (0xF5B800, "Ambre", "Amber"),
    (0xFF7A1A, "Orange", "Orange"),
    (0xFF4D5E, "Rouge", "Red"),
    (0xFF4FA3, "Rose", "Pink"),
    (0xA47CFF, "Violet", "Purple"),
    (0x3D8BFF, "Bleu", "Blue"),
    (0x1FC8D9, "Cyan", "Cyan"),
    (0x3DD68C, "Vert", "Green"),
];

macro_rules! palette {
    ($($(#[$doc:meta])* $name:ident),* $(,)?) => {
        /// Palette active : toutes les couleurs de l'interface.
        #[derive(Debug, Clone, Copy)]
        pub struct Palette {
            $(pub $name: Rgba,)*
        }

        $(
            $(#[$doc])*
            pub fn $name() -> Rgba {
                PALETTE.read().unwrap_or_else(|e| e.into_inner()).$name
            }
        )*
    };
}

palette! {
    /// Fond de la fenêtre.
    bg,
    /// Cartes (simulateur, liste des sons), boîte de dialogue.
    panel,
    /// Colonnes latérales (arborescence, panneau de détails, navigation des préférences).
    sidebar,
    /// Barres d'application et d'état.
    bar,
    /// Champs (recherche, chemins).
    field,
    /// Fond des pistes de spectre.
    strip,
    /// Groupes segmentés, cases.
    inset,
    /// Boutons secondaires.
    button,
    button_hover,
    /// Survol des lignes.
    hover,
    /// Pistes des jauges.
    track,
    divider,
    border,
    border_strong,
    border_hover,
    text,
    text_bright,
    text_soft,
    text_row,
    text_icon,
    text_dim,
    text_faint,
    text_disabled,
    accent,
    /// Accent à ~12 % (sélection, fonds teintés).
    accent_tint,
    /// Accent à ~14 % (bande de régime, section active).
    accent_band,
    /// Accent à ~15 % (voiture sélectionnée dans l'arborescence).
    accent_select,
    /// Texte posé sur l'accent.
    on_accent,
    /// Spectre d'un son au repos : remplissage et contour.
    spectrum_idle,
    spectrum_line,
    /// Spectre d'un son entendu (accent translucide, contour accent).
    spectrum_lit,
    /// Bande de régime d'un son au repos.
    accent_dim,
    /// Bordure d'un son entendu dans le simulateur.
    accent_edge,
    /// Fond d'un son entendu dans le simulateur.
    row_live,
    /// Interrupteur éteint.
    switch_off,
    danger,
    backdrop,
}

static PALETTE: RwLock<Palette> = RwLock::new(Palette::new(ThemeMode::Dark, cxssm_core::settings::DEFAULT_ACCENT));

/// Applique un thème et une couleur d'accentuation à toute l'interface.
pub fn apply(mode: ThemeMode, accent: u32) {
    *PALETTE.write().unwrap_or_else(|e| e.into_inner()) = Palette::new(mode, accent);
}

/// Accent `0xRRGGBB` avec une opacité `0..=0xFF`.
const fn tinted(rgb: u32, alpha: u32) -> Rgba {
    hexa(((rgb & 0xFF_FFFF) << 8) | (alpha & 0xFF))
}

/// Texte lisible sur l'accent : sombre sur une couleur claire, blanc sinon.
pub const fn on(rgb: u32) -> Rgba {
    let (r, g, b) = ((rgb >> 16) & 0xFF, (rgb >> 8) & 0xFF, rgb & 0xFF);
    // Luminance perçue (pondération ITU-R BT.601), en entiers pour rester `const`.
    if r * 299 + g * 587 + b * 114 > 150_000 {
        hex(0x111111)
    } else {
        hex(0xFFFFFF)
    }
}

impl Palette {
    pub const fn new(mode: ThemeMode, accent: u32) -> Self {
        let a = accent;
        match mode {
            ThemeMode::Dark => Self {
                bg: hex(0x0B0C0E),
                panel: hex(0x121316),
                sidebar: hex(0x101114),
                bar: hex(0x0F1013),
                field: hex(0x0B0C0E),
                strip: hex(0x0D0E11),
                inset: hex(0x15171B),
                button: hex(0x181A1E),
                button_hover: hex(0x21242A),
                hover: hex(0x1B1E23),
                track: hex(0x1D2025),
                divider: hex(0x23262C),
                border: hex(0x2A2E35),
                border_strong: hex(0x333840),
                border_hover: hex(0x3A3F48),
                text: hex(0xECEEF1),
                text_bright: hex(0xFFFFFF),
                text_soft: hex(0xD7DBE1),
                text_row: hex(0xC4C9D1),
                text_icon: hex(0xB6BBC4),
                text_dim: hex(0x9AA0AA),
                text_faint: hex(0x8B919B),
                text_disabled: hex(0x6F7580),
                accent: hex(a),
                accent_tint: tinted(a, 0x1F),
                accent_band: tinted(a, 0x24),
                accent_select: tinted(a, 0x26),
                on_accent: on(a),
                spectrum_idle: hexa(0x3A3F48C0),
                spectrum_line: hex(0x6F7580),
                spectrum_lit: tinted(a, 0x59),
                accent_dim: tinted(a, 0x66),
                accent_edge: tinted(a, 0x80),
                row_live: tinted(a, 0x14),
                switch_off: hex(0x3A3F48),
                danger: hex(0xFF6B4A),
                backdrop: hexa(0x000000A0),
            },
            ThemeMode::Light => Self {
                bg: hex(0xF3F4F6),
                panel: hex(0xFFFFFF),
                sidebar: hex(0xEDEFF2),
                bar: hex(0xF8F9FA),
                field: hex(0xFFFFFF),
                strip: hex(0xF1F3F5),
                inset: hex(0xE9ECF0),
                button: hex(0xFFFFFF),
                button_hover: hex(0xEEF0F3),
                hover: hex(0xE6E9ED),
                track: hex(0xDDE1E6),
                divider: hex(0xE1E4E9),
                border: hex(0xD3D7DD),
                border_strong: hex(0xC6CBD3),
                border_hover: hex(0xB8BEC7),
                text: hex(0x1A1D22),
                text_bright: hex(0x000000),
                text_soft: hex(0x2C3138),
                text_row: hex(0x3A4048),
                text_icon: hex(0x4F5661),
                text_dim: hex(0x646B76),
                text_faint: hex(0x737A85),
                text_disabled: hex(0x9AA0A9),
                accent: hex(a),
                accent_tint: tinted(a, 0x29),
                accent_band: tinted(a, 0x30),
                accent_select: tinted(a, 0x33),
                on_accent: on(a),
                spectrum_idle: hexa(0xB4BBC6C0),
                spectrum_line: hex(0x8C939E),
                spectrum_lit: tinted(a, 0x70),
                accent_dim: tinted(a, 0x80),
                accent_edge: tinted(a, 0xB0),
                row_live: tinted(a, 0x1C),
                switch_off: hex(0xC4C9D1),
                danger: hex(0xE0452A),
                backdrop: hexa(0x0000005C),
            },
        }
    }
}

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
