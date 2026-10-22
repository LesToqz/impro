#!/usr/bin/env python3
"""Run isolated, paired full-window parity scenes without building anything."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import time
from compare import compare

SCENES = [
    'default', 'explorer-expanded', 'explorer-selected', 'explorer-hover',
    'explorer-menu', 'explorer-rename', 'search-results', 'search-empty', 'explorer-notes',
    'editor-python', 'editor-json', 'editor-rust', 'editor-toml', 'editor-markdown',
    'editor-selection-find', 'editor-unsaved', 'tab-hover', 'tab-close',
    'terminal-bottom', 'terminal-right', 'terminal-split', 'terminal-fullscreen',
    'terminal-maximized', 'terminal-hover', 'palette', 'palette-query', 'switcher', 'notice',
    *['settings-'+p for p in ['appearance','editor','files','terminal','graph','gitgraph','notes','hotkeys','about']],
    'git-horizontal', 'git-vertical', 'commit-horizontal', 'commit-vertical', 'graph',
    'notes-live', 'notes-table', 'notes-selection',
]


def fixture(root):
    files = {
        'main.py': '# Example\nimport json\n\ndef greet(name: str) -> str:\n    return f"Hello {name}"\n\nprint(greet("world"))\n',
        'README.md': '# Example\n\nA small **project** with a [link](main.py).\n',
        'data.json': '{\n  "name": "example",\n  "enabled": true,\n  "count": 42\n}\n',
        'src/main.rs': '// Example\nfn main() {\n    let value = 42;\n    println!("{value}");\n}\n',
        'settings.toml': '# Example\n[project]\nname = "example"\nenabled = true\ncount = 42\n',
        '.impro/Notes/Welcome.md': '# Welcome\n\nRead [[Second]].\n',
        '.impro/Notes/Second.md': '# Second\n\nBack to [[Welcome]].\n',
    }
    for name, content in files.items():
        path = root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(content)
    # Keep displayed age stable across both runs.
    for path in root.rglob('*'):
        os.utime(path, (1700000000, 1700000000))


def repository(root):
    env = dict(os.environ, GIT_CONFIG_NOSYSTEM='1', GIT_CONFIG_GLOBAL='/dev/null',
               GIT_AUTHOR_NAME='Parity QA', GIT_AUTHOR_EMAIL='qa@example.test',
               GIT_COMMITTER_NAME='Parity QA', GIT_COMMITTER_EMAIL='qa@example.test',
               GIT_AUTHOR_DATE='2020-01-02T03:04:05+00:00', GIT_COMMITTER_DATE='2020-01-02T03:04:05+00:00')
    def git(*args):
        return subprocess.check_output(['git','-C',str(root),*args], env=env, stderr=subprocess.PIPE)
    if not (root/'.git').exists():
        git('init','-q','-b','work')
        git('add','.')
        git('commit','-qm','Initial files')
        (root/'main.py').write_text((root/'main.py').read_text()+'\nVALUE = 2\n')
        git('add','main.py')
        git('commit','-qm','Prepare release')
    return git('rev-parse','HEAD').decode().strip()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--reference', default=os.getenv('IMPRO_REFERENCE_DIR', os.getenv('IMPRO_PARITY_REFERENCE')))
    parser.add_argument('--impro-example', '--impro-bin', default='target/release/examples/window_qa')
    parser.add_argument('--out', type=Path, default=Path('.ceo/parity'))
    parser.add_argument('--theme', choices=['dark','light','both'], default='both')
    parser.add_argument('--scale', choices=['1','2'], default='1')
    parser.add_argument('--scene', action='append', choices=SCENES)
    parser.add_argument('--smoke', action='store_true')
    parser.add_argument('--list', action='store_true')
    parser.add_argument('--strict', action='store_true', help='fail on any pixel difference')
    args = parser.parse_args()
    if args.list:
        print('\n'.join(SCENES)); return
    if not args.reference:
        parser.error('set IMPRO_REFERENCE_DIR or pass --reference DIR')
    binary = Path(args.impro_example).resolve()
    if not binary.is_file():
        parser.error('QA example missing; build release examples first')
    out = args.out.resolve()
    out.mkdir(parents=True, exist_ok=True)
    root = out/'project'
    root.mkdir(exist_ok=True)
    fixture(root)
    head = repository(root)
    # A tiny noninteractive shell startup, outside the fixture project.
    shell = out/'shell'
    shell.write_text("#!/bin/sh\nexec /bin/sh -i\n")
    shell.chmod(0o755)
    config = out/'config'
    values = dict(reduce_motion=True, terminal_blink=False, terminal_shell=str(shell),
                  ui_font='DejaVu Sans', editor_font='DejaVu Sans Mono', md_live_preview=False,
                  gitgraph_time_format='absolute')
    (config/'impro').mkdir(parents=True, exist_ok=True)
    (config/'impro/settings.json').write_text(json.dumps(values))
    env = dict(os.environ, XDG_CONFIG_HOME=str(config), GDK_SCALE=args.scale,
               GDK_DPI_SCALE='1', IMPRO_REDUCE_MOTION='1', SHELL=str(shell), ENV='/dev/null',
               PS1='$ ', LC_ALL='C.UTF-8', TZ='UTC', NO_AT_BRIDGE='1',
               IMPRO_PARITY_SETTINGS=json.dumps(values))
    driver = Path(__file__).with_name('reference.py')
    rows = []
    started = time.monotonic()
    for scene in (['default'] if args.smoke else args.scene or SCENES):
        for theme in (['dark','light'] if args.theme == 'both' else [args.theme]):
            stem = f'{scene}-{theme}-{args.scale}x'
            row = dict(scene=scene, theme=theme, scale=int(args.scale))
            for side in ['impro', 'reference']:
                image = out/(stem+'-'+side+'.png')
                command = ([str(binary),'scene',scene,str(root),str(image),'impro-'+theme] if side == 'impro' else
                    ['/usr/bin/python3',str(driver),'--reference',str(Path(args.reference).resolve()),
                     '--scene',scene,'--root',str(root),'--output',str(image),'--theme',theme])
                process = subprocess.run(command, env=env, capture_output=True, timeout=45)
                (out/(stem+'-'+side+'.log')).write_bytes(process.stdout+process.stderr)
                if process.returncode == 77:
                    row.update(status='unsupported', side=side)
                    break
                if process.returncode:
                    row.update(status='error', side=side, exit=process.returncode)
                    break
                row[side+'_sha256'] = hashlib.sha256(image.read_bytes()).hexdigest()
            else:
                try:
                    row.update(compare(out/(stem+'-reference.png'),out/(stem+'-impro.png'),out/stem))
                    row['status'] = 'compared'
                except ValueError as error:
                    row.update(status='error', detail=str(error))
            rows.append(row)
            print(json.dumps(row, sort_keys=True), flush=True)
            (out/'summary.json').write_text(json.dumps(dict(head=head,rows=rows),indent=2)+'\n')
    print(f'{len(rows)} cases in {time.monotonic()-started:.1f}s', flush=True)
    if any(r['status']=='error' for r in rows) or (args.strict and any(r.get('changed',0) for r in rows)):
        return 1
    return 0


if __name__ == '__main__':
    sys.exit(main())
