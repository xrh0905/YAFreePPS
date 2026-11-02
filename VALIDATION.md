# v0.2.0 validation

Local Linux build tests cover protocol policy, event-loop behavior and session audit records. Tests specifically distinguish kernel verification from a module write, preserve receipts on pause/resume, clear them on a new attach, reject old-process reports, and ensure report-file inotify events do not cause failed-write retry loops.

WebUI tests use a mock DOM and KernelSU bridge. Release tests mock GitHub CLI calls: missing assets are uploaded, matching assets skipped, differing assets rejected, and drafts published only after successful uploads. Version-bump tests cover automatic release selection on main.

Real NETLINK_KOBJECT_UEVENT integration is skipped if the host returns EPERM. Other event-loop tests use actual directory inotify and poll with local datagrams. Android release uses NDK r27d / API 28, arm64; host and Android clippy run with warnings denied.

Phone hardware validation, real PPS power gains and native root-manager WebUI compatibility remain unverified. MTK remains unsupported. See README.md for device acceptance steps.
