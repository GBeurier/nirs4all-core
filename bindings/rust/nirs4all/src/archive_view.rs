//! Read-only view of a selected native Package2/3 Core V1 closure.

use std::fmt;

use serde_json::Value;

/// A malformed persisted manifest prevented a host projection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArchiveViewError(String);

impl fmt::Display for ArchiveViewError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "archive view refusal: {}", self.0)
    }
}

impl std::error::Error for ArchiveViewError {}

/// Native Package2/3 view over a selected neutral Core V1 scientific closure.
pub fn native_package_view(
    archive: &crate::n4a_native_package::LoadedNativePackage,
) -> Result<Value, ArchiveViewError> {
    Ok(archive.view())
}
