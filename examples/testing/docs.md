# Testing from scripts

The testing example shows how to orchestrate unit-style checks entirely within Koto. It calls into the `test` module manually so the runtime can run the assertions even when automatic test discovery is disabled.

## Step-by-step
1. Create a simple counter map whose methods mutate shared state.
2. Describe hooks and test cases inside a map that exports `@pre_test`, `@post_test`, and individual `@test` functions.
3. Call `test.run_tests` to execute the suite and collect output.
4. Each hook uses `host.log_info` so you can inspect the runtime log files after a run.

## Experiment ideas
- Add failing assertions to see how the runtime reports errors.
- Extend the counter with additional behavior and accompanying tests.
- Integrate with the performance example by timing how long the tests take to run.
