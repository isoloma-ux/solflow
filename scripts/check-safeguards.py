#!/usr/bin/env python3
"""Headless regression tests. No app launch, cloud access, installs or real records.
Rust uses rustc. Kotlin uses cached compiler jars and JAVA_HOME (or local toolchain).
"""
from pathlib import Path
import os
import subprocess
import tempfile

root = Path(__file__).resolve().parents[1]
cache = Path(os.environ.get('GRADLE_USER_HOME', str(Path.home() / '.gradle'))) / 'caches/modules-2/files-2.1'
java_home = os.environ.get('JAVA_HOME')
java = str(Path(java_home) / 'bin/java') if java_home else str(Path.home() / 'android-toolchain/jdk-21.0.12.1+1/Contents/Home/bin/java')

def jar(group, artifact, version):
    matches = list((cache / group / artifact / version).glob('*/*.jar'))
    if len(matches) != 1:
        raise SystemExit(f'Expected cached {group}:{artifact}:{version}; set GRADLE_USER_HOME if needed')
    return str(matches[0])

with tempfile.TemporaryDirectory(prefix='solflow-regression-') as out:
    rust = str(Path(out) / 'rust-tests')
    subprocess.run(['rustc', '--edition=2021', '--test', str(root / 'tests/rust/safeguards.rs'), '-o', rust], check=True)
    subprocess.run([rust], check=True)
    stdlib = jar('org.jetbrains.kotlin', 'kotlin-stdlib', '2.2.10')
    annotations = jar('org.jetbrains', 'annotations', '13.0')
    compiler_cp = os.pathsep.join([
        jar('org.jetbrains.kotlin', 'kotlin-compiler-embeddable', '2.2.10'),
        jar('org.jetbrains.kotlin', 'kotlin-script-runtime', '2.2.10'),
        jar('org.jetbrains.kotlinx', 'kotlinx-coroutines-core-jvm', '1.8.0'),
        stdlib, annotations,
    ])
    classes = str(Path(out) / 'kotlin-classes')
    sources = [root / 'app-android/app/src/main/java/com/handy/voice' / name for name in ['TextCleanup.kt', 'TranscriptFiles.kt', 'SpeakerAttribution.kt', 'AiAccessRules.kt']]
    sources.append(root / 'tests/kotlin/Safeguards.kt')
    subprocess.run([java, '-cp', compiler_cp, 'org.jetbrains.kotlin.cli.jvm.K2JVMCompiler',
                    '-no-stdlib', '-no-reflect', '-jvm-target', '17',
                    '-classpath', os.pathsep.join([stdlib, annotations]), '-d', classes,
                    *map(str, sources)], check=True)
    subprocess.run([java, '-cp', os.pathsep.join([classes, stdlib]), 'SafeguardsKt',
                    str(root / 'tests/fixtures/cleanup.tsv')], check=True)
