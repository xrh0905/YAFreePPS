//! A single writer and a blocking event loop: netlink, directory inotify, signalfd.
//! Only a pending attach classification arms a one-shot timeout; no periodic polling.
use crate::{
    device::{Device, read},
    policy::{Action, Policy},
};
use anyhow::{Context, Result, bail};
use std::{
    ffi::CString,
    fs::{self, File, OpenOptions},
    io, mem,
    os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd},
    time::Instant,
};

fn owned(fd: RawFd) -> Result<OwnedFd> {
    if fd < 0 {
        return Err(io::Error::last_os_error().into());
    }
    // SAFETY: successful fd is fresh and uniquely owned.
    Ok(unsafe { OwnedFd::from_raw_fd(fd) })
}
fn lock_file(d: &Device) -> Result<File> {
    Ok(OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(d.module.join("daemon.lock"))?)
}
pub fn is_running(d: &Device) -> Result<bool> {
    let file = lock_file(d)?;
    // SAFETY: valid owned fd, flock does not retain a pointer.
    let rc = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
    if rc == 0 {
        return Ok(false);
    }
    let e = io::Error::last_os_error();
    if e.raw_os_error() == Some(libc::EWOULDBLOCK) {
        return Ok(true);
    }
    Err(e.into())
}
fn signals() -> Result<OwnedFd> {
    // SAFETY: initialized local sigset, valid pointers for the duration of each call.
    unsafe {
        let mut set: libc::sigset_t = mem::zeroed();
        libc::sigemptyset(&mut set);
        libc::sigaddset(&mut set, libc::SIGINT);
        libc::sigaddset(&mut set, libc::SIGTERM);
        if libc::sigprocmask(libc::SIG_BLOCK, &set, std::ptr::null_mut()) < 0 {
            return Err(io::Error::last_os_error().into());
        }
        owned(libc::signalfd(
            -1,
            &set,
            libc::SFD_CLOEXEC | libc::SFD_NONBLOCK,
        ))
    }
}
fn netlink() -> Result<OwnedFd> {
    // SAFETY: syscall arguments point to correctly sized initialized structures.
    unsafe {
        let fd = owned(libc::socket(
            libc::AF_NETLINK,
            libc::SOCK_DGRAM | libc::SOCK_NONBLOCK | libc::SOCK_CLOEXEC,
            libc::NETLINK_KOBJECT_UEVENT,
        ))?;
        let mut addr: libc::sockaddr_nl = mem::zeroed();
        addr.nl_family = libc::AF_NETLINK as u16;
        addr.nl_groups = 1;
        let size = 1024 * 1024_i32;
        libc::setsockopt(
            fd.as_raw_fd(),
            libc::SOL_SOCKET,
            libc::SO_RCVBUF,
            &size as *const _ as *const _,
            mem::size_of_val(&size) as _,
        );
        if libc::bind(
            fd.as_raw_fd(),
            &addr as *const _ as *const _,
            mem::size_of_val(&addr) as _,
        ) < 0
        {
            return Err(io::Error::last_os_error().into());
        }
        Ok(fd)
    }
}
fn controls(d: &Device) -> Result<OwnedFd> {
    // Watch the directory so atomic rename of free/config cannot orphan a watch.
    let dir = CString::new(d.module.as_os_str().as_encoded_bytes())?;
    unsafe {
        let fd = owned(libc::inotify_init1(libc::IN_NONBLOCK | libc::IN_CLOEXEC))?;
        if libc::inotify_add_watch(
            fd.as_raw_fd(),
            dir.as_ptr(),
            libc::IN_CLOSE_WRITE
                | libc::IN_MOVED_TO
                | libc::IN_CREATE
                | libc::IN_DELETE
                | libc::IN_DELETE_SELF
                | libc::IN_MOVE_SELF,
        ) < 0
        {
            return Err(io::Error::last_os_error().into());
        }
        Ok(fd)
    }
}
fn drain_controls(fd: RawFd) -> Result<bool> {
    let mut changed = false;
    let mut buf = [0u8; 8192];
    loop {
        let n = unsafe { libc::read(fd, buf.as_mut_ptr().cast(), buf.len()) };
        if n < 0 {
            let e = io::Error::last_os_error();
            if e.kind() == io::ErrorKind::WouldBlock {
                break;
            }
            return Err(e.into());
        }
        if n == 0 {
            break;
        }
        let mut offset = 0;
        while offset + mem::size_of::<libc::inotify_event>() <= n as usize {
            // Byte arrays need not have inotify_event alignment.
            let event = unsafe {
                std::ptr::read_unaligned(buf.as_ptr().add(offset).cast::<libc::inotify_event>())
            };
            let start = offset + mem::size_of_val(&event);
            let end = start + event.len as usize;
            if end > n as usize {
                bail!("truncated inotify event");
            }
            if event.mask & (libc::IN_DELETE_SELF | libc::IN_MOVE_SELF | libc::IN_IGNORED) != 0 {
                bail!("module directory removed; stopping");
            }
            let name = buf[start..end]
                .split(|b| *b == 0)
                .next()
                .unwrap_or_default();
            changed |= matches!(name, b"free" | b"disable" | b"remove" | b"config")
                || event.mask & libc::IN_Q_OVERFLOW != 0;
            offset = end;
        }
    }
    Ok(changed)
}
fn drain_netlink(fd: RawFd, p: &mut Policy) -> Result<bool> {
    let mut relevant = false;
    let mut buf = [0u8; 65536];
    loop {
        let mut sender: libc::sockaddr_nl = unsafe { mem::zeroed() };
        let mut len = mem::size_of_val(&sender) as libc::socklen_t;
        let n = unsafe {
            libc::recvfrom(
                fd,
                buf.as_mut_ptr().cast(),
                buf.len(),
                libc::MSG_DONTWAIT,
                &mut sender as *mut _ as *mut _,
                &mut len,
            )
        };
        if n < 0 {
            let e = io::Error::last_os_error();
            if e.kind() == io::ErrorKind::WouldBlock {
                break;
            }
            if e.raw_os_error() == Some(libc::ENOBUFS) {
                // Lost events can hide detach. Discard all protocol conclusions.
                log::warn!("uevent overflow: reclassifying with a fresh settle window");
                p.invalidate();
                relevant = true;
                break;
            }
            return Err(e.into());
        }
        if n == 0 {
            break;
        }
        if sender.nl_pid != 0 {
            continue;
        } // Kernel messages only.
        if n as usize == buf.len() {
            p.invalidate();
            relevant = true;
            continue;
        }
        let fields: Vec<&str> = std::str::from_utf8(&buf[..n as usize])
            .unwrap_or("")
            .split('\0')
            .collect();
        let power = fields.contains(&"SUBSYSTEM=power_supply");
        let charger = fields
            .iter()
            .any(|f| f.contains("xm_power") || f.contains("qcom-battery") || f.contains("typec"));
        if power || charger {
            relevant = true;
            // Preserve a physical detach even if sysfs has advanced to the next attach
            // before this queue is drained. Never trust another power supply's status.
            if fields.contains(&"POWER_SUPPLY_NAME=battery")
                && fields.contains(&"POWER_SUPPLY_STATUS=Discharging")
            {
                p.invalidate();
            }
        }
    }
    Ok(relevant)
}
pub fn run(mut d: Device) -> Result<()> {
    let lock = lock_file(&d)?;
    if unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
        bail!("another YAFreePPS daemon is running");
    }
    let signal = signals()?;
    let control = controls(&d)?;
    let events = netlink().context("cannot monitor kernel uevents; refusing polling fallback")?;
    fs::write(
        d.module.join("daemon.pid"),
        format!("{}\n", std::process::id()),
    )?;
    let result = event_loop(
        &mut d,
        signal.as_raw_fd(),
        control.as_raw_fd(),
        events.as_raw_fd(),
    );
    let cleanup = d.restore_idle();
    let _ = fs::remove_file(d.module.join("daemon.pid"));
    result.and(cleanup)
}
fn event_loop(d: &mut Device, signal: RawFd, controls: RawFd, events: RawFd) -> Result<()> {
    let mut p = Policy::with_timing(d.settle_ms, d.stable_ms);
    let start = Instant::now();
    let mut was_enabled = false;
    let mut first = true;
    log::info!("started; no App, broadcasts, or MTK writes");
    loop {
        let now = start.elapsed().as_millis() as u64;
        let s = d.snapshot();
        if (first || was_enabled) && !s.enabled {
            d.restore_idle()?;
        }
        first = false;
        was_enabled = s.enabled;
        if p.update(now, &s) == Some(Action::Unlock) {
            match d.unlock() {
                Ok(true) => (),
                Ok(false) => {
                    p.unlock_failed();
                }
                Err(e) => {
                    p.unlock_failed();
                    log::error!("{e:#}");
                }
            }
        }
        let timeout = p.deadline().map_or(-1, |t| {
            t.saturating_sub(start.elapsed().as_millis() as u64)
                .min(i32::MAX as u64) as i32
        });
        let mut fds = [signal, controls, events].map(|fd| libc::pollfd {
            fd,
            events: libc::POLLIN,
            revents: 0,
        });
        let count = unsafe { libc::poll(fds.as_mut_ptr(), fds.len() as _, timeout) };
        if count < 0 {
            let e = io::Error::last_os_error();
            if e.kind() == io::ErrorKind::Interrupted {
                continue;
            }
            return Err(e.into());
        }
        if fds[..2]
            .iter()
            .any(|f| f.revents & (libc::POLLERR | libc::POLLHUP | libc::POLLNVAL) != 0)
            || fds[2].revents & (libc::POLLHUP | libc::POLLNVAL) != 0
        {
            bail!("event source failed; stopping rather than writing blind");
        }
        if fds[0].revents != 0 {
            log::info!("stopping");
            return Ok(());
        }
        if fds[1].revents != 0 && drain_controls(controls)? {
            // Disable is authoritative even when netlink arrives in the same batch.
            let config = read(d.module.join("config"));
            match Device::new(d.module.clone(), d.sys.clone()) {
                Ok(updated) => {
                    if p.settle_ms != updated.settle_ms || p.stable_ms != updated.stable_ms {
                        p.reconfigure(
                            start.elapsed().as_millis() as u64,
                            updated.settle_ms,
                            updated.stable_ms,
                        );
                    }
                    *d = updated;
                }
                Err(e) => {
                    log::error!("invalid config ({config:?}): {e:#}; pausing");
                    d.set_enabled(false)?;
                }
            }
        }
        if fds[2].revents != 0 {
            drain_netlink(events, &mut p)?;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{os::unix::net::UnixDatagram, path::PathBuf, thread, time::Duration};
    struct Harness {
        d: Device,
        stop: UnixDatagram,
        events: UnixDatagram,
        thread: Option<thread::JoinHandle<Result<()>>>,
        root: PathBuf,
    }
    impl Harness {
        fn new(svid: &str) -> Self {
            let root = std::env::temp_dir().join(format!(
                "yafreepps-reactor-{}-{}",
                std::process::id(),
                svid
            ));
            let _ = fs::remove_dir_all(&root);
            fs::create_dir_all(root.join("module")).unwrap();
            let d = Device::plain(root.join("module"), root.join("sys"));
            let (stop, signal) = UnixDatagram::pair().unwrap();
            let (events, socket) = UnixDatagram::pair().unwrap();
            socket.set_nonblocking(true).unwrap();
            let h = Self {
                d,
                stop,
                events,
                thread: None,
                root,
            };
            h.write("power_supply/battery/status", "Charging");
            h.write("power_supply/battery/capacity", "60");
            h.write("xm_power/typec/strategy_pd_auth/adapter_svid", svid);
            h.write("xm_power/charger/charger_common/real_type", "PD_PPS");
            h.write("qcom-battery/pd_verifed", "0");
            h.d.set_enabled(true).unwrap();
            let controls = controls(&h.d).unwrap();
            let mut device = h.d.clone();
            let mut h = h;
            h.thread = Some(thread::spawn(move || {
                event_loop(
                    &mut device,
                    signal.as_raw_fd(),
                    controls.as_raw_fd(),
                    socket.as_raw_fd(),
                )
            }));
            h
        }
        fn write(&self, name: &str, value: &str) {
            let file = self.d.node(name);
            fs::create_dir_all(file.parent().unwrap()).unwrap();
            fs::write(file, value).unwrap();
        }
        fn event(&self, text: &[u8]) {
            self.events.send(text).unwrap();
        }
        fn wait_pd(&self, value: &str) {
            let deadline = Instant::now() + Duration::from_secs(4);
            while read(self.d.node("qcom-battery/pd_verifed")) != value {
                assert!(Instant::now() < deadline, "pd did not become {value}");
                thread::sleep(Duration::from_millis(10));
            }
        }
    }
    impl Drop for Harness {
        fn drop(&mut self) {
            let _ = self.stop.send(b"stop");
            if let Some(handle) = self.thread.take() {
                handle.join().unwrap().unwrap();
            }
            let _ = fs::remove_dir_all(&self.root);
        }
    }
    #[test]
    fn reactor_timeout_resume_atomic_controls_and_fast_shutdown() {
        let h = Harness::new("0000");
        thread::sleep(Duration::from_millis(50));
        assert_eq!(read(h.d.node("qcom-battery/pd_verifed")), "0");
        h.d.set_enabled(false).unwrap();
        thread::sleep(Duration::from_millis(50));
        h.d.set_enabled(true).unwrap();
        h.wait_pd("1");
        // Pausing while attached never writes 0.
        h.d.set_enabled(false).unwrap();
        thread::sleep(Duration::from_millis(50));
        assert_eq!(read(h.d.node("qcom-battery/pd_verifed")), "1");
        // Explicit idle pause restoration, without writing at session-end.
        h.write("power_supply/battery/status", "Discharging");
        h.write("qcom-battery/pd_verifed", "0");
        h.event(
            b"SUBSYSTEM=power_supply\0POWER_SUPPLY_NAME=battery\0POWER_SUPPLY_STATUS=Discharging\0",
        );
        thread::sleep(Duration::from_millis(50));
        assert_eq!(read(h.d.node("qcom-battery/pd_verifed")), "0");
        let before = Instant::now();
        drop(h);
        assert!(before.elapsed() < Duration::from_millis(500));
    }
    #[test]
    fn reactor_xiaomi_veto_survives_node_regression() {
        let h = Harness::new("2717");
        thread::sleep(Duration::from_millis(100));
        h.write("xm_power/typec/strategy_pd_auth/adapter_svid", "0000");
        h.event(b"SUBSYSTEM=power_supply\0");
        thread::sleep(Duration::from_millis(2700));
        assert_eq!(read(h.d.node("qcom-battery/pd_verifed")), "0");
    }
}
