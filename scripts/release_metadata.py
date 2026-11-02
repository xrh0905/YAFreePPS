#!/usr/bin/env python3
"""Validate a release tag against all module version metadata. No network writes."""
import argparse
import os
from pathlib import Path
import re
import subprocess
import tomllib

ROOT = Path(__file__).resolve().parents[1]

def metadata(tag: str = '') -> dict[str, str]:
    version = tomllib.loads((ROOT / 'Cargo.toml').read_text())['package']['version']
    props = dict(line.split('=', 1) for line in (ROOT / 'module/module.prop').read_text().splitlines() if '=' in line)
    if props.get('version') != f'v{version}':
        raise ValueError('Cargo.toml and module.prop versions disagree')
    if props.get('id') != 'YAFreePPS' or not props.get('versionCode', '').isdigit():
        raise ValueError('module id/versionCode is invalid')
    if tag and (not re.fullmatch(r'v\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?', tag) or tag != f'v{version}'):
        raise ValueError(f'release tag must match v{version}, got {tag!r}')
    return {'version': version, 'release_tag': tag, 'prerelease': str(version.startswith('0.') or '-' in version).lower()}

def changed_version_tag():
    previous = subprocess.run(['git', 'show', 'HEAD^:Cargo.toml'], cwd=ROOT, text=True, capture_output=True)
    if previous.returncode:
        return ''  # No previous version to compare, do not publish implicitly.
    old_version = tomllib.loads(previous.stdout)['package']['version']
    current = metadata()['version']
    return f'v{current}' if old_version != current else ''

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--tag', default='')
    parser.add_argument('--release-on-version-change', action='store_true')
    args = parser.parse_args()
    values = metadata(args.tag or (changed_version_tag() if args.release_on_version_change else ''))
    if os.environ.get('GITHUB_OUTPUT'):
        with open(os.environ['GITHUB_OUTPUT'], 'a') as out:
            for key, value in values.items():
                out.write(f'{key}={value}\n')
    print('\n'.join(f'{key}={value}' for key, value in values.items()))

if __name__ == '__main__':
    main()
