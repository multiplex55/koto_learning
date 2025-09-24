use std::{
    collections::{BTreeMap, HashMap},
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, RwLock},
    time::SystemTime,
};

use anyhow::{Context, Result};
use notify::EventKind;
use once_cell::sync::OnceCell;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::{
    benchmarks,
    runtime::{logging, watcher},
};

pub mod tests;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ExampleMetadata {
    #[serde(default)]
    pub id: String,
    pub title: String,
    pub description: String,
    #[serde(default)]
    pub note: Option<String>,
    #[serde(default)]
    pub doc_url: Option<String>,
    #[serde(default)]
    pub run_instructions: Option<String>,
    #[serde(default)]
    pub categories: Vec<String>,
    #[serde(default)]
    pub documentation: Vec<ExampleLink>,
    #[serde(default)]
    pub how_it_works: Vec<String>,
    #[serde(default)]
    pub inputs: Vec<ExampleInput>,
    #[serde(default)]
    pub benchmarks: Option<ExampleResource>,
    #[serde(default)]
    pub tests: Option<ExampleResource>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ExampleLink {
    pub label: String,
    pub url: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ExampleInput {
    pub name: String,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub default: Option<String>,
    #[serde(default)]
    pub placeholder: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ExampleResource {
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
}

#[derive(Clone, Debug)]
pub struct ExampleDocs {
    pub path: PathBuf,
    pub summary: String,
}

#[derive(Clone, Debug)]
pub struct Example {
    pub metadata: ExampleMetadata,
    pub script: String,
    pub script_path: PathBuf,
    pub docs: Option<ExampleDocs>,
    pub loaded_at: SystemTime,
    pub benchmark_summary: Option<benchmarks::ExampleBenchmarkSummary>,
    pub test_suites: Vec<tests::ExampleTestSuite>,
}

pub struct ExampleLibrary {
    inner: Arc<ExampleLibraryInner>,
    _watcher: Option<watcher::Watcher>,
}

struct ExampleLibraryInner {
    examples_dir: PathBuf,
    examples: RwLock<BTreeMap<String, Example>>,
    version: AtomicUsize,
    recent_changes: Mutex<Vec<ScriptChange>>,
}

#[derive(Clone, Debug)]
pub struct ScriptChange {
    pub example_id: String,
    pub path: PathBuf,
    pub changed_at: SystemTime,
    pub kind: ScriptChangeKind,
}

#[derive(Clone, Debug)]
pub enum ScriptChangeKind {
    ScriptUpdated {
        previous: Option<String>,
        current: Option<String>,
    },
    TestSuiteUpdated {
        suite_id: String,
        previous: Option<String>,
        current: Option<String>,
    },
}

static GLOBAL_LIBRARY: OnceCell<ExampleLibrary> = OnceCell::new();

pub fn library() -> Result<&'static ExampleLibrary> {
    GLOBAL_LIBRARY.get_or_try_init(|| ExampleLibrary::new(default_examples_dir()))
}

impl ExampleLibrary {
    pub fn new(examples_dir: PathBuf) -> Result<Self> {
        Self::with_watcher(examples_dir, true)
    }

    pub fn new_unwatched(examples_dir: PathBuf) -> Result<Self> {
        Self::with_watcher(examples_dir, false)
    }

    pub fn refresh(&self) -> Result<()> {
        self.inner.reload()
    }

    pub fn snapshot(&self) -> Vec<Example> {
        self.inner.snapshot()
    }

    pub fn version(&self) -> usize {
        self.inner.version.load(Ordering::SeqCst)
    }

    pub fn get(&self, id: &str) -> Option<Example> {
        let guard = self.inner.examples.read().ok()?;
        let mut example = guard.get(id).cloned()?;
        example.benchmark_summary = benchmarks::load_example_summary(&example.metadata.id);
        Some(example)
    }

    pub fn take_recent_changes(&self) -> Vec<ScriptChange> {
        self.inner.take_recent_changes()
    }

    pub fn revert_change(&self, change: &ScriptChange) -> Result<()> {
        self.inner.revert_change(change)
    }

    fn with_watcher(examples_dir: PathBuf, watch: bool) -> Result<Self> {
        fs::create_dir_all(&examples_dir)
            .with_context(|| format!("Failed to ensure examples dir {examples_dir:?}"))?;

        let inner = Arc::new(ExampleLibraryInner::new(examples_dir.clone())?);

        let watcher = if watch {
            let inner = Arc::clone(&inner);
            Some(watcher::Watcher::new(examples_dir.clone(), move |event| {
                handle_watch_event(&inner, event);
            })?)
        } else {
            None
        };

        logging::with_runtime_subscriber(|| {
            tracing::info!(
                target: "runtime.examples",
                path = %examples_dir.display(),
                count = inner.snapshot().len(),
                "Example library initialized"
            );
        });

        Ok(Self {
            inner,
            _watcher: watcher,
        })
    }
}

impl ExampleLibraryInner {
    fn new(examples_dir: PathBuf) -> Result<Self> {
        let library = Self {
            examples_dir,
            examples: RwLock::new(BTreeMap::new()),
            version: AtomicUsize::new(0),
            recent_changes: Mutex::new(Vec::new()),
        };
        library.reload()?;
        Ok(library)
    }

    fn reload(&self) -> Result<()> {
        let new_examples = load_examples_from_dir(&self.examples_dir)?;
        let count = new_examples.len();
        let mut changes = Vec::new();
        if let Ok(mut guard) = self.examples.write() {
            let old = std::mem::replace(&mut *guard, new_examples);
            changes = diff_examples(&old, &*guard);
        }
        self.version.fetch_add(1, Ordering::SeqCst);
        if !changes.is_empty() {
            if let Ok(mut queue) = self.recent_changes.lock() {
                queue.extend(changes);
            }
        }
        logging::with_runtime_subscriber(|| {
            tracing::info!(
                target: "runtime.examples",
                path = %self.examples_dir.display(),
                count,
                "Reloaded examples"
            );
        });
        Ok(())
    }

    fn take_recent_changes(&self) -> Vec<ScriptChange> {
        self.recent_changes
            .lock()
            .map(|mut guard| guard.drain(..).collect())
            .unwrap_or_default()
    }

    fn revert_change(&self, change: &ScriptChange) -> Result<()> {
        match &change.kind {
            ScriptChangeKind::ScriptUpdated {
                previous,
                current: _,
            } => {
                apply_revert(change.path.as_path(), previous)?;
            }
            ScriptChangeKind::TestSuiteUpdated { previous, .. } => {
                apply_revert(change.path.as_path(), previous)?;
            }
        }
        Ok(())
    }

    fn snapshot(&self) -> Vec<Example> {
        self.examples
            .read()
            .map(|examples| {
                examples
                    .values()
                    .cloned()
                    .map(|mut example| {
                        example.benchmark_summary =
                            benchmarks::load_example_summary(&example.metadata.id);
                        example
                    })
                    .collect()
            })
            .unwrap_or_default()
    }
}

fn apply_revert(path: &Path, previous: &Option<String>) -> Result<()> {
    match previous {
        Some(content) => {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).with_context(|| {
                    format!("Failed to recreate parent directory for {:?}", path)
                })?;
            }
            fs::write(path, content)
                .with_context(|| format!("Failed to restore script at {:?}", path))?
        }
        None => {
            if path.exists() {
                fs::remove_file(path)
                    .with_context(|| format!("Failed to remove file at {:?}", path))?;
            }
        }
    }
    Ok(())
}

