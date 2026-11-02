# v0.1.0 validation

Validated on 2026-11-02 in a Linux build environment, not on a phone.

- Rust: 1.99.1 stable; Android target: aarch64-linux-android.
- Android NDK: r27d, API 28. Release ELF uses `/system/bin/linker64` and depends on Android `liblog`, `libdl`, and `libc`.
- Rust: 17 tests passed (15 policy traces, 2 inotify/poll reactor tests with local datagrams).
- Python: 11 tests passed; 1 real netlink integration test skipped because the host disallows `NETLINK_KOBJECT_UEVENT` subscription (EPERM).
- Release metadata checks include tag/version consistency and injection rejection. Publishing to GitHub is pending repository integration authorization.
- Node: 4 WebUI bridge/control/validation tests passed with a mock KernelSU bridge.
- `cargo fmt -- --check`, host clippy and Android clippy with `-D warnings`: passed.
- Shell scripts: `sh -n` passed.
- Packaging: Android ELF architecture/interpreter checked; ZIP includes Action, WebUI, uninstall hook, attribution and license; no upstream executable or APK installer.
- `git diff --check`: passed.

The WebUI was checked with mocked DOM/bridge tests. Browser screenshot verification was unavailable because the browser download failed. Native root-manager WebUI bridge compatibility remains to be checked on a phone.

Not verified: real PPS power unlock, real phone netlink delivery and node timing, MIPPS hardware acceptance, manager install/uninstall behavior, and any MTK path (unsupported). No claim of measured power gain or universal charger compatibility is made.

Hardware acceptance steps and the original report's unverified items are in README.md.
