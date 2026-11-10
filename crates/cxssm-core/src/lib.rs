//! # cxssm-core
//!
//! Couche métier indépendante de l'interface : l'application GPUI et la CLI
//! l'utilisent toutes les deux.
//!
//! - [`project::ModProject`] : bundle ouvert + liste des sons + remplacements en attente
//! - [`audio`] : analyse des fichiers audio fournis par l'utilisateur
//! - [`encoder`] : encodage FSB5 (PCM natif, ou `fsbankcl` de FMOD pour Vorbis/FADPCM)
//! - [`preview`] : décodage d'un son du jeu pour l'écouter ou l'exporter
//! - [`engine`] : simulateur moteur (API FMOD Studio) et carte des sons par régime
//! - [`export`] : écriture du bundle modifié dans un dossier de sortie (Steam Workshop)

pub mod audio;
pub mod encoder;
pub mod engine;
pub mod error;
pub mod export;
pub mod preview;
pub mod project;

pub use error::{Error, Result};
pub use project::{ModProject, SoundCategory, SoundRef, SoundStatus};
