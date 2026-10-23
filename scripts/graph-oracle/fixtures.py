"""Synthetic project trees. The same tree is handed to the reference and to the engine."""
import os
from pathlib import Path


def put(root, rel, text='', binary=None):
    path = Path(root) / rel
    path.parent.mkdir(parents=True, exist_ok=True)
    if binary is not None:
        path.write_bytes(binary)
    else:
        path.write_text(text, encoding='utf-8')


def links(root):
    put(root, 'index.md', '# Index\n[[alpha]] [[Alpha|the first]] [[alpha#Heading]] [[alpha^block1]] '
        '[[alpha#H^b]] ![[pic.png]] ![[alpha]] [[missing note]] [[ missing spaced ]] [[a|b|c]]\n'
        '[[alpha\\|escaped]] [[]] [[|alias only]] [[#heading only]]\n')
    put(root, 'alpha.md', '# Alpha\nhello world\n[beta](beta.md) [beta2](./beta.md) [enc](sub/with%20space.md) '
        '[anch](beta.md#top) [q](beta.md?x=1) [ang](<sub/with space.md>) [t](beta.md "title") '
        '[self](alpha.md) [[alpha]] <beta.md> <https://example.com> [web](https://x.y/z.md) '
        '[mail](mailto:a@b.c) [frag](#local) [abs](/beta.md) [up](../outside.md) [x](nofile.md)\n')
    put(root, 'beta.md', 'beta text\n[back](alpha.md)\n```\n[[fenced]] [x](fenced.md) #fencedtag\n```\n'
        '~~~md\n[[tilde]]\n~~~\n`[[inline]]` ``[[double `tick`]]`` [[real]]\n<!-- [[commented]] -->\n'
        '    [[indented-is-link]]\n````\n```\n[[still fenced]]\n````\n[[after fence]]\n')
    put(root, 'sub/with space.md', 'spaced\n[up](../alpha.md) [[../alpha]] [[sub/with space]] [[/alpha]] '
        '[[./alpha]] [[nested/deep]] [[Sub/With Space]]\n')
    put(root, 'sub/nested/deep.md', 'deep [rel](../../beta.md) [rel2](../with%20space.md) [r3](./../../alpha.md#x)\n')
    put(root, 'real.md', 'real\n')
    put(root, 'after fence.md', 'af\n')
    put(root, 'fenced.md', 'fenced\n')
    put(root, 'Mixed Case.MD', '[[alpha]]\n')
    put(root, 'long.markdown', '[[Mixed Case]] [[mixed case.md]] [[ALPHA.MD]]\n')
    put(root, 'pic.png', binary=b'\x89PNG\r\n\x1a\n\0\0')
    put(root, 'unref.png', binary=b'\x89PNG\r\n\x1a\n\0\0')
    put(root, 'dup.md', '[[a]] [[a]] [[a]] ![[a]] [a](a.md) [a2](a.md)\n')
    put(root, 'a.md', '')
    put(root, 'cr.md', 'line1\r\n[[alpha]]\r\n[[beta]]\r\n')
    put(root, 'empty.md', '')
    put(root, 'bom.md', '\ufeff---\ntags: [bomtag]\n---\n[[alpha]]\n')


def dups(root):
    put(root, 'x/note.md', '# x/note\n[[same]] [[other]] [[Note]]\n')
    put(root, 'x/same.md', '')
    put(root, 'x/other.md', '')
    put(root, 'y/note.md', '# y/note\n[[same]] [[other]]\n')
    put(root, 'y/same.md', '')
    put(root, 'y/z/same.md', '')
    put(root, 'y/z/note.md', '[[same]] [[note]]\n')
    put(root, 'a/b/c/same.md', '')
    put(root, 'Same.md', '')
    put(root, 'top.md', '[[same]] [[Same]] [[note]] [[other]] [[x/same]] [[y/z/same]] [[z/same]] [[SAME]]\n')
    put(root, 'ext/dup.md', '')
    put(root, 'ext/dup.markdown', '')
    put(root, 'ext/dup.png', binary=b'\0')
    put(root, 'ext/q.md', '[[dup]] [[dup.png]] ![[dup]] [[dup.markdown]]\n')
    put(root, 'u/Ünï.md', '[[ünï]] [[Ünï]] [[ÜNÏ]]\n')
    put(root, 'u/ünï.md', '')
    put(root, 'zz/a1.md', '[[a2]]\n')
    put(root, 'ab/a2.md', '')
    put(root, 'ab/cd/a2.md', '')
    put(root, 'z/a2.md', '')
    put(root, 'a-1/q.md', '[[a2]]\n')
    put(root, 'a_1/a2.md', '')
    put(root, 'B/a2.md', '')


