# Known differences from the reference graph logic

The differential run (`run.sh`) compares the engine with the reference on
synthetic trees, scripted live edits, a link-parse corpus, hostile preferences
and two real trees. Everything below is deliberate; `known.json` lists the
classes the comparison tolerates, the other two are modelled in `oracle.py`.

| id | difference | reason |
| --- | --- | --- |
| K1 | Imports are read from Python files that do not parse (syntax errors, Python 2 code); the reference reads none from such a file. | A half-typed file keeps its other links while it is being edited; a live graph should not drop edges on every keystroke. |
| K2 | `<br>`-style tags and other `<word>` text without a dot are not Markdown autolinks. The reference treats them as link targets (`oracle.py` filters them out of its side). | They are markup, not references; the reference creates unresolved nodes named `br`, `div`... |
| K3 | An attachment larger than the size cap stays a resolvable node (without searchable text). The reference drops it. | A link to a large image or PDF should resolve rather than show as missing (`oracle.py: keep_large_attachments`). |

Everything else matched: 49,827 comparisons, 0 unexplained. Behaviour changes
the engine took from the reference are covered by the goldens in
`crates/impro-graph/tests/golden`: only the reference's languages produce
import edges (Rust `mod`/`use` and shell `source` do not), an unfielded regex
is searched in `path\nlabel\ncontent` as one text, and a file's time is the
older of its change and modified time on Unix.
