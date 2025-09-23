use std::{fs, time::Duration};

use koto::prelude::runtime_error;
use koto_learning::{examples::ExampleLibrary, runtime::Runtime};
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
