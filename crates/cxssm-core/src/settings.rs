//! Préférences de l'utilisateur, enregistrées en TOML dans le dossier de configuration
//! du système : `~/.config/cxssm/` (Linux), `%APPDATA%\cxssm\` (Windows),
//! `~/Library/Application Support/cxssm/` (macOS).

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::project::SoundCategory;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    Fr,
    En,
}

impl Language {
    /// Langue du système : français si elle commence par `fr`, **anglais par défaut**.
    ///
    /// Les variables `LC_ALL` / `LC_MESSAGES` / `LANG` priment quand elles sont définies
    /// (Linux, terminal) ; sinon la langue est demandée au système (`sys-locale` :
    /// réglages de Windows et de macOS, même lancé depuis le Finder).
    pub fn from_system() -> Self {
        let env = ["LC_ALL", "LC_MESSAGES", "LANG"]
            .iter()
            .find_map(|v| std::env::var(v).ok().filter(|s| !s.is_empty() && s != "C" && s != "POSIX"));
        Self::from_locale(env.or_else(sys_locale::get_locale).as_deref())
    }

    /// `fr_FR.UTF-8`, `fr-CA`, `FR`… → français ; tout le reste (ou rien) → anglais.
    pub fn from_locale(locale: Option<&str>) -> Self {
        match locale {
            Some(l) if l.to_lowercase().starts_with("fr") => Self::Fr,
            _ => Self::En,
        }
    }
}

/// Thème de l'interface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemeMode {
    #[default]
    Dark,
    Light,
}

/// Raccourcis clavier : `[ctrl-][alt-][shift-][cmd-]touche`, touche telle que reçue du
/// clavier (`z`, `e`, `space`…). Une chaîne vide désactive le raccourci.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Shortcuts {
    pub undo: String,
    pub redo: String,
    pub select_all: String,
    /// Démarrer / arrêter le moteur du simulateur.
    pub engine: String,
    /// Balayage du ralenti au rupteur.
    pub sweep: String,
    /// Accélérateur (maintenu).
    pub throttle: String,
}

impl Default for Shortcuts {
    fn default() -> Self {
        let m = if cfg!(target_os = "macos") { "cmd" } else { "ctrl" };
        Self {
            undo: format!("{m}-z"),
            redo: format!("{m}-shift-z"),
            select_all: format!("{m}-a"),
            engine: "e".into(),
            sweep: "s".into(),
            throttle: "space".into(),
        }
    }
}

/// Couleur d'accentuation par défaut (ambre).
pub const DEFAULT_ACCENT: u32 = 0xF5B800;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub language: Language,
    /// Rupteur du simulateur moteur activé (sinon le régime monte jusqu'au maximum).
    pub rpm_limiter: bool,
    /// Catégories masquées dans la barre latérale.
    pub hidden_categories: Vec<SoundCategory>,
    /// Ouvrir automatiquement le bundle du jeu au démarrage.
    pub auto_open: bool,
    /// Bundle choisi par l'utilisateur (prioritaire sur la détection automatique).
    pub bundle_path: Option<PathBuf>,
    /// Dossier du FMOD Engine choisi par l'utilisateur (prioritaire sur la détection).
    pub fmod_dir: Option<PathBuf>,
    /// Vérifier au démarrage si une nouvelle version est publiée sur GitHub.
    pub check_updates: bool,
    /// Dernier projet `.cxssm` ouvert (rouvert au démarrage, avant le bundle du jeu).
    pub last_project: Option<PathBuf>,
    /// Thème clair ou sombre.
    pub theme: ThemeMode,
    /// Couleur d'accentuation, `#RRGGBB`.
    pub accent: String,
    pub shortcuts: Shortcuts,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            language: Language::from_system(),
            rpm_limiter: true,
            hidden_categories: Vec::new(),
            auto_open: true,
            bundle_path: None,
            fmod_dir: None,
            check_updates: true,
            last_project: None,
            theme: ThemeMode::Dark,
            accent: format_hex(DEFAULT_ACCENT),
            shortcuts: Shortcuts::default(),
        }
    }
}

impl Settings {
    pub fn path() -> Option<PathBuf> {
        Some(dirs::config_dir()?.join("cxssm").join("settings.toml"))
    }

    /// Charge les préférences ; valeurs par défaut si le fichier est absent ou invalide.
    pub fn load() -> Self {
        let Some(path) = Self::path() else { return Self::default() };
        match std::fs::read_to_string(&path) {
            Ok(text) => toml::from_str(&text).unwrap_or_else(|e| {
                log::warn!("préférences invalides ({}) : {e}", path.display());
                Self::default()
            }),
            Err(_) => Self::default(),
        }
    }

    pub fn save(&self) -> std::io::Result<()> {
        let path = Self::path().ok_or_else(|| std::io::Error::other("dossier de configuration introuvable"))?;
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let text = toml::to_string_pretty(self).map_err(std::io::Error::other)?;
        std::fs::write(path, text)
    }

