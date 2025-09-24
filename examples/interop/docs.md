# Interop walkthrough

The interop example shows how Koto scripts can talk to host functions exposed from Rust. All bindings used here originate from the runtime, including a UUID generator powered by the `uuid` crate.

## Step-by-step
1. `host.echo` round-trips a string from Koto into Rust and back again.
2. `host.now` returns a timestamp (seconds since the Unix epoch) computed in Rust.
3. `host.uuid_v4` surfaces an external dependency: the UUID crate generates a unique identifier for every run.
4. `host.log_info` emits a tracing event so you can inspect the runtime log file.

## Experiment ideas
- Replace the hard-coded message with user input from the UI to see how host functions handle different values.
- Call `host.uuid_v4` multiple times and compare the results for uniqueness.
- Tail the runtime log file while running the example to observe the effect of `host.log_info`.
