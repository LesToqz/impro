#!/usr/bin/env python3
"""Cheap codec/diff and reserved capability checks; no display required."""
import builtins
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
import compare
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

    def test_live_notes_reserved(self):
        self.assertEqual([s for s in SCENES if s.startswith('notes-')],
                         ['notes-live','notes-table','notes-selection'])
        driver = (Path(__file__).parents[2]/'crates/impro/examples/parity/scenes.rs').read_text()
        self.assertIn('scene.starts_with("notes-")',driver)
        self.assertIn('std::process::exit(77)',driver)


if __name__ == '__main__':
    unittest.main()
