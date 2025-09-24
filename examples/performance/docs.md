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

## Benchmarks

Run `cargo bench` to generate repeatable Criterion measurements that mirror this example. The benchmark harness runs the
recursive Koto implementation through `runtime::Executor` alongside an equivalent Rust helper so you can compare their mean
execution times. Results are written to `target/criterion/performance/`, and the HTML report at
`target/criterion/performance/report/index.html` provides trend charts and distribution plots.

Each row in the generated summary table shows the benchmark name, the input (e.g. `n=24`), the mean duration in milliseconds,
and the 95% confidence interval. Hover over a mean value in the app to view the standard deviation, or open the HTML report for
interactive charts. Enable longer-running workloads with the `--features bench-extended` flag or by setting the
`KOTO_BENCH_EXTENDED=1` environment variable before invoking `cargo bench`.
