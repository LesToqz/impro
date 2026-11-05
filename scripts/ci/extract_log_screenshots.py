#!/usr/bin/env python3
"""Recover the screenshots that a CI job printed into its log.

    python3 -I scripts/ci/extract_log_screenshots.py [-o OUTDIR] [LOG | -]

packaging/windows/print-screenshots.ps1 writes every PNG into the job log as base64 between two marker lines:

    -----BEGIN IMPRO-SCREENSHOT <file name>-----
    <base64, wrapped at 100 columns>
    -----END IMPRO-SCREENSHOT <file name>-----

This script reads such a log (a file, or standard input when LOG is `-` or missing), finds the blocks and writes
the PNG files to OUTDIR (default `screenshots`, created when needed). It tolerates what GitHub Actions adds to
every line: the `2026-11-05T07:24:12.1234567Z ` timestamp prefix, the `job<TAB>step<TAB>` columns of
`gh run view --log`, a byte order mark, ANSI colour codes and CR LF line ends. Lines inside a block that do not
look like the rest of the base64 body (an interleaved log annotation) are ignored.

A block is written only when its base64 decodes and the result starts with the PNG magic bytes. A block that is
unterminated, interleaved with another one, or fails those checks is reported on standard error and skipped. A
PNG whose chunks do not check out (truncated, bad CRC) is still written, with a warning. File names are reduced
to a safe base name; two blocks with the same name get -2, -3, ... before the extension.

Exit status: 0 every block was written, 3 some were written and some skipped, 1 nothing was written (no block, or
all skipped), 2 usage or I/O error.
"""

import argparse
import base64
import binascii
import os
import re
import struct
import sys
import zlib

PNG_MAGIC = b"\x89PNG\r\n\x1a\n"

# `2026-11-05T07:24:12.1234567Z ` as written by GitHub Actions, optionally behind the `job<TAB>step<TAB>` columns
# that `gh run view --log` puts in front.
_PREFIX = re.compile(r"^(?:[^\t]*\t[^\t]*\t)?﻿?\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d+)?Z(?: |$)")
_ANSI = re.compile(r"\x1b\[[0-9;?]*[A-Za-z]")
_BEGIN = re.compile(r"^-----BEGIN IMPRO-SCREENSHOT (?P<name>.+?)-----$")
_END = re.compile(r"^-----END IMPRO-SCREENSHOT (?P<name>.+?)-----$")
_BASE64 = re.compile(r"^[A-Za-z0-9+/]+={0,2}$")


class Shot:
    """One extracted screenshot: the (unsanitised) name from the marker line, the decoded bytes, an optional warning."""

    def __init__(self, name, data, warning=None):
        self.name = name
        self.data = data
        self.warning = warning


def clean_line(raw):
    """A log line without line end, BOM, ANSI colours and the GitHub timestamp prefix."""
    line = raw.rstrip("\r\n")
    line = line.lstrip("﻿")
    line = _ANSI.sub("", line)
    line = _PREFIX.sub("", line, count=1)
    return line.strip()


def safe_name(name):
    """A file name that cannot leave the output directory and always ends in .png."""
    base = name.replace("\\", "/").split("/")[-1].strip()
    base = re.sub(r"[^A-Za-z0-9._ -]", "_", base).strip(" .")
    if not base:
        base = "screenshot"
    if not base.lower().endswith(".png"):
        base += ".png"
    return base


def png_warning(data):
    """None when the chunk structure of the PNG is intact (every CRC right, IHDR first, IEND last), else a short reason."""
    pos = len(PNG_MAGIC)
    first = True
    while pos + 12 <= len(data):
        (length,) = struct.unpack(">I", data[pos : pos + 4])
        kind = data[pos + 4 : pos + 8]
        end = pos + 8 + length
        if end + 4 > len(data):
            return "truncated inside a chunk"
        if first and kind != b"IHDR":
            return "first chunk is not IHDR"
        first = False
        (crc,) = struct.unpack(">I", data[end : end + 4])
        if zlib.crc32(data[pos + 4 : end]) & 0xFFFFFFFF != crc:
            return f"bad CRC in the {kind.decode('latin-1')} chunk"
        pos = end + 4
        if kind == b"IEND":
            return None if pos == len(data) else "extra bytes after IEND"
    return "no IEND chunk (truncated?)"


