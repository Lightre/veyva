#!/usr/bin/env python3
"""Measure process and system CPU/memory during an explicit Veyva recording."""

from __future__ import annotations

import argparse
import csv
import ctypes
import datetime as dt
import json
import os
from pathlib import Path
import subprocess
import sys
import time

SAMPLE_INTERVAL = 0.5
CREATE_NO_WINDOW = 0x08000000


class FILETIME(ctypes.Structure):
    _fields_ = [("dwLowDateTime", ctypes.c_uint32), ("dwHighDateTime", ctypes.c_uint32)]


class PROCESS_MEMORY_COUNTERS_EX(ctypes.Structure):
    _fields_ = [
        ("cb", ctypes.c_uint32), ("PageFaultCount", ctypes.c_uint32),
        ("PeakWorkingSetSize", ctypes.c_size_t), ("WorkingSetSize", ctypes.c_size_t),
        ("QuotaPeakPagedPoolUsage", ctypes.c_size_t), ("QuotaPagedPoolUsage", ctypes.c_size_t),
        ("QuotaPeakNonPagedPoolUsage", ctypes.c_size_t), ("QuotaNonPagedPoolUsage", ctypes.c_size_t),
        ("PagefileUsage", ctypes.c_size_t), ("PeakPagefileUsage", ctypes.c_size_t),
        ("PrivateUsage", ctypes.c_size_t),
    ]


def filetime_value(value: FILETIME) -> int:
    return (value.dwHighDateTime << 32) | value.dwLowDateTime


def windows_apis():
    kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)
    psapi = ctypes.WinDLL("psapi", use_last_error=True)
    kernel32.GetSystemTimes.argtypes = [ctypes.POINTER(FILETIME)] * 3
    kernel32.GetSystemTimes.restype = ctypes.c_int
    kernel32.GetProcessTimes.argtypes = [ctypes.c_void_p] + [ctypes.POINTER(FILETIME)] * 4
    kernel32.GetProcessTimes.restype = ctypes.c_int
    psapi.GetProcessMemoryInfo.argtypes = [ctypes.c_void_p, ctypes.POINTER(PROCESS_MEMORY_COUNTERS_EX), ctypes.c_uint32]
    psapi.GetProcessMemoryInfo.restype = ctypes.c_int
    return kernel32, psapi


def system_times(kernel32):
    idle, kernel, user = FILETIME(), FILETIME(), FILETIME()
    if not kernel32.GetSystemTimes(ctypes.byref(idle), ctypes.byref(kernel), ctypes.byref(user)):
        raise ctypes.WinError(ctypes.get_last_error())
    return filetime_value(idle), filetime_value(kernel), filetime_value(user)


def process_times(kernel32, handle):
    creation, exit_time, kernel, user = FILETIME(), FILETIME(), FILETIME(), FILETIME()
    if not kernel32.GetProcessTimes(handle, ctypes.byref(creation), ctypes.byref(exit_time),
                                    ctypes.byref(kernel), ctypes.byref(user)):
        raise ctypes.WinError(ctypes.get_last_error())
    return filetime_value(kernel) + filetime_value(user)


def memory_info(psapi, handle):
    counters = PROCESS_MEMORY_COUNTERS_EX()
    counters.cb = ctypes.sizeof(counters)
    if not psapi.GetProcessMemoryInfo(handle, ctypes.byref(counters), counters.cb):
        raise ctypes.WinError(ctypes.get_last_error())
    return counters.WorkingSetSize, counters.PrivateUsage


def sample_phase(phase, seconds, kernel32, psapi, process_handle, rows, errors, process=None):
    start = time.monotonic()
    deadline = start + seconds
    prev_system = system_times(kernel32)
    prev_process = process_times(kernel32, process_handle) if process_handle else None
    valid = 0
    missed = 0
    while time.monotonic() < deadline and (process is None or process.poll() is None):
        target = min(deadline, start + (valid + missed + 1) * SAMPLE_INTERVAL)
        time.sleep(max(0, target - time.monotonic()))
        now = time.monotonic()
        try:
            current_system = system_times(kernel32)
            current_process = process_times(kernel32, process_handle) if process_handle else None
            ws, private = memory_info(psapi, process_handle) if process_handle else (None, None)
            idle_delta = current_system[0] - prev_system[0]
            total_delta = ((current_system[1] - prev_system[1]) +
                           (current_system[2] - prev_system[2]))
            busy_delta = total_delta - idle_delta
            sys_pct = (100 * busy_delta / total_delta) if total_delta > 0 else None
            proc_pct = None
            core_eq = None
            previous_clock = rows[-1]["_clock"] if rows and rows[-1]["phase"] == phase else start
            interval = now - previous_clock
            if process_handle and current_process is not None:
                core_eq = (current_process - prev_process) / 10_000_000 / interval
                proc_pct = 100 * core_eq / (os.cpu_count() or 1)
            rows.append({"phase": phase, "elapsed_seconds": round(now - start, 3),
                         "interval_seconds": interval,
                         "process_cpu_percent_logical": proc_pct,
                         "process_cpu_core_equivalents": core_eq,
                         "system_cpu_percent_excluding_idle": sys_pct,
                         "working_set_bytes": ws, "private_bytes": private, "_clock": now})
            valid += 1
            prev_system, prev_process = current_system, current_process
        except OSError as error:
            missed += 1
            errors.append({"phase": phase, "elapsed_seconds": round(now - start, 3),
                           "error": str(error)})
    return {"time_limit_seconds": seconds, "actual_seconds": round(time.monotonic() - start, 3),
            "valid_samples": valid, "skipped_samples": missed,
            "sample_coverage_seconds": round(sum(row["interval_seconds"] for row in rows
                                                  if row["phase"] == phase), 3)}


