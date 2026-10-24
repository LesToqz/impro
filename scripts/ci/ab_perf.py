#!/usr/bin/env python3
"""A/B comparison of two release builds of impro.

    xvfb-run -a dbus-run-session -- scripts/ci/ab_perf.py \
        --before DIR --after DIR --runs 20 --output ab.json [--summary ab.md]

Both directories use the CI layout: `impro` and `examples/{startup_floor,
window_qa,graph_bench}`. Startup timings come from
crates/impro/examples/startup_bench.py (interleaved fresh processes for the
empty window, the chrome and the workspace); the frame metrics reuse the
perf_gate.py measurements, alternating before and after for several rounds.

Only relative numbers are meaningful, so both builds must be produced with
the same compiler settings. Gated metrics: workspace first paint and the
3,000-node graph draw. Everything else is reported for context.

Exit status: 0 no gated regression, 1 a gated metric is more than
--threshold slower, 2 harness error (for example a base that cannot run).
"""
import argparse
import json
import os
from pathlib import Path
import shutil
import statistics
import subprocess
import sys
import tempfile

sys.path.insert(0, str(Path(__file__).resolve().parent))
import perf_gate  # noqa: E402  (same directory)

BENCH = Path(__file__).resolve().parents[2] / 'crates/impro/examples/startup_bench.py'
STARTUP_TIMEOUT_S = 1800


def stage_before(directory, stage):
    """startup_bench wants `startup_floor` beside `impro` in the before directory."""
    stage.mkdir()
    for name, source in (('impro', directory / 'impro'),
                         ('startup_floor', directory / 'examples' / 'startup_floor')):
        if not source.is_file():
            raise perf_gate.HarnessError(f'missing binary: {source}')
        (stage / name).symlink_to(source)
    return stage


def startup(before, after, runs, work):
    if not BENCH.is_file():
        raise perf_gate.HarnessError(f'missing {BENCH}')
    root = work / 'workspace-root'
    root.mkdir()
    (root / 'hello.txt').write_text('hello\n')
    output = work / 'startup' / 'startup.json'
    command = [sys.executable, str(BENCH), '--before', str(stage_before(before, work / 'before')),
               '--after', str(after), '--root', str(root), '--runs', str(runs),
               '--output', str(output)]
    try:
        process = subprocess.run(command, capture_output=True, text=True,
                                 timeout=STARTUP_TIMEOUT_S, check=False)
    except subprocess.TimeoutExpired as error:
        raise perf_gate.HarnessError('startup_bench timed out') from error
    if process.returncode != 0 or not output.is_file():
        raise perf_gate.HarnessError(
            f'startup_bench failed ({process.returncode})\n{(process.stdout + process.stderr)[-1500:]}')
    return json.loads(output.read_text())


def frames(before, after, rounds, work):
    gates = []
    for name, directory in (('before', before), ('after', after)):
        sub = work / f'frames-{name}'
        sub.mkdir()
        gates.append(perf_gate.Gate(directory, sub))
    samples = [{}, {}]
    for round_ in range(rounds):
        for side in ([0, 1] if round_ % 2 == 0 else [1, 0]):
            values, _ = gates[side].frames()
            for key, value in values.items():
                samples[side].setdefault(key, []).append(value)
            print(f'frames round {round_} {"before" if side == 0 else "after"}: ' +
                  ', '.join(f'{k}={v:.2f}' for k, v in values.items()), flush=True)
    return {key: (statistics.median(samples[0][key]), statistics.median(samples[1][key]))
            for key in samples[0]}


def main():
    parser = argparse.ArgumentParser(description=__doc__,
                                     formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('--before', type=Path, required=True)
    parser.add_argument('--after', type=Path, required=True)
    parser.add_argument('--runs', type=int, default=20, help='startup runs per variant (>= 10)')
    parser.add_argument('--frame-rounds', type=int, default=5,
                        help='alternating rounds of the frame benchmarks')
    parser.add_argument('--threshold', type=float, default=0.20,
                        help='allowed slowdown of a gated metric (0.20 = 20%%)')
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--summary', type=Path)
    args = parser.parse_args()
    if args.runs < 10:
        parser.error('--runs must be at least 10')

    for directory in (args.before, args.after):
        if not directory.is_dir():
            print(f'harness error: directory not found: {directory}', file=sys.stderr)
            return 2
    before, after = args.before.resolve(), args.after.resolve()
    work = Path(tempfile.mkdtemp(prefix='impro-ab-'))
    home = work / 'home'
    home.mkdir()
    os.environ.update(HOME=str(home), GDK_BACKEND='x11', GIO_USE_VFS='local')
    os.environ.pop('WAYLAND_DISPLAY', None)
    try:
        if not os.environ.get('DISPLAY'):
            raise perf_gate.HarnessError('DISPLAY is not set: run under xvfb-run -a dbus-run-session --')
        # Constructing the gates checks every binary on both sides first.
        startup_result = startup(before, after, args.runs, work)
        frame_result = frames(before, after, args.frame_rounds, work)
    except perf_gate.HarnessError as error:
        print(f'harness error: {error}', file=sys.stderr)
        return 2
    finally:
        shutil.rmtree(work, ignore_errors=True)

    rows = []
    summary = startup_result['summary']
    for variant, gated in (('workspace', True), ('chrome', False), ('empty', False)):
        rows.append(('startup_' + variant + '_ms', summary[variant]['before']['median'],
                     summary[variant]['after']['median'], gated))
    labels = {'graph_draw_p50_ms': True, 'graph_layout_tick_ms': False, 'key_to_paint_ms': False}
    for key, gated in labels.items():
        rows.append((key, *frame_result[key], gated))

    metrics, regressed = [], []
    for name, old, new, gated in rows:
        ratio = new / old if old else float('inf')
        status = 'info'
        if gated:
            status = 'REGRESSION' if ratio > 1 + args.threshold else 'ok'
            if status == 'REGRESSION':
                regressed.append(name)
        metrics.append(dict(name=name, before=old, after=new, ratio=ratio, gated=gated, status=status))

    result = dict(threshold=args.threshold, runs=args.runs, frame_rounds=args.frame_rounds,
                  metrics=metrics, startup=startup_result)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(result, indent=2) + '\n')

    lines = ['## Performance A/B (median, interleaved runs)', '',
             '| metric | before | after | after/before | status |',
             '| --- | ---: | ---: | ---: | :---: |']
    for m in metrics:
        lines.append(f'| `{m["name"]}`{"" if m["gated"] else " (info)"} | {m["before"]:.2f} | '
                     f'{m["after"]:.2f} | {m["ratio"]:.2f} | {m["status"]} |')
    lines += ['', f'Gate: a gated metric more than {args.threshold:.0%} slower fails the run. '
              'Both builds use identical compiler settings, so only ratios are meaningful.']
    text = '\n'.join(lines) + '\n'
    print(text)
    if args.summary:
        with args.summary.open('a') as handle:
            handle.write(text)
    if regressed:
        print(f'FAIL regression in: {", ".join(regressed)}', file=sys.stderr)
        return 1
    print('PASS no gated regression')
    return 0


if __name__ == '__main__':
    sys.exit(main())
