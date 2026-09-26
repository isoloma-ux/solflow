#!/usr/bin/env python3
"""Build the MCP sidecar for Tauri. Does not install, sign, publish or connect."""
import argparse
import os
from pathlib import Path
import shutil
import subprocess

parser = argparse.ArgumentParser()
parser.add_argument('--release', action='store_true')
parser.add_argument('--target')
args = parser.parse_args()
root = Path(__file__).resolve().parents[1]
command = ['cargo', 'build', '--locked', '--manifest-path', str(root / 'desktop/mcp-server/Cargo.toml')]
if args.release:
    command.append('--release')
if args.target:
    command += ['--target', args.target]
subprocess.run(command, check=True)
host = next(line.split(': ', 1)[1] for line in subprocess.check_output(['rustc', '-vV'], text=True).splitlines() if line.startswith('host: '))
triple = args.target or host
suffix = '.exe' if 'windows' in triple else ''
target = Path(os.environ.get('CARGO_TARGET_DIR', root / 'desktop/mcp-server/target')).resolve()
if args.target:
    target /= args.target
source = target / ('release' if args.release else 'debug') / ('solflow-mcp' + suffix)
destination = root / 'desktop/src-tauri/binaries' / ('solflow-mcp-' + triple + suffix)
destination.parent.mkdir(parents=True, exist_ok=True)
shutil.copy2(source, destination)
print('MCP sidecar prepared:', destination)
