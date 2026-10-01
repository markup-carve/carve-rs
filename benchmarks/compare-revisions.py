import argparse
import json
import os
import statistics
import subprocess
from pathlib import Path

parser = argparse.ArgumentParser()
parser.add_argument('--engine', choices=['js', 'rs', 'php'], required=True)
parser.add_argument('--harness', required=True)
parser.add_argument('--revision', action='append', required=True)
parser.add_argument('--output', required=True)
args = parser.parse_args()
revisions = [entry.split('=', 1) for entry in args.revision]
if len(revisions) != 3 or any(len(entry) != 2 for entry in revisions):
    parser.error('Provide three --revision label=path arguments')
launcher = {'js': 'node', 'rs': 'python3', 'php': 'php'}[args.engine]
env = os.environ.copy()
env['CARVE_BENCH_RELEASE'] = '1'
repository = Path(args.harness).resolve().parent.parent
commits = {}
for label, path in revisions:
    directory = Path(path)
    repo = directory if directory.is_dir() else repository
    ref = 'HEAD' if directory.is_dir() or label == 'dev-main' else f'{label}^{{commit}}'
    commits[label] = subprocess.check_output(
        ['git', '-C', str(repo), 'rev-parse', ref], text=True).strip()
initial_load = Path('/proc/loadavg').read_text().split()[:3]
runs = []
for round_index in range(3):
    order = revisions[round_index:] + revisions[:round_index]
    for label, path in order:
        print(f'{args.engine} round {round_index + 1}: {label}', flush=True)
        output = subprocess.check_output([launcher, args.harness, path], env=env, text=True)
        runs.append(dict(round=round_index + 1, revision=label, results=json.loads(output)))
summary = {}
for label, _ in revisions:
    grouped = {}
    for run in runs:
        if run['revision'] != label:
            continue
        for row in run['results']:
            key = (row['name'], row['n'])
            grouped.setdefault(key, []).append(row)
    rows = []
    for (name, n), samples in grouped.items():
        hashes = {row['hash'] for row in samples}
        if len(hashes) != 1 or len({row['bytes'] for row in samples}) != 1:
            raise ValueError(f'Unstable fixture or output: {label}, {name}, {n}')
        times = [value for row in samples for value in row['samples_ms']]
        rows.append(dict(name=name, n=n, bytes=samples[0]['bytes'],
                         median_ms=statistics.median(times), min_ms=min(times),
                         sample_count=len(times), hash=samples[0]['hash']))
    summary[label] = rows
result = dict(engine=args.engine, commits=commits, rounds=3,
              samples_per_round=7, initial_load=initial_load,
              final_load=Path('/proc/loadavg').read_text().split()[:3],
              summaries=summary, runs=runs)
Path(args.output).write_text(json.dumps(result, indent=2) + '\n')
