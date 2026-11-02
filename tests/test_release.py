import importlib.util
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('release_metadata', ROOT / 'scripts/release_metadata.py')
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)

class ReleaseTests(unittest.TestCase):
    def test_matching_experimental_release(self):
        version = module.metadata()['version']
        result = module.metadata(f'v{version}')
        self.assertEqual(result['release_tag'], f'v{version}')
        self.assertEqual(result['version'], version)
    def test_wrong_version_or_injection_is_rejected(self):
        for tag in ['v999.1.0', 'v0.1.0\nprerelease=false', 'v0.1.0;id', '../main']:
            with self.assertRaises(ValueError): module.metadata(tag)
    def test_ci_build_is_not_a_release(self):
        self.assertEqual(module.metadata()['release_tag'], '')
