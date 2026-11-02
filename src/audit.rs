//! Observations are daemon-owned. A kernel verification bit is never a write receipt.
use crate::{
    device::{Device, read},
    policy::{Policy, Snapshot},
};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    time::{SystemTime, UNIX_EPOCH},
};

pub fn unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Report {
    pub schema: u32,
    pub daemon_instance: String,
    pub session_id: u64,
    pub active: bool,
    pub classification: String,
    pub reason: String,
    pub write_attempts: u64,
    pub writes_succeeded: u64,
    pub last_write_result: String,
    pub last_write_error: Option<String>,
    pub last_write_at_ms: Option<u64>,
    pub settle_deadline_ms: Option<u64>,
    pub observed: Snapshot,
}
impl Report {
    pub fn new(instance: String) -> Self {
        Self {
            schema: 1,
            daemon_instance: instance,
            session_id: 0,
            active: false,
            classification: "unknown".into(),
            reason: "starting".into(),
            write_attempts: 0,
            writes_succeeded: 0,
            last_write_result: "not_attempted".into(),
            last_write_error: None,
            last_write_at_ms: None,
            settle_deadline_ms: None,
            observed: Snapshot::default(),
        }
    }
    pub fn observe(&mut self, p: &Policy, s: &Snapshot, wall_start: u64) {
        if self.session_id != p.session_id {
            self.write_attempts = 0;
            self.writes_succeeded = 0;
            self.last_write_result = "not_attempted".into();
            self.last_write_error = None;
            self.last_write_at_ms = None;
        }
        self.session_id = p.session_id;
        self.active = p.active;
        self.classification = p.classification(s).into();
        self.reason = p.reason(s).into();
        self.settle_deadline_ms = p.deadline().map(|d| wall_start + d);
        self.observed = s.clone();
    }
    pub fn record(&mut self, outcome: &Result<bool>) {
        match outcome {
            Ok(false) => {
                self.last_write_result = "cancelled_by_recheck".into();
                self.last_write_error = None;
            }
            Ok(true) => {
                self.write_attempts += 1;
                self.writes_succeeded += 1;
                self.last_write_result = "success".into();
                self.last_write_error = None;
                self.last_write_at_ms = Some(unix_ms());
            }
            Err(error) => {
                self.write_attempts += 1;
                self.last_write_result = "failed".into();
                self.last_write_error = Some(format!("{error:#}"));
                self.last_write_at_ms = Some(unix_ms());
            }
        }
    }
    pub fn publish(&self, d: &Device, last: &mut Option<Report>) -> Result<()> {
        if last.as_ref() == Some(self) {
            return Ok(());
        }
        let temp = d.module.join("session.json.tmp");
        fs::write(&temp, serde_json::to_vec(self)?)?;
        fs::rename(temp, d.module.join("session.json"))?;
        *last = Some(self.clone());
        Ok(())
    }
}
/// Reject a previous daemon's report, stopped-service records, and malformed input.
pub fn current(d: &Device, running: bool) -> Option<Report> {
    if !running {
        return None;
    }
    let instance = read(d.module.join("daemon.instance"));
    let report: Report = serde_json::from_str(&read(d.module.join("session.json"))).ok()?;
    (report.schema == 1 && !instance.is_empty() && report.daemon_instance == instance)
        .then_some(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn kernel_verification_does_not_imply_module_write() {
        let mut p = Policy::default();
        let s = Snapshot {
            status: "Charging".into(),
            enabled: true,
            svid: "2717".into(),
            verified: true,
            ..Snapshot::default()
        };
        p.update(0, &s);
        let mut r = Report::new("test".into());
        r.observe(&p, &s, 1000);
        assert_eq!(r.classification, "xiaomi");
        assert_eq!(r.writes_succeeded, 0);
    }
    #[test]
    fn resume_keeps_receipts_but_new_attach_clears_them() {
        let mut p = Policy::default();
        let mut s = Snapshot {
            status: "Charging".into(),
            enabled: true,
            ..Snapshot::default()
        };
        p.update(0, &s);
        let mut r = Report::new("test".into());
        r.observe(&p, &s, 0);
        r.record(&Ok(true));
        s.enabled = false;
        p.update(10, &s);
        r.observe(&p, &s, 0);
        s.enabled = true;
        p.update(20, &s);
        r.observe(&p, &s, 0);
        assert_eq!(r.writes_succeeded, 1);
        s.status = "Discharging".into();
        p.update(30, &s);
        r.observe(&p, &s, 0);
        s.status = "Charging".into();
        p.update(40, &s);
        r.observe(&p, &s, 0);
        assert_eq!(r.writes_succeeded, 0);
        assert_eq!(r.session_id, 2);
    }
    #[test]
    fn cancelled_and_failed_writes_are_not_success_receipts() {
        let mut r = Report::new("test".into());
        r.record(&Ok(false));
        assert_eq!(r.write_attempts, 0);
        r.record(&Err(anyhow::anyhow!("permission denied")));
        assert_eq!(r.write_attempts, 1);
        assert_eq!(r.writes_succeeded, 0);
        assert!(r.last_write_error.unwrap().contains("permission denied"));
    }
    #[test]
    fn stale_or_stopped_daemon_report_is_hidden() {
        let root = std::env::temp_dir().join(format!("yafreepps-audit-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let d = Device::plain(root.clone(), root.clone());
        let r = Report::new("old".into());
        fs::write(root.join("session.json"), serde_json::to_vec(&r).unwrap()).unwrap();
        fs::write(root.join("daemon.instance"), "new").unwrap();
        assert!(current(&d, true).is_none());
        fs::write(root.join("daemon.instance"), "old").unwrap();
        assert!(current(&d, true).is_some());
        assert!(current(&d, false).is_none());
        fs::remove_dir_all(root).unwrap();
    }
}
