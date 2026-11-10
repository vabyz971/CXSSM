//! Ressources intégrées à l'exécutable : icônes SVG (tracés de `docs/design/`) et polices.

use std::borrow::Cow;

use gpui::{px, svg, App, AssetSource, IntoElement, Pixels, Rgba, SharedString, Styled};

/// Icônes de l'interface (contour 24×24, couleur = couleur du texte).
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    Logo,
    Gear,
    Folder,
    Download,
    Search,
    Close,
    Chevron,
    Play,
    Stop,
    ArrowUp,
    PanelCollapse,
    PanelExpand,
    Check,
}

impl Icon {
    const ALL: [Icon; 13] = [
        Icon::Logo,
        Icon::Gear,
        Icon::Folder,
        Icon::Download,
        Icon::Search,
        Icon::Close,
        Icon::Chevron,
        Icon::Play,
        Icon::Stop,
        Icon::ArrowUp,
        Icon::PanelCollapse,
        Icon::PanelExpand,
        Icon::Check,
    ];

    fn name(self) -> &'static str {
        match self {
            Icon::Logo => "logo",
            Icon::Gear => "gear",
            Icon::Folder => "folder",
            Icon::Download => "download",
            Icon::Search => "search",
            Icon::Close => "close",
            Icon::Chevron => "chevron",
            Icon::Play => "play",
            Icon::Stop => "stop",
            Icon::ArrowUp => "arrow-up",
            Icon::PanelCollapse => "panel-collapse",
            Icon::PanelExpand => "panel-expand",
            Icon::Check => "check",
        }
    }

    fn path(self) -> SharedString {
        format!("icons/{}.svg", self.name()).into()
    }

    /// Contenu du dessin (`<path>`, `<circle>`…), sans la balise `<svg>`.
    fn body(self) -> &'static str {
        match self {
            Icon::Logo => r##"<path stroke-width="2.4" d="M4 12v0M8 8v8M12 4v16M16 9v6M20 11v2"/>"##,
            Icon::Gear => concat!(
                r##"<circle stroke-width="2" cx="12" cy="12" r="3"/>"##,
                r##"<path stroke-width="2" d="M19.4 15a1.7 1.7 0 0 0 .3 1.8l.1.1a2 2 0 1 1-2.8 2.8l-.1-.1a1.7 1.7 0 0 0-1.8-.3 1.7 1.7 0 0 0-1 1.5V21a2 2 0 1 1-4 0v-.1a1.7 1.7 0 0 0-1.1-1.5 1.7 1.7 0 0 0-1.8.3l-.1.1a2 2 0 1 1-2.8-2.8l.1-.1a1.7 1.7 0 0 0 .3-1.8 1.7 1.7 0 0 0-1.5-1H3a2 2 0 1 1 0-4h.1a1.7 1.7 0 0 0 1.5-1.1 1.7 1.7 0 0 0-.3-1.8l-.1-.1a2 2 0 1 1 2.8-2.8l.1.1a1.7 1.7 0 0 0 1.8.3H9a1.7 1.7 0 0 0 1-1.5V3a2 2 0 1 1 4 0v.1a1.7 1.7 0 0 0 1 1.5 1.7 1.7 0 0 0 1.8-.3l.1-.1a2 2 0 1 1 2.8 2.8l-.1.1a1.7 1.7 0 0 0-.3 1.8V9a1.7 1.7 0 0 0 1.5 1H21a2 2 0 1 1 0 4h-.1a1.7 1.7 0 0 0-1.5 1z"/>"##
            ),
            Icon::Folder => r##"<path stroke-width="2" d="M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z"/>"##,
            Icon::Download => r##"<path stroke-width="2.4" d="M12 3v12M7 10l5 5 5-5M5 21h14"/>"##,
            Icon::Search => r##"<circle stroke-width="2" cx="11" cy="11" r="7"/><path stroke-width="2" d="M20 20l-3.5-3.5"/>"##,
            Icon::Close => r##"<path stroke-width="2.2" d="M6 6l12 12M18 6L6 18"/>"##,
            Icon::Chevron => r##"<path stroke-width="2.2" d="M9 6l6 6-6 6"/>"##,
            Icon::Play => r##"<path fill="#000" stroke="none" d="M7 4l13 8-13 8z"/>"##,
            Icon::Stop => r##"<rect fill="#000" stroke="none" x="5" y="5" width="14" height="14" rx="2"/>"##,
            Icon::ArrowUp => r##"<path stroke-width="2.4" d="M12 19V5M5 12l7-7 7 7"/>"##,
            Icon::PanelCollapse => r##"<rect stroke-width="2" x="3" y="4" width="18" height="16" rx="2"/><path stroke-width="2" d="M15 4v16M9 10l2 2-2 2"/>"##,
            Icon::PanelExpand => r##"<rect stroke-width="2" x="3" y="4" width="18" height="16" rx="2"/><path stroke-width="2" d="M15 4v16M11 10l-2 2 2 2"/>"##,
            Icon::Check => r##"<path stroke-width="2.8" d="M5 12l5 5 9-10"/>"##,
        }
    }

    fn svg(self) -> String {
        format!(
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="#000" stroke-linecap="round" stroke-linejoin="round">{}</svg>"##,
            self.body()
        )
    }
}

/// Icône à la taille `size`, de la couleur `color`.
pub fn icon(icon: Icon, size: Pixels, color: Rgba) -> impl IntoElement {
    svg().path(icon.path()).flex_none().size(size).text_color(color)
}

/// Icône de 15 px (taille par défaut des boutons).
pub fn icon_sm(i: Icon, color: Rgba) -> impl IntoElement {
    icon(i, px(15.), color)
}

pub struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> gpui::Result<Option<Cow<'static, [u8]>>> {
        Ok(Icon::ALL
            .iter()
            .find(|i| i.path().as_ref() == path)
            .map(|i| Cow::Owned(i.svg().into_bytes())))
    }

    fn list(&self, path: &str) -> gpui::Result<Vec<SharedString>> {
        Ok(Icon::ALL.iter().map(|i| i.path()).filter(|p| p.starts_with(path)).collect())
    }
}

/// Polices intégrées (sous-ensembles latins d'Inter et JetBrains Mono, licence OFL).
const FONTS: [&[u8]; 6] = [
    include_bytes!("../assets/fonts/Inter-Regular.ttf"),
    include_bytes!("../assets/fonts/Inter-Medium.ttf"),
    include_bytes!("../assets/fonts/Inter-SemiBold.ttf"),
    include_bytes!("../assets/fonts/JetBrainsMono-Regular.ttf"),
    include_bytes!("../assets/fonts/JetBrainsMono-Medium.ttf"),
    include_bytes!("../assets/fonts/JetBrainsMono-SemiBold.ttf"),
];

pub fn load_fonts(cx: &App) {
    if let Err(e) = cx.text_system().add_fonts(FONTS.iter().map(|f| Cow::Borrowed(*f)).collect()) {
        log::warn!("polices intégrées : {e}");
    }
}
