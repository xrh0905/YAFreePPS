# Changelog

## 0.2.0

- Add daemon-owned session diagnostics to WebUI and `status`: charger classification, decision/skip reason, real write attempts/successes, last outcome, timestamp and error.
- Distinguish kernel verification from module writes; retain counts across pause/resume, reset for a new attach, reject old-process reports.
- Persist changed observations atomically, without background polling or self-triggered write retries.
- Make release publication resumable: add missing assets to existing releases, verify matching assets, refuse differing bytes, finish drafts only after successful uploads.
- Publish version changes on main after CI passes; retain tag and manual release triggers.
- Preserve no-App/no-broadcast behavior and existing MIPPS isolation policy.

## 0.1.0

Initial event-driven implementation with SVID classification, WebUI and Action controls, arm64 packaging and GitHub Actions.
