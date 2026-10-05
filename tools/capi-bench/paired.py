#!/usr/bin/env python3
"""Spec-owned interleaved before/after C and Rust public-API comparisons."""
import argparse
import json
import math
import os
from pathlib import Path
import statistics
import time

from run import command, sha, timing


def load(directory):
    result = json.loads((directory / 'results.json').read_text())
    rust = json.loads((directory / 'rust-baseline.json').read_text())
    if rust['candidate_code_commit'] != result['candidate_commit']:
        raise RuntimeError('Rust/C evidence commits differ')
    if rust['documents'] != result['documents']:
        raise RuntimeError('Rust/C evidence documents differ')
    for binary in rust['binaries'].values():
        if sha(Path(binary['path'])) != binary['sha256']:
            raise RuntimeError('recorded binary changed: ' + binary['path'])
    return result, rust


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--before', type=Path, required=True)
    parser.add_argument('--after', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--rounds', type=int, default=5)
    parser.add_argument('--samples', type=int, default=9)
    parser.add_argument('--sample-ms', type=float, default=25)
    parser.add_argument('--allow-compiler-change', action='store_true',
                        help='label compiler+code comparison; independently verify source identity '
                             'before describing results as compiler-only')
    args = parser.parse_args()
    if (args.rounds < 2 or args.samples < 3 or not math.isfinite(args.sample_ms)
            or args.sample_ms < 5):
        parser.error('at least two rounds, three samples and five milliseconds required')
    before_dir, after_dir = args.before.resolve(), args.after.resolve()
    before, before_rust = load(before_dir)
    after, after_rust = load(after_dir)
    for field in ('reference_commit', 'header_sha256', 'driver_sha256', 'documents'):
        if before[field] != after[field]:
            raise RuntimeError('incompatible before/after evidence: ' + field)
    if before['rustc'] != after['rustc'] and not args.allow_compiler_change:
        raise RuntimeError('compiler changed; compare code on the same compiler or explicitly label it')
    binaries = {
        'before-c': Path(before_rust['binaries']['serde-ucl-c']['path']),
        'after-c': Path(after_rust['binaries']['serde-ucl-c']['path']),
        'before-rust': Path(before_rust['binaries']['rust-api']['path']),
        'after-rust': Path(after_rust['binaries']['rust-api']['path']),
        'libucl': Path(after_rust['binaries']['libucl']['path']),
    }
    docs = {name: after_dir / 'documents' / (name + '.ucl') for name in after['documents']}
    for name, document in docs.items():
        if sha(document) != after['documents'][name]['sha256']:
            raise RuntimeError('input hash changed: ' + name)
        expected = None
        for tool, binary in binaries.items():
            output = command([binary, document, 'dump', '1'])
            if expected is not None and output != expected:
                raise RuntimeError('caller output differs: ' + name + '/' + tool)
            expected = output
        if sha_bytes(expected) != after['documents'][name]['output_sha256']:
            raise RuntimeError('recorded output differs: ' + name)
    modes = {'parse': list(binaries), 'emit': ['before-c', 'after-c', 'libucl']}
    print('All five callers match the eight recorded outputs', flush=True)
    target = args.sample_ms / 1000
    iterations = {}
    for mode, names in modes.items():
        for name, document in docs.items():
            for tool in names:
                count = 1
                while True:
                    elapsed = timing(binaries[tool], document, mode, count)
                    if elapsed >= target:
                        break
                    count = max(count + 1, math.ceil(count * target / elapsed * 1.15))
                iterations[(mode, name, tool)] = count
    started = time.strftime('%Y-%m-%dT%H:%M:%SZ', time.gmtime())
    samples, loads = [], []
    for round_id in range(args.rounds):
        for mode, names in modes.items():
            for document_index, (name, document) in enumerate(docs.items()):
                for sample in range(args.samples):
                    offset = (round_id + document_index + sample) % len(names)
                    for tool in names[offset:] + names[:offset]:
                        count = iterations[(mode, name, tool)]
                        elapsed = timing(binaries[tool], document, mode, count)
                        samples.append({'round': round_id + 1, 'sample': sample + 1,
                                        'mode': mode, 'document': name, 'tool': tool,
                                        'iterations': count, 'elapsed_seconds': elapsed,
                                        'seconds_per_operation': elapsed / count})
        loads.append(list(os.getloadavg()))
        print(f'paired round {round_id + 1}/{args.rounds} complete', flush=True)
    summary = {}
    for mode, names in modes.items():
        for name in docs:
            for tool in names:
                chosen = [sample for sample in samples if sample['mode'] == mode
                          and sample['document'] == name and sample['tool'] == tool]
                summary[f'{mode}/{name}/{tool}'] = {
                    'median_seconds': statistics.median(s['seconds_per_operation'] for s in chosen),
                    'round_medians_seconds': [statistics.median(
                        s['seconds_per_operation'] for s in chosen if s['round'] == round_id + 1)
                        for round_id in range(args.rounds)],
                }
    result = {'scope': 'interleaved complete parse/drop; separate pre-parsed compact JSON emission',
              'compiler_comparison': args.allow_compiler_change,
              'before_commit': before['candidate_commit'], 'after_commit': after['candidate_commit'],
              'before_code_dirty': before['candidate_code_dirty'],
              'after_code_dirty': after['candidate_code_dirty'],
              'before_rustc': before['rustc'], 'after_rustc': after['rustc'],
              'before_evidence': str(before_dir), 'after_evidence': str(after_dir),
              'reference_commit': after['reference_commit'], 'machine': after['machine'],
              'driver_sha256': after['driver_sha256'], 'header_sha256': after['header_sha256'],
              'paired_runner_sha256': sha(Path(__file__)),
              'rust_before': before_rust['binaries'], 'rust_after': after_rust['binaries'],
              'documents': after['documents'], 'started_utc': started, 'loads': loads,
              'rounds': args.rounds, 'samples_per_round': args.samples,
              'sample_ms': args.sample_ms, 'summary': summary, 'samples': samples}
    directory = args.output.resolve()
    directory.mkdir(parents=True, exist_ok=True)
    (directory / 'paired.json').write_text(json.dumps(result, indent=2) + '\n')
    lines = ['# Interleaved before/after public API measurements', '', result['scope'], '',
             f"Before `{result['before_commit']}`, after `{result['after_commit']}`.",
             f'{args.rounds} rounds × {args.samples} samples; calibrated to {args.sample_ms:g} ms.', '',
             '| Input | C before µs | C after µs | Rust before µs | Rust after µs | libucl µs | C speedup | C / Rust after |',
             '| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |']
    for name in docs:
        values = {tool: summary[f'parse/{name}/{tool}']['median_seconds'] for tool in binaries}
        row = ' | '.join(f'{values[tool] * 1e6:.2f}' for tool in binaries)
        lines.append(f'| {name} | {row} | {values["before-c"]/values["after-c"]:.3f}× | '
                     f'{values["after-c"]/values["after-rust"]:.3f}× |')
    lines += ['', '| Compact JSON input | C before µs | C after µs | libucl µs | C speedup |',
              '| --- | ---: | ---: | ---: | ---: |']
    for name in docs:
        values = {tool: summary[f'emit/{name}/{tool}']['median_seconds'] for tool in modes['emit']}
        row = ' | '.join(f'{values[tool] * 1e6:.2f}' for tool in modes['emit'])
        lines.append(f'| {name} | {row} | {values["before-c"]/values["after-c"]:.3f}× |')
    lines += ['', 'All binary/input/output/header/driver hashes were verified. Per-round medians,',
              'machine loads and raw samples are in paired.json. Inclusive CPU samples and',
              'allocation instrumentation are separate experiments. No timing threshold is asserted.', '']
    if args.allow_compiler_change:
        lines += ['Compiler changes are explicitly permitted in this experiment. Results do not',
                  'by themselves establish an adapter-code improvement or source identity.', '']
    (directory / 'paired.md').write_text('\n'.join(lines))


def sha_bytes(value):
    import hashlib
    return hashlib.sha256(value).hexdigest()


if __name__ == '__main__':
    main()
