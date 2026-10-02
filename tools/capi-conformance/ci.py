#!/usr/bin/env python3
"""Spec-owned black-box release gate; implementation participants may run, not read."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import shlex
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[2]
CONTRACT = ROOT / 'docs/spec/c-api/stage-a'
REPOSITORY = 'https://github.com/vstakhov/libucl.git'


def run(command, **kwargs):
    print('+ ' + shlex.join(map(str, command)), flush=True)
    return subprocess.run(command, check=True, timeout=600, **kwargs)


def sha256(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--candidate-include', required=True, type=Path)
    parser.add_argument('--candidate-library', required=True, action='append', type=Path)
    parser.add_argument('--golden-dir', required=True, type=Path)
    parser.add_argument('--work-dir', type=Path, default=ROOT / 'target/c15-oracle')
    parser.add_argument('--oracle-source', type=Path,
                        help='spec-side reuse of an existing pinned checkout; never an implementation input')
    args = parser.parse_args()
    if platform.system() not in ('Linux', 'Darwin') or platform.machine().lower() not in (
            'arm64', 'aarch64', 'x86_64', 'amd64'):
        parser.error('only native Linux amd64/arm64 and Darwin arm64 release probes are supported')
    if platform.system() == 'Darwin' and platform.machine().lower() != 'arm64':
        parser.error('Darwin release probes require arm64')
    for library in args.candidate_library:
        if not library.is_file():
            parser.error('candidate library does not exist: ' + str(library))
    work = args.work_dir.resolve()
    work.mkdir(parents=True, exist_ok=True)
    manifest = json.loads((CONTRACT / 'api.json').read_text())
    reference_commit = manifest['reference_commit']
    source = args.oracle_source.resolve() if args.oracle_source else work / 'upstream'
    if not source.exists():
        run(['git', 'init', str(source)])
        run(['git', '-C', str(source), 'remote', 'add', 'origin', REPOSITORY])
        run(['git', '-C', str(source), 'fetch', '--depth=1', 'origin', reference_commit])
        run(['git', '-C', str(source), 'checkout', '--detach', 'FETCH_HEAD'])
    commit = run(['git', '-C', str(source), 'rev-parse', 'HEAD'],
                 stdout=subprocess.PIPE, text=True).stdout.strip()
    if commit != reference_commit:
        raise SystemExit('oracle checkout is not the pinned reference commit')
    if run(['git', '-C', str(source), 'status', '--porcelain'],
           stdout=subprocess.PIPE, text=True).stdout:
        raise SystemExit('oracle source checkout must be clean')
    build = work / 'build'
    run(['cmake', '-S', str(source), '-B', str(build),
         '-DCMAKE_BUILD_TYPE=Release', '-DBUILD_SHARED_LIBS=OFF',
         '-DCMAKE_POLICY_VERSION_MINIMUM=3.5', '-DENABLE_URL_INCLUDE=OFF',
         '-DENABLE_URL_SIGN=OFF', '-DENABLE_LUA=OFF', '-DENABLE_UTILS=OFF'])
    run(['cmake', '--build', str(build), '--target', 'ucl', '--parallel',
         str(min(os.cpu_count() or 2, 8))])
    oracle_library = build / 'libucl.a'
    snapshots = args.golden_dir.resolve()
    snapshots.mkdir(parents=True, exist_ok=True)
    comparator = Path(__file__).with_name('run.py')
    common = [sys.executable, str(comparator), '--oracle-include', str(source / 'include'),
              '--oracle-library', str(oracle_library), '--golden-dir', str(snapshots)]
    # Only oracle-only executions can write expected outputs. Comparisons cannot update them.
    run(common + ['--update-golden'])
    config = json.loads((CONTRACT / 'cases.json').read_text())
    platform_id = platform.system().lower() + '-' + platform.machine().lower()
    released = ROOT / config['golden'] / platform_id
    # Preserve the frozen Darwin evidence; new Linux evidence is generated on that host.
    if released.is_dir():
        for case in config['cases']:
            if (released / (case + '.txt')).read_bytes() != (snapshots / (case + '.txt')).read_bytes():
                raise SystemExit('released oracle snapshot drift: ' + case)
    for library in args.candidate_library:
        run(common + ['--candidate-include', str(args.candidate_include.resolve()),
                      '--candidate-library', str(library.resolve())])
    evidence = {
        'reference_commit': commit,
        'reference_header_sha256': sha256(source / 'include/ucl.h'),
        'platform': platform_id,
        'machine': platform.machine(),
        'system': platform.system(),
        'contract_sha256': sha256(CONTRACT / 'api.json'),
        'probe_sha256': sha256(ROOT / config['source']),
        'cases': {case: sha256(snapshots / (case + '.txt')) for case in config['cases']},
        'provided_functions': sum(f['provided'] for f in manifest['functions']),
        'candidate_libraries': {str(p.resolve()): sha256(p.resolve()) for p in args.candidate_library},
        'header_library_combinations_per_candidate': 4,
        'validation': 'oracle-only snapshots followed by successful signature/header/link/runtime comparisons',
    }
    (snapshots / 'reference-evidence.json').write_text(json.dumps(evidence, indent=2) + '\n')
    print('PASS pinned-reference release gate; evidence: ' + str(snapshots), flush=True)


if __name__ == '__main__':
    main()
