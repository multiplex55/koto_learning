# Testing guide

The [`testing` example](../../examples/testing/docs.md) illustrates how to run unit-like assertions via dedicated Koto suites.

## Run the tests
1. Select **Koto Tests** in the explorer and execute the script to populate the console with structured log output.
2. Switch to the **Tests** tab to run individual suites or the entire set. Results include captured stdout/stderr and timing data.
3. Enable **Watch examples** and **Hot reload** to receive notifications (with revert buttons) whenever files inside the example change.

## Experiment further
- Add a new `.koto` file under `examples/testing/tests/` to prototype additional suites.
- Use `@pre_test` and `@post_test` hooks to configure fixtures or inspect captured logs.
- Combine the example with serialization helpers to export test results for reporting.
