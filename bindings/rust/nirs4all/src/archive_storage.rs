//! Shared archive storage errors.

use std::fmt;
use std::path::PathBuf;

#[derive(Debug)]
pub enum ArchiveStoreError {
    Io(std::io::Error),
    Format(String),
    Integrity(String),
    UnsupportedCapability(String),
    AlreadyExists(PathBuf),
    /// The archive is visible at `path`, but post-publication durability or
    /// temporary-file cleanup failed. Callers must not retry a write blindly.
    PublishedWithCleanupError {
        path: PathBuf,
        detail: String,
    },
}

impl fmt::Display for ArchiveStoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(f, "archive I/O error: {error}"),
            Self::Format(detail) => write!(f, "archive V1 format refusal: {detail}"),
            Self::Integrity(detail) => write!(f, "archive V1 integrity refusal: {detail}"),
            Self::UnsupportedCapability(detail) => {
                write!(f, "archive V1 unsupported capability: {detail}")
            }
            Self::AlreadyExists(path) => {
                write!(f, "archive target already exists: {}", path.display())
            }
            Self::PublishedWithCleanupError { path, detail } => write!(
                f,
                "archive was published at {} but final cleanup/durability failed: {detail}",
                path.display()
            ),
        }
    }
}

impl std::error::Error for ArchiveStoreError {}

impl From<std::io::Error> for ArchiveStoreError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}
