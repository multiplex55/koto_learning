use std::{
    fs,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use anyhow::{Context, Result};
use koto::prelude::*;

use crate::runtime::{self, Runtime};

#[derive(Clone, Debug)]
pub struct ExampleTestSuite {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub path: PathBuf,
    pub script: String,
}

#[derive(Clone, Debug)]
pub struct TestSuiteResult {
    pub suite_id: String,
    pub suite_name: String,
    pub description: Option<String>,
    pub path: PathBuf,
    pub setup_stdout: String,
    pub setup_stderr: String,
    pub cases: Vec<TestCaseResult>,
    pub total_duration: Duration,
    pub passed: bool,
}

#[derive(Clone, Debug)]
pub struct TestCaseResult {
    pub name: String,
    pub status: TestStatus,
    pub duration: Duration,
    pub stdout: String,
    pub stderr: String,
    pub error: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TestStatus {
    Passed,
    Failed,
}

pub fn load_suites(example_dir: &Path) -> Result<Vec<ExampleTestSuite>> {
    let tests_dir = example_dir.join("tests");
    if !tests_dir.exists() {
        return Ok(Vec::new());
    }

    let mut suites = Vec::new();

    for entry in fs::read_dir(&tests_dir).with_context(|| {
        format!(
            "Failed to read tests directory for {:?}",
            example_dir.display()
        )
    })? {
        let entry = entry?;
        let path = entry.path();
        if entry.file_type()?.is_dir() {
            continue;
        }
        if path.extension().and_then(|ext| ext.to_str()) != Some("koto") {
            continue;
        }

        let script = fs::read_to_string(&path)
            .with_context(|| format!("Failed to read test script {path:?}"))?;
        let id = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| "suite".to_string());
        let metadata = parse_metadata(&script, &id);

        suites.push(ExampleTestSuite {
            id,
            name: metadata.name,
            description: metadata.description,
            path,
            script,
        });
    }

    suites.sort_by(|a, b| a.name.cmp(&b.name));

    Ok(suites)
}

pub fn run_suite(suite: &ExampleTestSuite) -> Result<TestSuiteResult> {
    runtime::logging::with_runtime_subscriber(|| {
        tracing::info!(
            target: "runtime.tests",
            suite = suite.id.as_str(),
            path = %suite.path.display(),
            "Running test suite",
        );
    });

    let runtime = Runtime::new().context("Failed to initialize runtime for tests")?;
    let execution = runtime
        .execute_script(&suite.script)
        .with_context(|| format!("Failed to evaluate test suite '{}'", suite.name))?;

    let cases = runtime.with_koto(|koto| execute_suite_cases(&runtime, koto, suite))?;
    let total_duration = cases.iter().map(|case| case.duration).sum();
    let passed = cases.iter().all(|case| case.status == TestStatus::Passed);

    runtime::logging::with_runtime_subscriber(|| {
        tracing::info!(
            target: "runtime.tests",
            suite = suite.id.as_str(),
            case_count = cases.len(),
            passed,
            "Test suite finished",
        );
    });

    Ok(TestSuiteResult {
        suite_id: suite.id.clone(),
        suite_name: suite.name.clone(),
        description: suite.description.clone(),
        path: suite.path.clone(),
        setup_stdout: execution.stdout,
        setup_stderr: execution.stderr,
        cases,
        total_duration,
        passed,
    })
}

pub fn run_suites(suites: &[ExampleTestSuite]) -> Result<Vec<TestSuiteResult>> {
    suites.iter().map(run_suite).collect()
}

fn execute_suite_cases(
    runtime: &Runtime,
    koto: &mut Koto,
    suite: &ExampleTestSuite,
) -> Result<Vec<TestCaseResult>> {
    let mut test_maps = Vec::new();

    for (key, value) in koto.exports().data().iter() {
        if let KValue::Map(map) = value {
            if map_contains_tests(map) {
                test_maps.push((key.to_string(), map.clone()));
            }
        }
    }

    let (entry_name, tests_map) = test_maps
        .into_iter()
        .next()
        .ok_or_else(|| anyhow::anyhow!("No @test definitions were exported by '{}'", suite.name))?;

    runtime::logging::with_runtime_subscriber(|| {
        tracing::debug!(
            target: "runtime.tests",
            suite = suite.id.as_str(),
            entry = entry_name,
            "Discovered test map",
        );
    });

    run_cases(runtime, koto, &tests_map)
}

fn run_cases(runtime: &Runtime, koto: &mut Koto, tests: &KMap) -> Result<Vec<TestCaseResult>> {
    use TestStatus::{Failed, Passed};

    let (pre_test, post_test, meta_entry_count) = match tests.meta_map() {
        Some(meta) => {
            let meta = meta.borrow();
            (
                meta.get(&MetaKey::PreTest).cloned(),
                meta.get(&MetaKey::PostTest).cloned(),
                meta.len(),
            )
        }
        None => (None, None, 0),
    };

    let mut cases = Vec::new();
    let self_arg = KValue::Map(tests.clone());

    for index in 0..meta_entry_count {
        let meta_entry = tests.meta_map().and_then(|meta| {
            meta.borrow()
                .get_index(index)
                .map(|(key, value)| (key.clone(), value.clone()))
        });

        let Some((MetaKey::Test(test_name), test_fn)) = meta_entry else {
            continue;
        };

        let mut status = Passed;
        let mut error = None;
        runtime.clear_output();
        let start = Instant::now();

        if let Some(pre) = pre_test.clone() {
            if let Err(message) = call_stage(koto, &self_arg, &pre) {
                status = Failed;
                error = Some(format!("pre-test failed: {message}"));
            }
        }

        if status == Passed {
            if let Err(message) = call_stage(koto, &self_arg, &test_fn) {
                status = Failed;
                error = Some(message);
            }
        }

        if status == Passed {
            if let Some(post) = post_test.clone() {
                if let Err(message) = call_stage(koto, &self_arg, &post) {
                    status = Failed;
                    error = Some(format!("post-test failed: {message}"));
                }
            }
        }

        let duration = start.elapsed();
        let stdout = runtime.take_stdout();
        let stderr = runtime.take_stderr();

        cases.push(TestCaseResult {
            name: test_name.to_string(),
            status,
            duration,
            stdout,
            stderr,
            error,
        });
    }

    Ok(cases)
}

fn call_stage(koto: &mut Koto, instance: &KValue, function: &KValue) -> Result<(), String> {
    if !function.is_callable() {
        return Err("stage is not callable".to_string());
    }

    koto.call_instance_function(instance.clone(), function.clone(), &[])
        .map(|_| ())
        .map_err(|error| error.to_string())
}

fn map_contains_tests(map: &KMap) -> bool {
    map.meta_map().map_or(false, |meta| {
        meta.borrow()
            .iter()
            .any(|(key, _)| matches!(key, MetaKey::Test(_)))
    })
}

fn parse_metadata(script: &str, fallback_id: &str) -> SuiteMetadata {
    let mut name = None;
    let mut description = None;

    for line in script.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if !trimmed.starts_with('#') {
            break;
        }
        let content = trimmed.trim_start_matches('#').trim();
        if let Some(rest) = content.strip_prefix("Title:") {
            name = Some(rest.trim().to_string());
        } else if let Some(rest) = content.strip_prefix("Description:") {
            description = Some(rest.trim().to_string());
        }
    }

    SuiteMetadata {
        name: name.unwrap_or_else(|| fallback_id.to_string()),
        description,
    }
}

struct SuiteMetadata {
    name: String,
    description: Option<String>,
}
