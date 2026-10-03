#!/usr/bin/env python3
"""Compare the public Rust value API with the existing two C API callers."""
import argparse
import importlib.util
import json
import math
from pathlib import Path
import statistics
import subprocess
import sys
import shutil
import re

SPEC = importlib.util.spec_from_file_location('capi_bench', Path(__file__).with_name('run.py'))
BENCH = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(BENCH)
ROOT = BENCH.ROOT


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, default=ROOT / 'target/c-bench')
    parser.add_argument('--rounds', type=int, default=5)
    parser.add_argument('--samples', type=int, default=9)
    args = parser.parse_args()
    if args.rounds < 2 or args.samples < 3:
        parser.error('at least 2 rounds and 3 samples required')
    output = args.output.resolve()
    original = json.loads((output / 'results.json').read_text())
    candidate_root = Path(original.get('candidate_root', ROOT))
    for library in original['libraries'].values():
        if BENCH.sha(Path(library['path'])) != library['sha256']:
            raise RuntimeError('C library changed; regenerate original benchmark')
    package = output / 'rust-package'
    package.mkdir(parents=True, exist_ok=True)
    rust_source = ROOT / 'tools/capi-bench/rust'
    manifest = (rust_source / 'Cargo.toml').read_text().replace('path = "../../.."',
                                                              'path = ' + json.dumps(str(candidate_root)))
    (package / 'Cargo.toml').write_text(manifest)
    shutil.copy2(rust_source / 'main.rs', package / 'main.rs')
    # Preserve candidate dependency versions; only this private caller package is adjusted.
    shutil.copy2(candidate_root / 'capi/Cargo.lock', package / 'Cargo.lock')
    build = ['cargo', 'build', '--release', '--offline', '--manifest-path',
             str(package / 'Cargo.toml'), '--target-dir', str(output / 'rust-build')]
    subprocess.run(build, cwd=ROOT, check=True)
    versions = lambda path: dict(re.findall(r'name = "([^"]+)"\nversion = "([^"]+)"',
                                            path.read_text()))
    pinned_versions = versions(candidate_root / 'capi/Cargo.lock')
    for name, version in versions(package / 'Cargo.lock').items():
        if name != 'capi-bench-rust-baseline' and pinned_versions.get(name) != version:
            raise RuntimeError('Rust/C dependency version disagreement: ' + name)
    binaries = {'rust-api': output / 'rust-build/release/rust-api-baseline',
                'serde-ucl-c': output / 'bench-serde-ucl-c', 'libucl': output / 'bench-libucl'}
    docs = BENCH.documents(output / 'documents')
    for name, path in docs.items():
        if BENCH.sha(path) != original['documents'][name]['sha256']:
            raise RuntimeError('input changed: ' + name)
        values = [BENCH.command([binary, path, 'dump', '1']) for binary in binaries.values()]
        if any(value != values[0] for value in values[1:]):
            raise RuntimeError('Rust/C output disagreement: ' + name)
    print('Rust and both C callers agree on all eight outputs', flush=True)
    iterations = {}
    for name, path in docs.items():
        for tool, binary in binaries.items():
            count = 1
            while True:
                elapsed = BENCH.timing(binary, path, 'parse', count)
                if elapsed >= 0.025:
                    break
                count = max(count + 1, math.ceil(count * 0.025 / elapsed * 1.15))
            iterations[(name, tool)] = count
    samples = []
    for round_id in range(args.rounds):
        for name, path in docs.items():
            for sample in range(args.samples):
                order = list(binaries)
                offset = (round_id + sample) % len(order)
                order = order[offset:] + order[:offset]
                for tool in order:
                    count = iterations[(name, tool)]
                    elapsed = BENCH.timing(binaries[tool], path, 'parse', count)
                    samples.append({'round': round_id + 1, 'sample': sample + 1,
                                    'document': name, 'tool': tool, 'iterations': count,
                                    'elapsed_seconds': elapsed,
                                    'seconds_per_operation': elapsed / count})
        print(f'baseline round {round_id + 1}/{args.rounds} complete', flush=True)
    summary = {}
    lines = ['# Rust API versus C API parsing', '',
             'Same inputs, release profiles, and machine as the C comparison. All three',
             'callers emit byte-identical compact JSON before timings are accepted.',
             'The Rust caller times `parse::parse` and drops its result every iteration,',
             'matching the API used in the existing README comparison. C callers include',
             'parser creation, submission, retained-root retrieval, parser free and unref.', '',
             f'{args.rounds} rotated rounds × {args.samples} samples, calibrated to 25ms.', '',
             '| Input | Rust API µs | Our C API µs | libucl µs | C API / Rust API |',
             '| --- | ---: | ---: | ---: | ---: |']
    for name in docs:
        summary[name] = {tool: statistics.median(s['seconds_per_operation'] for s in samples
                                               if s['document'] == name and s['tool'] == tool)
                         for tool in binaries}
        values = summary[name]
        lines.append(f"| {name} | {values['rust-api']*1e6:.2f} | "
                     f"{values['serde-ucl-c']*1e6:.2f} | {values['libucl']*1e6:.2f} | "
                     f"{values['serde-ucl-c']/values['rust-api']:.2f}× |")
    lines += ['', 'The C/Rust difference covers all differences in the public API paths;',
              'this experiment alone does not isolate one internal allocation or copy.',
              'Raw results and binary hashes: local `target/c-bench/rust-baseline.json`.', '']
    result = {'candidate_code_commit': original['candidate_commit'],
              'reference_commit': original['reference_commit'], 'machine': original['machine'],
              'measurement_commit': BENCH.command(['git', 'rev-parse', 'HEAD'], cwd=ROOT,
                                                 text=True).strip(),
              'build_command': build, 'dependency_versions_verified': True,
              'rounds': args.rounds, 'samples_per_round': args.samples,
              'documents': original['documents'], 'summary': summary, 'samples': samples,
              'binaries': {tool: {'path': str(path), 'sha256': BENCH.sha(path)}
                           for tool, path in binaries.items()}}
    (output / 'rust-baseline.json').write_text(json.dumps(result, indent=2) + '\n')
    (output / 'rust-baseline.md').write_text('\n'.join(lines))
    print('\n'.join(lines))


if __name__ == '__main__':
    main()
