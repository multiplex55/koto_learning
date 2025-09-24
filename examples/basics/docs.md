# Basics walkthrough

The basics example highlights core Koto language features: defining functions, looping through lists, and composing maps that hold both data and behavior. Running the script prints a welcome message and reports the average score from a configurable list.

## Step-by-step
1. The script defines `greet`, a simple function that interpolates a name into a string.
2. A list of `scores` is processed with a `for` loop to accumulate a running total and count.
3. The `profile` map stores values alongside the `describe` function, demonstrating how maps can encapsulate state and methods.
4. A friendly message is printed and the script returns a summary map containing the computed average and formatted text.

## Experiment ideas
- Change the numbers in `scores` to see how the average and summary string react.
- Add new keys to the `profile` map and access them from the returned result.
- Replace the loop with iterator helpers from the standard library once you are comfortable with the syntax.
