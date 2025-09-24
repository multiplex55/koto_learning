# koto_learning

An interactive Rust application for learning the [Koto](https://koto.dev) scripting language and showcasing Rhai-like scripting capabilities embedded in a native UI.

## Prerequisites

Install the Rust toolchain using [rustup](https://rustup.rs/):

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

## Building

Compile the project with Cargo:

```bash
cargo build
```

## Running

Launch the explorer application:

```bash
cargo run
```

For the best runtime performance when exploring the examples, build and launch the
release profile:

```bash
cargo run --release
```

The desktop UI is powered by `eframe`, so the same command works across Windows,
macOS, and Linux environments with the standard Rust toolchain.

## Benchmarks

Use Criterion to measure the bundled performance examples:

```bash
cargo bench
```

The Fibonacci benchmark exercises both the recursive Koto script (through the runtime `Executor`) and a pure Rust helper, then
stores results under `target/criterion/performance/`. Open `target/criterion/performance/report/index.html` for interactive
charts, and inspect the "Benchmarks" panel in the UI to view the aggregated mean times and confidence intervals. Enable
additional, longer-running workloads with either `cargo bench --features bench-extended` or by setting
`KOTO_BENCH_EXTENDED=1` before running the command.

## Project Goals

- Provide a desktop shell for exploring the Koto runtime interactively.
- Collect and showcase examples that highlight expressive, Rhai-like scripting patterns in Koto.
