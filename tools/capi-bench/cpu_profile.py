#!/usr/bin/env python3
"""Spec-owned native macOS CPU profiling of the public C benchmark caller."""
import argparse
from collections import Counter
import hashlib
import json
from pathlib import Path
import platform
import subprocess
import xml.etree.ElementTree as ET


def summarize(path):
    root = ET.parse(path).getroot()
    definitions = {e.get('id'): e for e in root.iter() if e.get('id')}

    def resolve(element):
        if element is not None and element.get('ref'):
            return definitions[element.get('ref')]
        return element

    own = Counter()
    inclusive = Counter()
    groups = Counter()
    samples = 0
    for row in root.findall('.//row'):
        trace = resolve(row.find('backtrace'))
        if trace is None:
            continue
        names = [resolve(frame).get('name', '?') for frame in trace.findall('frame')]
        if not any('ucl_parser_add_chunk' in name or 'ucl_object_unref' in name for name in names):
            continue
        samples += 1
        own[names[0]] += 1
        inclusive.update(set(names))
        name = names[0]
        if any(token in name for token in ('malloc', 'realloc', 'calloc', 'free',
                                           'alloc_type', '__rdl_alloc', 'madvise')):
            groups['allocator/memory-release leaf'] += 1
        elif any(token in name for token in ('memmove', 'memcpy', 'memset')):
            groups['byte-copy/zero leaf'] += 1
        else:
            groups['other leaf'] += 1
    if samples < 100:
        raise RuntimeError('too few measured application samples in CPU trace')
    return {'samples': samples, 'self': own.most_common(40),
            'inclusive': inclusive.most_common(40),
            'leaf_groups': {group: {'samples': count, 'percent': count / samples * 100}
                            for group, count in groups.items()},
            'scope': 'stacks inside input submission or object destruction; inclusive frames overlap',
            'grouping': 'leaf-symbol substring categories; not an allocation counter'}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--benchmark-dir', type=Path, required=True)
    parser.add_argument('--name', default='cpu')
    parser.add_argument('--seconds', type=int, default=8)
    parser.add_argument('--export-only', type=Path,
                        help='summarize an existing exported time-profile XML')
    args = parser.parse_args()
    directory = args.benchmark_dir.resolve()
    binary = directory / 'bench-serde-ucl-c'
    document = directory / 'documents/records-10000.ucl'
    if args.seconds < 2:
        parser.error('at least 2 seconds required')
    if args.export_only:
        xml_path = args.export_only.resolve()
        recording = None
        return_code = None
    else:
        if platform.system() != 'Darwin':
            parser.error('native Time Profiler currently requires macOS/Xcode')
        trace = directory / (args.name + '.trace')
        if trace.exists():
            parser.error('choose a fresh name; profiling output already exists')
        recording = ['xcrun', 'xctrace', 'record', '--template', 'Time Profiler',
                     '--time-limit', f'{args.seconds}s', '--no-prompt', '--output', str(trace),
                     '--launch', '--', str(binary), str(document), 'parse', '10000']
        # Time-limited recordings kill the disposable benchmark process. Its elapsed
        # output/exit code is not a throughput result; verify the saved trace instead.
        result = subprocess.run(recording, capture_output=True, text=True, timeout=90)
        (directory / (args.name + '.log')).write_text(result.stdout + result.stderr)
        return_code = result.returncode
        toc = directory / (args.name + '-toc.xml')
        subprocess.run(['xcrun', 'xctrace', 'export', '--input', str(trace),
                        '--toc', '--output', str(toc)], check=True, timeout=60)
        if ET.parse(toc).find('.//process[@type="launched"]') is None:
            raise RuntimeError('CPU trace did not record the benchmark process')
        xml_path = directory / (args.name + '.xml')
        subprocess.run(['xcrun', 'xctrace', 'export', '--input', str(trace), '--xpath',
                        '/trace-toc/run[@number="1"]/data/table[@schema="time-profile"]',
                        '--output', str(xml_path)], check=True, timeout=60)
    summary = summarize(xml_path)
    summary.update({'record_command': recording, 'record_exit_code': return_code,
                    'binary_sha256': hashlib.sha256(binary.read_bytes()).hexdigest(),
                    'input_sha256': hashlib.sha256(document.read_bytes()).hexdigest(),
                    'xml_path': str(xml_path)})
    destination = directory / (args.name + '-summary.json')
    destination.write_text(json.dumps(summary, indent=2) + '\n')
    print(json.dumps({'samples': summary['samples'], 'leaf_groups': summary['leaf_groups'],
                      'summary': str(destination)}, indent=2))


if __name__ == '__main__':
    main()
