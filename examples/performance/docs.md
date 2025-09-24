# Performance comparison

This example compares the same algorithm implemented in Koto and in Rust. Koto runs the canonical recursive Fibonacci function while Rust exposes a fast helper along with a millisecond timer.

## Step-by-step
1. Define `fib` in Koto using naive recursion to emphasize language clarity over raw speed.
2. Use the `measure` helper to time a call by sampling the millisecond clock from `host.performance.now_ms` before and after executing an action.
3. Invoke `host.performance.fast_fib` to retrieve a result calculated in Rust for a larger input.
4. Return a map that captures the measured timings for both implementations so you can inspect them side by side.

## Experiment ideas
- Increase `small_target` and `large_target` to see how recursion scales compared to the Rust helper.
- Replace the recursive Koto implementation with an iterative approach to close the performance gap.
- Plot the recorded timings across several runs by exporting the returned map to a CSV or JSON file.
