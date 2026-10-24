#!/usr/bin/env python3
"""Performance budget gate for release builds of impro.

    xvfb-run -a dbus-run-session -- scripts/ci/perf_gate.py \
        --bin-dir DIR --budgets scripts/ci/perf-budgets.json --output perf.json \
        [--summary summary.md] [--budget-scale F]

DIR holds `impro` and `examples/{startup_floor,window_qa,graph_bench}` (the
layout of the CI `release-bin` artifact; execute bits are restored here if
missing). The caller provides the display and the session bus. The gate only
catches gross regressions: shared runners vary 1.5-3x, so budgets are loose and
the startup check also uses the ratio to an empty GTK window, which does not
depend on machine speed.

Metric groups: startup (impro and empty-window first paint), frames
(keyboard-to-paint and the graph draw benchmark), idle (CPU ticks over ten
seconds) and size (release binary). A group whose metric breaches its `fail`
budget is measured once more; only a second breach fails the gate.

Exit status: 0 pass (warnings allowed), 1 budget failure, 2 harness error.
"""
import argparse
import json
import os
from pathlib import Path
import re
import shutil
import signal
import statistics
import subprocess
import sys
import tempfile
import time

STARTUP_RUNS = 10
IDLE_SETTLE_S = 3
IDLE_WINDOW_S = 10
RUN_TIMEOUT_S = 60


class HarnessError(Exception):
    """The measurement itself broke (missing binary, unparseable output)."""


def run_logged(command, env, label, timeout=RUN_TIMEOUT_S):
    try:
        process = subprocess.run(command, env=env, capture_output=True, text=True,
                                 timeout=timeout, check=False)
    except subprocess.TimeoutExpired as error:
        raise HarnessError(f'{label}: timed out after {timeout}s') from error
    except OSError as error:
        raise HarnessError(f'{label}: cannot start {command[0]}: {error}') from error
    if process.returncode != 0:
        tail = (process.stdout + process.stderr)[-1500:]
        raise HarnessError(f'{label}: exit status {process.returncode}\n{tail}')
    return process.stdout, process.stderr


def percentile(values, fraction):
    ordered = sorted(values)
    return ordered[min(len(ordered) - 1, int(round((len(ordered) - 1) * fraction)))]


