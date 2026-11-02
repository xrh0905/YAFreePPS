mod device;
mod policy;
mod reactor;

use anyhow::{Context, Result, bail};
use device::{Device, MODULE, SYS};
use std::{fs, path::PathBuf};

fn main() {
    #[cfg(target_os = "android")]
    android_logger::init_once(
        android_logger::Config::default()
            .with_max_level(log::LevelFilter::Info)
            .with_tag("YAFreePPS"),
    );
    if let Err(e) = execute() {
        eprintln!("YAFreePPS: {e:#}");
        log::error!("{e:#}");
        std::process::exit(1);
    }
}
fn execute() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let mut module = PathBuf::from(MODULE);
    let mut sys = PathBuf::from(SYS);
    let mut command = args.next().unwrap_or_else(|| "run".into());
    // Alternate roots support deterministic integration tests, not device autodetection.
    while command.starts_with("--") {
        match command.as_str() {
            "--module-dir" => module = args.next().context("missing module directory")?.into(),
            "--sys-root" => sys = args.next().context("missing sysfs root")?.into(),
            _ => bail!("unknown option {command}"),
        }
        command = args.next().unwrap_or_else(|| "run".into());
    }
    let device = if matches!(
        command.as_str(),
        "enable" | "disable" | "toggle" | "configure" | "restore-idle"
    ) {
        Device::plain(module, sys)
    } else {
        Device::new(module, sys)?
    };
    match command.as_str() {
        "run" => reactor::run(device)?,
        "status" => println!("{}", device.status_json(reactor::is_running(&device)?)),
        "enable" => device.set_enabled(true)?,
        "disable" => {
            device.set_enabled(false)?;
            if !reactor::is_running(&device)? {
                device.restore_idle()?;
            }
        }
        "toggle" => {
            device.set_enabled(!device.enabled())?;
            if !device.enabled() && !reactor::is_running(&device)? {
                device.restore_idle()?;
            }
            println!(
                "{}",
                if device.enabled() {
                    "PPS enabled"
                } else {
                    "PPS paused"
                }
            );
        }
        "restore-idle" => device.restore_idle()?,
        "configure" => {
            let settle: u64 = args.next().context("missing settle_ms")?.parse()?;
            let stable: u64 = args.next().context("missing svid_stable_ms")?.parse()?;
            if !(2000..=30000).contains(&settle) || !(1000..=30000).contains(&stable) {
                bail!("timing outside safe range");
            }
            let temp = device
                .module
                .join(format!("config.{}.tmp", std::process::id()));
            fs::write(
                &temp,
                format!("settle_ms={settle}\nsvid_stable_ms={stable}\n"),
            )?;
            fs::rename(temp, device.module.join("config"))?;
        }
        _ => bail!(
            "commands: run | status | enable | disable | toggle | configure SETTLE_MS STABLE_MS | restore-idle"
        ),
    }
    Ok(())
}
