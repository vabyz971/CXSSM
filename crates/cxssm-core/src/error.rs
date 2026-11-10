use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("bundle : {0}")]
    Unity(#[from] cxssm_unity::Error),
    #[error("FMOD : {0}")]
    Fmod(#[from] cxssm_fmod::Error),
    #[error("audio : {0}")]
    Audio(String),
    #[error("encodeur : {0}")]
    Encoder(String),
    #[error("aucune banque FMOD trouvée dans ce bundle")]
    NoBank,
}

pub type Result<T> = std::result::Result<T, Error>;
