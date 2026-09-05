//! File-handle occupancy (Windows).
//!
//! Full `NtQuerySystemInformation(SystemHandleInformation)` scanning is deferred:
//! it needs careful privilege handling and can be expensive. The trait is wired;
//! callers get a clear privilege / not-implemented message for now.

use std::path::Path;

use crate::domain::{OpenFile, PathHolder, Pid, ProbeError};
use crate::platform::HandleProbe;

pub struct WindowsHandleProbe;

impl HandleProbe for WindowsHandleProbe {
    fn open_files(&self, _pid: Pid) -> Result<Vec<OpenFile>, ProbeError> {
        Err(ProbeError::msg(
            "file handle listing is not implemented yet on Windows (P0 follow-up)",
        ))
    }

    fn holders_of_path(&self, _path: &Path) -> Result<Vec<PathHolder>, ProbeError> {
        Err(ProbeError::msg(
            "path occupancy scan is not implemented yet on Windows (P0 follow-up); often requires elevation",
        ))
    }
}