fn diff_examples(
    old: &BTreeMap<String, Example>,
    new: &BTreeMap<String, Example>,
) -> Vec<ScriptChange> {
    let mut changes = Vec::new();

    for (id, new_example) in new {
        match old.get(id) {
            Some(old_example) => {
                if old_example.script != new_example.script {
                    changes.push(ScriptChange {
                        example_id: id.clone(),
                        path: new_example.script_path.clone(),
                        changed_at: SystemTime::now(),
                        kind: ScriptChangeKind::ScriptUpdated {
                            previous: Some(old_example.script.clone()),
                            current: Some(new_example.script.clone()),
                        },
                    });
                }

                let old_suites: HashMap<_, _> = old_example
                    .test_suites
                    .iter()
                    .map(|suite| (suite.id.clone(), suite))
                    .collect();
                let new_suites: HashMap<_, _> = new_example
                    .test_suites
                    .iter()
                    .map(|suite| (suite.id.clone(), suite))
                    .collect();

                for (suite_id, suite) in &new_suites {
                    match old_suites.get(suite_id) {
                        Some(previous) => {
                            if previous.script != suite.script {
                                changes.push(ScriptChange {
                                    example_id: id.clone(),
                                    path: suite.path.clone(),
                                    changed_at: SystemTime::now(),
                                    kind: ScriptChangeKind::TestSuiteUpdated {
                                        suite_id: suite_id.clone(),
                                        previous: Some(previous.script.clone()),
                                        current: Some(suite.script.clone()),
                                    },
                                });
                            }
                        }
                        None => {
                            changes.push(ScriptChange {
                                example_id: id.clone(),
                                path: suite.path.clone(),
                                changed_at: SystemTime::now(),
                                kind: ScriptChangeKind::TestSuiteUpdated {
                                    suite_id: suite_id.clone(),
                                    previous: None,
                                    current: Some(suite.script.clone()),
                                },
                            });
                        }
                    }
                }

                for (suite_id, suite) in old_suites {
                    if !new_suites.contains_key(&suite_id) {
                        changes.push(ScriptChange {
                            example_id: id.clone(),
                            path: suite.path.clone(),
                            changed_at: SystemTime::now(),
                            kind: ScriptChangeKind::TestSuiteUpdated {
                                suite_id,
                                previous: Some(suite.script.clone()),
                                current: None,
                            },
                        });
                    }
                }
            }
            None => {
                changes.push(ScriptChange {
                    example_id: id.clone(),
                    path: new_example.script_path.clone(),
                    changed_at: SystemTime::now(),
                    kind: ScriptChangeKind::ScriptUpdated {
                        previous: None,
                        current: Some(new_example.script.clone()),
                    },
                });
                for suite in &new_example.test_suites {
                    changes.push(ScriptChange {
                        example_id: id.clone(),
                        path: suite.path.clone(),
                        changed_at: SystemTime::now(),
                        kind: ScriptChangeKind::TestSuiteUpdated {
                            suite_id: suite.id.clone(),
                            previous: None,
                            current: Some(suite.script.clone()),
                        },
                    });
                }
            }
        }
    }

    for (id, old_example) in old {
        if !new.contains_key(id) {
            changes.push(ScriptChange {
                example_id: id.clone(),
                path: old_example.script_path.clone(),
                changed_at: SystemTime::now(),
                kind: ScriptChangeKind::ScriptUpdated {
                    previous: Some(old_example.script.clone()),
                    current: None,
                },
            });
            for suite in &old_example.test_suites {
                changes.push(ScriptChange {
                    example_id: id.clone(),
                    path: suite.path.clone(),
                    changed_at: SystemTime::now(),
                    kind: ScriptChangeKind::TestSuiteUpdated {
                        suite_id: suite.id.clone(),
                        previous: Some(suite.script.clone()),
                        current: None,
                    },
                });
            }
        }
    }

    changes
}