def markup(root):
    """HTML-ish text in notes: only dotted `<name.ext>` autolinks are references."""
    put(root, 'html.md', 'a<br>b <div class="x">t</div> <details><summary>s</summary></details> <b>x</b>\n'
        '<file.md> <sub/page.md> <v1.2> <a.b> <https://example.com> <mailto:a@b.c> <>\n')
    put(root, 'file.md', '')
    put(root, 'sub/page.md', '')
    put(root, 'v1.2', 'x\n')


def aliases(root):
    put(root, 'real.md', '# real\n')
    put(root, 'alias-holder.md', '---\naliases: [real, Nick, "Quoted Name", \'Single\']\n---\n')
    put(root, 'alias-one.md', '---\nalias: Solo\n---\n')
    put(root, 'alias-list.md', '---\naliases:\n  - Item One\n  - Item Two\n---\n')
    put(root, 'alias-both.md', '---\nalias: Ignored\naliases: [Used]\n---\n')
    put(root, 'alias-empty.md', '---\naliases:\nalias: z\n---\n')
    put(root, 'alias-dup.md', '---\naliases: [shared]\n---\n')
    put(root, 'alias-dup2.md', '---\naliases: [Shared]\n---\n')
    put(root, 'sub/alias-dup3.md', '---\naliases: shared\n---\n')
    put(root, 'alias-case.md', '---\nAliases: [CaseKey]\nTAGS: [CaseTag]\n---\n')
    put(root, 'alias-code.py', 'x = 1\n')
    put(root, 'alias-nested.md', '---\naliases: [a/b, "c#d"]\n---\n')
    put(root, 'source.md', '[[real]] [[nick]] [[quoted name]] [[Single]] [[Solo]] [[item one]] [[Item Two]] '
        '[[Ignored]] [[Used]] [[shared]] [[casekey]] [[a/b]] [[c#d]] [[z]] [[alias-code]] [[nomatch]]\n')
    put(root, 'weird.md', '---\naliases: [a, , b]\ntags: x y, z\n---\n[[a]] [[b]]\n')
    put(root, 'closed.md', '---\naliases: [InClosed]\n...\n[[InClosed]]\n')
    put(root, 'unclosed.md', '---\naliases: [Unclosed]\n[[Unclosed]]\n')
    put(root, 'real-two.md', '---\naliases: [real]\n---\n')


def tags(root):
    put(root, 'a.md', '---\ntags: [x, y/z, "quoted"]\n---\n#inline #a/b #a/b/c text#no #123 #1a #_u #-d #a-b #é\n')
    put(root, 'b.md', '---\ntags:\n  - x\n  - "#hash"\n  - Y/Z\n---\n#X #y/z/w #y\n')
    put(root, 'c.md', '---\ntags: one two, three\n---\n[[a]] #tag-in-link\n[l](a.md#frag) http://a.b/#nolink mailto:#x\n')
    put(root, 'd.md', '# Heading\n## sub\n#tag at start\n(#paren) "#quote" a/#slash a#b /#c [#d] {#e} \\#esc\n`#code` ```\n#fence\n```\n#end')
    put(root, 'e.md', 'no tags 100% #100 # #/ #// #a// #a/ #/a\n')
    put(root, 'code.py', '# comment #pytag\nprint("#str")\n')
    put(root, 'f.md', '---\ntags: []\n---\n#only\n')
    put(root, 'g.md', '---\ntags: x\n---\n#X\n')
    put(root, 'h.md', '#Case #case #CASE #Éa #éa\n')
    put(root, 'i.md', '<!-- #hidden -->\n#shown <!-- #hidden2\n')
    put(root, 'j.md', '---\ntags: [ "a b", c.d, e/f/, /g, "#h#i", -j, _k, 9, 9a ]\n---\n')
    put(root, 'k.md', '#tag1#tag2 #tag3,#tag4 #tag5; #tag6. #tag7! 😀#emoji #日本語 #ñandú\n')
    put(root, 'tag/x.md', 'x\n')


