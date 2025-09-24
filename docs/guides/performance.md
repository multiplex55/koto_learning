# Performance tuning guide

Use the [`performance` example](../../examples/performance/docs.md) to explore how Koto compares with host-backed implementations. It times a recursive Fibonacci function in pure Koto and contrasts the result with a Rust helper that uses an optimized loop.

## Run the comparison
1. Choose **Performance Comparison** in the explorer and run the script.
2. Observe the timing messages printed for both the Koto and Rust implementations.
3. Inspect the returned map to review the recorded input sizes and elapsed milliseconds.

## Experiment further
- Increase the `small_target` value cautiously to see how recursion impacts runtime.
- Switch the Koto implementation to an iterative version and re-run the comparison.
- Pair this example with the testing harness to measure how long your test suite takes to execute.
