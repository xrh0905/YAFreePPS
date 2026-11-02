#!/usr/bin/env python3
"""Resumable release upload via immutable release/asset IDs, including drafts."""
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile
from urllib.parse import quote, urlsplit
from release_metadata import metadata

ROOT = Path(__file__).resolve().parents[1]

def gh(*args, missing_ok=False):
    result = subprocess.run(['gh', *args], text=True, capture_output=True)
    if result.returncode:
        if missing_ok and 'HTTP 404' in result.stderr:
            return None
        raise RuntimeError(result.stderr or result.stdout or 'GitHub CLI failed')
    return result.stdout

def api(path, missing_ok=False, method='GET', fields=None, paginate=False):
    args = ['api', path, '--method', method]
    if paginate: args += ['--paginate', '--slurp']
    for key, value in (fields or {}).items():
        if isinstance(value, bool): args += ['--field', f'{key}={str(value).lower()}']
        else: args += ['--raw-field', f'{key}={value}']
    result = gh(*args, missing_ok=missing_ok)
    return None if result is None else json.loads(result)

def find_release(repo, tag):
    published = api(f'repos/{repo}/releases/tags/{tag}', missing_ok=True)
    if published is not None: return published
    # Drafts can be absent from the by-tag endpoint. Listing includes authorized drafts.
    pages = api(f'repos/{repo}/releases?per_page=100', paginate=True)
    return next((r for page in pages for r in page if r['tag_name'] == tag), None)

def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

def download(repo, asset, path):
    with path.open('wb') as output:
        result = subprocess.run(['gh', 'api', f"repos/{repo}/releases/assets/{asset['id']}",
            '--header', 'Accept: application/octet-stream'], stdout=output, stderr=subprocess.PIPE)
    if result.returncode: raise RuntimeError(result.stderr.decode(errors='replace'))

def upload(release, path):
    url = release['upload_url'].split('{', 1)[0]
    parsed = urlsplit(url)
    if parsed.scheme != 'https' or parsed.netloc != 'uploads.github.com':
        raise ValueError('unexpected upload host')
    content_type = 'application/zip' if path.suffix == '.zip' else 'text/plain'
    gh('api', f'{url}?name={quote(path.name, safe="")}', '--method', 'POST',
        '--header', f'Content-Type: {content_type}', '--input', str(path))

def publish(repo, tag, target, output):
    info = metadata(tag)
    if not tag: raise ValueError('release tag is required')
    ref = api(f'repos/{repo}/git/ref/tags/{tag}', missing_ok=True)
    commit = api(f'repos/{repo}/commits/{tag}') if ref is not None else None
    if commit is not None and commit['sha'] != target:
        raise ValueError('Tag points at another commit; refusing mismatched binaries')
    files = [output / f'YAFreePPS-{tag}.zip', output / f'YAFreePPS-{tag}-source.zip', output / 'SHA256SUMS']
    if not all(p.is_file() for p in files): raise ValueError('missing release assets')
    release = find_release(repo, tag)
    if release is None:
        release = api(f'repos/{repo}/releases', method='POST', fields={
            'tag_name': tag, 'target_commitish': target, 'name': f'YAFreePPS {tag}',
            'body': (output / 'RELEASE_NOTES.md').read_text(), 'draft': True,
            'prerelease': info['prerelease'] == 'true'})
    release_path = f"repos/{repo}/releases/{release['id']}"
    # An empty, unpublished draft may safely follow the corrected tested commit.
    if ref is None and release['draft'] and release.get('target_commitish') != target:
        if release['assets']:
            raise ValueError('draft contains assets from another target; refusing mixed builds')
        api(release_path, method='PATCH', fields={'target_commitish': target})
    assets = {a['name']: a for a in release['assets']}
    with tempfile.TemporaryDirectory(prefix='yafreepps-release-') as folder:
        for file in files:
            if file.name not in assets: continue
            remote = Path(folder) / file.name
            download(repo, assets[file.name], remote)
            if digest(remote) != digest(file):
                raise ValueError(f'existing asset differs: {file.name}; refusing to overwrite')
    for file in files:
        if file.name not in assets: upload(release, file)
    if release['draft']:
        fields = {'tag_name': tag, 'target_commitish': target, 'draft': False, 'prerelease': info['prerelease'] == 'true'}
        if info['prerelease'] == 'true': fields['make_latest'] = 'false'
        published = api(release_path, method='PATCH', fields=fields)
        if published['tag_name'] != tag or published['draft']:
            raise RuntimeError('GitHub did not publish the requested tag')
    print(f'https://github.com/{repo}/releases/tag/{tag}')

def retire_superseded(repo, release_id, expected_target, tag):
    old = api(f'repos/{repo}/releases/{release_id}', missing_ok=True)
    if old is None or old['draft']: return
    if not old['tag_name'].startswith('untagged-') or old['name'] != f'YAFreePPS {tag}' or old['target_commitish'] != expected_target:
        raise ValueError('superseded release identity does not match; refusing to change it')
    # Reversible cleanup of the accidental release created during v0.2.0 validation.
    api(f'repos/{repo}/releases/{release_id}', method='PATCH', fields={'draft': True})

def main():
    repo = os.environ['GH_REPO']
    if not repo or '/' not in repo: raise ValueError('invalid GH_REPO')
    publish(repo, os.environ['RELEASE_TAG'], os.environ['TARGET_SHA'], ROOT / 'output')

if __name__ == '__main__': main()