def attachments(root):
    put(root, 'doc.md', '![[img.png]] [[ref.pdf]] [[ref.pdf]] ![[nothere.png]] ![[missing.md]] [[nothere.txt]] '
        '[[noext]] ![[dir/inner.jpg]] [pdf](files/spec.pdf) [none](files/none.pdf) [t](x.TXT) [[up.PNG]] '
        '[[code.py]] [[noimg.svg]] [[fake.md.png]] ![[a.b.c]] ![[.hidden]] [[trail.]]\n')
    put(root, 'img.png', binary=b'\0\1')
    put(root, 'ref.pdf', binary=b'%PDF\0')
    put(root, 'dir/inner.jpg', binary=b'\0')
    put(root, 'files/spec.pdf', binary=b'\0')
    put(root, 'files/orphan.pdf', binary=b'\0')
    put(root, 'x.TXT', 'text file\n')
    put(root, 'up.png', binary=b'\0')
    put(root, 'code.py', 'x = 1\n')
    put(root, 'noimg.svg', '<svg/>')
    put(root, 'fake.md.png', binary=b'\0')
    put(root, 'a.b.c', 'plain\n')
    put(root, '.hidden', 'h\n')
    put(root, 'LICENSE', 'license\n')
    put(root, 'Makefile', 'all:\n')
    put(root, 'other.md', '[[LICENSE]] [[Makefile]] [[license]] [x](LICENSE)\n')
    put(root, 'data.csv', 'a,b\n')
    put(root, 'README', 'readme\n')
    put(root, 'ref2.md', '[[data.csv]] [[README]] [[data]]\n')


def code_py(root):
    put(root, 'main.py', 'import os, sys\nimport pkg\nimport pkg.mod\nimport pkg.mod as m\nfrom pkg import mod, other\n'
        'from pkg.mod import thing\nfrom . import sibling\nfrom .sibling import x\nfrom .. import up\n'
        'from ..up import y\nfrom ... import toofar\nfrom .pkg import z\nimport src_only\nimport util\n'
        'from pkg.sub import *\nimport pkg.sub\nimport a.b.c\n'
        'def f():\n    import lazy\n    from pkg import lazy2\nif True:\n    import cond\n')
    put(root, 'sibling.py', 'import main\n')
    put(root, 'up.py', '')
    put(root, 'pkg/__init__.py', 'from . import mod\nfrom .mod import thing\nfrom .. import main\n')
    put(root, 'pkg/mod.py', 'import pkg\nimport pkg.sub\nfrom . import other\nfrom .other import q\n')
    put(root, 'pkg/other.py', 'import util\nimport mod\n')
    put(root, 'pkg/sub/__init__.py', 'from .. import mod\nfrom ... import main\nfrom ...up import y\n')
    put(root, 'pkg/sub/util.py', 'import util\n')
    put(root, 'src/src_only.py', 'x = 1\n')
    put(root, 'src/util.py', '')
    put(root, 'util.py', '')
    put(root, 'a/b/c.py', 'import a.b.c\nimport b.c\nimport c\n')
    put(root, 'a/b/__init__.py', '')
    put(root, 'a/__init__.py', '')
    put(root, 'iface.pyi', 'import pkg\n')
    put(root, 'tricky.py', '"""docstring\nimport pkg\n"""\n# import util\ns = "import sibling"\nimport up  # trailing\n'
        'try:\n    import sibling\nexcept ImportError:\n    pass\nfrom pkg import (mod,\n    other)\nimport pkg.mod\n')
    put(root, 'note.md', '[[main]] [[main.py]] [[pkg/mod]] [[pkg/mod.py]]\n')
    put(root, 'dir.v2/m.py', 'import dir.v2\nimport m\nfrom . import m\n')