def decode_block(name, lines, first):
    """(Shot, None) for a valid PNG, (None, problem text) otherwise."""
    if not lines:
        return None, f"line {first}: block {name!r} is empty"
    # The producer wraps at a fixed width; a line of another width in the middle is noise that looked like base64.
    width = max(len(line) for line in lines)
    kept = [line for i, line in enumerate(lines) if len(line) == width or i == len(lines) - 1]
    try:
        data = base64.b64decode("".join(kept), validate=True)
    except (binascii.Error, ValueError) as exc:
        return None, f"line {first}: block {name!r} is not valid base64 ({exc})"
    if not data.startswith(PNG_MAGIC):
        return None, f"line {first}: block {name!r} does not start with the PNG magic bytes (starts with {data[:8].hex()})"
    why = png_warning(data)
    warning = f"{name!r}: {why}" if why else None
    return Shot(name, data, warning), None


def extract(text):
    """Find the blocks in a log. Returns (shots, problems): Shot objects in log order and one string per skipped block."""
    shots = []
    problems = []
    name = None  # name of the open block, None between blocks
    body = []
    first = 0
    for number, raw in enumerate(text.split("\n"), 1):
        line = clean_line(raw)
        m = _BEGIN.match(line)
        if m:
            if name is not None:
                problems.append(f"line {first}: block {name!r} has no END line before the next BEGIN (line {number})")
            name, body, first = m.group("name"), [], number
            continue
        m = _END.match(line)
        if m:
            if name is None:
                problems.append(f"line {number}: END line for {m.group('name')!r} without a BEGIN line")
            elif m.group("name") != name:
                problems.append(f"line {first}: block {name!r} is closed by an END line for {m.group('name')!r} (line {number})")
            else:
                shot, problem = decode_block(name, body, first)
                if problem:
                    problems.append(problem)
                else:
                    shots.append(shot)
            name, body = None, []
            continue
        if name is not None and _BASE64.match(line):
            body.append(line)
    if name is not None:
        problems.append(f"line {first}: block {name!r} is not terminated (the log ends or was truncated)")
    return shots, problems


def write_shots(shots, out_dir):
    """Write the shots below out_dir with unique safe names. Returns the list of (path, size)."""
    os.makedirs(out_dir, exist_ok=True)
    used = set()
    written = []
    for shot in shots:
        name = safe_name(shot.name)
        stem, ext = os.path.splitext(name)
        candidate, n = name, 1
        while candidate.lower() in used:
            n += 1
            candidate = f"{stem}-{n}{ext}"
        used.add(candidate.lower())
        path = os.path.join(out_dir, candidate)
        with open(path, "wb") as handle:
            handle.write(shot.data)
        written.append((path, len(shot.data)))
    return written


def read_log(source):
    if source in (None, "-"):
        data = sys.stdin.buffer.read()
    else:
        with open(source, "rb") as handle:
            data = handle.read()
    return data.decode("utf-8", errors="replace")


def main(argv=None):
    parser = argparse.ArgumentParser(description="Extract screenshots printed into a CI job log.")
    parser.add_argument("log", nargs="?", default="-", help="job log file, or - for standard input (default)")
    parser.add_argument("-o", "--out-dir", default="screenshots", help="directory for the PNG files (default: screenshots)")
    args = parser.parse_args(argv)
    try:
        text = read_log(args.log)
    except OSError as exc:
        print(f"error: cannot read the log: {exc}", file=sys.stderr)
        return 2
    shots, problems = extract(text)
    for shot in shots:
        if shot.warning:
            print(f"warning: {shot.warning}", file=sys.stderr)
    for problem in problems:
        print(f"skipped: {problem}", file=sys.stderr)
    try:
        written = write_shots(shots, args.out_dir)
    except OSError as exc:
        print(f"error: cannot write to {args.out_dir}: {exc}", file=sys.stderr)
        return 2
    for path, size in written:
        print(f"wrote {path} ({size} bytes)")
    if not written:
        print("no screenshot found in the log", file=sys.stderr)
        return 1
    return 3 if problems else 0


if __name__ == "__main__":
    sys.exit(main())
