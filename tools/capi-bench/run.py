#!/usr/bin/env python3
"""Spec-owned benchmark of the released C API; never an implementation input."""
import argparse
import hashlib
import json
import math
import os
from pathlib import Path
import platform
import shutil
import statistics
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parents[2]


def command(argv, **kwargs):
    return subprocess.check_output(list(map(str, argv)), **kwargs)


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def documents(directory):
    values = {
        'tiny-ucl': b'host = "example.com"; port = 8080; enabled = true;\n',
        'tiny-json': b'{"host":"example.com","port":8080,"enabled":true}\n',
        'flat-1000': ''.join(f'key_{i} = {i};\n' for i in range(1000)).encode(),
        'floats-1000': ''.join(f'key_{i} = {i / 13:.8f};\n' for i in range(1000)).encode(),
        'strings-1000': json.dumps({f'key_{i}': 'escaped "value" \\ path\n' * 3
                                    for i in range(1000)}, separators=(',', ':')).encode(),
    }
    for count in (1000, 10000):
        values[f'records-{count}'] = json.dumps({'records': [
            {'id': i, 'name': f'service-{i}', 'enabled': i % 3 != 0,
             'ports': [8000 + i % 100, 9000 + i % 100]}
            for i in range(count)]}, separators=(',', ':')).encode()
    values['rspamd-rbl'] = (ROOT / 'benches/corpus/rspamd/scores.d/rbl_group.conf').read_bytes()
    directory.mkdir(parents=True, exist_ok=True)
    for name, content in values.items():
        (directory / (name + '.ucl')).write_bytes(content)
    return {name: directory / (name + '.ucl') for name in values}


def timing(binary, document, mode, count):
    result = float(command([binary, document, mode, str(count)], text=True))
    if not math.isfinite(result) or result <= 0:
        raise RuntimeError('invalid benchmark duration')
    return result