    /// Couleur d'accentuation (`0xRRGGBB`) ; l'ambre par défaut si le réglage est invalide.
    pub fn accent_rgb(&self) -> u32 {
        parse_hex(&self.accent).unwrap_or(DEFAULT_ACCENT)
    }

    pub fn set_accent(&mut self, rgb: u32) {
        self.accent = format_hex(rgb);
    }

    /// Installation FMOD à utiliser (variables d'environnement, préférences, détection).
    pub fn fmod(&self) -> Option<crate::FmodInstall> {
        crate::FmodInstall::resolve(self.fmod_dir.as_deref())
    }

    pub fn is_hidden(&self, c: SoundCategory) -> bool {
        self.hidden_categories.contains(&c)
    }

    pub fn toggle_category(&mut self, c: SoundCategory) {
        if let Some(i) = self.hidden_categories.iter().position(|x| *x == c) {
            self.hidden_categories.remove(i);
        } else {
            self.hidden_categories.push(c);
        }
    }

    /// À ouvrir au démarrage : le dernier projet s'il existe encore, sinon le bundle du jeu.
    pub fn startup_path(&self) -> Option<PathBuf> {
        if !self.auto_open {
            return None;
        }
        self.last_project.clone().filter(|p| p.is_file()).or_else(|| self.startup_bundle())
    }

    /// Bundle à ouvrir au démarrage : celui choisi par l'utilisateur s'il existe encore,
    /// sinon le premier trouvé par la détection automatique.
    pub fn startup_bundle(&self) -> Option<PathBuf> {
        if !self.auto_open {
            return None;
        }
        self.bundle_path
            .clone()
            .filter(|p| p.is_file())
            .or_else(|| crate::game::find_bundles().into_iter().next())
    }
}

/// `#RRGGBB` (ou `RRGGBB`) → `0xRRGGBB`.
pub fn parse_hex(s: &str) -> Option<u32> {
    let h = s.trim().trim_start_matches('#');
    (h.len() == 6).then(|| u32::from_str_radix(h, 16).ok()).flatten()
}

pub fn format_hex(rgb: u32) -> String {
    format!("#{:06X}", rgb & 0xFF_FFFF)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toml_roundtrip() {
        let mut s = Settings { language: Language::En, ..Default::default() };
        s.toggle_category(SoundCategory::Voice);
        let text = toml::to_string_pretty(&s).unwrap();
        let back: Settings = toml::from_str(&text).unwrap();
        assert_eq!(back, s);
        // Champs manquants → valeurs par défaut.
        let partial: Settings = toml::from_str("language = \"fr\"").unwrap();
        assert!(partial.rpm_limiter && partial.auto_open);
        assert_eq!((partial.theme, partial.accent_rgb()), (ThemeMode::Dark, DEFAULT_ACCENT));
        assert_eq!(partial.shortcuts, Shortcuts::default());
        // Un raccourci modifié est relu ; les autres gardent leur valeur par défaut.
        let custom: Settings = toml::from_str("[shortcuts]\nundo = \"ctrl-w\"\n").unwrap();
        assert_eq!((custom.shortcuts.undo.as_str(), custom.shortcuts.throttle.as_str()), ("ctrl-w", "space"));
    }

    #[test]
    fn accent_hex() {
        let mut s = Settings { theme: ThemeMode::Light, ..Default::default() };
        s.set_accent(0x3D8BFF);
        assert_eq!(s.accent, "#3D8BFF");
        let back: Settings = toml::from_str(&toml::to_string_pretty(&s).unwrap()).unwrap();
        assert_eq!((back.theme, back.accent_rgb()), (ThemeMode::Light, 0x3D8BFF));
        assert_eq!(parse_hex("22c3e6"), Some(0x22C3E6));
        assert_eq!(parse_hex("#12345"), None);
        // Fichier écrit à la main, clés du thème en tête.
        let file: Settings = toml::from_str("theme = \"light\"\naccent = \"#3D8BFF\"\nlanguage = \"fr\"\nrpm_limiter = true\n").unwrap();
        assert_eq!((file.theme, file.accent_rgb(), file.language), (ThemeMode::Light, 0x3D8BFF, Language::Fr));
        s.accent = "rouge".into();
        assert_eq!(s.accent_rgb(), DEFAULT_ACCENT);
    }

    #[test]
    fn language_from_locale_defaults_to_english() {
        assert_eq!(Language::from_locale(Some("fr_FR.UTF-8")), Language::Fr);
        assert_eq!(Language::from_locale(Some("fr-CA")), Language::Fr);
        assert_eq!(Language::from_locale(Some("en_US.UTF-8")), Language::En);
        assert_eq!(Language::from_locale(Some("de-DE")), Language::En);
        assert_eq!(Language::from_locale(None), Language::En);
    }
}