def aggregate_cpu(rows, phase, key):
    selected = [row for row in rows if row["phase"] == phase and row[key] is not None]
    if len(selected) < 2:
        return None
    # Aggregate CPU deltas by their measured intervals, not by an unweighted mean.
    weights = [row["interval_seconds"] for row in selected]
    total = sum(weights)
    return sum(r[key] * w for r, w in zip(selected, weights)) / total if total else None


def peak_value(rows, phase, key):
    values = [row[key] for row in rows if row["phase"] == phase and row[key] is not None]
    return max(values) if values else None


def main():
    parser = argparse.ArgumentParser(description="Measure CPU and memory during a controlled Veyva recording.")
    parser.add_argument("--seconds", type=int, default=30, choices=range(1, 61))
    parser.add_argument("--baseline-seconds", type=int, default=5, choices=range(1, 61))
    parser.add_argument("--output-dir", type=Path, help="Parent directory for a unique measurement folder")
    parser.add_argument("--exe", type=Path, default=Path(__file__).resolve().parents[1] / "target" / "release" / "veyva.exe")
    parser.add_argument("--monitor", type=int, default=0)
    args = parser.parse_args()
    if os.name != "nt":
        parser.error("This tool requires Windows")
    root = args.output_dir or (Path(__file__).resolve().parents[1] / "work")
    root.mkdir(parents=True, exist_ok=True)
    stamp = dt.datetime.now(dt.timezone.utc).strftime("%Y%m%dT%H%M%SZ")
    output = root / f"measurement-{stamp}"
    suffix = 1
    while True:
        try:
            output.mkdir()
            break
        except FileExistsError:
            output = root / f"measurement-{stamp}-{suffix}"
            suffix += 1
    exe = args.exe.resolve()
    recording = (output / "capture.mp4").resolve()
    rows = []
    errors = []
    kernel32, psapi = windows_apis()
    try:
        baseline = sample_phase("baseline", args.baseline_seconds, kernel32, psapi, None, rows, errors)
        stdout_path = output / "record.log"
        with stdout_path.open("w", encoding="utf-8", errors="replace") as log:
            started = time.monotonic()
            process = subprocess.Popen([str(exe), "record", "--seconds", str(args.seconds),
                                        "--monitor", str(args.monitor), "--output", str(recording)],
                                       stdout=log, stderr=subprocess.STDOUT, creationflags=CREATE_NO_WINDOW)
            handle = ctypes.c_void_p(process._handle)
            try:
                capture = sample_phase("capture", args.seconds + 30, kernel32, psapi, handle, rows, errors, process)
                collector_timed_out = process.poll() is None
            finally:
                if process.poll() is None:
                    process.terminate()
            try:
                returncode = process.wait(timeout=10)
            except subprocess.TimeoutExpired:
                process.kill()
                returncode = process.wait()
            runtime = time.monotonic() - started
        clean_rows = [{k: v for k, v in row.items() if k != "_clock"} for row in rows]
        with (output / "metrics.csv").open("w", newline="", encoding="utf-8") as csv_file:
            writer = csv.DictWriter(csv_file, fieldnames=["phase", "elapsed_seconds", "interval_seconds", "process_cpu_percent_logical",
                "process_cpu_core_equivalents", "system_cpu_percent_excluding_idle", "working_set_bytes", "private_bytes"])
            writer.writeheader()
            writer.writerows(clean_rows)
        with (output / "sampling-errors.csv").open("w", newline="", encoding="utf-8") as error_file:
            writer = csv.DictWriter(error_file, fieldnames=["phase", "elapsed_seconds", "error"])
            writer.writeheader()
            writer.writerows(errors)
        summary = {"executable": str(exe), "recording": str(recording), "logical_processor_count": os.cpu_count(),
                   "nominal_recording_seconds": args.seconds,
                   "capture_phase_scope": "Process launch through capture, encode, and export",
                   "recording_process_runtime_seconds": round(runtime, 3), "return_code": returncode,
                   "baseline": baseline, "capture": capture,
                   "baseline_system_cpu_percent_excluding_idle": aggregate_cpu(rows, "baseline", "system_cpu_percent_excluding_idle"),
                   "capture_system_cpu_percent_excluding_idle": aggregate_cpu(rows, "capture", "system_cpu_percent_excluding_idle"),
                   "capture_process_cpu_percent_logical": aggregate_cpu(rows, "capture", "process_cpu_percent_logical"),
                   "capture_process_cpu_core_equivalents": aggregate_cpu(rows, "capture", "process_cpu_core_equivalents"),
                   "capture_peak_working_set_bytes": peak_value(rows, "capture", "working_set_bytes"),
                   "capture_peak_private_bytes": peak_value(rows, "capture", "private_bytes"),
                   "capture_sample_coverage_seconds": capture["sample_coverage_seconds"],
                   "capture_skipped_samples": capture["skipped_samples"],
                   "sampling_error_count": len(errors),
                   "capture_timed_out": collector_timed_out}
        (output / "summary.json").write_text(json.dumps(summary, indent=2), encoding="utf-8")
        print(output)
        return 0 if returncode == 0 else returncode or 1
    except Exception as error:
        print(f"Measurement failed: {error}", file=sys.stderr)
        print(f"Partial results, if any, are in: {output}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
