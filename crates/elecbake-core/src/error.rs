use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("cannot read {path}: {message}")]
    Altium { path: PathBuf, message: String },
    #[error("unsupported file type: {0} (expected .PrjPcb or .SchDoc)")]
    Unsupported(PathBuf),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, Error>;