def code_js(root):
    put(root, 'app.js', "import a from './a';\nimport './b.css';\nimport {x} from \"./sub\";\nimport y from 'lib';\n"
        "import z from '/abs';\nconst q = require('./q');\nconst r = require ( \"../r\" );\nexport * from './a.js';\n"
        "export {s} from './s';\nimport('./dyn');\nimport w from '.';\nimport v from '..';\nimport u from './data.json';\n"
        "import t from './comp.vue';\nimport s from './missing';\nimport a2 from './a/';\nimport h from 'http://x/y.js';\n"
        "import i from '#local';\nimport m from './m.mjs';\nimport * as ns from './ns';\nimport def, {n} from './mix';\n")
    put(root, 'a.js', 'module.exports = 1\n')
    put(root, 'a.ts', '')
    put(root, 'a.tsx', '')
    put(root, 'b.css', '')
    put(root, 'sub/index.js', '')
    put(root, 'sub/index.ts', '')
    put(root, 'q.jsx', '')
    put(root, 'r.js', '')
    put(root, 's.ts', '')
    put(root, 'dyn.js', '')
    put(root, 'index.js', 'import "./app"\n')
    put(root, 'data.json', '{}')
    put(root, 'comp.vue', '<script>\nimport c from "./a"\n</script>\n')
    put(root, 'a/index.mjs', '')
    put(root, 'm.mjs', "import 'app.js'\nimport './app.js'\n")
    put(root, 'ns.ts', "import type { T } from './a';\nexport type { U } from './sub'\nimport x = require('./q')\n")
    put(root, 'mix.cjs', "const x = require('./q'), y = require('./r')\n// import './commented'\n/* import './block' */\n")
    put(root, 'tpl.ts', "const s = `import './in-template'`;\nconst t = \"import './in-string'\";\nimport './real-one'\n")
    put(root, 'real-one.js', '')
    put(root, 'mid/deep/x.ts', "import '../../a'\nimport '../b'\nimport './y'\nimport '../../../out'\n")
    put(root, 'mid/b.ts', '')
    put(root, 'mid/deep/y/index.tsx', '')
    put(root, 'svelte.svelte', "import a from './a'\n")
    put(root, 'cts.cts', "import a from './a'\n")
    put(root, 'x.mts', "import a from './a'\n")
    put(root, 'note.md', '[[app]] [[app.js]] [[sub/index]]\n')


def code_style(root):
    put(root, 'main.css', "@import 'base.css';\n@import \"./theme\";\n@import url(extra.css);\n@import url('quoted.css');\n"
        ".a { background: url(img/bg.png); }\n.b { background: url( 'img/bg2.png' ); }\n.c { background: url(\"img/bg3.png\") }\n"
        ".d { background: url(data:image/png;base64,AAA); }\n.e { background: url(http://x/y.png); }\n"
        ".f { background: url(#frag); }\n/* @import 'commented.css'; */\n@import url(//cdn/x.css);\n"
        ".g { background:url(../up.png) }\n@IMPORT 'upper.css';\n")
    put(root, 'base.css', '@import "./reset.css";\n')
    put(root, 'reset.css', '')
    put(root, 'theme', '')
    put(root, 'theme.css', '')
    put(root, 'extra.css', '')
    put(root, 'quoted.css', '')
    put(root, 'upper.css', '')
    put(root, 'img/bg.png', binary=b'\0')
    put(root, 'img/bg2.png', binary=b'\0')
    put(root, 'img/bg3.png', binary=b'\0')
    put(root, 'up.png', binary=b'\0')
    put(root, 'sass/main.scss', "@import 'partial';\n@use \"other\";\n@forward 'fwd';\n@import 'a', 'b';\n.x { b: url(i.png) }\n")
    put(root, 'sass/_partial.scss', '')
    put(root, 'sass/partial.scss', '')
    put(root, 'sass/other.scss', '')
    put(root, 'sass/i.png', binary=b'\0')
    put(root, 'old/main.sass', "@import base\n@import 'x.sass'\n")
    put(root, 'old/x.sass', '')
    put(root, 'page.html', '<link href="main.css" rel=stylesheet><script src=app.js></script>\n'
        '<img src="img/bg.png"><a href="sub/page2.html#x">x</a><a href=\'quoted.html\'>q</a>\n'
        '<a href="https://x">ext</a><a href="mailto:a@b">m</a><a href="#top">t</a><a href="/main.css">root</a>\n'
        '<img SRC="IMG/BG.PNG"><a href = "spaced.html">s</a><a data-href="data.css">d</a><!-- <a href="comment.html"> -->\n'
        '<a href="beta.md">md</a>\n')
    put(root, 'app.js', '')
    put(root, 'sub/page2.html', '<a href="../page.html">up</a><a href="./page2.htm">self</a>\n')
    put(root, 'quoted.html', '')
    put(root, 'spaced.html', '')
    put(root, 'data.css', '')
    put(root, 'comment.html', '')
    put(root, 'beta.md', '')
    put(root, 'x.htm', '<script src="app.js"></script>\n')
    put(root, 'page.vue', '<template><img src="img/bg.png"></template>\n<style>@import "main.css";</style>\n')


