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
    /// Langue du système (`LANG`, `LC_ALL`…) : français si elle commence par `fr`, sinon anglais.
    pub fn from_system() -> Self {
        let lang = ["LC_ALL", "LC_MESSAGES", "LANG"]
            .iter()
            .find_map(|v| std::env::var(v).ok().filter(|s| !s.is_empty()))
            .unwrap_or_default();
        if lang.to_lowercase().starts_with("fr") {
            Self::Fr
        } else {
            Self::En
        }
    }
}

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
    }
}