class Gate:
    def __init__(self, bin_dir, work):
        self.impro = bin_dir / 'impro'
        self.floor = bin_dir / 'examples' / 'startup_floor'
        self.window_qa = bin_dir / 'examples' / 'window_qa'
        self.graph_bench = bin_dir / 'examples' / 'graph_bench'
        for path in (self.impro, self.floor, self.window_qa, self.graph_bench):
            if not path.is_file():
                raise HarnessError(f'missing binary: {path}')
            if not os.access(path, os.X_OK):
                path.chmod(path.stat().st_mode | 0o111)
        self.work = work
        self.folder = work / 'project'
        self.folder.mkdir()
        (self.folder / 'hello.txt').write_text('hello\n')
        home = work / 'home'
        home.mkdir()
        self.env = dict(os.environ, HOME=str(home), GDK_BACKEND='x11',
                        GIO_USE_VFS='local')
        self.env.pop('WAYLAND_DISPLAY', None)
        if not self.env.get('DISPLAY'):
            raise HarnessError('DISPLAY is not set: run under xvfb-run -a dbus-run-session --')

    # ------------------------------------------------------------ startup
    def first_paint(self, command, label):
        png = self.work / 'paint.png'
        png.unlink(missing_ok=True)
        env = dict(self.env, IMPRO_QA_SCREENSHOT=str(png), IMPRO_QA_TIMING='1')
        out, err = run_logged(command, env, label)
        found = re.findall(r'^first-paint-ms=([\d.]+)\s*$', out + err, re.M)
        if not found:
            raise HarnessError(f'{label}: no first-paint-ms in output\n{(out + err)[-800:]}')
        if not png.is_file() or png.stat().st_size == 0:
            raise HarnessError(f'{label}: no screenshot written')
        return float(found[0])

    def startup(self):
        app = [str(self.impro), str(self.folder)]
        floor = [str(self.floor)]
        # Warm-up: the first process of a session starts portals and bus services.
        self.first_paint(app, 'startup warm-up')
        self.first_paint(floor, 'startup_floor warm-up')
        app_ms, floor_ms = [], []
        for run in range(STARTUP_RUNS):
            order = [('app', app), ('floor', floor)]
            if run % 2:
                order.reverse()
            for kind, command in order:
                value = self.first_paint(command, f'startup {kind} run {run}')
                (app_ms if kind == 'app' else floor_ms).append(value)
        median, floor_median = statistics.median(app_ms), statistics.median(floor_ms)
        return {
            'startup_median_ms': median,
            'startup_p90_ms': percentile(app_ms, 0.9),
            'startup_floor_median_ms': floor_median,
            'startup_ratio': median / floor_median,
        }, {'startup_samples_ms': app_ms, 'startup_floor_samples_ms': floor_ms}

    # ------------------------------------------------------------- frames
    def frames(self):
        out, _ = run_logged([str(self.window_qa), 'performance'], self.env,
                            'window_qa performance', timeout=120)
        result = None
        for line in reversed(out.splitlines()):
            if line.startswith('{'):
                try:
                    result = json.loads(line)
                except ValueError as error:
                    raise HarnessError(f'window_qa: unparseable JSON: {line[:200]}') from error
                break
        if result is None:
            raise HarnessError(f'window_qa: no JSON result line\n{out[-800:]}')
        try:
            key_ms = [float(result[k]) for k in
                      ('open_to_paint_ms', 'switch_to_paint_ms', 'toggle_to_paint_ms')]
        except (KeyError, TypeError, ValueError) as error:
            raise HarnessError(f'window_qa: JSON lacks the paint timings: {result}') from error

        out, _ = run_logged([str(self.graph_bench)], self.env, 'graph_bench', timeout=180)
        block = out.split('[3000 nodes', 1)
        if len(block) != 2:
            raise HarnessError(f'graph_bench: no 3000-node section\n{out[-800:]}')
        text = block[1]
        draw = re.search(r'^\s+fit\s+hover=false\s+draw p50 ([\d.]+) ms', text, re.M)
        tick = re.search(r'^\s+layout tick p50 ([\d.]+) ms', text, re.M)
        if not draw or not tick:
            raise HarnessError(f'graph_bench: cannot find draw/tick lines\n{text[:800]}')
        return {
            'key_to_paint_ms': statistics.median(key_ms),
            'graph_draw_p50_ms': float(draw.group(1)),
            'graph_layout_tick_ms': float(tick.group(1)),
        }, {'key_to_paint_samples_ms': key_ms, 'window_qa_idle_ticks': result.get('idle_ticks')}

    # --------------------------------------------------------------- idle
    @staticmethod
    def cpu_ticks(pid):
        try:
            stat = Path(f'/proc/{pid}/stat').read_text()
        except OSError as error:
            raise HarnessError(f'idle: process {pid} is gone: {error}') from error
        fields = stat[stat.rindex(')') + 2:].split()
        return int(fields[11]) + int(fields[12])  # utime + stime

    def idle(self):
        env = {k: v for k, v in self.env.items() if not k.startswith('IMPRO_QA_')}
        process = subprocess.Popen([str(self.impro), str(self.folder)], env=env,
                                   stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                                   start_new_session=True)
        try:
            time.sleep(IDLE_SETTLE_S)
            if process.poll() is not None:
                raise HarnessError(f'idle: impro exited early with {process.returncode}')
            start = self.cpu_ticks(process.pid)
            time.sleep(IDLE_WINDOW_S)
            ticks = self.cpu_ticks(process.pid) - start
        finally:
            try:
                os.killpg(process.pid, signal.SIGTERM)
            except ProcessLookupError:
                pass
            try:
                process.wait(timeout=10)
            except subprocess.TimeoutExpired:
                os.killpg(process.pid, signal.SIGKILL)
                process.wait()
        return {'idle_cpu_ticks': float(ticks)}, {}

    # --------------------------------------------------------------- size
    def size(self):
        return {'binary_size_mib': self.impro.stat().st_size / 1048576}, {}


GROUPS = {
    'startup': ('startup_median_ms', 'startup_p90_ms', 'startup_floor_median_ms', 'startup_ratio'),
    'frames': ('key_to_paint_ms', 'graph_draw_p50_ms', 'graph_layout_tick_ms'),
    'idle': ('idle_cpu_ticks',),
    'size': ('binary_size_mib',),
}


def classify(value, budget, scale):
    if budget is None:
        return 'info'
    if value > budget['fail'] * scale:
        return 'FAIL'
    if value > budget['warn'] * scale:
        return 'warn'
    return 'ok'


