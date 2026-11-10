//! # cxssm-unity
//!
//! Lecture et réécriture des AssetBundles Unity au format **UnityFS**
//! (celui de `fmod_assets_all.bundle` dans CarX Street).
//!
//! Chaîne de traitement :
//! ```text
//! UnityFS (.bundle) ──► blocs compressés LZ4/LZMA ──► nœuds (fichiers virtuels)
//!                                                     └─► SerializedFile "CAB-xxxx"
//!                                                          └─► objets (TextAsset = banque FMOD)
//! ```

pub mod bundle;
pub mod error;
mod io;
pub mod serialized;
pub mod text_asset;

pub use bundle::{Bundle, Compression, Node};
pub use error::{Error, Result};
pub use io::Endian;
pub use serialized::{ObjectInfo, SerializedFile};
pub use text_asset::TextAsset;

/// ID de classe Unity d'un `TextAsset` (FMOD for Unity y stocke les `.bank`).
pub const CLASS_TEXT_ASSET: i32 = 49;
