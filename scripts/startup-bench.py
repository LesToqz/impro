#!/usr/bin/env python3
"""Measure ten process-launch→mapped-window runs on one X11 display.

Run under xvfb-run -a dbus-run-session. --reference-root is optional and is
read-only: Python bytecode writes are disabled. No application files are edited.
The Rust application also reports its own main→first-completed-paint timestamp.
"""
import argparse
import ctypes as C
import ctypes.util
import json
import os
from pathlib import Path
import statistics
import subprocess
import time


class Attributes(C.Structure):
    _fields_ = [(n, t) for n, t in (
        ("x", C.c_int), ("y", C.c_int), ("width", C.c_int), ("height", C.c_int),
        ("border_width", C.c_int), ("depth", C.c_int), ("visual", C.c_void_p),
        ("root", C.c_ulong), ("window_class", C.c_int), ("bit_gravity", C.c_int),
        ("win_gravity", C.c_int), ("backing_store", C.c_int),
        ("backing_planes", C.c_ulong), ("backing_pixel", C.c_ulong),
        ("save_under", C.c_int), ("colormap", C.c_ulong),
        ("map_installed", C.c_int), ("map_state", C.c_int),
        ("all_event_masks", C.c_long), ("your_event_mask", C.c_long),
        ("do_not_propagate_mask", C.c_long), ("override_redirect", C.c_int),
        ("screen", C.c_void_p))]


class X11:
    def __init__(self):
        self.x = C.CDLL(ctypes.util.find_library("X11"))
        signatures = {
            "XOpenDisplay": ([C.c_char_p], C.c_void_p),
            "XDefaultRootWindow": ([C.c_void_p], C.c_ulong),
            "XInternAtom": ([C.c_void_p, C.c_char_p, C.c_int], C.c_ulong),
            "XQueryTree": ([C.c_void_p, C.c_ulong, C.POINTER(C.c_ulong),
                            C.POINTER(C.c_ulong), C.POINTER(C.POINTER(C.c_ulong)),
                            C.POINTER(C.c_uint)], C.c_int),
            "XGetWindowAttributes": ([C.c_void_p, C.c_ulong, C.POINTER(Attributes)], C.c_int),
            "XGetWindowProperty": ([C.c_void_p, C.c_ulong, C.c_ulong, C.c_long,
                                    C.c_long, C.c_int, C.c_ulong, C.POINTER(C.c_ulong),
                                    C.POINTER(C.c_int), C.POINTER(C.c_ulong),
                                    C.POINTER(C.c_ulong), C.POINTER(C.c_void_p)], C.c_int),
            "XFree": ([C.c_void_p], C.c_int),
            "XCloseDisplay": ([C.c_void_p], C.c_int),
        }
        for name, (args, result) in signatures.items():
            getattr(self.x, name).argtypes = args
            getattr(self.x, name).restype = result
        # Other processes may destroy a window between enumeration and lookup.
        self.error_handler = C.CFUNCTYPE(C.c_int, C.c_void_p, C.c_void_p)(lambda *_: 0)
        self.x.XSetErrorHandler(self.error_handler)
        self.display = self.x.XOpenDisplay(None)
        if not self.display:
            raise RuntimeError("requires an X11 display")
        self.root = self.x.XDefaultRootWindow(self.display)
        self.pid_atom = self.x.XInternAtom(self.display, b"_NET_WM_PID", 0)

    def mapped(self, pid):
        root, parent, count = C.c_ulong(), C.c_ulong(), C.c_uint()
        children = C.POINTER(C.c_ulong)()
        self.x.XQueryTree(self.display, self.root, C.byref(root), C.byref(parent),
                          C.byref(children), C.byref(count))
        windows = list(children[:count.value])
        if children:
            self.x.XFree(children)
        for window in windows:
            actual_type, fmt, items, remaining = C.c_ulong(), C.c_int(), C.c_ulong(), C.c_ulong()
            data = C.c_void_p()
            self.x.XGetWindowProperty(self.display, window, self.pid_atom, 0, 1, 0, 0,
                C.byref(actual_type), C.byref(fmt), C.byref(items), C.byref(remaining), C.byref(data))
            owner = C.cast(data, C.POINTER(C.c_ulong))[0] if data and items.value else None
            if data:
                self.x.XFree(data)
            if owner == pid:
                attributes = Attributes()
                if self.x.XGetWindowAttributes(self.display, window, C.byref(attributes)) and attributes.map_state == 2:
                    return True
        return False


def measure(x, command, environment, runs, qa=False):
    mapped, painted = [], []
    for i in range(runs):
        env = dict(environment)
        if qa:
            env["IMPRO_QA_SCREENSHOT"] = str(Path(".ceo") / f"startup-{i}.png")
            env["IMPRO_QA_TIMING"] = "1"
        started = time.monotonic()
        child = subprocess.Popen(command, env=env, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                                 start_new_session=True)
        try:
            while not x.mapped(child.pid):
                if child.poll() is not None:
                    out, err = child.communicate()
                    raise RuntimeError(f"application exited before mapping ({child.returncode}): {err.decode()}")
                if time.monotonic() - started > 15:
                    raise RuntimeError("application did not map a window in 15 s")
                time.sleep(.001)
            mapped.append((time.monotonic() - started) * 1000)
            if qa:
                out, err = child.communicate(timeout=15)
                if child.returncode or err:
                    raise RuntimeError(f"screenshot run failed: {err.decode()}")
                painted.append(float(out.decode().split("first-paint-ms=")[1].splitlines()[0]))
            else:
                child.terminate()
                child.communicate(timeout=5)
        finally:
            if child.poll() is None:
                child.kill()
                child.communicate()
    result = {"launch_to_map_ms": mapped, "median_map_ms": statistics.median(mapped)}
    if painted:
        result.update(main_to_paint_ms=painted, median_paint_ms=statistics.median(painted))
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", default="target/release/impro")
    parser.add_argument("--reference-root", type=Path)
    parser.add_argument("--runs", type=int, default=10)
    parser.add_argument("--output", type=Path, help="write clean JSON independently of session daemon stdout")
    args = parser.parse_args()
    Path(".ceo").mkdir(exist_ok=True)
    env = dict(os.environ, GDK_BACKEND="x11", GIO_USE_VFS="local", PYTHONDONTWRITEBYTECODE="1")
    x = X11()
    result = {"impro": measure(x, [str(Path(args.binary).resolve()), "/tmp"], env, args.runs, qa=True)}
    if args.reference_root:
        root = args.reference_root.resolve()
        reference_env = dict(env, PYTHONPATH=str(root))
        result["reference"] = measure(x, ["python3", "-m", root.name.lower(), "/tmp"], reference_env, args.runs)
        result["map_speedup"] = result["reference"]["median_map_ms"] / result["impro"]["median_map_ms"]
    x.x.XCloseDisplay(x.display)
    payload = json.dumps(result, indent=2)
    if args.output:
        args.output.write_text(payload + "\n", encoding="utf-8")
    print(payload)


if __name__ == "__main__":
    main()
