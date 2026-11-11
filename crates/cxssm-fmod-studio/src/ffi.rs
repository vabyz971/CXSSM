//! Appels C vers `libfmod` / `libfmodstudio`, résolus à l'exécution.
//!
//! Signatures tirées de `fmod_studio.h` / `fmod.h` (API C, stable dans FMOD 2.x).

use std::ffi::{c_char, c_void, CStr};
use std::path::Path;
use std::ptr::null_mut;

use libloading::Library;

use crate::{c_str, check, Error, EventInfo, Layer, ParamInfo, Result};

type R = i32;
type Ptr = *mut c_void;

pub(crate) const OUTPUT_AUTODETECT: i32 = 0;
pub(crate) const OUTPUT_NOSOUND_NRT: i32 = 4;
pub(crate) const STUDIO_INIT_SYNCHRONOUS_UPDATE: u32 = 0x2;
const LOAD_MEMORY: i32 = 0; // FMOD_STUDIO_LOAD_MEMORY : FMOD copie les octets
const STOP_IMMEDIATE: i32 = 1;
const TIMEUNIT_PCM: u32 = 2;
const ERR_HEADER_MISMATCH: R = 20;

#[repr(C)]
struct ParamDesc {
    name: *const c_char,
    id: [u32; 2],
    minimum: f32,
    maximum: f32,
    default: f32,
    kind: i32,
    flags: u32,
    guid: [u8; 16],
}

macro_rules! fmod_fns {
    ($( $field:ident : $sym:literal => fn($($arg:ty),*) ),* $(,)?) => {
        /// Bibliothèques FMOD chargées et pointeurs de fonctions résolus.
        pub struct FmodLib {
            _core: Library,
            _studio: Library,
            /// Version d'en-tête acceptée par la bibliothèque (ex. 0x00020400).
            pub version: u32,
            $( $field: unsafe extern "C" fn($($arg),*) -> R, )*
        }

        impl FmodLib {
            fn resolve(core: Library, studio: Library) -> Result<Self> {
                // SAFETY: les symboles sont des fonctions C de l'API FMOD dont les
                // signatures correspondent aux en-têtes officiels.
                unsafe {
                    $(
                        let $field = *studio
                            .get::<unsafe extern "C" fn($($arg),*) -> R>(concat!($sym, "\0").as_bytes())
                            .or_else(|_| core.get(concat!($sym, "\0").as_bytes()))
                            .map_err(|e| Error::Library(format!("{}: {e}", $sym)))?;
                    )*
                    let mut lib = Self { _core: core, _studio: studio, version: 0, $( $field, )* };
                    lib.version = lib.detect_version()?;
                    Ok(lib)
                }
            }
        }
    };
}

