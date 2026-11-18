"""Run isolated timing benchmarks or verify the native/Python compatibility boundary."""

import argparse
import concurrent.futures
import importlib.util
import json
import math
import os
from pathlib import Path
import statistics
import sys
from tempfile import TemporaryDirectory
import time
import unittest
from unittest.mock import patch


def modules(repo, native):
    paths = ("src/macro/core/fps_utils.py", "src-linux/core/fps_utils.py")
    with patch.dict(os.environ):
        os.environ.pop("CRYSS_RUST_TIMING", None)
        if native:
            os.environ["CRYSS_RUST_TIMING"] = "1"
        loaded = []
        for index, path in enumerate(paths):
            spec = importlib.util.spec_from_file_location(f"fps_utils_{index}", repo / path)
            module = importlib.util.module_from_spec(spec)
            spec.loader.exec_module(module)
            loaded.append(module)
    return loaded


def benchmark(module):
    durations = (0.002, 0.008, 0.016) * 120
    errors = []
    cpu_start = time.process_time_ns()
    wall_start = time.perf_counter_ns()
    for duration in durations:
        started = time.perf_counter()
        module.wait_exact(duration, started)
        errors.append((time.perf_counter() - started - duration) * 1000)
    wall_seconds = (time.perf_counter_ns() - wall_start) / 1e9
    cpu_seconds = (time.process_time_ns() - cpu_start) / 1e9
    errors.sort()
    return {
        "samples": len(errors),
        "cpu_s": round(cpu_seconds, 6),
        "wall_s": round(wall_seconds, 6),
        "p50_overshoot_ms": round(statistics.median(errors), 6),
        "p95_overshoot_ms": round(errors[math.ceil(0.95 * len(errors)) - 1], 6),
        "p99_overshoot_ms": round(errors[math.ceil(0.99 * len(errors)) - 1], 6),
        "max_overshoot_ms": round(errors[-1], 6),
    }


class TimingCompatibility(unittest.TestCase):
    def test_native_library_is_loaded_by_both_entry_points(self):
        for module in self.native:
            with self.subTest(path=module.__file__):
                self.assertIsNotNone(getattr(module, "_NATIVE_WAIT", None))

    def test_python_remains_the_default(self):
        for module in self.legacy:
            with self.subTest(path=module.__file__):
                self.assertIsNone(module._NATIVE_WAIT)
                self.check_duration(module, 0.004)

    def test_native_respects_relative_and_absolute_deadlines(self):
        for module in self.native:
            with self.subTest(path=module.__file__):
                self.check_duration(module, 0.004)
                started = time.perf_counter() - 0.020
                with patch.object(module, "_NATIVE_WAIT", wraps=module._NATIVE_WAIT) as native:
                    module.wait_exact(0.010, started)
                    native.assert_not_called()
                    module.wait_exact(-0.010)
                    native.assert_not_called()
                    module.wait_exact(0.070, started)
                    self.assertGreaterEqual(time.perf_counter() - started, 0.070)
                    self.assertGreater(native.call_args.args[0], 0)
                    self.assertLess(native.call_args.args[0], 0.051)

    def test_missing_bundled_library_falls_back_to_python(self):
        with TemporaryDirectory() as directory:
            with patch.dict(os.environ, {"CRYSS_RUST_TIMING": "1"}):
                with patch.multiple(sys, frozen=True, _MEIPASS=directory, create=True):
                    for module in self.native:
                        with self.subTest(path=module.__file__):
                            self.assertIsNone(module._load_native_wait())

    def test_concurrent_waits(self):
        for module in self.native:
            with self.subTest(path=module.__file__):
                with concurrent.futures.ThreadPoolExecutor(max_workers=4) as workers:
                    futures = [workers.submit(self.check_duration, module, 0.008) for _ in range(12)]
                    for future in futures:
                        future.result()

    def test_fps_interpolation_matches_python(self):
        for native, legacy in zip(self.native, self.legacy):
            for name in (name for name in vars(legacy) if name.startswith(("T_FPS_", "T_FRAME_"))):
                table = getattr(legacy, name)
                for fps in range(20, 301):
                    self.assertEqual(native.fps2t(getattr(native, name), fps), legacy.fps2t(table, fps))

    def check_duration(self, module, duration):
        started = time.perf_counter()
        module.wait_exact(duration)
        self.assertGreaterEqual(time.perf_counter() - started, duration)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", type=Path, default=Path(__file__).resolve().parent.parent)
    parser.add_argument("--benchmark", choices=("python", "rust", "both"))
    parser.add_argument("--rust-first", action="store_true", help="Reverse the order of a paired benchmark")
    args = parser.parse_args()
    if args.rust_first and args.benchmark != "both":
        parser.error("--rust-first requires --benchmark both")
    platform_module = 1 if sys.platform == "linux" else 0
    TimingCompatibility.legacy = modules(args.repo, native=False)
    if args.benchmark == "python":
        print(json.dumps({"python": benchmark(TimingCompatibility.legacy[platform_module])}, indent=2))
        return
    TimingCompatibility.native = modules(args.repo, native=True)
    if args.benchmark:
        rust_module = TimingCompatibility.native[platform_module]
        if rust_module._NATIVE_WAIT is None:
            parser.error("Rust release library was not loaded")
        targets = [("rust", rust_module)]
        if args.benchmark == "both":
            targets.insert(0, ("python", TimingCompatibility.legacy[platform_module]))
            if args.rust_first:
                targets.reverse()
        results = {name: benchmark(module) for name, module in targets}
        print(json.dumps(results, indent=2))
        return
    unittest.main(argv=[__file__])


if __name__ == "__main__":
    main()
