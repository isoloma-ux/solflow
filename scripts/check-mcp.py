#!/usr/bin/env python3
"""Build and test local MCP with synthetic recordings, without app/client launch."""
from pathlib import Path
import os
import subprocess

root = Path(__file__).resolve().parents[1]
env = os.environ.copy()
subprocess.run(['cargo', 'build', '--locked', '--manifest-path', str(root / 'desktop/mcp-server/Cargo.toml')], env=env, check=True, timeout=600)
name = 'solflow-mcp.exe' if os.name == 'nt' else 'solflow-mcp'
target = Path(env.get('CARGO_TARGET_DIR', root / 'desktop/mcp-server/target'))
env['SOLFLOW_MCP_BIN'] = str((target / 'debug' / name).resolve())
subprocess.run(['cargo', 'test', '--locked', '--manifest-path', str(root / 'desktop/mcp-core/Cargo.toml'), '--', '--include-ignored'], env=env, check=True, timeout=180)
