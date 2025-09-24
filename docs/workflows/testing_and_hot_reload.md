# Testing and hot reload workflow

This guide explains how to pair Koto test suites with the explorer's hot-reload aware tooling. The [`testing` example](../../examples/testing/docs.md) demonstrates every concept in a working configuration.

## Authoring suites
- Create a `tests/` directory inside an example folder and add `.koto` files for each suite.
- Start each file with optional metadata comments (e.g. `# Title:` and `# Description:`) to populate UI labels.
- Export a map containing your tests. Annotate entries with `@test` functions. Optional `@pre_test` and `@post_test` hooks run before and after each test and are a good place to emit log messages or prepare fixtures.
- Use helpers exported from the example's `script.koto` when possible so that suites exercise the same code paths.

## Running suites in the UI
1. Run the example once to load it and stream any immediate log output into the console.
2. Open the **Tests** tab. Each suite is listed with a **Run** button and collapsible sections that show captured stdout/stderr per test case.
3. Use **Run all suites** to execute every `.koto` file in the `tests/` directory. The explorer records durations, pass/fail counts, and recent results so you can compare subsequent runs.

## Structured logging pipeline
- Call `host.log_info` (or helper functions that wrap it) to emit structured strings—JSON works well when paired with `serde.to_json`.
- Runtime log messages are written to `logs/runtime.log` on disk. The explorer polls this file and forwards new lines into the console tab so you can monitor events without leaving the app.
- Because the logger integrates with `tracing`, any Rust-side instrumentation that uses `tracing::info!` also flows through the same pipeline.

## Hot reload feedback loop
1. Enable **Watch examples** to keep the explorer in sync with on-disk changes.
2. Toggle **Hot reload** so the UI automatically re-runs the currently selected example after reload events.
3. When files change, a "Hot reload updates" panel appears above the run controls. It lists the modified script or suite, how long ago it changed, and provides a **Revert change** button. Reverts restore the prior file contents and refresh the example catalog in place.
4. Notifications also surface in the console and snackbar feed. Use these to decide when to re-run suites or inspect diffs.

## Tips
- Pair failing test cases with targeted log output so you can inspect the console while iterating.
- Add new suites with descriptive titles—each file becomes a selectable card in the **Tests** tab.
- CI can execute the same suites by calling into `examples::tests::run_suite`, making it straightforward to promote smoke tests into automated coverage.