def fmt(value):
    if value is None:
        return '-'
    return f'{value:.2f}' if abs(value) < 100 else f'{value:.0f}'


def main():
    parser = argparse.ArgumentParser(description=__doc__,
                                     formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('--bin-dir', type=Path, required=True)
    parser.add_argument('--budgets', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--summary', type=Path)
    parser.add_argument('--budget-scale', type=float, default=1.0,
                        help='multiply every warn/fail threshold (0.2 forces a red gate)')
    args = parser.parse_args()
    if args.budget_scale <= 0:
        parser.error('--budget-scale must be positive')

    try:
        spec = json.loads(args.budgets.read_text())
        budgets = {m['name']: m for m in spec['metrics']}
    except (OSError, ValueError, KeyError) as error:
        print(f'harness error: cannot read budgets {args.budgets}: {error}', file=sys.stderr)
        return 2
    known = {name for names in GROUPS.values() for name in names}
    unknown = sorted(set(budgets) - known)
    if unknown:
        print(f'harness error: budgets name unknown metrics: {unknown}', file=sys.stderr)
        return 2
    if not args.bin_dir.is_dir():
        print(f'harness error: binary directory not found: {args.bin_dir}', file=sys.stderr)
        return 2

    work = Path(tempfile.mkdtemp(prefix='impro-perf-'))
    started = time.monotonic()
    attempts = {}
    values, details = {}, {}
    try:
        gate = Gate(args.bin_dir.resolve(), work)
        for group, names in GROUPS.items():
            measure = getattr(gate, group)
            attempts[group] = []
            for attempt in (1, 2):
                result, extra = measure()
                breached = [n for n in names
                            if classify(result[n], budgets.get(n), args.budget_scale) == 'FAIL']
                attempts[group].append({'values': result, 'breached': breached})
                print(f'{group} attempt {attempt}: ' +
                      ', '.join(f'{n}={fmt(v)}' for n, v in result.items()), flush=True)
                if not breached:
                    break
                if attempt == 1:
                    print(f'{group}: budget breached by {breached}; measuring once more', flush=True)
            values.update(result)
            details.update(extra)
    except HarnessError as error:
        print(f'harness error: {error}', file=sys.stderr)
        return 2
    finally:
        shutil.rmtree(work, ignore_errors=True)

    rows, failed = [], []
    for names in GROUPS.values():
        for name in names:
            budget = budgets.get(name)
            status = classify(values[name], budget, args.budget_scale)
            if status == 'FAIL':
                failed.append(name)
            rows.append({
                'name': name, 'value': values[name],
                'unit': budget['unit'] if budget else None,
                'baseline': budget['baseline'] if budget else None,
                'warn': budget['warn'] * args.budget_scale if budget else None,
                'fail': budget['fail'] * args.budget_scale if budget else None,
                'status': status,
            })
    elapsed = time.monotonic() - started
    result = {'provisional': spec.get('provisional', False), 'budget_scale': args.budget_scale,
              'elapsed_s': round(elapsed, 1), 'metrics': rows, 'attempts': attempts,
              'details': details}
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(result, indent=2) + '\n')

    lines = ['## Performance budget', '',
             '| metric | value | baseline | warn | fail | status |',
             '| --- | ---: | ---: | ---: | ---: | :---: |']
    for row in rows:
        unit = f' {row["unit"]}' if row['unit'] else ''
        lines.append(f'| `{row["name"]}` | {fmt(row["value"])}{unit} | {fmt(row["baseline"])} | '
                     f'{fmt(row["warn"])} | {fmt(row["fail"])} | {row["status"]} |')
    lines.append('')
    if spec.get('provisional'):
        lines.append('Budgets are provisional: recalibrate them from the first runs on this runner.')
    if args.budget_scale != 1.0:
        lines.append(f'Thresholds scaled by {args.budget_scale}.')
    if any(len(a) > 1 for a in attempts.values()):
        retried = ', '.join(g for g, a in attempts.items() if len(a) > 1)
        lines.append(f'Re-measured after a first breach: {retried}.')
    lines.append(f'Measured in {elapsed:.0f} s.')
    summary = '\n'.join(lines) + '\n'
    print(summary)
    if args.summary:
        with args.summary.open('a') as handle:
            handle.write(summary)

    if failed:
        print(f'FAIL budget breached twice: {", ".join(failed)}', file=sys.stderr)
        return 1
    print('PASS performance budgets')
    return 0


if __name__ == '__main__':
    sys.exit(main())
