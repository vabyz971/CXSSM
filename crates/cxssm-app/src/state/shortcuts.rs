//! **Raccourcis clavier**, réglables dans les préférences.
//!
//! Les touches sont comparées par CXSSM lui-même à ce que le clavier envoie
//! (`ctrl-z`, `e`, `space`…), plutôt que par la table de raccourcis de GPUI : un
//! raccourci enregistré en appuyant sur la touche correspond donc toujours à ce que
//! produit le clavier de l'utilisateur, quelle que soit sa disposition (AZERTY…).

use std::time::{Duration, Instant};

use cxssm_core::Shortcuts;
use gpui::{Context, Keystroke};

use super::AppState;

/// Sous X11, une touche maintenue envoie des paires « relâchée + enfoncée » à chaque
/// répétition (et GPUI ne signale pas `is_held`) : un appui qui suit de moins de
/// [`REPEAT_GAP`] le relâchement de la même touche est une répétition.
const REPEAT_GAP: Duration = Duration::from_millis(50);

/// Dernier relâchement de touche (pour reconnaître les répétitions).
#[derive(Debug, Default)]
pub struct KeyTracker {
    last_release: Option<(String, Instant)>,
    /// Relâchement de l'accélérateur, appliqué après [`REPEAT_GAP`] s'il n'est pas
    /// suivi d'une répétition.
    throttle_release: Option<Instant>,
}

