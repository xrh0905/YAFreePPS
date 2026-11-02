#!/usr/bin/env python3
"""Reproducible arm64 Android build. Requires Rust + an Android NDK (API 28+)."""
import argparse
import os
from pathlib import Path
import platform
import shutil
import subprocess

ROOT = Path(__file__).resolve().parent

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--ndk', default=os.environ.get('ANDROID_NDK_HOME') or os.environ.get('ANDROID_NDK_ROOT'))
    parser.add_argument('--api', type=int, default=28)
    parser.add_argument('--host-tag', help='NDK prebuilt directory; defaults to host OS')
    args = parser.parse_args()
    if not args.ndk:
        parser.error('set ANDROID_NDK_HOME or pass --ndk')
    if args.api < 28:
        parser.error('minimum supported Android API is 28')
    host = args.host_tag or {'Linux': 'linux-x86_64', 'Darwin': 'darwin-x86_64', 'Windows': 'windows-x86_64'}[platform.system()]
    toolchain = Path(args.ndk).resolve() / 'toolchains/llvm/prebuilt' / host / 'bin'
    linker = toolchain / f'aarch64-linux-android{args.api}-clang'
    if platform.system() == 'Windows':
        linker = linker.with_suffix('.cmd')
    if not linker.is_file():
        parser.error(f'NDK linker missing: {linker}')
    env = os.environ.copy()
    env['CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER'] = str(linker)
    env['PATH'] = str(toolchain) + os.pathsep + env.get('PATH', '')
    for command in [
        ['cargo', 'fmt', '--', '--check'],
        ['cargo', 'test', '--locked'],
        ['cargo', 'clippy', '--locked', '--all-targets', '--', '-D', 'warnings'],
        ['cargo', 'clippy', '--locked', '--target', 'aarch64-linux-android', '--', '-D', 'warnings'],
        ['cargo', 'build', '--locked', '--release', '--target', 'aarch64-linux-android'],
    ]:
        subprocess.run(command, cwd=ROOT, env=env, check=True)
    output = ROOT / 'output'
    output.mkdir(exist_ok=True)
    shutil.copy2(ROOT / 'target/aarch64-linux-android/release/yafreepps', output / 'yafreepps')
    subprocess.run(['python3' if platform.system() != 'Windows' else 'python', str(ROOT / 'package_module.py')], cwd=ROOT, check=True)

if __name__ == '__main__':
    main()