def code_other(root):
    put(root, 'src/lib.rs', 'mod a;\nmod b;\npub mod c;\nuse crate::a::thing;\nuse self::b::x;\nuse super::s;\nuse std::io;\n'
        'extern crate foo;\n#[path = "custom.rs"]\nmod d;\n')
    put(root, 'src/a.rs', 'use crate::b;\n')
    put(root, 'src/b/mod.rs', '')
    put(root, 'src/c.rs', '')
    put(root, 'src/custom.rs', '')
    put(root, 'src/d.rs', '')
    put(root, 'run.sh', 'source ./lib.sh\n. ./other.sh\nsource lib2.sh\nbash ./third.sh\n')
    put(root, 'lib.sh', '')
    put(root, 'other.sh', '')
    put(root, 'lib2.sh', '')
    put(root, 'third.sh', '')
    put(root, 'main.c', '#include "util.h"\n#include <stdio.h>\n#include "sub/x.h"\n')
    put(root, 'util.h', '')
    put(root, 'sub/x.h', '#include "../util.h"\n')
    put(root, 'main.cpp', '#include "util.hpp"\n')
    put(root, 'util.hpp', '')
    put(root, 'main.go', 'package main\nimport "fmt"\nimport "./local"\n')
    put(root, 'main.java', 'import java.util.List;\nimport pkg.Thing;\n')
    put(root, 'Thing.java', '')
    put(root, 'app.rb', "require 'x'\nrequire_relative 'helper'\nrequire_relative './other'\n")
    put(root, 'helper.rb', '')
    put(root, 'other.rb', '')
    put(root, 'q.sql', 'select 1;\n')
    put(root, 'c.json', '{"$ref": "./d.json"}')
    put(root, 'd.json', '{}')
    put(root, 'c.yaml', 'include: d.yaml\n')
    put(root, 'd.yaml', '')
    put(root, 'c.toml', 'x = "d.toml"\n')
    put(root, 'conf.xml', '<a href="d.xml"/>')
    put(root, 'm.md', '[[main]] [[run.sh]] [[lib.rs]]\n')