fn handle_watch_event(inner: &Arc<ExampleLibraryInner>, event: watcher::WatchEvent) {
    match event {
        watcher::WatchEvent::FileEvent { event, .. } if should_reload(&event.kind) => {
            if let Err(error) = inner.reload() {
                logging::with_runtime_subscriber(|| {
                    tracing::error!(target: "runtime.examples", error = %error, "Failed to reload examples");
                });
            } else {
                logging::with_runtime_subscriber(|| {
                    tracing::debug!(target: "runtime.examples", ?event, "Example directory change detected");
                });
            }
        }
        watcher::WatchEvent::FileEvent { .. } => {}
        watcher::WatchEvent::Error { error } => {
            logging::with_runtime_subscriber(|| {
                tracing::error!(target: "runtime.examples", %error, "File watcher error");
            });
        }
    }
}

fn should_reload(kind: &EventKind) -> bool {
    matches!(
        kind,
        EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_) | EventKind::Any
    )
}

fn load_examples_from_dir(dir: &Path) -> Result<BTreeMap<String, Example>> {
    let mut examples = BTreeMap::new();

    if !dir.exists() {
        return Ok(examples);
    }

    for entry in fs::read_dir(dir).with_context(|| format!("Failed to read {dir:?}"))? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let folder_name = entry.file_name().to_string_lossy().to_string();
        let example_dir = entry.path();
        let meta_path = example_dir.join("meta.json");
        let script_path = example_dir.join("script.koto");

        match (
            fs::read_to_string(&meta_path),
            fs::read_to_string(&script_path),
        ) {
            (Ok(meta_content), Ok(script_content)) => {
                match serde_json::from_str::<ExampleMetadata>(&meta_content) {
                    Ok(mut metadata) => {
                        if metadata.id.is_empty() {
                            metadata.id = folder_name.clone();
                        }
                        let test_suites = match tests::load_suites(&example_dir) {
                            Ok(suites) => suites,
                            Err(error) => {
                                logging::with_runtime_subscriber(|| {
                                    tracing::warn!(
                                        target: "runtime.examples",
                                        path = %example_dir.display(),
                                        %error,
                                        "Failed to load test suites",
                                    );
                                });
                                Vec::new()
                            }
                        };
                        let docs_path = example_dir.join("docs.md");
                        let docs = match fs::read_to_string(&docs_path) {
                            Ok(content) => {
                                let summary = doc_summary(&content);
                                let docs = ExampleDocs {
                                    path: docs_path.clone(),
                                    summary,
                                };
                                if metadata.doc_url.is_none() {
                                    metadata.doc_url = Some(doc_url_from_path(&docs.path));
                                }
                                Some(docs)
                            }
                            Err(_) => None,
                        };
                        if metadata.doc_url.is_none() {
                            metadata.doc_url = Some(format!("examples/{}/docs.md", metadata.id));
                        }
                        let benchmark_summary = benchmarks::load_example_summary(&metadata.id);
                        let example = Example {
                            script: script_content,
                            script_path: script_path.clone(),
                            metadata,
                            docs,
                            loaded_at: SystemTime::now(),
                            benchmark_summary,
                            test_suites,
                        };
                        examples.insert(example.metadata.id.clone(), example);
                    }
                    Err(error) => {
                        logging::with_runtime_subscriber(|| {
                            tracing::warn!(
                                target: "runtime.examples",
                                path = %meta_path.display(),
                                %error,
                                "Failed to parse example metadata"
                            );
                        });
                    }
                }
            }
            (Err(error), _) => {
                logging::with_runtime_subscriber(|| {
                    tracing::warn!(
                        target: "runtime.examples",
                        path = %meta_path.display(),
                        %error,
                        "Failed to read example metadata"
                    );
                });
            }
            (_, Err(error)) => {
                logging::with_runtime_subscriber(|| {
                    tracing::warn!(
                        target: "runtime.examples",
                        path = %script_path.display(),
                        %error,
                        "Failed to read example script"
                    );
                });
            }
        }
    }

    Ok(examples)
}

fn default_examples_dir() -> PathBuf {
    if let Ok(path) = std::env::var("KOTO_EXAMPLES_DIR") {
        return PathBuf::from(path);
    }

    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf));

    if let Some(dir) = exe_dir {
        let candidate = dir.join("examples");
        if candidate.exists() {
            return candidate;
        }
        if let Some(parent) = dir.parent() {
            let parent_candidate = parent.join("examples");
            if parent_candidate.exists() {
                return parent_candidate;
            }
        }
    }

    PathBuf::from("examples")
}

fn doc_summary(content: &str) -> String {
    for paragraph in content.split("\n\n") {
        let trimmed = paragraph.trim();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed.starts_with('#') {
            continue;
        }
        return trimmed.replace('\n', " ");
    }
    content
        .lines()
        .find(|line| !line.trim().is_empty())
        .map(|line| line.trim().to_string())
        .unwrap_or_default()
}

fn doc_url_from_path(path: &Path) -> String {
    match path.canonicalize() {
        Ok(canonical) => format!("file://{}", canonical.display()),
        Err(_) => format!("file://{}", path.display()),
    }
}