fmod_fns! {
    studio_create: "FMOD_Studio_System_Create" => fn(*mut Ptr, u32),
    studio_core: "FMOD_Studio_System_GetCoreSystem" => fn(Ptr, *mut Ptr),
    studio_init: "FMOD_Studio_System_Initialize" => fn(Ptr, i32, u32, u32, Ptr),
    studio_release: "FMOD_Studio_System_Release" => fn(Ptr),
    studio_update: "FMOD_Studio_System_Update" => fn(Ptr),
    studio_load_mem: "FMOD_Studio_System_LoadBankMemory" => fn(Ptr, *const c_char, i32, i32, u32, *mut Ptr),
    studio_get_event: "FMOD_Studio_System_GetEvent" => fn(Ptr, *const c_char, *mut Ptr),
    bank_unload: "FMOD_Studio_Bank_Unload" => fn(Ptr),
    bank_load_samples: "FMOD_Studio_Bank_LoadSampleData" => fn(Ptr),
    bank_event_count: "FMOD_Studio_Bank_GetEventCount" => fn(Ptr, *mut i32),
    bank_event_list: "FMOD_Studio_Bank_GetEventList" => fn(Ptr, *mut Ptr, i32, *mut i32),
    ed_path: "FMOD_Studio_EventDescription_GetPath" => fn(Ptr, *mut c_char, i32, *mut i32),
    ed_param_count: "FMOD_Studio_EventDescription_GetParameterDescriptionCount" => fn(Ptr, *mut i32),
    ed_param: "FMOD_Studio_EventDescription_GetParameterDescriptionByIndex" => fn(Ptr, i32, *mut ParamDesc),
    ed_create: "FMOD_Studio_EventDescription_CreateInstance" => fn(Ptr, *mut Ptr),
    ei_set_param: "FMOD_Studio_EventInstance_SetParameterByName" => fn(Ptr, *const c_char, f32, i32),
    ei_start: "FMOD_Studio_EventInstance_Start" => fn(Ptr),
    ei_stop: "FMOD_Studio_EventInstance_Stop" => fn(Ptr, i32),
    ei_release: "FMOD_Studio_EventInstance_Release" => fn(Ptr),
    ei_group: "FMOD_Studio_EventInstance_GetChannelGroup" => fn(Ptr, *mut Ptr),
    core_set_output: "FMOD_System_SetOutput" => fn(Ptr, i32),
    cg_num_channels: "FMOD_ChannelGroup_GetNumChannels" => fn(Ptr, *mut i32),
    cg_channel: "FMOD_ChannelGroup_GetChannel" => fn(Ptr, i32, *mut Ptr),
    cg_num_groups: "FMOD_ChannelGroup_GetNumGroups" => fn(Ptr, *mut i32),
    cg_group: "FMOD_ChannelGroup_GetGroup" => fn(Ptr, i32, *mut Ptr),
    ch_sound: "FMOD_Channel_GetCurrentSound" => fn(Ptr, *mut Ptr),
    ch_audibility: "FMOD_Channel_GetAudibility" => fn(Ptr, *mut f32),
    ch_pitch: "FMOD_Channel_GetPitch" => fn(Ptr, *mut f32),
    ch_position: "FMOD_Channel_GetPosition" => fn(Ptr, *mut u32, u32),
    cg_pitch: "FMOD_ChannelGroup_GetPitch" => fn(Ptr, *mut f32),
    sound_length: "FMOD_Sound_GetLength" => fn(Ptr, *mut u32, u32),
}

impl FmodLib {
    pub(crate) fn load(api: &Path) -> Result<Self> {
        let (core_path, studio_path) = if cfg!(windows) {
            (api.join("core/lib/x64/fmod.dll"), api.join("studio/lib/x64/fmodstudio.dll"))
        } else if cfg!(target_os = "macos") {
            (api.join("core/lib/libfmod.dylib"), api.join("studio/lib/libfmodstudio.dylib"))
        } else {
            (api.join("core/lib/x86_64/libfmod.so"), api.join("studio/lib/x86_64/libfmodstudio.so"))
        };
        // SAFETY: chargement de bibliothèques FMOD officielles ; leurs constructeurs
        // d'initialisation n'ont pas d'effet de bord connu.
        let core = unsafe { open_global(&core_path) }?;
        let studio = unsafe { open_global(&studio_path) }?;
        let lib = Self::resolve(core, studio)?;
        log::info!("FMOD Studio chargé ({}), version {:#010x}", studio_path.display(), lib.version);
        Ok(lib)
    }

    /// FMOD refuse un `headerversion` différent du sien : on cherche celui qu'il accepte.
    fn detect_version(&self) -> Result<u32> {
        for minor in (0u32..=15).rev() {
            for patch in 0u32..=60 {
                let v = 0x0002_0000 | (minor << 8) | patch;
                let mut sys = null_mut();
                // SAFETY: appel C avec un pointeur de sortie valide.
                let r = unsafe { (self.studio_create)(&mut sys, v) };
                if r == 0 {
                    // SAFETY: système valide tout juste créé.
                    unsafe { (self.studio_release)(sys) };
                    return Ok(v);
                }
                if r != ERR_HEADER_MISMATCH {
                    return Err(Error::Fmod { call: "Studio_System_Create", code: r });
                }
            }
        }
        Err(Error::Library("version de FMOD non reconnue".into()))
    }

