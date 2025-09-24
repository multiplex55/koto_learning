# Serialization guide

This example covers the built-in `serde` module that the runtime registers for every script. The helper functions convert between native Koto values and JSON/YAML text using serde under the hood.

## Step-by-step
1. Compose a nested map with lists, numbers, and booleans in pure Koto code.
2. Convert the map to JSON and YAML strings via `serde.to_json` and `serde.to_yaml`.
3. Parse the text back to Koto values with `serde.from_json` and `serde.from_yaml` to prove that round-tripping preserves the structure.

## Experiment ideas
- Add optional fields to the payload and see how they appear in the exported formats.
- Pipe the generated text into a file using `io` helpers for later consumption.
- Compare the JSON and YAML representations to understand when each format is most readable.
