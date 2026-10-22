#!/usr/bin/env python3
"""Cheap codec/diff and reserved capability checks; no display required."""
import builtins
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
import compare
import check_runs
import json
from matrix import SCENES


class HarnessTests(unittest.TestCase):
    def test_exact_rgba_metrics(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            a = bytes([10,20,30,255])*4
            b = bytearray(a)
            b[4] = 14
            compare.write_png(root/'a.png',2,2,a)
            compare.write_png(root/'b.png',2,2,b)
            result = compare.compare(root/'a.png',root/'b.png',root/'comparison')
            self.assertEqual(result['changed'],1)
            self.assertEqual(result['bbox'],[1,0,2,1])
            self.assertEqual(result['mean_channel_error'],.25)
            self.assertEqual(compare.read_png(root/'comparison-side.png')[:2],(4,2))

    def test_stdlib_codec(self):
        real_import = builtins.__import__
        def without_pillow(name, *args, **kwargs):
            if name == 'PIL':
                raise ImportError('test fallback')
            return real_import(name, *args, **kwargs)
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory)/'a.png'
            rgba = bytes([1,2,3,255,4,5,6,128])
            compare.write_png(path,2,1,rgba)
            with patch('builtins.__import__',without_pillow):
                self.assertEqual(compare.read_png(path),(2,1,rgba))

    def test_clipped_capture_is_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory)/'clipped.png'
            compare.write_png(path,64,64,bytes([10,20,30,255])*64*32 + bytes([0,0,0,255])*64*32)
            with self.assertRaisesRegex(ValueError,'clipped capture'):
                compare.validate_capture(path)

    def test_repeat_checker_rejects_changed_counts(self):
        with tempfile.TemporaryDirectory() as directory:
            first, second = Path(directory)/'first.json', Path(directory)/'second.json'
            row = dict(scene='default',theme='dark',scale=1,status='compared',changed=0)
            first.write_text(json.dumps(dict(rows=[row])))
            second.write_text(first.read_text())
            self.assertEqual(check_runs.check(first,second),1)
            row['changed'] = 1
            second.write_text(json.dumps(dict(rows=[row])))
            with self.assertRaisesRegex(ValueError,'changed'):
                check_runs.check(first,second)

    def test_live_note_scenes_are_implemented_on_both_sides(self):
        notes = [s for s in SCENES if s.startswith('notes-')]
        self.assertEqual(notes, ['notes-live','notes-list','notes-code','notes-table',
                                 'notes-selection','notes-scrolled','notes-fullwidth'])
        driver = (Path(__file__).parents[2]/'crates/impro/examples/parity/scenes.rs').read_text()
        reference = (Path(__file__).parent/'reference.py').read_text()
        self.assertNotIn('std::process::exit(77)',driver)
        for scene in notes:
            self.assertIn('"%s"' % scene,driver)
            self.assertIn("'%s'" % scene,reference)


if __name__ == '__main__':
    unittest.main()
