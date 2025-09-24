# Testing from scripts

The testing example demonstrates how to pair structured logging with harness-driven suites.

## What's included
- `script.koto` exports reusable helpers (`log_event`, `make_counter`) and emits JSON log messages while it runs.
- The `tests/` directory contains dedicated Koto files that describe suites using `@test`, `@pre_test`, and `@post_test` metadata.
- The runtime captures everything written via `host.log_info`, piping it to `logs/runtime.log` and the console tab in the UI.

## Try it out
1. Run the example to watch the logging helper stream JSON events into the console.
2. Open the **Tests** tab to execute either suite individually or the full set.
3. Enable **Watch examples** and **Hot reload** so edits trigger UI notifications and optional automatic reruns.

## Experiment ideas
- Add a new `.koto` file under `tests/` to prototype a feature-specific suite.
- Introduce deliberate failures to see how the harness surfaces stderr, stdout, and error messages.
- Extend `log_event` with additional fields (such as a UUID or timing information) and observe how the structured output appears in the runtime log file.
