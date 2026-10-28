//! Whole-volume samples for the Overview header and snapshot metadata.
use serde::{Deserialize, Serialize};
use std::{os::unix::fs::MetadataExt, path::Path};
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiskSample {
    pub device: u64,
    pub total: u64,
    pub used: u64,
}
impl DiskSample {
    pub fn capture(path: &Path) -> Option<Self> {
        let (total, free) = crate::scan::disk_space(path)?;
        Some(Self {
            device: std::fs::metadata(path).ok()?.dev(),
            total,
            used: total.saturating_sub(free),
        })
    }
}
