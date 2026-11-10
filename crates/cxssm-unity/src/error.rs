use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("E/S : {0}")]
    Io(#[from] std::io::Error),
    #[error("signature invalide : attendu {expected:?}, trouvé {found:?}")]
    BadSignature { expected: &'static str, found: String },
    #[error("version non supportée : {0}")]
    UnsupportedVersion(u32),
    #[error("compression non supportée : {0}")]
    UnsupportedCompression(u32),
    #[error("décompression : {0}")]
    Decompress(String),
    #[error("fichier tronqué ou corrompu à l'offset {0:#x}")]
    UnexpectedEof(usize),
    #[error("introuvable : {0}")]
    NotFound(String),
}

pub type Result<T> = std::result::Result<T, Error>;
