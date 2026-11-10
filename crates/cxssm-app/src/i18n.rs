//! Traductions de l'interface (français / anglais).
//!
//! Chaque texte est écrit dans les deux langues là où il est utilisé :
//! `self.t("Ouvrir", "Open")`. Simple à lire et impossible à désynchroniser.

use cxssm_core::{Language, SoundCategory};

pub fn tr(lang: Language, fr: &'static str, en: &'static str) -> &'static str {
    match lang {
        Language::Fr => fr,
        Language::En => en,
    }
}

pub fn category(lang: Language, c: SoundCategory) -> &'static str {
    use SoundCategory::*;
    match c {
        Engine => tr(lang, "Moteur", "Engine"),
        Turbo => "Turbo",
        Exhaust => tr(lang, "Échappement", "Exhaust"),
        Music => tr(lang, "Musique", "Music"),
        Voice => tr(lang, "Voix", "Voices"),
        Ui => tr(lang, "Interface", "UI"),
        Other => tr(lang, "Autre", "Other"),
    }
}
