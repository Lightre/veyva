# Controlled capture measurement

Run `python tools/measure_capture.py` from the project directory to launch the
release executable for a 30-second silent desktop recording. The tool samples
system CPU during a five-second pre-capture baseline, then samples system CPU, Veyva
process CPU, working set, and private bytes during capture at roughly 0.5-second
intervals. The capture phase includes process startup, capture, encoding, and
export. Process CPU percent is normalized by the machine's logical processor
count; core equivalents are also reported. System CPU excludes idle time.

Use `--seconds 1..60`, `--baseline-seconds 1..60`, `--exe PATH`, and
`--monitor INDEX` to adjust the run. `--output-dir PATH` selects a parent
directory; otherwise results go under `work/`. Each run creates a unique
`measurement-<UTC timestamp>` directory and never overwrites one. It contains:

- `capture.mp4`: the recording requested from Veyva.
- `record.log`: combined stdout and stderr from Veyva.
- `metrics.csv`: timestamped samples labeled `baseline` or `capture`, with each
  measured interval recorded explicitly.
- `sampling-errors.csv`: API sampling failures, kept separate from valid samples.
- `summary.json`: nominal duration, process runtime and exit code, sample
  coverage, skipped samples, peak sampled memory, and interval-weighted CPU
  summaries. Coverage sums the successful measured sample intervals.

Run this on an open desktop with a stable, uncontrolled workload: do not launch
a game or benchmark during the measurement. Baseline CPU is a reference for
system activity, not a matched no-capture Veyva run. This tool reports CPU and
memory only; it makes no GPU utilization claim. A capture collector timeout is
limited to the requested recording duration plus 30 seconds; a still-running
process is terminated and the timeout is recorded in the summary.
