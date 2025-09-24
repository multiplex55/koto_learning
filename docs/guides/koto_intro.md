# Getting started with Koto

The example explorer ships with a curated set of scripts that highlight core language features. Start with the [`basics` example](../../examples/basics/docs.md) to see variables, loops, and map composition in action, then move on to [`structs`](../../examples/structs/docs.md) to learn how maps can feel like lightweight structs.

## Running the basics example
1. Open the **Language Basics** entry in the UI.
2. Click **Run Example** to execute `examples/basics/script.koto`.
3. Review stdout to watch the greeting and average calculations update.
4. Edit the `scores` list in the script and re-run to observe how the returned map changes.

## Extending with struct-style code
1. Switch to the **Struct-like Maps** example.
2. Run the script to print vectors before and after calling `move`.
3. Experiment with the function definitions inside `make_vector` to add new behavior such as `scale` or `dot`.
4. Notice how the `@display` metamethod controls the rendering of the returned map in the console and logs.

Use these two examples together to build an intuition for how functions, lists, and maps compose inside Koto scripts.
