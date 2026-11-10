use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("signature invalide : {0}")]
    BadSignature(String),
    #[error("données tronquées à l'offset {0:#x}")]
    UnexpectedEof(usize),
    #[error("chunk introuvable : {0}")]
    ChunkNotFound(&'static str),
    #[error("index d'échantillon invalide : {0}")]
    BadIndex(usize),
    #[error("incompatible : {0}")]
    Incompatible(String),
}

pub type Result<T> = std::result::Result<T, Error>;
