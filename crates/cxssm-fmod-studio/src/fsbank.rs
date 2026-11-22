//! Encodage FSB5 par la bibliothèque **FSBank** du FMOD Engine (`api/fsbank/lib`), appelée
//! directement : aucun programme externe n'est lancé.
//!
//! Indispensable sous macOS, où l'encodeur fourni est l'application graphique `FSBank.app` :
//! la lancer ouvre sa fenêtre à chaque encodage. Signatures tirées de `fsbank.h` (FMOD 2.x).
//!
//! L'API FSBank a un état global (`FSBank_Init` / `FSBank_Release`) : les encodages sont
//! sérialisés derrière un verrou, et la bibliothèque reste chargée jusqu'à la fin du programme.

use std::ffi::{c_char, c_void, CStr, CString};
use std::path::{Path, PathBuf};
use std::ptr::{null, null_mut};
use std::sync::Mutex;

use libloading::Library;

use crate::{Error, Result};

type R = i32;

const FSBVERSION_FSB5: i32 = 0;
const INIT_DONTLOADCACHEFILES: u32 = 0x08;
const INIT_GENERATEPROGRESSITEMS: u32 = 0x10;
const BUILD_DONTLOOP: u32 = 0x02;
const STATE_FAILED: i32 = 6;

/// Format d'encodage (`FSBANK_FORMAT`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FsbankFormat {
    Pcm = 0,
    Vorbis = 3,
    FAdpcm = 4,
    Opus = 5,
}

impl FsbankFormat {
    /// Depuis le nom accepté par l'outil en ligne de commande (`-format vorbis`…).
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "pcm" => Some(Self::Pcm),
            "vorbis" => Some(Self::Vorbis),
            "fadpcm" => Some(Self::FAdpcm),
            "opus" => Some(Self::Opus),
            _ => None,
        }
    }
}

#[repr(C)]
struct SubSound {
    file_names: *const *const c_char,
    file_data: *const *const c_void,
    file_data_lengths: *const u32,
    num_files: u32,
    override_flags: u32,
    override_quality: u32,
    desired_sample_rate: f32,
    percent_optimized_rate: f32,
    num_syncpoints: u32,
    syncpoint_positions: *mut u32,
    syncpoint_names: *const *const c_char,
    loop_start: u32,
    loop_end: u32,
}

#[repr(C)]
struct ProgressItem {
    sub_sound_index: i32,
    thread_index: i32,
    state: i32,
    state_data: *const c_void,
}

#[repr(C)]
struct StateFailed {
    error_code: i32,
    error_string: [c_char; 256],
}

struct Fsbank {
    api: PathBuf,
    _libs: Vec<Library>,
    init: unsafe extern "system" fn(i32, u32, u32, *const c_char) -> R,
    build: unsafe extern "system" fn(*const SubSound, u32, i32, u32, u32, *const c_char, *const c_char) -> R,
    next_progress: unsafe extern "system" fn(*mut *const ProgressItem) -> R,
    release_progress: unsafe extern "system" fn(*const ProgressItem) -> R,
}

// SAFETY: les pointeurs de fonctions et bibliothèques ne sont utilisés que sous le verrou `FSBANK`.
unsafe impl Send for Fsbank {}

static FSBANK: Mutex<Option<Fsbank>> = Mutex::new(None);

/// Bibliothèque FSBank relative au dossier `api/`, selon le système.
pub fn fsbank_lib_path(api: &Path) -> PathBuf {
    if cfg!(windows) {
        api.join("fsbank/lib/x64/fsbank.dll")
    } else if cfg!(target_os = "macos") {
        api.join("fsbank/lib/libfsbank.dylib")
    } else {
        api.join("fsbank/lib/x86_64/libfsbank.so")
    }
}

fn core_lib_path(api: &Path) -> PathBuf {
    if cfg!(windows) {
        api.join("core/lib/x64/fmod.dll")
    } else if cfg!(target_os = "macos") {
        api.join("core/lib/libfmod.dylib")
    } else {
        api.join("core/lib/x86_64/libfmod.so")
    }
}

impl Fsbank {
    fn load(api: &Path) -> Result<Self> {
        let fsbank_path = fsbank_lib_path(api);
        let dir = fsbank_path.parent().unwrap_or(api);
        let mut libs = Vec::new();
        // SAFETY: bibliothèques FMOD officielles, sans effet de bord connu au chargement.
        // libfsbank dépend de libfmod ; ses encodeurs (libfsbvorbis, libopus) sont préchargés
        // pour être trouvés quel que soit le dossier courant.
        unsafe {
            libs.push(crate::ffi::open_global(&core_lib_path(api))?);
            for name in ["libfsbvorbis", "libopus"] {
                let file = if cfg!(windows) {
                    format!("{name}64.dll")
                } else if cfg!(target_os = "macos") {
                    format!("{name}.dylib")
                } else {
                    format!("{name}.so")
                };
                if let Ok(lib) = crate::ffi::open_global(&dir.join(file)) {
                    libs.push(lib);
                }
            }
            let lib = crate::ffi::open_global(&fsbank_path)?;
            macro_rules! sym {
                ($name:literal) => {
                    *lib.get(concat!($name, "\0").as_bytes()).map_err(|e| Error::Library(format!("{}: {e}", $name)))?
                };
            }
            let s = Self {
                api: api.to_path_buf(),
                init: sym!("FSBank_Init"),
                build: sym!("FSBank_Build"),
                next_progress: sym!("FSBank_FetchNextProgressItem"),
                release_progress: sym!("FSBank_ReleaseProgressItem"),
                _libs: Vec::new(),
            };
            libs.push(lib);
            let cache = std::env::temp_dir().join(format!("cxssm-fsbank-cache-{}", std::process::id()));
            let _ = std::fs::create_dir_all(&cache);
            let cache = c_path(&cache)?;
            let _cwd = LibDirCwd::enter(dir);
            check("FSBank_Init", (s.init)(FSBVERSION_FSB5, INIT_DONTLOADCACHEFILES | INIT_GENERATEPROGRESSITEMS, 1, cache.as_ptr()))?;
            log::info!("FSBank chargé ({})", fsbank_path.display());
            Ok(Self { _libs: libs, ..s })
        }
    }

