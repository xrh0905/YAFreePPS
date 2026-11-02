import importlib.util
from pathlib import Path
import tempfile
import unittest
import zipfile

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('package_module', ROOT / 'package_module.py')
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)

class PackagingTests(unittest.TestCase):
    def test_host_binary_is_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            with self.assertRaises(ValueError):
                module.package(ROOT / 'target/debug/yafreepps', Path(directory) / 'bad.zip')
    def test_module_layout_excludes_app_and_includes_webui_and_action(self):
        # A miniature Android-shaped ELF exercises packaging, not execution.
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            header = bytearray(20); header[:6] = b'\x7fELF\x02\x01'; header[18:20] = (183).to_bytes(2, 'little')
            binary = root / 'fake-android-elf'; binary.write_bytes(header + b'/system/bin/linker64\0')
            output = module.package(binary, root / 'module.zip')
            with zipfile.ZipFile(output) as archive:
                names = archive.namelist()
                self.assertEqual(len(names), len(set(names)))
                self.assertIn('webroot/index.html', names); self.assertIn('action.sh', names)
                self.assertIn('bin/yafreepps', names); self.assertIn('uninstall.sh', names)
                self.assertIn('README.md', names); self.assertIn('LICENSE', names)
                self.assertNotIn('bin/FreePPS', names); self.assertNotIn('freepps-app.sh', names)
                self.assertFalse(any(name.endswith('.apk') for name in names))
                self.assertEqual(archive.getinfo('bin/yafreepps').external_attr >> 16 & 0o777, 0o755)
