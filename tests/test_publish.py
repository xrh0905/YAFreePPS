from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / 'scripts'))
import publish_release as module

class PublishTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(); self.output = Path(self.temp.name)
        self.tag = 'v' + module.metadata()['version']
        self.files = [f'YAFreePPS-{self.tag}.zip', f'YAFreePPS-{self.tag}-source.zip', 'SHA256SUMS']
        for file in self.files: (self.output / file).write_bytes(b'expected')
        (self.output / 'RELEASE_NOTES.md').write_text('notes')
        self.release = {'id': 7, 'tag_name': self.tag, 'draft': False, 'assets': [], 'target_commitish': 'tested', 'upload_url': 'https://uploads.github.com/repos/owner/repo/releases/7/assets{?name,label}'}
        self.calls = []; self.missing_tag = False; self.hidden_draft = False; self.new = False
    def tearDown(self): self.temp.cleanup()
    def api(self, path, missing_ok=False, method='GET', fields=None, paginate=False):
        self.calls.append((method, path, fields))
        if method == 'POST': return self.release
        if method == 'PATCH': return self.release
        if '/git/ref/tags/' in path: return None if self.missing_tag else {'object': {}}
        if '/commits/' in path:
            if self.missing_tag: raise AssertionError('do not look up a missing commit')
            return {'sha': 'tested'}
        if '/releases/tags/' in path: return None if self.hidden_draft or self.new else self.release
        if paginate: return [[]] if self.new else [[self.release]]
        raise AssertionError(path)
    def download(self, repo, asset, path): path.write_bytes(b'expected')
    def execute(self):
        with patch.object(module, 'api', side_effect=self.api), patch.object(module, 'download', side_effect=self.download), patch.object(module, 'upload') as upload:
            module.publish('owner/repo', self.tag, 'tested', self.output)
            return upload.call_count
    def test_existing_empty_release_gets_missing_assets(self): self.assertEqual(self.execute(), 3)
    def test_existing_matching_assets_are_skipped(self):
        self.release['assets'] = [{'name': f, 'id': i} for i, f in enumerate(self.files)]
        self.assertEqual(self.execute(), 0)
    def test_differing_asset_refuses_all_uploads(self):
        self.release['assets'] = [{'name': self.files[0], 'id': 1}]
        (self.output / self.files[0]).write_bytes(b'different')
        with self.assertRaisesRegex(ValueError, 'differs'): self.execute()
    def test_hidden_draft_is_found_by_listing_and_finished_by_id(self):
        self.release['draft'] = True; self.hidden_draft = True; self.missing_tag = True
        self.assertEqual(self.execute(), 3)
        self.assertFalse(any(c[0] == 'POST' for c in self.calls))
        self.assertEqual(self.calls[-1], ('PATCH', 'repos/owner/repo/releases/7', {'draft': False, 'prerelease': True, 'make_latest': 'false'}))
    def test_empty_draft_can_follow_corrected_commit_before_upload(self):
        self.release['draft'] = True; self.missing_tag = True; self.release['target_commitish'] = 'old'
        self.execute()
        self.assertIn(('PATCH', 'repos/owner/repo/releases/7', {'target_commitish': 'tested'}), self.calls)
    def test_partial_draft_from_another_commit_is_rejected(self):
        self.release.update(draft=True, target_commitish='old', assets=[{'name': self.files[0], 'id': 1}]); self.missing_tag = True
        with self.assertRaisesRegex(ValueError, 'another target'): self.execute()
    def test_new_release_uses_id_from_creation_response(self):
        self.new = True; self.missing_tag = True; self.release['draft'] = True
        self.execute()
        created = [c for c in self.calls if c[0] == 'POST']
        self.assertEqual(len(created), 1); self.assertEqual(created[0][1], 'repos/owner/repo/releases')
        self.assertEqual(self.calls[-1][1], 'repos/owner/repo/releases/7')
    def test_upload_failure_never_publishes_draft(self):
        self.release['draft'] = True
        with patch.object(module, 'api', side_effect=self.api), patch.object(module, 'upload', side_effect=RuntimeError('failed')):
            with self.assertRaises(RuntimeError): module.publish('owner/repo', self.tag, 'tested', self.output)
        self.assertFalse(any(c[0] == 'PATCH' and c[2].get('draft') is False for c in self.calls))
    def test_tag_from_another_commit_refuses_mutations(self):
        with patch.object(module, 'api', return_value={'sha': 'another'}), patch.object(module, 'upload') as upload:
            with self.assertRaisesRegex(ValueError, 'another commit'): module.publish('owner/repo', self.tag, 'tested', self.output)
            upload.assert_not_called()
    def test_upload_host_is_validated(self):
        with self.assertRaisesRegex(ValueError, 'host'):
            module.upload({'upload_url': 'https://example.org/assets'}, self.output / self.files[0])
