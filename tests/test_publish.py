import importlib.util
import json
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
        self.temp = tempfile.TemporaryDirectory()
        self.output = Path(self.temp.name)
        self.tag = 'v' + module.metadata()['version']
        self.files = [f'YAFreePPS-{self.tag}.zip', f'YAFreePPS-{self.tag}-source.zip', 'SHA256SUMS']
        for file in self.files: (self.output / file).write_bytes(b'expected')
        (self.output / 'RELEASE_NOTES.md').write_text('notes')
        self.calls = []
        self.release = {'draft': False, 'assets': []}
    def tearDown(self): self.temp.cleanup()
    def api(self, path, missing_ok=False):
        return {'sha': 'tested'} if '/commits/' in path else self.release
    def gh(self, *args, **kwargs):
        self.calls.append(args)
        if args[:2] == ('release', 'download'):
            name = args[args.index('--pattern') + 1]
            folder = args[args.index('--dir') + 1]
            (Path(folder) / name).write_bytes(b'expected')
        return ''
    def run_publish(self):
        with patch.object(module, 'api', side_effect=self.api), patch.object(module, 'gh', side_effect=self.gh):
            module.publish('owner/repo', self.tag, 'tested', self.output)
    def test_existing_empty_release_gets_missing_assets(self):
        self.run_publish()
        self.assertEqual(sum(c[:2] == ('release', 'upload') for c in self.calls), 3)
        self.assertFalse(any(c[:2] == ('release', 'create') for c in self.calls))
    def test_matching_existing_assets_are_skipped(self):
        self.release['assets'] = [{'name': f} for f in self.files]
        self.run_publish()
        self.assertEqual(sum(c[:2] == ('release', 'download') for c in self.calls), 3)
        self.assertFalse(any(c[:2] == ('release', 'upload') for c in self.calls))
    def test_mismatched_asset_refuses_overwrite(self):
        self.release['assets'] = [{'name': self.files[0]}]
        (self.output / self.files[0]).write_bytes(b'different')
        with self.assertRaisesRegex(ValueError, 'differs'): self.run_publish()
        self.assertFalse(any(c[:2] == ('release', 'upload') for c in self.calls))
    def test_draft_published_only_after_uploads(self):
        self.release['draft'] = True
        self.run_publish()
        self.assertEqual(self.calls[-1][:2], ('release', 'edit'))
        self.assertIn('--draft=false', self.calls[-1])
    def test_wrong_tag_commit_refuses_mutations(self):
        with patch.object(module, 'api', return_value={'sha': 'another'}), patch.object(module, 'gh') as gh:
            with self.assertRaisesRegex(ValueError, 'another commit'):
                module.publish('owner/repo', self.tag, 'tested', self.output)
            gh.assert_not_called()
    def test_failed_upload_keeps_draft(self):
        self.release['draft'] = True
        def fail(*args, **kwargs):
            if args[:2] == ('release', 'upload'): raise RuntimeError('upload failed')
            return self.gh(*args, **kwargs)
        with patch.object(module, 'api', side_effect=self.api), patch.object(module, 'gh', side_effect=fail):
            with self.assertRaises(RuntimeError): module.publish('owner/repo', self.tag, 'tested', self.output)
        self.assertFalse(any(c[:2] == ('release', 'edit') for c in self.calls))

    def test_new_release_uses_draft_then_uploads_and_publishes(self):
        self.release['draft'] = True
        responses = iter([None, None, self.release])
        with patch.object(module, 'api', side_effect=lambda *a, **k: next(responses)), patch.object(module, 'gh', side_effect=self.gh):
            module.publish('owner/repo', self.tag, 'tested', self.output)
        self.assertEqual(self.calls[0][:2], ('release', 'create'))
        self.assertIn('--draft', self.calls[0])
        self.assertEqual(self.calls[-1][:2], ('release', 'edit'))
