# Struct-style composition

This example demonstrates how idiomatic Koto code can mimic traditional structs. The `make_vector` factory builds a map with stored data (`x` and `y`) plus functions that treat the map as an object using `self`.

## Step-by-step
1. `make_vector` constructs a map containing coordinates, helper methods, and an `@display` metamethod for readable output.
2. The script creates a vector at the origin and prints it, invoking the custom `@display` behavior.
3. Calling `move` mutates the internal `x` and `y` values before reporting the updated vector and its computed length.

## Experiment ideas
- Extend the map with additional helpers such as `dot` or `normalize` methods.
- Swap the `move` implementation for an immutable version that returns a new map instead of mutating state.
- Combine the vector with other maps to see how nested structures behave when returned from the script.
