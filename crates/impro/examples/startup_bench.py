#!/usr/bin/env python3
"""Interleave fresh release processes; run under xvfb-run and dbus-run-session.

Save baseline binaries before building changes. --before must contain impro and
startup_floor; --after contains impro and examples/startup_floor. One preliminary
empty-window process starts session services; its timing is retained separately.
PNG encoding is outside the application's first-paint timestamp.
"""
import argparse
import json
import os
from pathlib import Path
import re
import statistics
import subprocess

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--before', type=Path, required=True)
parser.add_argument('--after', type=Path, default=Path('target/release'))
parser.add_argument('--root', type=Path, default=Path('/tmp'))
parser.add_argument('--runs', type=int, default=10)
parser.add_argument('--output', type=Path, required=True)
args = parser.parse_args()
if args.runs < 10:
    parser.error('at least ten runs are required')
args.output.parent.mkdir(parents=True, exist_ok=True)
artifacts = args.output.parent / (args.output.stem + '-artifacts')
artifacts.mkdir(exist_ok=True)
env = dict(os.environ, GDK_BACKEND='x11', IMPRO_QA_TIMING='1', IMPRO_QA_TRACE='1',
           IMPRO_QA_SCREENSHOT=str((artifacts / 'paint.png').resolve()))
variants = {
    'empty': ([str(args.before / 'startup_floor')],
              [str(args.after / 'examples/startup_floor')]),
    'chrome': ([str(args.before / 'startup_floor'), 'chrome'],
               [str(args.after / 'examples/startup_floor'), 'chrome']),
    'workspace': ([str(args.before / 'impro'), str(args.root.resolve())],
                  [str(args.after / 'impro'), str(args.root.resolve())]),
}

def measure(command, label):
    process = subprocess.run(command, env=env, capture_output=True, text=True,
                             timeout=30, check=True)
    log = process.stdout + process.stderr
    (artifacts / (label + '.log')).write_text(log)
    values = {key: float(value) for key, value in
              re.findall(r'([\w-]+)-ms=([\d.]+)', log)}
    if 'first-paint' not in values:
        raise RuntimeError('missing complete-paint timestamp: ' + label)
    return values

cold_session = measure(variants['empty'][0], 'cold-session-empty')
rows = []
for run in range(args.runs):
    for variant, commands in variants.items():
        for version in ([0, 1] if run % 2 == 0 else [1, 0]):
            label = f'{variant}-{version}-{run}'
            rows.append(dict(variant=variant, version=version, run=run,
                             ms=measure(commands[version], label)))
summary = {}
for variant in variants:
    summary[variant] = {}
    for version, name in enumerate(['before', 'after']):
        values = [r['ms']['first-paint'] for r in rows
                  if r['variant'] == variant and r['version'] == version]
        summary[variant][name] = dict(median=statistics.median(values),
                                      minimum=min(values), maximum=max(values))
result = dict(root=str(args.root.resolve()), runs=args.runs,
              cold_session_empty=cold_session, summary=summary, samples=rows)
args.output.write_text(json.dumps(result, indent=2) + '\n')
print(json.dumps(summary, indent=2))
