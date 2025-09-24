use std::time::Duration;

use criterion::{BenchmarkId, Criterion, black_box, criterion_group, criterion_main};
use koto_learning::runtime::Executor;

fn performance_benchmarks(c: &mut Criterion) {
    let mut group = c.benchmark_group("performance");
    group.sample_size(30);
    group.measurement_time(Duration::from_secs(4));

    let executor = Executor::default();
    let scripts: Vec<(u32, String)> = fibonacci_inputs()
        .into_iter()
        .map(|n| (n, fibonacci_script(n)))
        .collect();

    for (n, script) in &scripts {
        let benchmark_id = BenchmarkId::new("koto_recursive_fib", format!("n={n}"));
        let exec = executor;
        group.bench_with_input(benchmark_id, script, |b, script| {
            b.iter(|| {
                let value = run_koto_fibonacci(exec, script);
                black_box(value)
            });
        });

        let benchmark_id = BenchmarkId::new("rust_iterative_fib", format!("n={n}"));
        group.bench_with_input(benchmark_id, n, |b, &n| {
            b.iter(|| black_box(rust_fibonacci(n)));
        });
    }

    group.finish();
}

fn run_koto_fibonacci(executor: Executor, script: &str) -> i64 {
    let output = executor
        .execute_script(script)
        .expect("failed to execute Koto fibonacci script");
    let value = output.return_value.expect("Koto script returned no value");
    value
        .trim()
        .parse::<i64>()
        .expect("unexpected non-numeric fibonacci result")
}

fn rust_fibonacci(n: u32) -> u128 {
    let mut a: u128 = 0;
    let mut b: u128 = 1;
    for _ in 0..n {
        let next = a + b;
        a = b;
        b = next;
    }
    a
}

fn fibonacci_script(n: u32) -> String {
    format!("fib = |n|\n  if n <= 1\n    n\n  else\n    fib(n - 1) + fib(n - 2)\n\nfib({n})\n")
}

fn fibonacci_inputs() -> Vec<u32> {
    let mut inputs = vec![20, 24];
    if extended_inputs_requested() {
        inputs.extend([28, 32]);
    }
    inputs
}

fn extended_inputs_requested() -> bool {
    cfg!(feature = "bench-extended") || std::env::var_os("KOTO_BENCH_EXTENDED").is_some()
}

criterion_group!(benches, performance_benchmarks);
criterion_main!(benches);
