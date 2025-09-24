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

## Project Goals

- Provide a desktop shell for exploring the Koto runtime interactively.
- Collect and showcase examples that highlight expressive, Rhai-like scripting patterns in Koto.
