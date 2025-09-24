use std::{fs, path::PathBuf, time::Duration};

use koto::prelude::runtime_error;
use koto_learning::{
    examples::{ExampleLibrary, ScriptChangeKind, tests as example_tests},
    runtime::Runtime,
};
use tempfile::tempdir;

#[test]
fn example_library_loads_and_refreshes() {
    let temp = tempdir().expect("temp dir");
    let base = temp.path();
    let example_dir = base.join("demo");
    fs::create_dir_all(&example_dir).unwrap();
    fs::write(
        example_dir.join("meta.json"),
        r#"{"id":"demo","title":"Demo","description":"Test example"}"#,
    )
    .unwrap();
    fs::write(example_dir.join("script.koto"), "print(\"hello\")\n42").unwrap();

    let library = ExampleLibrary::new_unwatched(base.to_path_buf()).expect("library");
    let snapshot = library.snapshot();
    assert_eq!(snapshot.len(), 1);
    let example = &snapshot[0];
    assert_eq!(example.metadata.id, "demo");
    assert_eq!(example.metadata.categories.len(), 0);
    assert!(example.script.contains("hello"));

    fs::write(example_dir.join("script.koto"), "1 + 1").unwrap();
    library.refresh().unwrap();
    let refreshed = library.get("demo").expect("refreshed example");
    assert!(refreshed.script.contains("1 + 1"));
}

#[test]
fn runtime_executes_and_captures_output() {
    let runtime = Runtime::new().expect("runtime");
    let output = runtime
        .execute_script("print(\"testing\")\n1 + 2")
        .expect("script execution");
    assert_eq!(output.return_value.as_deref(), Some("3"));
    assert!(output.stdout.contains("testing"));
    assert!(output.stderr.is_empty());
}

#[test]
fn runtime_reports_script_errors() {
    let runtime = Runtime::new().expect("runtime");
    let error = runtime.execute_script("unknown_function() ").unwrap_err();
    assert!(error.to_string().contains("unknown_function"));
}

#[test]
fn runtime_supports_host_functions() {
    let runtime = Runtime::new().expect("runtime");
    runtime
        .register_host_function("greet", |ctx| match ctx.args() {
            [koto::prelude::KValue::Str(name), ..] => {
                Ok(format!("Hello {}!", name.as_str()).into())
            }
            _ => runtime_error!("expected name"),
        })
        .expect("register host function");

    let output = runtime
        .execute_script("greet(\"Runtime\")")
        .expect("script execution");
    assert_eq!(output.return_value.as_deref(), Some("Hello Runtime!"));
}

#[test]
fn runtime_provides_serialization_helpers() {
    let runtime = Runtime::new().expect("runtime");
    let output = runtime
        .execute_script("serde.to_json({ greeting: \"hi\" })")
        .expect("serialization helpers");
    let value = output.return_value.expect("json string");
    assert!(value.contains("greeting"));
}

#[test]
fn runtime_honors_execution_timeout_updates() {
    let runtime = Runtime::new().expect("runtime");
    runtime
        .set_execution_timeout(Some(Duration::from_millis(50)))
        .expect("set timeout");
    runtime.execute_script("1").expect("script");
}

#[test]
fn runtime_reports_missing_shared_library() {
    let runtime = Runtime::new().expect("runtime");
    let result = runtime.load_shared_library("nonexistent_library.so");
    assert!(result.is_err());
}