    pub(crate) fn create_system(&self) -> Result<(Ptr, Ptr)> {
        let mut sys = null_mut();
        let mut core = null_mut();
        // SAFETY: pointeurs de sortie valides.
        unsafe {
            check("Studio_System_Create", (self.studio_create)(&mut sys, self.version))?;
            check("Studio_System_GetCoreSystem", (self.studio_core)(sys, &mut core))?;
        }
        Ok((sys, core))
    }

    pub(crate) fn set_output(&self, core: Ptr, out: i32) -> R {
        // SAFETY: système core valide, avant initialisation.
        unsafe { (self.core_set_output)(core, out) }
    }

    pub(crate) fn initialize(&self, sys: Ptr, channels: i32, studio_flags: u32) -> R {
        // SAFETY: système valide, extradriverdata nul.
        unsafe { (self.studio_init)(sys, channels, studio_flags, 0, null_mut()) }
    }

    pub(crate) fn release_system(&self, sys: Ptr) {
        // SAFETY: appelé une seule fois (Drop).
        unsafe { (self.studio_release)(sys) };
    }

    pub(crate) fn update(&self, sys: Ptr) {
        // SAFETY: système valide.
        unsafe { (self.studio_update)(sys) };
    }

    pub(crate) fn load_bank_memory(&self, sys: Ptr, bytes: &[u8]) -> Result<Ptr> {
        let mut bank = null_mut();
        // SAFETY: mode LOAD_MEMORY : FMOD copie le tampon avant de rendre la main.
        unsafe {
            check(
                "Studio_System_LoadBankMemory",
                (self.studio_load_mem)(sys, bytes.as_ptr().cast(), bytes.len() as i32, LOAD_MEMORY, 0, &mut bank),
            )?;
        }
        Ok(bank)
    }

    pub(crate) fn bank_unload(&self, bank: Ptr) {
        // SAFETY: banque valide.
        unsafe { (self.bank_unload)(bank) };
    }

    pub(crate) fn bank_load_sample_data(&self, bank: Ptr) {
        // SAFETY: banque valide.
        unsafe { (self.bank_load_samples)(bank) };
    }

    pub(crate) fn event_list(&self, bank: Ptr) -> Result<Vec<Ptr>> {
        let mut n = 0;
        // SAFETY: tampon dimensionné d'après GetEventCount.
        unsafe {
            check("Bank_GetEventCount", (self.bank_event_count)(bank, &mut n))?;
            let mut list = vec![null_mut(); n.max(0) as usize];
            let mut got = 0;
            check("Bank_GetEventList", (self.bank_event_list)(bank, list.as_mut_ptr(), n, &mut got))?;
            list.truncate(got.max(0) as usize);
            Ok(list)
        }
    }

    pub(crate) fn event_info(&self, desc: Ptr) -> EventInfo {
        let mut buf = vec![0 as c_char; 512];
        let mut count = 0;
        // SAFETY: tampon de 512 octets, description d'évènement valide.
        let path = unsafe {
            (self.ed_path)(desc, buf.as_mut_ptr(), buf.len() as i32, null_mut());
            CStr::from_ptr(buf.as_ptr()).to_string_lossy().into_owned()
        };
        let mut params = Vec::new();
        // SAFETY: indices bornés par GetParameterDescriptionCount.
        unsafe {
            (self.ed_param_count)(desc, &mut count);
            for i in 0..count {
                let mut pd: ParamDesc = std::mem::zeroed();
                if (self.ed_param)(desc, i, &mut pd) == 0 {
                    params.push(ParamInfo {
                        name: c_str(pd.name),
                        min: pd.minimum,
                        max: pd.maximum,
                        default: pd.default,
                    });
                }
            }
        }
        EventInfo { path, params }
    }

    pub(crate) fn get_event(&self, sys: Ptr, path: &CStr) -> Result<Ptr> {
        let mut desc = null_mut();
        // SAFETY: chaîne C valide, pointeur de sortie valide.
        unsafe { check("Studio_System_GetEvent", (self.studio_get_event)(sys, path.as_ptr(), &mut desc))? };
        Ok(desc)
    }

