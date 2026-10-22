#!/usr/bin/env python3
"""Fresh GTK process measurements; use release executables on an idle machine."""
import argparse
import json
import math
import os
from pathlib import Path
import re
import statistics
import subprocess


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--before", type=Path, required=True)
    parser.add_argument("--after", type=Path, required=True)
    parser.add_argument("--runs", type=int, default=5)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    if args.runs < 5:
        parser.error("at least five runs are required")
    records = []
    env = dict(os.environ, GDK_BACKEND="x11", GIO_USE_VFS="local")
    for size in ("102400", "1048576", "lines", "10485760"):
        for run in range(args.runs):
            for phase in ("before", "after"):
                binary = getattr(args, phase).resolve()
                result = subprocess.run(
                    ["xvfb-run", "-a", "dbus-run-session", "--", str(binary), size],
                    env=env, text=True, capture_output=True, check=True,
                )
                record = {"phase": phase, "size": size, "run": run,
                          "stdout": result.stdout, "stderr": result.stderr}
                records.append(record)
                args.output.parent.mkdir(parents=True, exist_ok=True)
                args.output.write_text(json.dumps(records, indent=2) + "\n")
                print(f"{phase} size={size} run={run + 1} complete", flush=True)
    for size in ("102400", "1048576", "lines", "10485760"):
        for phase in ("before", "after"):
            samples = []
            for record in records:
                if record["size"] == size and record["phase"] == phase:
                    match = re.search(r"first-text-paint-ms=([\d.]+)", record["stdout"])
                    if match:
                        samples.append(float(match[1]))
            if samples:
                samples.sort()
                print(f"{phase} size={size} first-paint-p50-ms={statistics.median(samples):.3f} "
                      f"p95-ms={samples[math.ceil(len(samples) * .95) - 1]:.3f}")
            else:
                print(f"{phase} size={size} rejected; see captured output")


if __name__ == "__main__":
    main()