#[test]
fn test_suite_runner_reports_results() {
    let script = r#"
# Title: Sample suite
# Description: Exercises pass/fail status and captured output.

print('setup output')

export tests =
  @pre_test: || print('pre hook ran')
  @post_test: || print('post hook ran')
  @test passes: || 1
  @test fails: || throw 'boom'
"#;

    let suite = example_tests::ExampleTestSuite {
        id: "sample".to_string(),
        name: "Sample suite".to_string(),
        description: Some("Exercises pass/fail status and captured output.".to_string()),
        path: PathBuf::from("sample.koto"),
        script: script.to_string(),
    };

    let result = example_tests::run_suite(&suite).expect("suite run");
    assert_eq!(result.suite_id, "sample");
    assert_eq!(result.cases.len(), 2);
    assert!(result.setup_stdout.contains("setup output"));
    assert!(result.setup_stderr.is_empty());

    let pass_case = &result.cases[0];
    assert_eq!(pass_case.name, "passes");
    assert_eq!(pass_case.status, example_tests::TestStatus::Passed);
    assert!(pass_case.error.is_none());

    let fail_case = &result.cases[1];
    assert_eq!(fail_case.name, "fails");
    assert_eq!(fail_case.status, example_tests::TestStatus::Failed);
    assert!(
        fail_case
            .error
            .as_ref()
            .map(|error| error.contains("boom"))
            .unwrap_or(false)
    );
}

#[test]
fn example_library_tracks_script_and_test_changes() {
    let temp = tempdir().expect("temp dir");
    let base = temp.path();
    let example_dir = base.join("demo");
    let tests_dir = example_dir.join("tests");
    fs::create_dir_all(&tests_dir).unwrap();

    let initial_script = "print(\"hi\")\n1";
    fs::write(
        example_dir.join("meta.json"),
        r#"{"id":"demo","title":"Demo","description":"Test example"}"#,
    )
    .unwrap();
    fs::write(example_dir.join("script.koto"), initial_script).unwrap();
    let suite_path = tests_dir.join("sample.koto");
    fs::write(&suite_path, "tests =\n  @test pass: || 1\nexport tests\n").unwrap();

    let library = ExampleLibrary::new_unwatched(base.to_path_buf()).expect("library");
    // Drain initial load notifications.
    let _ = library.take_recent_changes();

    // Modify the script file.
    fs::write(example_dir.join("script.koto"), "print(\"hi\")\n2").unwrap();
    library.refresh().unwrap();
    let changes = library.take_recent_changes();
    let script_change = changes
        .iter()
        .find(|change| matches!(change.kind, ScriptChangeKind::ScriptUpdated { .. }))
        .cloned()
        .expect("script change");
    match &script_change.kind {
        ScriptChangeKind::ScriptUpdated { previous, current } => {
            assert!(
                previous
                    .as_ref()
                    .map(|text| text.contains(initial_script))
                    .unwrap_or(false)
            );
            assert!(
                current
                    .as_ref()
                    .map(|text| text.contains("2"))
                    .unwrap_or(false)
            );
        }
        _ => unreachable!(),
    }

    library.revert_change(&script_change).unwrap();
    let reverted_script = fs::read_to_string(example_dir.join("script.koto")).unwrap();
    assert!(reverted_script.contains(initial_script));

    // Update the test suite file.
    fs::write(
        &suite_path,
        "tests =\n  @test pass: || 1\n  @test another: || throw 'nope'\nexport tests\n",
    )
    .unwrap();
    library.refresh().unwrap();
    let changes = library.take_recent_changes();
    let suite_change = changes
        .into_iter()
        .find(|change| matches!(change.kind, ScriptChangeKind::TestSuiteUpdated { .. }))
        .expect("suite change");
    match &suite_change.kind {
        ScriptChangeKind::TestSuiteUpdated {
            previous, current, ..
        } => {
            assert!(
                previous
                    .as_ref()
                    .map(|text| text.contains("pass"))
                    .unwrap_or(false)
            );
            assert!(
                current
                    .as_ref()
                    .map(|text| text.contains("another"))
                    .unwrap_or(false)
            );
        }
        _ => unreachable!(),
    }

    library.revert_change(&suite_change).unwrap();
    let reverted_suite = fs::read_to_string(&suite_path).unwrap();
    assert!(reverted_suite.contains("@test pass"));
    assert!(!reverted_suite.contains("another"));
}
