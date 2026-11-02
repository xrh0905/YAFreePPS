"""Integration tests use fake sysfs; never access a real charging node."""
import json
import os
from pathlib import Path
import subprocess
import tempfile
import time
import unittest

ROOT = Path(__file__).resolve().parents[1]
BIN = ROOT / 'target/debug/yafreepps'

class RuntimeTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix='yafreepps-test-')
        base = Path(self.temp.name)
        self.module = base / 'module'; self.module.mkdir()
        self.sys = base / 'sys'; self.sys.mkdir()
        self.daemon = None
        (self.module / 'free').write_text('1')
        self.write('power_supply/battery/status', 'Charging')
        self.write('power_supply/battery/capacity', '60')
        self.write('xm_power/typec/strategy_pd_auth/adapter_svid', '0000')
        self.write('xm_power/charger/charger_common/real_type', 'PD_PPS')
        self.write('qcom-battery/pd_verifed', '0')
        self.write('xm_power/typec/apdo_max', '55')
        self.write('qcom-battery/quick_charge_type', '1')
    def tearDown(self):
        if self.daemon:
            if self.daemon.poll() is None:
                self.daemon.terminate()
            self.daemon.communicate(timeout=2)
        self.temp.cleanup()
    def write(self, path, value):
        file = self.sys / path; file.parent.mkdir(parents=True, exist_ok=True); file.write_text(value)
    def pd(self):
        return (self.sys / 'qcom-battery/pd_verifed').read_text()
    def cli(self, *args, check=True):
        return subprocess.run([str(BIN), '--module-dir', str(self.module), '--sys-root', str(self.sys), *args], text=True, capture_output=True, check=check)
    def start(self):
        self.daemon = subprocess.Popen([str(BIN), '--module-dir', str(self.module), '--sys-root', str(self.sys), 'run'], text=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        self.wait(lambda: (self.module / 'daemon.pid').exists())
    def wait(self, condition, timeout=5):
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            if condition(): return
            if self.daemon and self.daemon.poll() is not None:
                output = self.daemon.communicate()
                if 'Operation not permitted' in output[1] and 'kernel uevents' in output[1]:
                    self.skipTest('host sandbox blocks NETLINK_KOBJECT_UEVENT; reactor socket/inotify tests run in cargo test')
                self.fail(f'daemon exited: {output}')
            time.sleep(.02)
        self.fail('timed out waiting for expected state')
    def wake(self):
        # A real control-directory event wakes poll after changing a fake node.
        self.cli('configure', '2500', '1500')
    def test_real_event_loop_boot_resume_atomic_rename_and_single_writer(self):
        self.start()
        status = json.loads(self.cli('status').stdout)
        self.assertTrue(status['running']); self.assertEqual(status['apdo_max'], 55)
        # Startup never prewrites; pending attach timeout is cancelable by atomic rename.
        self.assertEqual(self.pd(), '0')
        self.cli('disable'); time.sleep(.1)
        self.assertEqual(self.pd(), '0')
        self.cli('enable')
        time.sleep(.1); self.assertEqual(self.pd(), '0')
        self.wait(lambda: self.pd() == '1')
        duplicate = self.cli('run', check=False)
        self.assertNotEqual(duplicate.returncode, 0)
        self.assertIn('another YAFreePPS', duplicate.stderr)
        # Attached pause must leave the negotiated value alone.
        self.cli('disable'); time.sleep(.1); self.assertEqual(self.pd(), '1')
        # Kernel clears on detach. Paused daemon does not prepare the next session.
        self.write('power_supply/battery/status', 'Discharging')
        self.write('qcom-battery/pd_verifed', '0'); self.wake(); time.sleep(.1)
        self.assertEqual(self.pd(), '0')
        # Resume on an already attached MIPPS adapter does not unlock.
        self.write('power_supply/battery/status', 'Charging')
        self.write('xm_power/typec/strategy_pd_auth/adapter_svid', '2717')
        self.cli('enable'); time.sleep(2.7)
        self.assertEqual(self.pd(), '0')
        # SIGTERM must wake the blocked event loop promptly.
        start = time.monotonic(); self.daemon.terminate(); self.daemon.communicate(timeout=1)
        self.assertLess(time.monotonic() - start, 1)
        self.assertFalse((self.module / 'daemon.pid').exists())
    def test_idle_pause_restores_but_attached_pause_does_not(self):
        self.write('qcom-battery/pd_verifed', '1')
        self.cli('disable'); self.assertEqual(self.pd(), '1')
        self.write('power_supply/battery/status', 'Discharging')
        self.cli('disable'); self.assertEqual(self.pd(), '0')
    def test_direct_restore_cannot_write_while_enabled_or_attached(self):
        self.write('qcom-battery/pd_verifed', '1')
        self.cli('restore-idle'); self.assertEqual(self.pd(), '1')
        self.write('power_supply/battery/status', 'Discharging')
        self.cli('restore-idle'); self.assertEqual(self.pd(), '1')
    def test_bad_config_can_still_be_paused_and_repaired(self):
        (self.module / 'config').write_text('settle_ms=0\n')
        self.assertNotEqual(self.cli('run', check=False).returncode, 0)
        self.cli('disable')
        self.cli('configure', '3000', '2000')
        self.assertFalse(json.loads(self.cli('status').stdout)['enabled'])
    def test_configuration_rejects_injection_and_unsafe_values(self):
        for args in [('100', '1000'), ('2000', '999'), ('2000;id', '1500')]:
            self.assertNotEqual(self.cli('configure', *args, check=False).returncode, 0)
        self.assertFalse((self.module / 'config').exists())
    def test_disabled_or_removing_modules_cannot_be_enabled_by_action(self):
        for name in ['disable', 'remove']:
            (self.module / name).touch()
            self.cli('enable')
            self.assertFalse(json.loads(self.cli('status').stdout)['enabled'])
            (self.module / name).unlink()
    def test_status_escapes_untrusted_node_text(self):
        self.write('xm_power/charger/charger_common/real_type', '"PPS\\\n\t')
        data = json.loads(self.cli('status').stdout)
        self.assertEqual(data['real_type'], '"PPS\\')

if __name__ == '__main__':
    unittest.main()
