# Serialization guide

Koto scripts gain JSON and YAML support via the runtime's `serde` bindings. The [`serialization` example](../../examples/serialization/docs.md) provides a ready-made payload that can be tweaked to see how conversions behave.

## Run the serialization example
1. Select **JSON and YAML** in the explorer and execute it.
2. Review stdout for the JSON block followed by the YAML block.
3. Inspect the return value in the UI to confirm that round-tripping produced the same nested map.

## Experiment further
- Extend the payload with nested lists or optional values to observe how the serializers handle them.
- Serialize the same data twice and compare the output ordering to understand how maps are rendered.
- Pipe the resulting strings into files for interoperability tests with other tools.
