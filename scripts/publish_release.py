#!/usr/bin/env python3
"""Idempotent GitHub Release upload: fill missing assets, never overwrite bytes."""
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile
from release_metadata import metadata

ROOT = Path(__file__).resolve().parents[1]

def gh(*args, missing_ok=False):
    result = subprocess.run(['gh', *args], text=True, capture_output=True)
    if result.returncode:
        if missing_ok and 'HTTP 404' in result.stderr:
            return None
        raise RuntimeError(result.stderr or result.stdout or 'GitHub CLI failed')
    return result.stdout

def api(path, missing_ok=False):
    output = gh('api', path, missing_ok=missing_ok)
    return None if output is None else json.loads(output)

def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

def publish(repo, tag, target, output):
    info = metadata(tag)
    if not tag:
        raise ValueError('release tag is required')
    # The commit endpoint returns 422 for an unknown tag; git/ref returns 404.
    ref = api(f'repos/{repo}/git/ref/tags/{tag}', missing_ok=True)
    commit = api(f'repos/{repo}/commits/{tag}') if ref is not None else None
    if commit is not None and commit['sha'] != target:
        raise ValueError('Tag points at another commit; refusing mismatched binaries')
    files = [output / f'YAFreePPS-{tag}.zip', output / f'YAFreePPS-{tag}-source.zip', output / 'SHA256SUMS']
    if not all(p.is_file() for p in files):
        raise ValueError('missing release assets')
    release = api(f'repos/{repo}/releases/tags/{tag}', missing_ok=True)
    if release is None:
        gh('release', 'create', tag, '--draft', '--target', target, '--title', f'YAFreePPS {tag}', '--notes-file', str(output / 'RELEASE_NOTES.md'))
        release = api(f'repos/{repo}/releases/tags/{tag}')
    assets = {a['name']: a for a in release['assets']}
    # Validate every existing asset before adding anything to a partial release.
    with tempfile.TemporaryDirectory(prefix='yafreepps-release-') as folder:
        for file in files:
            if file.name not in assets:
                continue
            gh('release', 'download', tag, '--pattern', file.name, '--dir', folder)
            if digest(Path(folder) / file.name) != digest(file):
                raise ValueError(f'existing asset differs: {file.name}; refusing to overwrite')
    for file in files:
        if file.name not in assets:
            gh('release', 'upload', tag, str(file))
    # A partially created draft is published only after all uploads succeed.
    if release['draft']:
        flags = ['--draft=false', f"--prerelease={info['prerelease']}"]
        if info['prerelease'] == 'true':
            flags.append('--latest=false')
        gh('release', 'edit', tag, *flags)
    print(f'https://github.com/{repo}/releases/tag/{tag}')

def main():
    repo = os.environ['GH_REPO']
    if not repo or '/' not in repo:
        raise ValueError('invalid GH_REPO')
    publish(repo, os.environ['RELEASE_TAG'], os.environ['TARGET_SHA'], ROOT / 'output')

if __name__ == '__main__':
    main()
