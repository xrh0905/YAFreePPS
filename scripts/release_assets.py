#!/usr/bin/env python3
"""Collect an exact-commit source ZIP, module ZIP, release notes and SHA256SUMS."""
import hashlib
from pathlib import Path
import subprocess
from release_metadata import metadata

ROOT = Path(__file__).resolve().parents[1]

def main():
    version = metadata()['version']
    output = ROOT / 'output'
    module = output / f'YAFreePPS-v{version}.zip'
    if not module.is_file():
        raise SystemExit(f'build the module first: {module}')
    source = output / f'YAFreePPS-v{version}-source.zip'
    subprocess.run(['git', 'archive', '--format=zip', f'--prefix=YAFreePPS-v{version}/', '-o', str(source), 'HEAD'], cwd=ROOT, check=True)
    sums = ''.join(f'{hashlib.sha256(p.read_bytes()).hexdigest()}  {p.name}\n' for p in [module, source])
    (output / 'SHA256SUMS').write_text(sums)
    (output / 'RELEASE_NOTES.md').write_text(f'''YAFreePPS v{version}

- arm64 Android 9+ module, WebUI + Action controls.
- Session diagnostics show identification, decisions and actual write receipts.
- No standalone App, APK downloader, or broadcast/UI forgery.
- Xiaomi SVID 2717 is kernel-managed; public PPS uses delayed SVID classification.
- MTK is unsupported. Actual PPS power gains and phone compatibility remain unverified.

Install `YAFreePPS-v{version}.zip` via your root manager and reboot. Remove the original FreePPS and reboot before migrating. The `-source.zip` is source code, not an installable module.

See README.md and VALIDATION.md for device acceptance steps and test limitations. Verify both downloads against SHA256SUMS.
''')
    print(sums, end='')

if __name__ == '__main__':
    main()