    pub(crate) fn create_instance(&self, desc: Ptr) -> Result<Ptr> {
        let mut inst = null_mut();
        // SAFETY: description valide.
        unsafe { check("EventDescription_CreateInstance", (self.ed_create)(desc, &mut inst))? };
        Ok(inst)
    }

    pub(crate) fn set_param(&self, inst: Ptr, name: &CStr, value: f32) {
        // SAFETY: instance valide ; ignoreseekspeed = 1 pour un changement immédiat.
        unsafe { (self.ei_set_param)(inst, name.as_ptr(), value, 1) };
    }

    pub(crate) fn start(&self, inst: Ptr) {
        // SAFETY: instance valide.
        unsafe { (self.ei_start)(inst) };
    }

    pub(crate) fn stop_release(&self, inst: Ptr) {
        // SAFETY: instance valide, libérée une seule fois (Instance est consommée).
        unsafe {
            (self.ei_stop)(inst, STOP_IMMEDIATE);
            (self.ei_release)(inst);
        }
    }

    pub(crate) fn layers(&self, inst: Ptr) -> Vec<Layer> {
        let mut group = null_mut();
        let mut out = Vec::new();
        // SAFETY: instance valide ; le groupe n'existe qu'une fois l'évènement démarré.
        unsafe {
            if (self.ei_group)(inst, &mut group) == 0 && !group.is_null() {
                self.walk(group, &mut out, 0, 1.);
            }
        }
        out
    }

    /// Parcourt récursivement les canaux d'un groupe (les instruments FMOD sont des sous-groupes).
    /// `parent_pitch` : produit des hauteurs des groupes parents (automations d'instrument).
    unsafe fn walk(&self, group: Ptr, out: &mut Vec<Layer>, depth: u32, parent_pitch: f32) {
        if depth > 16 {
            return;
        }
        let mut n = 0;
        // SAFETY: groupe valide, indices bornés par les compteurs FMOD.
        unsafe {
            let mut gp = 1f32;
            (self.cg_pitch)(group, &mut gp);
            let group_pitch = parent_pitch * gp;
            (self.cg_num_channels)(group, &mut n);
            for i in 0..n {
                let mut ch = null_mut();
                let mut snd = null_mut();
                if (self.cg_channel)(group, i, &mut ch) != 0 || (self.ch_sound)(ch, &mut snd) != 0 || snd.is_null() {
                    continue;
                }
                let (mut len, mut vol, mut pitch, mut pos) = (0u32, 0f32, 1f32, 0u32);
                (self.sound_length)(snd, &mut len, TIMEUNIT_PCM);
                (self.ch_audibility)(ch, &mut vol);
                (self.ch_pitch)(ch, &mut pitch);
                (self.ch_position)(ch, &mut pos, TIMEUNIT_PCM);
                out.push(Layer { length_pcm: len, volume: vol, pitch: pitch * group_pitch, position_pcm: pos });
            }
            let mut g = 0;
            (self.cg_num_groups)(group, &mut g);
            for i in 0..g {
                let mut sub = null_mut();
                if (self.cg_group)(group, i, &mut sub) == 0 && !sub.is_null() {
                    self.walk(sub, out, depth + 1, group_pitch);
                }
            }
        }
    }
}

/// Ouvre une bibliothèque en rendant ses symboles globaux (libfmodstudio dépend de libfmod).
unsafe fn open_global(path: &Path) -> Result<Library> {
    #[cfg(unix)]
    let lib = {
        use libloading::os::unix::{Library as UnixLib, RTLD_GLOBAL, RTLD_NOW};
        // SAFETY: voir l'appelant.
        unsafe { UnixLib::open(Some(path), RTLD_NOW | RTLD_GLOBAL) }.map(Library::from)
    };
    #[cfg(not(unix))]
    // SAFETY: voir l'appelant.
    let lib = unsafe { Library::new(path) };
    lib.map_err(|e| Error::Library(format!("{} : {e}", path.display())))
}
