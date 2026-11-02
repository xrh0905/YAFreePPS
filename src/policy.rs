//! Pure policy: no sysfs writes or subprocesses. Time is monotonic milliseconds.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Snapshot {
    pub status: String,
    pub enabled: bool,
    pub svid: String,
    pub real_type: String,
    pub verified: bool,
    pub apdo: u32,
    pub capacity: Option<u32>,
    pub qct: Option<u32>,
    pub stopped: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Unlock,
}

/// A Xiaomi SVID veto survives pause/resume, but never physical detach.
#[derive(Clone, Debug)]
pub struct Policy {
    pub settle_ms: u64,
    pub stable_ms: u64,
    pub(crate) active: bool,
    pub(crate) session_id: u64,
    enabled: bool,
    started: u64,
    candidate: Option<u64>,
    xiaomi: bool,
    unlocked: bool,
    public_ready: bool,
    due: Option<u64>,
}
impl Default for Policy {
    fn default() -> Self {
        Self {
            settle_ms: 2500,
            stable_ms: 1500,
            active: false,
            session_id: 0,
            enabled: false,
            started: 0,
            candidate: None,
            xiaomi: false,
            unlocked: false,
            public_ready: false,
            due: None,
        }
    }
}
impl Policy {
    pub fn with_timing(settle_ms: u64, stable_ms: u64) -> Self {
        Self {
            settle_ms,
            stable_ms,
            ..Self::default()
        }
    }
    fn reset(&mut self, now: u64) {
        self.started = now;
        self.candidate = None;
        self.xiaomi = false;
        self.unlocked = false;
        self.public_ready = false;
        self.due = None;
    }
    pub fn reconfigure(&mut self, now: u64, settle: u64, stable: u64) {
        self.settle_ms = settle;
        self.stable_ms = stable;
        self.started = now;
        self.candidate = None;
        self.public_ready = false;
        self.due = None;
    }
    pub fn invalidate(&mut self) {
        self.reset(0);
        self.active = false;
    }
    /// Retry only on a subsequent event, never in a busy loop.
    pub fn unlock_failed(&mut self) {
        self.unlocked = false;
    }
    pub fn classification(&self, s: &Snapshot) -> &'static str {
        if !self.active {
            "disconnected"
        } else if self.xiaomi {
            "xiaomi"
        } else if s.svid == "0000" && s.real_type == "PD_PPS" {
            if self.public_ready {
                "public_pps"
            } else {
                "public_candidate"
            }
        } else {
            "unknown"
        }
    }
    pub fn reason(&self, s: &Snapshot) -> &'static str {
        if !self.active {
            "disconnected"
        } else if !s.enabled {
            "paused"
        } else if s.status != "Charging" {
            "unknown_status"
        } else if self.xiaomi {
            "xiaomi_kernel"
        } else if s.svid.is_empty() {
            "waiting_svid"
        } else if s.svid != "0000" {
            "unsupported_svid"
        } else if s.real_type != "PD_PPS" {
            "not_pps"
        } else if self.due.is_some() {
            "settling"
        } else if s.capacity.is_none() {
            "capacity_unreadable"
        } else if s.capacity.is_some_and(|v| v >= 100) || s.stopped || s.qct == Some(0) {
            "full_or_trickle"
        } else if s.verified {
            "verified"
        } else if self.unlocked {
            "write_completed"
        } else {
            "eligible"
        }
    }
    pub fn deadline(&self) -> Option<u64> {
        self.due
    }
    pub fn update(&mut self, now: u64, s: &Snapshot) -> Option<Action> {
        self.due = None;
        if s.status == "Discharging" {
            self.active = false;
            self.reset(now);
            self.enabled = s.enabled;
            return None; // Never prepare the next attach by writing anything.
        }
        if s.status == "Charging" && !self.active {
            self.reset(now);
            self.session_id += 1;
            self.active = true;
        }
        if s.enabled && !self.enabled {
            let xiaomi = self.xiaomi;
            self.reset(now);
            self.xiaomi = xiaomi;
        }
        self.enabled = s.enabled;
        if self.active && s.svid == "2717" {
            self.xiaomi = true;
        }
        if !s.enabled || s.status != "Charging" || !self.active || self.xiaomi {
            self.candidate = None;
            return None;
        }
        if s.svid != "0000" || s.real_type != "PD_PPS" {
            self.candidate = None;
            self.public_ready = false;
            return None; // Empty/unknown VID is never a public adapter.
        }
        let since = *self.candidate.get_or_insert(now);
        let ready = (self.started + self.settle_ms).max(since + self.stable_ms);
        if now < ready {
            self.due = Some(ready);
            return None;
        }
        self.public_ready = true;
        // Full/trickle charge does not need an unlock. No UI is forged anywhere.
        if s.capacity.is_none_or(|v| v >= 100) || s.stopped || s.qct == Some(0) {
            return None;
        }
        if !s.verified && !self.unlocked {
            self.unlocked = true;
            return Some(Action::Unlock);
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn generic() -> Snapshot {
        Snapshot {
            status: "Charging".into(),
            enabled: true,
            svid: "0000".into(),
            real_type: "PD_PPS".into(),
            capacity: Some(60),
            qct: Some(1),
            ..Snapshot::default()
        }
    }
    #[test]
    fn boot_attached_waits_then_unlocks_once() {
        let mut p = Policy::default();
        let s = generic();
        assert_eq!(p.update(0, &s), None);
        assert_eq!(p.deadline(), Some(2500));
        assert_eq!(p.update(2499, &s), None);
        assert_eq!(p.update(2500, &s), Some(Action::Unlock));
        assert_eq!(p.update(90000, &s), None);
        assert_eq!(p.deadline(), None);
    }
    #[test]
    fn xiaomi_never_writes_even_if_both_types_are_pps() {
        let mut p = Policy::default();
        let mut s = generic();
        s.svid = "2717".into();
        for t in [0, 1000, 2500, 10000] {
            assert_eq!(p.update(t, &s), None);
        }
        s.svid = "0000".into(); // Do not reinterpret a damaged/late-changing MIPPS session.
        assert_eq!(p.update(50000, &s), None);
    }
    #[test]
    fn initial_zero_then_xiaomi_is_vetoed() {
        let mut p = Policy::default();
        let mut s = generic();
        p.update(0, &s);
        s.svid = "2717".into();
        p.update(900, &s);
        assert_eq!(p.update(2500, &s), None);
        assert_eq!(p.deadline(), None);
    }
    #[test]
    fn no_prewrite_at_boot_idle_or_detach() {
        let mut p = Policy::default();
        let mut s = generic();
        s.status = "Discharging".into();
        assert_eq!(p.update(0, &s), None);
        assert_eq!(p.deadline(), None);
        s.status = "Charging".into();
        p.update(100, &s);
        s.status = "Discharging".into();
        assert_eq!(p.update(200, &s), None);
        assert_eq!(p.deadline(), None);
    }
    #[test]
    fn each_attach_has_fresh_vid_and_timer() {
        let mut p = Policy::default();
        let mut s = generic();
        p.update(0, &s);
        assert_eq!(p.update(2500, &s), Some(Action::Unlock));
        s.status = "Discharging".into();
        p.update(2600, &s);
        s.status = "Charging".into();
        s.svid = "2717".into();
        assert_eq!(p.update(18000, &s), None);
        assert_eq!(p.update(25000, &s), None);
        s.status = "Discharging".into();
        p.update(30000, &s);
        s.status = "Charging".into();
        s.svid = "0000".into();
        p.update(31000, &s);
        assert_eq!(p.update(33500, &s), Some(Action::Unlock));
    }
    #[test]
    fn resume_restarts_settle_and_pending_pause_cancels() {
        let mut p = Policy::default();
        let mut s = generic();
        p.update(0, &s);
        s.enabled = false;
        assert_eq!(p.update(2300, &s), None);
        assert_eq!(p.deadline(), None);
        s.enabled = true;
        assert_eq!(p.update(5000, &s), None);
        assert_eq!(p.update(7499, &s), None);
        assert_eq!(p.update(7500, &s), Some(Action::Unlock));
    }
    #[test]
    fn paused_still_tracks_xiaomi_veto() {
        let mut p = Policy::default();
        let mut s = generic();
        s.enabled = false;
        s.svid = "2717".into();
        p.update(0, &s);
        s.enabled = true;
        s.svid = "0000".into();
        p.update(1000, &s);
        assert_eq!(p.update(10000, &s), None);
    }
    #[test]
    fn unknown_vid_type_or_status_never_unlocks() {
        for vid in ["", "ffff", "0", "0x0000"] {
            let mut p = Policy::default();
            let mut s = generic();
            s.svid = vid.into();
            p.update(0, &s);
            assert_eq!(p.update(60000, &s), None);
        }
        for status in ["", "Unknown", "Full", "Not charging"] {
            let mut p = Policy::default();
            let mut s = generic();
            s.status = status.into();
            p.update(0, &s);
            assert_eq!(p.update(60000, &s), None);
        }
        let mut p = Policy::default();
        let mut s = generic();
        s.real_type = "USB_PD".into();
        p.update(0, &s);
        assert_eq!(p.update(60000, &s), None);
    }
    #[test]
    fn unstable_svid_restarts_stability_window() {
        let mut p = Policy::default();
        let mut s = generic();
        p.update(0, &s);
        s.svid.clear();
        p.update(2000, &s);
        s.svid = "0000".into();
        p.update(2300, &s);
        assert_eq!(p.update(2500, &s), None);
        assert_eq!(p.deadline(), Some(3800));
        assert_eq!(p.update(3800, &s), Some(Action::Unlock));
    }
    #[test]
    fn full_trickle_and_unreadable_capacity_do_nothing() {
        for s in [
            Snapshot {
                capacity: Some(100),
                ..generic()
            },
            Snapshot {
                stopped: true,
                ..generic()
            },
            Snapshot {
                qct: Some(0),
                ..generic()
            },
            Snapshot {
                capacity: None,
                ..generic()
            },
        ] {
            let mut p = Policy::default();
            p.update(0, &s);
            assert_eq!(p.update(2500, &s), None);
        }
    }
    #[test]
    fn delayed_public_adapter_is_not_lost_after_two_seconds() {
        let mut p = Policy::default();
        let mut s = generic();
        s.svid.clear();
        p.update(0, &s);
        s.svid = "0000".into();
        p.update(20000, &s);
        assert_eq!(p.update(21500, &s), Some(Action::Unlock));
    }
    #[test]
    fn already_verified_does_not_write() {
        let mut p = Policy::default();
        let mut s = generic();
        s.verified = true;
        p.update(0, &s);
        assert_eq!(p.update(2500, &s), None);
    }
    #[test]
    fn configuration_change_preserves_mipps_veto() {
        let mut p = Policy::default();
        let mut s = generic();
        s.svid = "2717".into();
        p.update(0, &s);
        p.reconfigure(200, 3000, 2000);
        s.svid = "0000".into();
        assert_eq!(p.update(10000, &s), None);
    }
    #[test]
    fn lost_events_or_restart_require_fresh_settle() {
        let mut p = Policy::default();
        let s = generic();
        p.update(0, &s);
        p.invalidate();
        assert_eq!(p.update(20000, &s), None);
        assert_eq!(p.update(22500, &s), Some(Action::Unlock));
    }
    #[test]
    fn failed_write_can_retry_on_later_event() {
        let mut p = Policy::default();
        let s = generic();
        p.update(0, &s);
        assert_eq!(p.update(2500, &s), Some(Action::Unlock));
        p.unlock_failed();
        assert_eq!(p.deadline(), None);
        assert_eq!(p.update(3000, &s), Some(Action::Unlock));
    }
}
