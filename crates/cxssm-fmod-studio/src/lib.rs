//! # cxssm-fmod-studio
//!
//! Liaison **minimale** vers l'API FMOD Studio, chargée dynamiquement depuis le
//! *FMOD Engine* installé par l'utilisateur (rien de FMOD n'est compilé ni
//! redistribué avec CXSSM).
//!
//! Elle sert à rejouer les évènements moteur du jeu exactement comme FMOD le fait
//! en jeu, et à observer quels sons sont mélangés pour un régime donné.
//!
//! Tout le code `unsafe` du projet est confiné dans [`ffi`] ; l'API publique est sûre.

mod ffi;

use std::ffi::{c_void, CStr, CString};
use std::path::Path;
use std::sync::Arc;

use thiserror::Error;

pub use ffi::FmodLib;

#[derive(Debug, Error)]
pub enum Error {
    #[error("bibliothèque FMOD introuvable ou invalide : {0}")]
    Library(String),
    #[error("{call} a échoué (FMOD_RESULT {code})")]
    Fmod { call: &'static str, code: i32 },
}

pub type Result<T> = std::result::Result<T, Error>;

fn check(call: &'static str, code: i32) -> Result<()> {
    if code == 0 {
        Ok(())
    } else {
        Err(Error::Fmod { call, code })
    }
}

/// Sortie audio du système FMOD.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Output {
    /// Carte son par défaut (écoute).
    Speakers,
    /// Aucune sortie, mixage non temps réel : pour analyser très vite (balayage des RPM).
    Analysis,
}

/// Description d'un paramètre d'évènement (ex. `RPM` 0..10500).
#[derive(Debug, Clone, PartialEq)]
pub struct ParamInfo {
    pub name: String,
    pub min: f32,
    pub max: f32,
    pub default: f32,
}

#[derive(Debug, Clone)]
pub struct EventInfo {
    /// Chemin complet, ex. `event:/GameEvents/Engines/eng_V8_MGTC`.
    pub path: String,
    pub params: Vec<ParamInfo>,
}

impl EventInfo {
    /// Dernier segment du chemin (`eng_V8_MGTC`).
    pub fn short_name(&self) -> &str {
        self.path.rsplit('/').next().unwrap_or(&self.path)
    }

    pub fn param(&self, name: &str) -> Option<&ParamInfo> {
        self.params.iter().find(|p| p.name.eq_ignore_ascii_case(name))
    }
}

/// Son en cours de lecture dans une instance (une « couche » du mélange moteur).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Layer {
    /// Longueur en échantillons PCM : sert à retrouver le son dans le FSB.
    pub length_pcm: u32,
    /// Volume effectif (audibilité FMOD, toutes atténuations comprises).
    pub volume: f32,
    /// Hauteur relative (1.0 = vitesse d'origine), groupes parents compris.
    pub pitch: f32,
    /// Tête de lecture dans le son, en échantillons PCM.
    pub position_pcm: u32,
}

/// Un système FMOD Studio (avec ses banques chargées).
pub struct System {
    lib: Arc<FmodLib>,
    raw: *mut c_void,
}

// L'API FMOD Studio est thread-safe (sauf FMOD_STUDIO_INIT_SYNCHRONOUS_UPDATE, que
// l'on n'utilise qu'avec un système possédé par un seul thread).
unsafe impl Send for System {}

/// Banque chargée dans un [`System`].
#[derive(Debug, Clone, Copy)]
pub struct Bank(*mut c_void);
unsafe impl Send for Bank {}

/// Instance d'évènement en cours.
#[derive(Debug)]
pub struct Instance(*mut c_void);
unsafe impl Send for Instance {}

impl System {
    pub fn new(lib: Arc<FmodLib>, output: Output) -> Result<Self> {
        let (raw, core) = lib.create_system()?;
        let (out_type, studio_flags) = match output {
            Output::Speakers => (ffi::OUTPUT_AUTODETECT, 0),
            Output::Analysis => (ffi::OUTPUT_NOSOUND_NRT, ffi::STUDIO_INIT_SYNCHRONOUS_UPDATE),
        };
        let sys = Self { lib, raw };
        check("System_SetOutput", sys.lib.set_output(core, out_type))?;
        check("Studio_System_Initialize", sys.lib.initialize(raw, 512, studio_flags))?;
        Ok(sys)
    }

    /// Charge une banque depuis la mémoire (les octets sont copiés par FMOD).
    pub fn load_bank(&self, bytes: &[u8]) -> Result<Bank> {
        let bank = self.lib.load_bank_memory(self.raw, bytes)?;
        Ok(Bank(bank))
    }

    pub fn unload_bank(&self, bank: Bank) {
        self.lib.bank_unload(bank.0);
    }

    /// Charge les données audio d'une banque (évite la latence au démarrage d'un évènement).
    pub fn load_sample_data(&self, bank: Bank) {
        self.lib.bank_load_sample_data(bank.0);
    }

    pub fn update(&self) {
        self.lib.update(self.raw);
    }

    pub fn events(&self, bank: Bank) -> Result<Vec<EventInfo>> {
        let descs = self.lib.event_list(bank.0)?;
        Ok(descs.into_iter().map(|d| self.lib.event_info(d)).collect())
    }

    pub fn create_instance(&self, path: &str) -> Result<Instance> {
        let path = CString::new(path).map_err(|e| Error::Library(e.to_string()))?;
        let desc = self.lib.get_event(self.raw, &path)?;
        Ok(Instance(self.lib.create_instance(desc)?))
    }

    pub fn set_param(&self, inst: &Instance, name: &str, value: f32) {
        if let Ok(name) = CString::new(name) {
            self.lib.set_param(inst.0, &name, value);
        }
    }

    pub fn start(&self, inst: &Instance) {
        self.lib.start(inst.0);
    }

    /// Arrête et libère l'instance.
    pub fn release(&self, inst: Instance) {
        self.lib.stop_release(inst.0);
    }

    /// Sons actuellement joués par l'instance.
    pub fn layers(&self, inst: &Instance) -> Vec<Layer> {
        self.lib.layers(inst.0)
    }
}

impl Drop for System {
    fn drop(&mut self) {
        self.lib.release_system(self.raw);
    }
}

/// Lit une chaîne C renvoyée par FMOD.
fn c_str(p: *const std::ffi::c_char) -> String {
    if p.is_null() {
        String::new()
    } else {
        // SAFETY: FMOD renvoie des chaînes terminées par \0 valides pendant l'appel.
        unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned()
    }
}

/// Charge la bibliothèque FMOD depuis le dossier `api/` du FMOD Engine.
pub fn load_library(api_dir: &Path) -> Result<Arc<FmodLib>> {
    FmodLib::load(api_dir).map(Arc::new)
}