    /// Vide la file des messages de progression et renvoie le premier message d'échec.
    fn drain_errors(&self) -> Option<String> {
        let mut first = None;
        loop {
            let mut item: *const ProgressItem = null();
            // SAFETY: pointeur de sortie valide ; chaque élément obtenu est rendu à FSBank.
            unsafe {
                if (self.next_progress)(&mut item) != 0 || item.is_null() {
                    break;
                }
                let it = &*item;
                if first.is_none() && it.state == STATE_FAILED && !it.state_data.is_null() {
                    let failed = &*(it.state_data as *const StateFailed);
                    first = Some(CStr::from_ptr(failed.error_string.as_ptr()).to_string_lossy().into_owned());
                }
                (self.release_progress)(item);
            }
        }
        first
    }
}

/// macOS : FSBank charge ses encodeurs par leur seul nom (`dlopen("libfsbvorbis.dylib")`),
/// que `dyld` cherche notamment dans le dossier courant. On s'y place le temps de l'appel
/// (sous le verrou `FSBANK`, avec des chemins absolus), puis on revient au dossier initial.
struct LibDirCwd(Option<PathBuf>);

impl LibDirCwd {
    fn enter(dir: &Path) -> Self {
        if !cfg!(target_os = "macos") {
            return Self(None);
        }
        let previous = std::env::current_dir().ok();
        if previous.is_some() && std::env::set_current_dir(dir).is_ok() {
            Self(previous)
        } else {
            Self(None)
        }
    }
}

impl Drop for LibDirCwd {
    fn drop(&mut self) {
        if let Some(p) = self.0.take() {
            let _ = std::env::set_current_dir(p);
        }
    }
}

fn check(call: &'static str, code: R) -> Result<()> {
    if code == 0 {
        Ok(())
    } else {
        Err(Error::Fmod { call, code })
    }
}

fn c_path(p: &Path) -> Result<CString> {
    CString::new(p.to_string_lossy().as_bytes()).map_err(|_| Error::Library(format!("chemin invalide : {}", p.display())))
}

/// Encode `input` (WAV, OGG, FLAC…) en un fichier FSB5 `output` d'un seul son.
/// `quality` : 1–100 (Vorbis / Opus). `looping: false` évite l'encodage « en boucle ».
pub fn build_fsb(api: &Path, input: &Path, output: &Path, format: FsbankFormat, quality: u32, looping: bool) -> Result<()> {
    let mut guard = FSBANK.lock().unwrap_or_else(|e| e.into_inner());
    if guard.as_ref().is_none_or(|f| f.api != api) {
        // Une seule instance par processus : la précédente reste chargée (FSBank_Release
        // suivi d'un nouvel Init ne pose pas de problème, mais rien ne l'exige ici).
        *guard = Some(Fsbank::load(api)?);
    }
    let fsbank = guard.as_ref().expect("FSBank chargé");
    let abs = |p: &Path| std::path::absolute(p).map_err(|e| Error::Library(format!("{} : {e}", p.display())));
    let input = c_path(&abs(input)?)?;
    let output = c_path(&abs(output)?)?;
    let names = [input.as_ptr()];
    let sub = SubSound {
        file_names: names.as_ptr(),
        file_data: null(),
        file_data_lengths: null(),
        num_files: 1,
        override_flags: 0,
        override_quality: 0,
        desired_sample_rate: 0.0,
        percent_optimized_rate: 0.0,
        num_syncpoints: 0,
        syncpoint_positions: null_mut(),
        syncpoint_names: null(),
        loop_start: 0,
        loop_end: 0,
    };
    let flags = if looping { 0 } else { BUILD_DONTLOOP };
    let dir = fsbank_lib_path(api);
    let cwd = LibDirCwd::enter(dir.parent().unwrap_or(api));
    // SAFETY: `sub` et les chaînes vivent jusqu'au retour de l'appel (synchrone).
    let r = unsafe { (fsbank.build)(&sub, 1, format as i32, flags, quality.clamp(1, 100), null(), output.as_ptr()) };
    drop(cwd);
    let detail = fsbank.drain_errors();
    if r != 0 {
        return Err(Error::Library(format!("FSBank_Build a échoué (FSBANK_RESULT {r}){}", detail.map(|d| format!(" : {d}")).unwrap_or_default())));
    }
    Ok(())
}
