use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::{Arc, RwLock},
    time::SystemTime,
};

use anyhow::{Context, Result};
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use once_cell::sync::OnceCell;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::runtime::logging;

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
pub struct Example {
    pub metadata: ExampleMetadata,
    pub script: String,
    pub loaded_at: SystemTime,
}

pub struct ExampleLibrary {
    inner: Arc<ExampleLibraryInner>,
    _watcher: Option<RecommendedWatcher>,
}

struct ExampleLibraryInner {
    examples_dir: PathBuf,
    examples: RwLock<BTreeMap<String, Example>>,
    version: AtomicUsize,
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
        guard.get(id).cloned()
    }

    fn with_watcher(examples_dir: PathBuf, watch: bool) -> Result<Self> {
        fs::create_dir_all(&examples_dir)
            .with_context(|| format!("Failed to ensure examples dir {examples_dir:?}"))?;

        let inner = Arc::new(ExampleLibraryInner::new(examples_dir.clone())?);

        let watcher = if watch {
            let inner = Arc::clone(&inner);
            let mut watcher = notify::recommended_watcher(move |event| {
                handle_watch_event(&inner, event);
            })?;
            watcher.watch(&examples_dir, RecursiveMode::Recursive)?;
            Some(watcher)
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
        };
        library.reload()?;
        Ok(library)
    }

    fn reload(&self) -> Result<()> {
        let examples = load_examples_from_dir(&self.examples_dir)?;
        let count = examples.len();
        if let Ok(mut guard) = self.examples.write() {
            *guard = examples;
        }
        self.version.fetch_add(1, Ordering::SeqCst);
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

    fn snapshot(&self) -> Vec<Example> {
        self.examples
            .read()
            .map(|examples| examples.values().cloned().collect())
            .unwrap_or_default()
    }
}

fn handle_watch_event(inner: &Arc<ExampleLibraryInner>, event: notify::Result<Event>) {
    match event {
        Ok(event) if should_reload(&event.kind) => {
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
        Ok(_) => {}
        Err(error) => {
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
                        let example = Example {
                            script: script_content,
                            metadata,
                            loaded_at: SystemTime::now(),
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
