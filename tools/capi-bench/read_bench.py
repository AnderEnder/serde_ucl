#!/usr/bin/env python3
"""Spec-owned public read timings on pre-parsed containers, with matched callers."""
import argparse
import json
from pathlib import Path
import statistics
import subprocess

from run import ROOT, sha


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--before', type=Path, required=True)
    parser.add_argument('--after', type=Path, required=True)
    parser.add_argument('--rounds', type=int, default=5)
    parser.add_argument('--samples', type=int, default=9)
    args = parser.parse_args()
    if args.rounds < 2 or args.samples < 3:
        parser.error('at least two rounds and three samples required')
    before = json.loads((args.before / 'results.json').read_text())
    after = json.loads((args.after / 'results.json').read_text())
    output = args.after.resolve()
    caller = ROOT / 'tools/capi-bench/read.c'
    variants = [('before', before, 0, 'serde-ucl-c'),
                ('after', after, 0, 'serde-ucl-c'), ('libucl', after, 1, 'libucl')]
    binaries, builds = {}, {}
    for name, evidence, index, library in variants:
        archive = Path(evidence['libraries'][library]['path'])
        if sha(archive) != evidence['libraries'][library]['sha256']:
            raise RuntimeError(f'{name} archive changed since measured benchmark')
        binary = output / ('read-' + name)
        build = [str(caller) if value.endswith('/tools/capi-bench/bench.c') else value
                 for value in evidence['compile_commands'][index]]
        build[-1] = str(binary)
        subprocess.run(build, check=True)
        binaries[name], builds[name] = binary, build
    cases = [('records-10000', 'records', mode) for mode in ('first', 'full', 'lookup')]
    cases += [('flat-1000', '-', 'first'), ('flat-1000', '-', 'full'),
              ('flat-1000', 'key_999', 'lookup')]
    samples, counts = [], {}

    def measure(name, document, key, mode, iterations):
        result = json.loads(subprocess.check_output(
            [str(binaries[name]), str(document), key, mode, str(iterations)], text=True))
        if result['seconds'] <= 0 or result['count'] <= 0:
            raise RuntimeError('invalid timing/traversal')
        return result

    for case, key, mode in cases:
        document = output / 'documents' / (case + '.ucl')
        if sha(document) != after['documents'][case]['sha256']:
            raise RuntimeError('input changed since main benchmark')
        repetitions, expected = {}, None
        for name in binaries:
            result = measure(name, document, key, mode, 1000)
            if expected is not None and result['count'] != expected:
                raise RuntimeError('library traversal counts differ')
            expected = result['count']
            repetitions[name] = min(10000000, max(1, int(0.03 / (result['seconds'] / 1000))))
        counts[f'{case}/{mode}'] = expected
        for round_index in range(args.rounds):
            for sample_index in range(args.samples):
                names = list(binaries)
                offset = (round_index + sample_index) % len(names)
                for name in names[offset:] + names[:offset]:
                    iterations = repetitions[name]
                    result = measure(name, document, key, mode, iterations)
                    if result['count'] != expected:
                        raise RuntimeError('inconsistent traversal count')
                    samples.append({'case': case, 'key': key, 'mode': mode, 'tool': name,
                                    'round': round_index, 'sample': sample_index,
                                    'iterations': iterations,
                                    'seconds_per_operation': result['seconds'] / iterations})
        print(f'{case}/{mode} complete', flush=True)
    summary = {f'{case}/{mode}/{name}': statistics.median(
        value['seconds_per_operation'] for value in samples
        if value['case'] == case and value['mode'] == mode and value['tool'] == name)
        for case, _, mode in cases for name in binaries}
    evidence = {'before_commit': before['candidate_commit'], 'after_commit': after['candidate_commit'],
                'reference_commit': after['reference_commit'], 'caller_sha256': sha(caller),
                'build_commands': builds, 'before_libraries': before['libraries'],
                'after_libraries': after['libraries'], 'documents': after['documents'],
                'rounds': args.rounds, 'samples_per_round': args.samples,
                'scope': 'pre-parsed immediate children; handle new/next/free, or one indexed lookup',
                'counts': counts, 'summary': summary, 'samples': samples}
    (output / 'reads.json').write_text(json.dumps(evidence, indent=2) + '\n')
    lines = ['# Public C read operations', '', evidence['scope'], '',
             '| Input / operation | Before µs | After µs | libucl µs |',
             '| --- | ---: | ---: | ---: |']
    for case, _, mode in cases:
        values = [f'{summary[f"{case}/{mode}/{name}"] * 1e6:.3f}' for name in binaries]
        lines.append(f'| {case}/{mode} | ' + ' | '.join(values) + ' |')
    (output / 'reads.md').write_text('\n'.join(lines) + '\n')


if __name__ == '__main__':
    main()
