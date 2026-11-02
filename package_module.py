#!/usr/bin/env python3
"""Package only a freshly supplied Android arm64 ELF, never the upstream binary."""
from pathlib import Path
import argparse
import struct
import tomllib
import zipfile

ROOT = Path(__file__).resolve().parent

def package(binary: Path, output: Path) -> Path:
    header = binary.read_bytes()[:20]
    if len(header) != 20 or header[:6] != b'\x7fELF\x02\x01' or struct.unpack_from('<H', header, 18)[0] != 183:
        raise ValueError('expected a 64-bit little-endian AArch64 Android ELF')
    # Android ELF must name Android's dynamic linker (host/Linux ARM binaries are rejected).
    if b'/system/bin/linker64\0' not in binary.read_bytes():
        raise ValueError('ELF does not use the Android linker')
    output.parent.mkdir(parents=True, exist_ok=True)
    with zipfile.ZipFile(output, 'w', zipfile.ZIP_DEFLATED) as archive:
        paths = [(p, p.relative_to(ROOT / 'module').as_posix()) for p in sorted((ROOT / 'module').rglob('*')) if p.is_file()]
        paths += [(binary, 'bin/yafreepps'), (ROOT / 'LICENSE', 'LICENSE'), (ROOT / 'NOTICES', 'NOTICES'), (ROOT / 'README.md', 'README.md'), (ROOT / 'VALIDATION.md', 'VALIDATION.md')]
        for path, name in paths:
            info = zipfile.ZipInfo(name, (2026, 11, 2, 0, 0, 0))
            info.compress_type = zipfile.ZIP_DEFLATED
            executable = name.endswith('.sh') or name in ('bin/yafreepps', 'META-INF/com/google/android/update-binary')
            info.external_attr = (0o100755 if executable else 0o100644) << 16
            archive.writestr(info, path.read_bytes())
    return output

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, default=ROOT / 'output/yafreepps')
    parser.add_argument('--output', type=Path)
    args = parser.parse_args()
    version = tomllib.loads((ROOT / 'Cargo.toml').read_text())['package']['version']
    result = package(args.binary, args.output or ROOT / f'output/YAFreePPS-v{version}.zip')
    print(result)

if __name__ == '__main__':
    main()
