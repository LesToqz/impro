"""Shared fixture and action sequence; no implementation source is copied."""
import base64
import os
from pathlib import Path

THEMES = ('dark', 'light', 'midnight', 'ember')
SLIDERS = ('localDepth', 'textFadeMultiplier', 'nodeSizeMultiplier',
           'lineSizeMultiplier', 'centerStrength', 'repelStrength',
           'linkStrength', 'linkDistance')


def fixture(root):
    root.mkdir(parents=True, exist_ok=True)
    files = {'hub/Hub.md': '# Hub\n#topic\n' + ''.join(f'[[spoke-{i}]]\n' for i in range(14)),
             'orphan.md': 'A lone file\n',
             'gallery.md': '[[Missing Note]] ![[pixel.png]] #topic\n[chain](chain/chain-0.md)\n',
             'src/main.py': 'import util\n', 'src/util.py': 'VALUE = 1\n',
             'src/widget.ts': "import './helper';\n", 'src/helper.ts': 'export const x = 1;\n',
             '.impro/Notes/First.md': '[[Second]] #notes\n',
             '.impro/Notes/Second.md': 'A second note\n'}
    files.update({f'hub/spoke-{i}.md': '[[Hub]] #topic\n' for i in range(14)})
    files.update({f'chain/chain-{i}.md': f'[[chain-{i+1}]]\n' if i < 7 else 'end\n' for i in range(8)})
    files.update({f'cluster/cluster-{i}.md': f'[[cluster-{(i+1)%5}]]\n' for i in range(5)})
    # 36 text files and one valid image; stable mtimes, actual birth times remain OS supplied.
    for name, content in files.items():
        target = root/name
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(content)
        os.utime(target, (1700000000, 1700000000))
    (root/'pixel.png').write_bytes(base64.b64decode('iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+jRZkAAAAASUVORK5CYII='))
    for name in ('saved.md', 'renamed.md'):
        (root/name).unlink(missing_ok=True)


def actions():
    a = []
    def add(op, name='', **kw):
        a.append(dict(op=op, name=name, **kw))
    add('capture', 'default', png=True)
    add('type-menu', 'type-menu')
    add('sections', 'sections-toggled')
    add('sections')
    add('prefs', showTags=True, showAttachments=True)
    add('capture', 'expanded')
    for theme in THEMES:
        add('theme', theme=theme)
        add('capture', 'theme-'+theme, png=True)
        add('active', id='hub/Hub.md')
        add('capture', 'active-'+theme, png=True)
        add('active', id=None)
        add('hover', id='hub/Hub.md')
        add('capture', 'hover-'+theme, png=True)
        add('leave')
    add('theme', theme='dark')
    add('active', id='hub/Hub.md')
    add('capture', 'active', png=True)
    add('accent', 'accent', color='#dd6677')
    add('theme', theme='dark')
    add('active', id=None)
    add('prefs', search='file:main OR file:util', hideUnresolved=True, close=True, scale=1)
    add('focus', id='src/main.py')
    for z in (0.1, 0.25, 0.7, 1.0, 2.0):
        add('prefs', scale=z)
        add('capture', f'zoom-{z:g}', png=True)
    add('prefs', scale=1.0)
    add('prefs', close=True)
    add('prefs', search='file:orphan', hideUnresolved=True)
    add('focus', id='orphan.md')
    add('capture', 'font-before', png=True)
    add('font', 'font', family='DejaVu Serif', png=True)
    add('font', family='DejaVu Sans')
    for z in (0.1, 0.25, 0.7, 1.0, 2.0):
        add('prefs', scale=z)
        add('capture', f'label-{z:g}', png=True)
    for detail in (-1, 2):
        add('prefs', scale=0.7, textFadeMultiplier=detail)
        add('capture', f'detail-{detail}', png=True)
    add('prefs', scale=1.0, textFadeMultiplier=0.2, close=False, search='', hideUnresolved=False)
    add('hover', id='hub/Hub.md')
    add('capture', 'hover', png=True)
    add('leave')
    for name, button, ctrl, move in [('click',1,False,0),('ctrl-click',1,True,0),('middle-click',2,False,0),('slop-6',1,False,6),('slop-7',1,False,7)]:
        add('click', name, id='hub/Hub.md', button=button, ctrl=ctrl, move=move, png=True)
    add('menu', 'file-menu', id='hub/Hub.md')
    add('menu-actions', 'menu-actions', id='hub/Hub.md')
    add('click', 'tag-click', id='#topic', button=1)
    add('click', 'missing-click', id='?Missing Note', button=1)
    add('click', 'tag-right', id='#topic', button=3)
    add('click', 'missing-right', id='?Missing Note', button=3)
    add('drag', 'drag', id='hub/Hub.md')
    add('prefs', reduceMotion=False)
    add('pan', 'pan', png=True)
    add('stop', 'stop-on-press')
    for name, delta, ctrl in [('wheel-in',-120,False),('wheel-out',120,False),('ctrl-wheel',-8,True),('wheel-max',-1e6,False),('wheel-min',1e6,False)]:
        add('wheel', name, delta=delta, ctrl=ctrl)
    add('prefs', scale=1, reduceMotion=True)
    add('wheel', 'reduced-wheel', delta=-120, ctrl=False)
    add('prefs', scale=1, reduceMotion=False)
    for key in ('ArrowLeft','ArrowRight','ArrowUp','ArrowDown','+','-'):
        add('key', 'key-'+key, key=key)
    add('key', 'key-shift', key='ArrowLeft', shift=True)
    add('camera-reset', 'camera-reset')
    add('focus', id='hub/Hub.md')
    add('hover', id='hub/Hub.md')
    add('key', 'key-enter', key='Enter')
    add('key', 'key-escape', key='Escape', png=True)
    add('key', 'key-escape-again', key='Escape', png=True)
    add('prefs', close=False, search='')
    add('search', 'search', text='path:chain')
    add('search', text='')
    add('local', 'local', id='hub/Hub.md', depth=2)
    add('prefs', localIncoming=False, localOutgoing=True)
    add('capture', 'local-outgoing')
    add('global')
    add('group-add', 'group-add')
    add('prefs', colorGroups=[dict(query='path:hub', color='#ff5577')])
    add('capture', 'group-colour')
    add('group-remove', 'group-remove')
    add('prefs', search='path:hub', scale=0.7, nodeSizeMultiplier=2, localDepth=4)
    add('reset', 'reset')
    add('prefs', search='', scale=1)
    add('replay', 'replay')
    add('prefs', reduceMotion=True)
    add('replay', 'replay-reduced')
    add('global')
    add('buffer', 'buffer', id='orphan.md', text='[[Buffer Target]]\n')
    add('buffer-clear', id='orphan.md')
    add('save', 'save')
    add('rename', 'rename')
    add('delete', 'delete')
    add('prefs', search='impossible-empty-query-12345', hideUnresolved=True)
    add('capture', 'empty', png=True)
    add('prefs', search='', hideUnresolved=False)
    add('local-empty', 'local-empty')
    add('note', 'note')
    add('global')
    add('prefs', reduceMotion=False)
    add('resize', 'resize')
    add('hidden', 'hidden')
    add('capture', 'shown', png=True)
    add('idle', 'idle')
    add('cap', 'cap')
    add('root-restore')
    add('section', id='display')
    add('persist', 'persist')
    return a
