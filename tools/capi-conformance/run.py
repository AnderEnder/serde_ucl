#!/usr/bin/env python3
"""Spec-owned C15 ABI/signature and black-box comparison harness."""
import argparse
import difflib
import hashlib
import json
import os
from pathlib import Path
import platform
import shlex
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[2]
HERE = Path(__file__).resolve().parent
CONTRACT = ROOT / 'docs/spec/c-api/stage-a'



def run(command, **kwargs):
    return subprocess.run(command, check=True, timeout=60, **kwargs)


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument('--oracle-include', required=True, type=Path)
    ap.add_argument('--oracle-library', required=True, type=Path)
    ap.add_argument('--contract', type=Path, default=CONTRACT)
    ap.add_argument('--candidate-include', type=Path)
    ap.add_argument('--candidate-library', type=Path)
    ap.add_argument('--golden-dir', type=Path,
                    help='target-specific case snapshots; defaults to released platform snapshots')
    ap.add_argument('--update-golden', action='store_true',
                    help='write snapshots from the oracle only (spec team)')
    args = ap.parse_args()
    if args.update_golden and args.candidate_library:
        ap.error('--update-golden requires an oracle-only run')
    contract = args.contract.resolve()
    manifest = json.loads((contract / 'api.json').read_text())
    case_config = contract / 'cases.json'
    config = json.loads(case_config.read_text())
    cases = config['cases']
    probe = ROOT / config['source']
    golden_root = ROOT / config['golden']
    candidate_include = args.candidate_include or contract / 'include'
    upstream = (args.oracle_include / 'ucl.h').read_bytes()
    if hashlib.sha256(upstream).hexdigest() != manifest['reference_header_sha256']:
        ap.error('oracle header differs from the pinned public header')
    functions = [f for f in manifest['functions'] if f['provided']]
    # Derive function pointer types from the contract and compile assignments with
    # each header. This catches missing declarations and incompatible signatures;
    # global pointer initializers force link resolution of every provided symbol.
    declarations = []
    for n, f in enumerate(functions):
        decl = f['declaration'].rstrip(';')
        typ = decl.replace(f['name'] + '(', f'(*signature_{n})(', 1)
        declarations += [f'typedef {typ};',
                         f'signature_{n} exported_{n} = {f["name"]};']
    symbols = '#include <ucl.h>\n' + '\n'.join(declarations) + '\nint main(void) { return 0; }\n'
    cc = shlex.split(os.environ.get('CC', 'cc'))
    flags = ['-std=c11', '-D_POSIX_C_SOURCE=200809L', '-Wall', '-Wextra', '-Werror',
             '-Wno-deprecated-declarations']
    env = dict(os.environ, LC_ALL='C', TZ='UTC')
    build_root = ROOT / 'target/c15-conformance'
    build_root.mkdir(parents=True, exist_ok=True)
    platform_id = platform.system().lower() + '-' + platform.machine().lower()
    golden = args.golden_dir.resolve() if args.golden_dir else golden_root / platform_id
    if args.update_golden:
        golden.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(dir=build_root) as temp:
        work = Path(temp)
        (work / 'symbols.c').write_text(symbols)
        combinations = [
            ('oracle', args.oracle_include, args.oracle_library),
            ('contract-oracle', contract / 'include', args.oracle_library),
        ]
        if args.candidate_library:
            combinations += [
                ('upstream-candidate', args.oracle_include, args.candidate_library),
                ('candidate', candidate_include, args.candidate_library),
            ]
        programs = []
        for label, include, library in combinations:
            include, library = include.resolve(), library.resolve()
            for source, output in [(work / 'symbols.c', label + '-symbols'),
                                   (probe, label)]:
                command = cc + flags + ['-I', str(include), str(source), str(library),
                                        '-lm', '-o', str(work / output)]
                if platform.system() == 'Linux':
                    command += ['-ldl', '-lpthread', '-lrt', '-lutil']
                if '.so' in library.name or library.suffix == '.dylib':
                    command += ['-Wl,-rpath,' + str(library.parent)]
                run(command)
            run([str(work / (label + '-symbols'))], cwd=work, env=env)
            # Verify that the public header is usable from C++ too.
            (work / 'header.cc').write_text('#include <ucl.h>\nint main() { return UCL_PRIORITY_MAX != 15; }\n')
            run(shlex.split(os.environ.get('CXX', 'c++')) + ['-std=c++11', '-Wall', '-Wextra', '-Werror',
                '-I', str(include), '-fsyntax-only', str(work / 'header.cc')])
            programs.append((label, work / label))
        for case in cases:
            outputs = [(label, run([str(program), case], cwd=work, env=env,
                                  stdout=subprocess.PIPE).stdout)
                       for label, program in programs]
            reference = outputs[0][1]
            for label, actual in outputs[1:]:
                if actual != reference:
                    diff = ''.join(difflib.unified_diff(
                        reference.decode(errors="backslashreplace").splitlines(True),
                        actual.decode(errors="backslashreplace").splitlines(True),
                        fromfile='oracle/' + case, tofile=label + '/' + case))
                    raise SystemExit(diff)
            path = golden / (case + '.txt')
            if args.update_golden:
                path.write_bytes(reference)
            elif path.exists():
                if path.read_bytes() != reference:
                    raise SystemExit('golden drift: ' + str(path))
            else:
                raise SystemExit('no snapshot for ' + platform_id + ': run --update-golden as spec team')
            print('PASS ' + case)
        print(f'{len(cases)} cases; {len(functions)} signatures/symbols; '
              f'{len(programs)} header/library combinations; {platform_id}')


if __name__ == '__main__':
    main()
