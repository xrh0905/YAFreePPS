use crate::policy::Snapshot;
use anyhow::{Context, Result, bail};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

pub const SYS: &str = "/sys/class";
pub const MODULE: &str = "/data/adb/modules/YAFreePPS";

pub fn read(path: impl AsRef<Path>) -> String {
    fs::read_to_string(path)
        .unwrap_or_default()
        .trim()
        .to_owned()
}

#[derive(Clone, Debug)]
pub struct Device {
    pub module: PathBuf,
    pub sys: PathBuf,
    pub settle_ms: u64,
    pub stable_ms: u64,
}
impl Device {
    pub fn plain(module: PathBuf, sys: PathBuf) -> Self {
        Self {
            module,
            sys,
            settle_ms: 2500,
            stable_ms: 1500,
        }
    }
    pub fn new(module: PathBuf, sys: PathBuf) -> Result<Self> {
        let mut d = Self::plain(module, sys);
        let config = read(d.module.join("config"));
        for line in config
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty() && !l.starts_with('#'))
        {
            let (key, value) = line
                .split_once('=')
                .context("config must contain key=value")?;
            let value: u64 = value
                .trim()
                .parse()
                .context("config timing must be an integer")?;
            match key.trim() {
                "settle_ms" if (2000..=30000).contains(&value) => d.settle_ms = value,
                "svid_stable_ms" if (1000..=30000).contains(&value) => d.stable_ms = value,
                _ => bail!("unknown config key or unsafe timing: {line}"),
            }
        }
        Ok(d)
    }
    pub fn node(&self, name: &str) -> PathBuf {
        self.sys.join(name)
    }
    fn value(&self, name: &str) -> String {
        read(self.node(name))
    }
    fn first(&self, names: &[&str]) -> String {
        names
            .iter()
            .map(|n| self.value(n))
            .find(|v| !v.is_empty())
            .unwrap_or_default()
    }
    pub fn enabled(&self) -> bool {
        read(self.module.join("free")) == "1"
            && !self.module.join("disable").exists()
            && !self.module.join("remove").exists()
    }
    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            status: self.value("power_supply/battery/status"),
            enabled: self.enabled(),
            svid: self
                .value("xm_power/typec/strategy_pd_auth/adapter_svid")
                .to_ascii_lowercase(),
            real_type: self.value("xm_power/charger/charger_common/real_type"),
            verified: self.value("qcom-battery/pd_verifed") == "1",
            apdo: self.value("xm_power/typec/apdo_max").parse().unwrap_or(0),
            capacity: self.value("power_supply/battery/capacity").parse().ok(),
            qct: self
                .first(&[
                    "qcom-battery/quick_charge_type",
                    "xm_power/charger/charger_common/quick_charge_type",
                ])
                .parse()
                .ok(),
            stopped: self.first(&[
                "qcom-battery/stop_handle_charge",
                "xm_power/charger/charger_common/stop_handle_charge",
            ]) == "1",
        }
    }
    /// The only write of 1 in this project. Re-read all gates immediately before it.
    pub fn unlock(&self) -> Result<bool> {
        let s = self.snapshot();
        if !s.enabled
            || s.status != "Charging"
            || s.svid != "0000"
            || s.real_type != "PD_PPS"
            || s.verified
            || s.stopped
            || s.capacity.is_none_or(|v| v >= 100)
            || s.qct == Some(0)
        {
            return Ok(false);
        }
        fs::write(self.node("qcom-battery/pd_verifed"), "1").context("write pd_verifed=1")?;
        log::info!("write pd_verifed=1 (settled public PPS)");
        Ok(true)
    }
    /// Used only for explicit pause/removal/startup-paused, never session-end.
    pub fn restore_idle(&self) -> Result<()> {
        if self.enabled()
            || self.value("power_supply/battery/status") != "Discharging"
            || self.value("qcom-battery/pd_verifed") != "1"
        {
            return Ok(());
        }
        // Do not restore while attached, even if a control file changed mid-read.
        if !self.enabled() && self.value("power_supply/battery/status") == "Discharging" {
            fs::write(self.node("qcom-battery/pd_verifed"), "0").context("idle restore")?;
            log::info!("write pd_verifed=0 (explicit pause while unplugged)");
        }
        Ok(())
    }
    pub fn set_enabled(&self, value: bool) -> Result<()> {
        let temporary = self.module.join(format!("free.{}.tmp", std::process::id()));
        fs::write(&temporary, if value { "1\n" } else { "0\n" })?;
        fs::rename(temporary, self.module.join("free"))?;
        Ok(())
    }
    pub fn status_json(&self, running: bool) -> String {
        let s = self.snapshot();
        let protocol = match s.svid.as_str() {
            "2717" => "MIPPS — kernel managed",
            "0000" if s.real_type == "PD_PPS" => "Public PPS (candidate)",
            _ => "Unknown / unsupported",
        };
        let report = crate::audit::current(self, running);
        let matches_current = report.as_ref().is_some_and(|r| r.observed == s);
        let fields = BTreeMap::from([
            ("version", json_string(env!("CARGO_PKG_VERSION"))),
            (
                "session",
                report.map_or("null".into(), |r| {
                    serde_json::to_string(&r).unwrap_or_else(|_| "null".into())
                }),
            ),
            ("session_matches_current", matches_current.to_string()),
            ("enabled", s.enabled.to_string()),
            ("running", running.to_string()),
            ("status", json_string(&s.status)),
            ("adapter_svid", json_string(&s.svid)),
            ("real_type", json_string(&s.real_type)),
            ("pd_verifed", s.verified.to_string()),
            ("apdo_max", s.apdo.to_string()),
            ("protocol", json_string(protocol)),
            (
                "capacity",
                s.capacity.map_or("null".into(), |v| v.to_string()),
            ),
            ("qct", s.qct.map_or("null".into(), |v| v.to_string())),
            ("stop_handle_charge", s.stopped.to_string()),
            ("settle_ms", self.settle_ms.to_string()),
            ("svid_stable_ms", self.stable_ms.to_string()),
            (
                "supported",
                self.node("qcom-battery/pd_verifed").exists().to_string(),
            ),
        ]);
        format!(
            "{{{}}}",
            fields
                .into_iter()
                .map(|(k, v)| format!("{}:{v}", json_string(k)))
                .collect::<Vec<_>>()
                .join(",")
        )
    }
}
fn json_string(s: &str) -> String {
    let mut out = String::from("\"");
    for ch in s.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            ch if ch.is_control() => out.push_str(&format!("\\u{:04x}", ch as u32)),
            _ => out.push(ch),
        }
    }
    out.push('"');
    out
}
