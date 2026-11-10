//! # cxssm-fmod
//!
//! ```text
//! .bank (RIFF "FEV ")
//!   ├─ chunks d'évènements / métadonnées (LIST, PROJ, EVNT, …)  → conservés tels quels
//!   └─ chunk "SND " ─► FSB5 (sound bank)
//!                        ├─ en-tête
//!                        ├─ en-têtes d'échantillons (fréquence, canaux, nb samples, …)
//!                        ├─ table des noms
//!                        └─ données audio (Vorbis / FADPCM / PCM …)
//! ```
//! Les évènements FMOD (ex. le moteur piloté par le paramètre RPM) référencent
//! les sons **par index** dans le FSB : on remplace donc un échantillon en
//! conservant sa position et son nom.

pub mod bank;
pub mod error;
pub mod fsb5;
pub mod vorbis;

pub use bank::Bank;
pub use error::{Error, Result};
pub use fsb5::{Codec, Fsb5, Sample};