impl KeyTracker {
    /// Un relâchement de l'accélérateur attend d'être confirmé (il faut des images).
    pub fn releasing(&self) -> bool {
        self.throttle_release.is_some()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shortcut {
    Undo,
    Redo,
    SelectAll,
    Engine,
    Sweep,
    Throttle,
}

impl Shortcut {
    pub const ALL: [Shortcut; 6] = [Shortcut::Engine, Shortcut::Sweep, Shortcut::Throttle, Shortcut::Undo, Shortcut::Redo, Shortcut::SelectAll];

    /// Nom affiché (français, anglais).
    pub fn label(self) -> (&'static str, &'static str) {
        match self {
            Shortcut::Undo => ("Annuler", "Undo"),
            Shortcut::Redo => ("Rétablir", "Redo"),
            Shortcut::SelectAll => ("Sélectionner tous les sons", "Select all sounds"),
            Shortcut::Engine => ("Démarrer / arrêter le moteur", "Start / stop the engine"),
            Shortcut::Sweep => ("Balayage jusqu'au rupteur", "Sweep up to the limiter"),
            Shortcut::Throttle => ("Accélérer (maintenir)", "Throttle (hold)"),
        }
    }

    pub fn get(self, s: &Shortcuts) -> &str {
        match self {
            Shortcut::Undo => &s.undo,
            Shortcut::Redo => &s.redo,
            Shortcut::SelectAll => &s.select_all,
            Shortcut::Engine => &s.engine,
            Shortcut::Sweep => &s.sweep,
            Shortcut::Throttle => &s.throttle,
        }
    }

    fn slot(self, s: &mut Shortcuts) -> &mut String {
        match self {
            Shortcut::Undo => &mut s.undo,
            Shortcut::Redo => &mut s.redo,
            Shortcut::SelectAll => &mut s.select_all,
            Shortcut::Engine => &mut s.engine,
            Shortcut::Sweep => &mut s.sweep,
            Shortcut::Throttle => &mut s.throttle,
        }
    }
}

/// Identifiant d'une touche : `[ctrl-][alt-][shift-][cmd-]touche`.
pub fn key_id(k: &Keystroke) -> String {
    let m = &k.modifiers;
    let mut id = String::new();
    for (on, name) in [(m.control, "ctrl-"), (m.alt, "alt-"), (m.shift, "shift-"), (m.platform, "cmd-")] {
        if on {
            id.push_str(name);
        }
    }
    id.push_str(&k.key.to_lowercase());
    id
}

/// Touche de modification seule (Maj, Ctrl…) : pas un raccourci à elle seule.
fn is_modifier_key(key: &str) -> bool {
    ["shift", "control", "alt", "super", "meta", "hyper", "iso_level", "caps", "platform", "function", "cmd"]
        .iter()
        .any(|p| key.to_lowercase().starts_with(p))
}

/// Le raccourci n'utilise ni Ctrl, ni Alt, ni Cmd (il taperait du texte).
fn is_plain(id: &str) -> bool {
    !(id.contains("ctrl-") || id.contains("alt-") || id.contains("cmd-"))
}

/// Texte affiché d'un raccourci : `Ctrl+Maj+Z`, `Espace`, `E`.
pub fn pretty(id: &str, fr: bool) -> String {
    if id.is_empty() {
        return if fr { "aucun" } else { "none" }.into();
    }
    let parts: Vec<&str> = id.split('-').collect();
    // Le tiret est aussi une touche (`ctrl--`).
    let (mods, key) = match id.strip_suffix("--") {
        Some(rest) => (rest.split('-').collect::<Vec<_>>(), "-"),
        None => (parts[..parts.len() - 1].to_vec(), parts[parts.len() - 1]),
    };
    let mut out: Vec<String> = mods
        .iter()
        .map(|m| match *m {
            "ctrl" => "Ctrl".into(),
            "alt" => "Alt".into(),
            "shift" => (if fr { "Maj" } else { "Shift" }).into(),
            "cmd" => "⌘".into(),
            o => o.into(),
        })
        .collect();
    out.push(match key {
        "space" => (if fr { "Espace" } else { "Space" }).into(),
        "enter" => (if fr { "Entrée" } else { "Enter" }).into(),
        "escape" => (if fr { "Échap" } else { "Esc" }).into(),
        "backspace" => (if fr { "Retour arrière" } else { "Backspace" }).into(),
        k if k.chars().count() == 1 => k.to_uppercase(),
        k => {
            let mut c = k.chars();
            c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
        }
    });
    out.join("+")
}

impl AppState {
    pub fn shortcut_text(&self, sc: Shortcut) -> String {
        pretty(sc.get(&self.settings.shortcuts), self.lang() == cxssm_core::Language::Fr)
    }

    /// « Modifier » : la prochaine touche pressée devient le raccourci.
    pub fn capture_shortcut(&mut self, sc: Option<Shortcut>, cx: &mut Context<Self>) {
        self.capturing = sc;
        cx.notify();
    }

    pub fn reset_shortcuts(&mut self, cx: &mut Context<Self>) {
        self.settings.shortcuts = Shortcuts::default();
        self.capturing = None;
        self.save_settings();
        cx.notify();
    }

    fn assign_shortcut(&mut self, sc: Shortcut, id: String) {
        // Une touche ne sert qu'à une action : l'ancienne perd ce raccourci.
        for other in Shortcut::ALL {
            if other != sc && other.get(&self.settings.shortcuts) == id {
                other.slot(&mut self.settings.shortcuts).clear();
            }
        }
        *sc.slot(&mut self.settings.shortcuts) = id;
        self.save_settings();
    }

    /// Touche enfoncée. `typing` : un champ de texte a le focus (les raccourcis sans
    /// Ctrl / Alt / Cmd lui sont laissés). Renvoie vrai si la touche est utilisée.
    pub fn key_down(&mut self, k: &Keystroke, held: bool, typing: bool, cx: &mut Context<Self>) -> bool {
        if is_modifier_key(&k.key) {
            return false;
        }
        let id = key_id(k);
        let key = k.key.to_lowercase();
        let held = held || matches!(&self.keys.last_release, Some((last, t)) if *last == key && t.elapsed() < REPEAT_GAP);
        if let Some(sc) = self.capturing {
            if id != "escape" {
                self.assign_shortcut(sc, id);
            }
            self.capturing = None;
            cx.notify();
            return true;
        }
        // Fenêtre modale ouverte : pas d'action sur le projet ni le moteur.
        if self.prefs_open || self.project_dialog {
            return false;
        }
        let Some(sc) = Shortcut::ALL.into_iter().find(|sc| sc.get(&self.settings.shortcuts) == id) else { return false };
        if typing && is_plain(&id) {
            return false;
        }
        match sc {
            Shortcut::Throttle => {
                self.keys.throttle_release = None;
                if !self.engine.throttle {
                    self.press_throttle(cx);
                }
            }
            // Touche maintenue : une seule action, pas une par répétition.
            _ if held => {}
            Shortcut::Undo => self.undo(cx),
            Shortcut::Redo => self.redo(cx),
            Shortcut::SelectAll => self.select_all(cx),
            Shortcut::Engine => self.toggle_engine(cx),
            Shortcut::Sweep => self.start_sweep(cx),
        }
        true
    }

    /// Touche relâchée : fin de l'accélération (après [`REPEAT_GAP`], cf. [`KeyTracker`]).
    pub fn key_up(&mut self, k: &Keystroke, cx: &mut Context<Self>) -> bool {
        let key = k.key.to_lowercase();
        self.keys.last_release = Some((key.clone(), Instant::now()));
        let throttle = &self.settings.shortcuts.throttle;
        // Les modificateurs peuvent avoir changé entre l'appui et le relâchement.
        if self.engine.throttle && !throttle.is_empty() && throttle.rsplit('-').next() == Some(key.as_str()) {
            self.keys.throttle_release = Some(Instant::now());
            cx.notify();
            return true;
        }
        false
    }

    /// À chaque image : applique un relâchement de l'accélérateur confirmé.
    pub(super) fn tick_keys(&mut self) {
        if self.keys.throttle_release.is_some_and(|t| t.elapsed() >= REPEAT_GAP) {
            self.keys.throttle_release = None;
            self.engine.throttle = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_and_labels() {
        let k = Keystroke::parse("ctrl-shift-z").unwrap();
        assert_eq!(key_id(&k), "ctrl-shift-z");
        assert_eq!(key_id(&Keystroke::parse("space").unwrap()), "space");
        assert_eq!(pretty("ctrl-shift-z", true), "Ctrl+Maj+Z");
        assert_eq!(pretty("ctrl-shift-z", false), "Ctrl+Shift+Z");
        assert_eq!(pretty("space", true), "Espace");
        assert_eq!(pretty("e", true), "E");
        assert_eq!(pretty("ctrl--", false), "Ctrl+-");
        assert_eq!(pretty("f5", false), "F5");
        assert_eq!(pretty("", true), "aucun");
        assert!(is_modifier_key("shift_l") && is_modifier_key("control_r") && !is_modifier_key("s"));
        assert!(is_plain("e") && !is_plain("ctrl-e"));
    }
}