def filler(size, ch='x'):
    """Exactly `size` characters in short lines (the reference's tag scan is quadratic in a single long word)."""
    line = ch * 79 + '\n'
    return (line * (size // 80 + 1))[:size]


def specials(root):
    put(root, 'ok.md', 'ok [[big]] [[binary]] [[latin]] [[nul]] [[huge.py]] [[ünï]] [[sp ace]] [[日本語]] [[emoji😀]]\n')
    put(root, 'big.md', filler(1024 * 1024 + 1))
    put(root, 'edge.md', filler(1024 * 1024 - 20, 'y') + ' [[ok]] ')
    put(root, 'exact.md', filler(1024 * 1024, 'z'))
    put(root, 'huge.py', '# ' + filler(2 * 1024 * 1024) + '\nimport ok\n')
    put(root, 'binary.md', binary=b'bin\0ary [[ok]]')
    put(root, 'latin.md', binary=b'caf\xe9 [[ok]]')
    put(root, 'nul.py', binary=b'import ok\0\n')
    put(root, 'bin.png', binary=b'\xff\xfe\0\0')
    put(root, 'badutf.js', binary=b'import "./ok" \xff\xfe')
    put(root, 'blob.dat', binary=b'\xff\0\xfe')
    put(root, 'ünï.md', 'unicode [[ok]]\n')
    put(root, 'sp ace.md', 'space [[ok]]\n')
    put(root, '日本語.md', '[[ok]] #日本語\n')
    put(root, 'emoji😀.md', '[[ok]]\n')
    put(root, 'empty.md', '')
    put(root, 'empty.py', '')
    put(root, 'empty.txt', '')
    put(root, 'nfd.md', '[[ünï]]\n')
    put(root, 'nfd-é.md', '[[ok]]\n')
    put(root, '.dotfile.md', '[[ok]]\n')
    put(root, '.dotdir/in.md', '[[ok]]\n')
    for d in ('.git', '.hg', '.svn', '__pycache__', 'node_modules', '.venv', 'venv', '.mypy_cache', '.pytest_cache',
              '.tox', '.idea', '.ceo', 'target', 'build', 'dist', '.cache', '.vscode', '.next', '.github', 'vendor'):
        put(root, f'{d}/skipped.md', '[[ok]]\n')
        put(root, f'{d}/sub/deeper.md', '[[ok]]\n')
        put(root, f'keep/{d}/nested.md', '[[ok]]\n')
    put(root, 'Venv/upper.md', '[[ok]]\n')
    put(root, 'ignored-ref.md', '[[skipped]] [[deeper]] [[nested]] [[upper]] [[in]]\n')
    put(root, 'real/target.md', 'target [[ok]]\n')
    put(root, 'link-target.md', 'lt [[ok]]\n')
    try:
        os.symlink('link-target.md', Path(root) / 'sym-file.md')
        os.symlink('real', Path(root) / 'sym-dir')
        os.symlink('/nonexistent', Path(root) / 'sym-dead.md')
        os.symlink('../outside', Path(root) / 'real' / 'sym-up')
        os.symlink('.', Path(root) / 'sym-loop')
    except OSError:
        pass
    put(root, 'sym-ref.md', '[[sym-file]] [[sym-dir/target]] [[sym-loop/ok]] [[target]]\n')


def chain(count):
    """`count` notes; note i links to a few earlier ones so degrees vary and ties occur."""
    def build(root):
        for i in range(count):
            body = [f'note {i}']
            for k in (1, 7, 31, 100):
                if i - k >= 0 and (i + k) % 3 != 0:
                    body.append(f'[[n{i - k:05d}]]')
            if i % 50 == 0:
                body.append('#hub')
            if i % 997 == 0:
                body.append('[[never]]')
            put(root, f'd{i % 17}/n{i:05d}.md', ' '.join(body) + '\n')
    return build


def big_mixed(root):
    """Notes + tags + unresolved + attachments so the cap sorts all kinds."""
    for i in range(3300):
        body = [f'[[m{(i * 7) % 3300:05d}]]', f'#t{i % 40}']
        if i % 11 == 0:
            body.append(f'[[ghost{i % 300}]]')
        if i % 13 == 0:
            body.append(f'![[img{i % 20}.png]]')
        put(root, f'g{i % 9}/m{i:05d}.md', ' '.join(body) + '\n')
    for i in range(20):
        put(root, f'img{i}.png', binary=b'\0')


def live_base(root):
    put(root, 'a.md', '[[b]] #t1\n')
    put(root, 'b.md', '[[c]] [[a]]\n')
    put(root, 'c.md', '')
    put(root, 'dir/d.md', '[[a]] [[e]]\n')
    put(root, 'dir/sub/e.md', '---\naliases: [Eee]\n---\n[[b]]\n')
    put(root, 'f.md', '[[Eee]] [[img.png]] ![[img.png]]\n')
    put(root, 'img.png', binary=b'\0')
    put(root, 'm.py', 'import n\n')
    put(root, 'n.py', '')
    put(root, 'g.md', '[[b]]\n')


def broken_py(root):
    """Text-corpus only: the reference reads no imports at all from a file it cannot parse."""
    put(root, 'bad.py', 'import os\ndef broken(:\nimport pkg\n')
    put(root, 'py3.py', 'import pkg.mod, util, sibling\nimport (\n')
    put(root, 'py2.py', 'import os\nprint "x"\nfrom pkg import mod\n')
    put(root, 'half.py', 'import os\nfrom pkg import (\n')


TEXT_ONLY = {'broken_py'}

FIXTURES = {
    'links': links, 'markup': markup, 'dups': dups, 'aliases': aliases, 'tags': tags, 'attachments': attachments,
    'code_py': code_py, 'code_js': code_js, 'code_style': code_style, 'code_other': code_other,
    'specials': specials, 'broken_py': broken_py, 'empty': lambda root: None, 'one': lambda root: put(root, 'only.md', '[[only]] #solo\n'),
    'chain_3000': chain(3000), 'chain_3001': chain(3001), 'chain_3500': chain(3500),
    'mixed_3300': big_mixed, 'live_base': live_base,
}


# --- random projects ---------------------------------------------------------

NAMES = ['alpha', 'Alpha', 'beta', 'gamma', 'Gamma Ray', 'delta', 'x', 'index', 'main', 'util', 'é', 'Straße', 'strasse', 'a.b']
DIRS = ['', '', '', 'sub', 'Sub', 'sub/deep', 'other', 'src', 'a', 'a/b']
TAGS = ['t1', 'T1', 't1/x', 't2', 'proj/a', 'proj/a/b', 'Proj']


def random_text(rng, kind, files):
    """A body for a file of `kind` that points at random names and files."""
    def pick_name():
        name = rng.choice(NAMES)
        return rng.choice([name, name.upper(), name.lower(), f'{name}.md', f'{name}.png'])

    def pick_path():
        return rng.choice(['', '../', './', 'sub/', '../sub/', '/']) + pick_name()

    lines = []
    if kind == 'note':
        if rng.random() < 0.3:
            fm = []
            if rng.random() < 0.7:
                fm.append('aliases: [' + ', '.join(rng.sample(NAMES, rng.randint(1, 2))) + ']')
            if rng.random() < 0.5:
                fm.append('tags: [' + ', '.join(rng.sample(TAGS, 2)) + ']')
            lines += ['---'] + fm + ['---']
        for _ in range(rng.randint(0, 8)):
            choice = rng.random()
            if choice < 0.35:
                lines.append(f'[[{pick_name()}]]')
            elif choice < 0.45:
                lines.append(f'[[{pick_path()}#h|alias]]')
            elif choice < 0.55:
                lines.append(f'![[{pick_name()}]]')
            elif choice < 0.7:
                lines.append(f'[t]({pick_path().replace(" ", "%20")})')
            elif choice < 0.8:
                lines.append('#' + rng.choice(TAGS))
            elif choice < 0.85:
                lines.append('`[[' + pick_name() + ']]`')
            elif choice < 0.9:
                lines.append(f'<{pick_path()}>')
            else:
                lines.append(f'![i]({pick_path()})')
    elif kind == 'py':
        for _ in range(rng.randint(0, 5)):
            c = rng.random()
            mod = rng.choice(['alpha', 'beta', 'util', 'sub.util', 'sub.deep.alpha', 'a.b', 'index', 'src.main', 'gamma'])
            if c < 0.4:
                lines.append(f'import {mod}')
            elif c < 0.7:
                lines.append(f'from {rng.choice(["", ".", "..", "...", ".sub"])}{mod if rng.random() < .7 else ""} import {rng.choice(["x", "alpha", "*"])}'.replace('from  import', 'from . import').replace('from . import *', 'from . import x'))
            else:
                lines.append(f'import {mod} as m')
    elif kind in ('js', 'ts'):
        for _ in range(rng.randint(0, 5)):
            spec = rng.choice(['./alpha', '../beta', './sub', './sub/index', '.', '..', './util.js', './main.css', 'lib', './gamma', './Alpha', '../sub/alpha'])
            lines.append(rng.choice([f"import x from '{spec}'", f"require('{spec}')", f"export * from '{spec}'", f"import '{spec}'"]))
    elif kind == 'css':
        for _ in range(rng.randint(0, 4)):
            spec = rng.choice(['alpha.css', 'sub/beta.css', '../main.css', 'img/a.png', '/alpha.css', 'x.png'])
            lines.append(rng.choice([f"@import '{spec}';", f'.a{{background:url({spec})}}']))
    elif kind == 'html':
        for _ in range(rng.randint(0, 4)):
            spec = rng.choice(['alpha.css', 'sub/beta.html', '../main.js', 'x.png', '/alpha.css', '#top', 'https://x.y'])
            lines.append(rng.choice([f'<a href="{spec}">', f'<img src={spec}>', f"<script src='{spec}'>"]))
    return '\n'.join(lines) + '\n'


def random_project(seed):
    import random
    rng = random.Random(seed)

    def build(root):
        files = []
        for _ in range(rng.randint(8, 40)):
            kind = rng.choice(['note', 'note', 'note', 'py', 'js', 'ts', 'css', 'html', 'png', 'txt'])
            name = rng.choice(NAMES)
            ext = {'note': rng.choice(['.md', '.md', '.markdown', '.MD']), 'py': '.py', 'js': '.js', 'ts': '.ts',
                   'css': '.css', 'html': '.html', 'png': '.png', 'txt': '.txt'}[kind]
            rel = '/'.join(p for p in (rng.choice(DIRS), name + ext) if p)
            if rel.casefold() in {f.casefold() for f in files}:
                continue  # names that fold together make the reference's lookup tables stale after removals
            files.append(rel)
            if kind == 'png':
                put(root, rel, binary=b'\0\1')
            else:
                put(root, rel, random_text(rng, kind if kind != 'txt' else 'note', files))
    return build


def random_ops(seed, root, count=40):
    """A scripted edit session over the files of an existing random project."""
    import random
    rng = random.Random(seed * 7919 + 1)
    present = set()
    for d, _, fs in os.walk(root):
        for f in fs:
            present.add(os.path.relpath(os.path.join(d, f), root).replace(os.sep, '/'))
    ops = []
    for _ in range(count):
        c = rng.random()
        if c < 0.35 or not present:
            name = rng.choice(NAMES)
            rel = '/'.join(p for p in (rng.choice(DIRS), name + rng.choice(['.md', '.md', '.py', '.js', '.css', '.png', '.txt'])) if p)
            kind = 'py' if rel.endswith('.py') else 'js' if rel.endswith('.js') else 'css' if rel.endswith('.css') else 'note'
            if rel.casefold() in {f.casefold() for f in present} and rel not in present:
                continue
            ops.append({'op': 'write', 'path': rel, 'text': random_text(rng, kind, present)})
            present.add(rel)
        elif c < 0.5:
            rel = rng.choice(sorted(present))
            kind = 'py' if rel.endswith('.py') else 'js' if rel.endswith('.js') else 'css' if rel.endswith('.css') else 'note'
            ops.append({'op': 'set_text', 'path': rel, 'text': random_text(rng, kind, present)})
        elif c < 0.58:
            rel = rng.choice(sorted(present))
            ops.append({'op': 'set_text', 'path': rel, 'text': None})
        elif c < 0.75:
            rel = rng.choice(sorted(present))
            ops.append({'op': 'delete', 'path': rel})
            present.discard(rel)
        else:
            rel = rng.choice(sorted(present))
            name = rng.choice(NAMES)
            ext = os.path.splitext(rel)[1] if rng.random() < 0.8 else '.md'
            target = '/'.join(p for p in (rng.choice(DIRS), name + ext) if p)
            if target.casefold() in {f.casefold() for f in present}:
                continue
            ops.append({'op': 'rename', 'from': rel, 'to': target})
            present.discard(rel)
            present.add(target)
    return ops
