//! Whole-volume usage is an inexpensive hint, not proof of folder freshness.
use serde::{Deserialize, Serialize};
use std::{
    os::unix::fs::MetadataExt,
    path::Path,
    time::{Duration, Instant},
};
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
    fn differs(self, other: Self) -> bool {
        self.device != other.device
            || self.total != other.total
            || self.used.abs_diff(other.used) >= 64 * 1024 * 1024
    }
}
#[derive(Default)]
pub struct AutoRefresh {
    baseline: Option<DiskSample>,
    pending: Option<Instant>,
    not_before: Option<Instant>,
    initialized: bool,
}
impl AutoRefresh {
    pub fn restore(&mut self, baseline: Option<DiskSample>) {
        self.baseline = baseline;
        self.pending = None;
        self.initialized = true;
    }
    pub fn observe(&mut self, sample: DiskSample, now: Instant) {
        if !self.initialized {
            return;
        }
        if self.baseline.is_none_or(|b| b.differs(sample)) {
            self.pending.get_or_insert(now);
        } else {
            self.pending = None;
        }
    }
    pub fn due(&self, now: Instant) -> bool {
        self.pending
            .is_some_and(|p| now.duration_since(p) >= Duration::from_secs(8))
            && self.not_before.is_none_or(|t| now >= t)
    }
    pub fn pending(&self) -> bool {
        self.pending.is_some()
    }
    pub fn completed(&mut self, sample: Option<DiskSample>, now: Instant) {
        self.restore(sample);
        self.not_before = Some(now + Duration::from_secs(60));
    }
    pub fn failed(&mut self, now: Instant) {
        self.pending = None;
        self.not_before = Some(now + Duration::from_secs(300));
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn sample(used: u64) -> DiskSample {
        DiskSample {
            device: 1,
            total: 1_000_000_000,
            used,
        }
    }
    #[test]
    fn noise_debounce_reversion_and_cooldown() {
        let mut p = AutoRefresh::default();
        let t = Instant::now();
        let mib = 1024 * 1024;
        p.restore(Some(sample(0)));
        p.observe(sample(mib), t);
        assert!(!p.pending());
        p.observe(sample(64 * mib), t);
        assert!(!p.due(t));
        p.observe(sample(90 * mib), t + Duration::from_secs(4));
        assert!(p.due(t + Duration::from_secs(8)));
        p.observe(sample(0), t + Duration::from_secs(9));
        assert!(!p.pending());
        p.completed(Some(sample(0)), t);
        p.observe(sample(64 * mib), t);
        assert!(!p.due(t + Duration::from_secs(59)));
        assert!(p.due(t + Duration::from_secs(60)));
    }
    #[test]
    fn legacy_disk_identity_and_error_backoff() {
        let mut p = AutoRefresh::default();
        let t = Instant::now();
        p.observe(sample(0), t);
        assert!(!p.pending());
        p.restore(None);
        p.observe(sample(0), t);
        assert!(p.due(t + Duration::from_secs(8)));
        p.failed(t);
        p.observe(sample(0), t);
        assert!(!p.due(t + Duration::from_secs(299)));
        assert!(p.due(t + Duration::from_secs(300)));
        p.restore(Some(sample(0)));
        let mut s = sample(0);
        s.device = 2;
        p.observe(s, t);
        assert!(p.pending());
    }
}