def report(result):
    lines = ['# C library performance comparison', '',
             f"Measured on {result['machine']['cpu']}, {result['machine']['platform']}.", '',
             f"Candidate commit: `{result['candidate_commit']}`; libucl: `{result['reference_commit']}`.", '',
             'The same C11 caller and released header link separately against static libraries.',
             'Rust uses Cargo release (fat LTO, one codegen unit); libucl uses CMake Release.',
             'The caller uses `-O3`. Both parser flags are zero; input is already in memory.',
             'Parsing includes parser creation, input submission, retained-root retrieval, parser',
             'cleanup and root destruction. Emission times compact JSON from one pre-parsed',
             'object and frees each emitted buffer. File IO and process startup are not timed.', '',
             f"{result['rounds']} alternating rounds × {result['samples_per_round']} samples; "
             f"samples calibrated to at least {result['sample_ms']:g} ms.",
             'Every input first passed byte-identical compact-JSON output comparison.',
             'Times are medians of all samples; spread is the min–max of round medians.', '',
             '| Operation | Input | Bytes | serde-ucl-c µs | libucl µs | serde-ucl-c / libucl |',
             '| --- | --- | ---: | ---: | ---: | ---: |']
    for mode in ('parse', 'emit'):
        for name, case in result['documents'].items():
            rust = result['summary'][f'{mode}/{name}/serde-ucl-c']['median_seconds']
            oracle = result['summary'][f'{mode}/{name}/libucl']['median_seconds']
            lines.append(f'| {mode} | {name} | {case["bytes"]:,} | {rust * 1e6:.2f} | '
                         f'{oracle * 1e6:.2f} | {rust / oracle:.2f}× |')
    lines += ['', 'A ratio above 1 means serde-ucl-c is slower. These are local wall-clock',
              'measurements, not Linux/other-machine results or a memory benchmark.',
              'The Stage A adapter creates the public C object representation; results cover',
              'that complete API path, not only the Rust parser. Includes, callbacks, mutations',
              'and other deferred APIs are not measured.', '',
              '## Round spread (µs)', '',
              '| Operation/input | serde-ucl-c | libucl |', '| --- | ---: | ---: |']
    for mode in ('parse', 'emit'):
        for name in result['documents']:
            spans = []
            for tool in ('serde-ucl-c', 'libucl'):
                medians = result['summary'][f'{mode}/{name}/{tool}']['round_medians_seconds']
                spans.append(f'{min(medians)*1e6:.2f}–{max(medians)*1e6:.2f}')
            lines.append(f'| {mode}/{name} | {spans[0]} | {spans[1]} |')
    lines += ['', '## Public-call breakdown (µs)', '',
              'Separate measurements on the two record inputs include a clock read at each',
              'call boundary; these explain the larger-input results rather than replacing',
              'the aggregate timings above. Values are median time per call.', '',
              '| Input/library | parser_new | add_chunk | get_object | parser_free | object_unref |',
              '| --- | ---: | ---: | ---: | ---: | ---: |']
    for name, tools in result['phases'].items():
        for tool, phases in tools.items():
            values = ' | '.join(f'{v*1e6:.2f}' for v in phases['median_seconds'].values())
            lines.append(f'| {name}/{tool} | {values} |')
    lines += ['', 'Raw timings, iteration counts, input/library/header hashes, compiler versions,',
              'load averages and exact build commands are in the adjacent `results.json`.', '']
    return '\n'.join(lines)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--rounds', type=int, default=5)
    parser.add_argument('--samples', type=int, default=9)
    parser.add_argument('--sample-ms', type=float, default=25)
    parser.add_argument('--output', type=Path, default=ROOT / 'target/c-bench')
    args = parser.parse_args()
    if (args.rounds < 2 or args.samples < 3 or not math.isfinite(args.sample_ms)
            or args.sample_ms < 5):
        parser.error('use at least 2 rounds, 3 samples and 5ms per sample')
    if platform.system() not in ('Darwin', 'Linux'):
        parser.error('supported benchmark hosts: Linux and macOS')
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    environment = dict(os.environ, CARGO_TERM_COLOR='never')
    build = ['cargo', 'build', '--release', '--locked', '--manifest-path', 'capi/Cargo.toml',
             '--target-dir', str(ROOT / 'capi/target')]
    subprocess.run(build, cwd=ROOT, env=environment, check=True)
    candidate = ROOT / 'capi/target/release/libucl.a'
    gate = [sys.executable, str(ROOT / 'tools/capi-conformance/ci.py'),
            '--candidate-include', str(ROOT / 'capi/include'),
            '--candidate-library', str(candidate),
            '--golden-dir', str(output / 'reference/golden'),
            '--work-dir', str(output / 'reference')]
    with (output / 'conformance.log').open('w') as log:
        subprocess.run(gate, cwd=ROOT, check=True, stdout=log, stderr=log)
    libraries = {'serde-ucl-c': candidate, 'libucl': output / 'reference/build/libucl.a'}
    link_flags = ['-liconv', '-lSystem', '-lc', '-lm'] if platform.system() == 'Darwin' else [
        '-ldl', '-lpthread', '-lrt', '-lutil', '-lm']
    compiler = shutil.which(os.environ.get('CC', 'cc'))
    if not compiler:
        raise RuntimeError('C compiler not found')
    source = Path(__file__).with_name('bench.c')
    header = ROOT / 'docs/spec/c-api/stage-a/include/ucl.h'
    binaries = {}
    compilations = []
    for name, library in libraries.items():
        binary = output / ('bench-' + name)
        compile_args = [compiler, '-O3', '-std=c11', '-Wall', '-Wextra', '-Werror',
                        '-I', str(header.parent), str(source), str(library),
                        *link_flags, '-o', str(binary)]
        subprocess.run(compile_args, check=True)
        binaries[name] = binary
        compilations.append(compile_args)
    docs = documents(output / 'documents')
    metadata = {}
    for name, path in docs.items():
        outputs = [command([binary, path, 'dump', '1']) for binary in binaries.values()]
        if outputs[0] != outputs[1]:
            for tool, value in zip(binaries, outputs):
                (output / (name + '.' + tool + '.json')).write_bytes(value)
            raise RuntimeError('compact JSON disagreement: ' + name)
        metadata[name] = {'bytes': path.stat().st_size, 'sha256': sha(path),
                          'output_sha256': hashlib.sha256(outputs[0]).hexdigest()}
    print(f'{len(docs)} identical-output cases; conformance gate passed', flush=True)
    iterations = {}
    for mode in ('parse', 'emit'):
        for name, path in docs.items():
            for tool, binary in binaries.items():
                count = 1
                while True:
                    elapsed = timing(binary, path, mode, count)
                    if elapsed >= args.sample_ms / 1000:
                        break
                    count = max(count + 1, math.ceil(count * args.sample_ms / 1000 / elapsed * 1.15))
                iterations[f'{mode}/{name}/{tool}'] = count
    samples = []
    loads = []
    started = time.strftime('%Y-%m-%dT%H:%M:%SZ', time.gmtime())
    for round_id in range(args.rounds):
        loads.append(list(os.getloadavg()))
        tools = list(binaries)
        if round_id % 2:
            tools.reverse()
        for mode in ('parse', 'emit'):
            for name, path in docs.items():
                for sample in range(args.samples):
                    order = tools if sample % 2 == 0 else tools[::-1]
                    for tool in order:
                        count = iterations[f'{mode}/{name}/{tool}']
                        elapsed = timing(binaries[tool], path, mode, count)
                        samples.append({'round': round_id + 1, 'sample': sample + 1,
                                        'tool': tool, 'mode': mode, 'document': name,
                                        'iterations': count, 'elapsed_seconds': elapsed,
                                        'seconds_per_operation': elapsed / count})
        print(f'round {round_id + 1}/{args.rounds} complete; load {loads[-1]}', flush=True)
    summary = {}
    for key in iterations:
        mode, name, tool = key.split('/')
        chosen = [s for s in samples if (s['mode'], s['document'], s['tool']) == (mode, name, tool)]
        summary[key] = {
            'median_seconds': statistics.median(s['seconds_per_operation'] for s in chosen),
            'round_medians_seconds': [statistics.median(s['seconds_per_operation'] for s in chosen
                                                        if s['round'] == r + 1)
                                     for r in range(args.rounds)]}
    phases = {}
    for name in ('records-1000', 'records-10000'):
        phases[name] = {}
        for tool, binary in binaries.items():
            count = iterations[f'parse/{name}/{tool}']
            values = [json.loads(command([binary, docs[name], 'phases', str(count)], text=True))
                      for _ in range(args.samples)]
            phases[name][tool] = {'samples_seconds': values, 'median_seconds': {
                key: statistics.median(value[key] for value in values) for key in values[0]}}
    cpu = platform.processor()
    if platform.system() == 'Darwin':
        cpu = command(['sysctl', '-n', 'machdep.cpu.brand_string'], text=True).strip()
    manifest = json.loads((ROOT / 'docs/spec/c-api/stage-a/api.json').read_text())
    result = {'candidate_commit': command(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip(),
              'candidate_code_dirty': bool(command(['git', 'status', '--porcelain', '--', 'src', 'capi'],
                                                   cwd=ROOT, text=True).strip()),
              'reference_commit': manifest['reference_commit'], 'started_utc': started,
              'machine': {'cpu': cpu, 'platform': platform.platform(), 'machine': platform.machine()},
              'rustc': command(['rustc', '-Vv'], text=True),
              'compiler': command([compiler, '--version'], text=True),
              'build_command': build, 'compile_commands': compilations,
              'driver_sha256': sha(source), 'header_sha256': sha(header),
              'libraries': {tool: {'path': str(path), 'sha256': sha(path)}
                            for tool, path in libraries.items()},
              'rounds': args.rounds, 'samples_per_round': args.samples,
              'sample_ms': args.sample_ms, 'loads': loads,
              'documents': metadata, 'summary': summary, 'samples': samples, 'phases': phases}
    (output / 'results.json').write_text(json.dumps(result, indent=2) + '\n')
    (output / 'report.md').write_text(report(result))
    print(report(result))


if __name__ == '__main__':
    main()
