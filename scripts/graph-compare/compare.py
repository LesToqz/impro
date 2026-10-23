#!/usr/bin/env python3
"""Paired graph audit; differences are evidence, never a failing exit status."""
import argparse
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys

HERE = Path(__file__).resolve().parent
REPO = HERE.parent.parent
spec = importlib.util.spec_from_file_location('scenarios', HERE/'scenarios.py')
scenarios = importlib.util.module_from_spec(spec)
spec.loader.exec_module(scenarios)


def drive(command, out, env):
    out.mkdir(parents=True, exist_ok=True)
    with (out/'driver.log').open('w') as log:
        process = subprocess.run(command, stdout=log, stderr=subprocess.STDOUT,
                                 env=env, cwd=REPO, timeout=1200, check=False)
    if process.returncode:
        tail = '\n'.join((out/'driver.log').read_text().splitlines()[-25:])
        raise RuntimeError(f'driver exited {process.returncode}: {out}/driver.log\n{tail}')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--reference', type=Path, default=os.getenv('IMPRO_REFERENCE_DIR'))
    parser.add_argument('--out', type=Path, default=REPO/'.ceo/graph-compare')
    parser.add_argument('--impro-bin', type=Path, default=REPO/'target/release/examples/graph_probe')
    parser.add_argument('--skip-repeat', action='store_true', help='omit the fresh-start repeat probes')
    parser.add_argument('--reuse', action='store_true', help='compare existing measurements only')
    args = parser.parse_args()
    out = args.out.resolve()
    out.mkdir(parents=True, exist_ok=True)
    if not args.reuse:
        if args.reference is None or not args.reference.is_dir():
            raise RuntimeError('set IMPRO_REFERENCE_DIR to the read-only reference tree')
        if not args.impro_bin.is_file():
            raise RuntimeError('build cargo build --release -p impro-graph --examples first')
        if out.is_relative_to(args.reference.resolve()):
            raise RuntimeError('output must not be inside the read-only reference tree')
        action_file = out/'actions.json'
        action_file.write_text(json.dumps(scenarios.actions(), indent=2))
        env = dict(os.environ, PYTHONDONTWRITEBYTECODE='1', GDK_BACKEND='x11',
                   XDG_CONFIG_HOME=str(out/'config'), XDG_CACHE_HOME=str(out/'cache'),
                   GIO_USE_VFS='local')
        env.pop('WAYLAND_DISPLAY', None)
        env.pop('IMPRO_REDUCE_MOTION', None)
        root = out/'fixture'
        cap=root.with_name('cap-fixture')
        cap.mkdir(exist_ok=True)
        for i in range(3005):
            (cap/f'file-{i:04}.md').write_text('A file without links\n')
        scenarios.fixture(root)
        drive(['/usr/bin/python3','-I','-B',str(HERE/'reference.py'),
               '--reference',str(args.reference.resolve()),'--root',str(root),
               '--actions',str(action_file),'--out',str(out/'reference')],out/'reference',env)
        scenarios.fixture(root)
        drive([str(args.impro_bin.resolve()),str(root),str(action_file),str(out/'impro')],out/'impro',env)
        if not args.skip_repeat:
            repeat=out/'repeat-actions.json'
            repeat.write_text(json.dumps([dict(op='capture',name='default',png=True)]))
            scenarios.fixture(root)
            drive(['/usr/bin/python3','-I','-B',str(HERE/'reference.py'),
                   '--reference',str(args.reference.resolve()),'--root',str(root),
                   '--actions',str(repeat),'--out',str(out/'reference-repeat')],out/'reference-repeat',env)
            scenarios.fixture(root)
            drive([str(args.impro_bin.resolve()),str(root),str(repeat),str(out/'impro-repeat')],out/'impro-repeat',env)
    reference = json.loads((out/'reference/measurements.json').read_text())
    impro = json.loads((out/'impro/measurements.json').read_text())
    # Detailed measurement rows are provided by the inventory module.
    module_spec = importlib.util.spec_from_file_location('inventory', HERE/'inventory.py')
    inventory = importlib.util.module_from_spec(module_spec)
    module_spec.loader.exec_module(inventory)
    inventory.compare(reference, impro, out)


if __name__ == '__main__':
    try:
        main()
    except (RuntimeError, OSError, ValueError, subprocess.TimeoutExpired) as error:
        print('graph audit error:', error, file=sys.stderr)
        sys.exit(1)
