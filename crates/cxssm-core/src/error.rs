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
    #[error("FMOD Studio : {0}")]
    FmodStudio(#[from] cxssm_fmod_studio::Error),
    #[error("encodeur : {0}")]
    Encoder(String),
    #[error("mise à jour : {0}")]
    Update(String),
    #[error("projet : {0}")]
    Project(String),
    #[error("aucune banque FMOD trouvée dans ce bundle")]
    NoBank,
}

pub type Result<T> = std::result::Result<T, Error>;
