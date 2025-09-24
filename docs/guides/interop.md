# Host interop guide

Rust integrations are surfaced through the `host` module that the runtime injects into every script. The [`interop` example](../../examples/interop/docs.md) demonstrates how to call these bindings, including functionality that depends on external crates such as `uuid`.

## Execute the interop example
1. Open **Rust Interop** in the explorer and run the script.
2. Observe the stdout entries for `host.echo`, `host.now`, and the generated UUID.
3. Check `logs/runtime.log` (created by the runtime) to confirm that `host.log_info` emitted tracing messages.

## Experiment further
- Call `host.uuid_v4` multiple times inside the script to ensure each run returns a unique identifier.
- Pipe values returned from `host.echo` into other functions or data structures to understand how Koto values cross the boundary.
- Combine the interop script with the serialization helpers to capture host data and export it as JSON or YAML.
