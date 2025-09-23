use std::{
    collections::HashMap,
    ffi::{CStr, c_char},
    fs,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant, SystemTime},
};

use anyhow::{Context, anyhow};
use koto::{Koto, KotoSettings, prelude::*, runtime::Result as KotoRuntimeResult};
use libloading::Library;
use once_cell::sync::Lazy;
use serde_json::Value as JsonValue;
use tracing::Level;

pub static RUNTIME: Lazy<Runtime> = Lazy::new(|| Runtime::new().expect("runtime init failed"));

pub struct Runtime {
    state: Mutex<RuntimeState>,
    stdout: BufferHandle,
    stderr: BufferHandle,
    profiling_enabled: Arc<AtomicBool>,
}

#[derive(Clone, Debug)]
pub struct ExecutionOutput {
    pub return_value: Option<String>,
    pub stdout: String,
    pub stderr: String,
    pub duration: Duration,
}

struct RuntimeState {
    koto: Koto,
    config: RuntimeConfig,
    host_bindings: HashMap<String, KValue>,
    shared_libraries: Vec<SharedLibrary>,
    profiling_flag: Arc<AtomicBool>,
}

#[derive(Clone, Default)]
struct RuntimeConfig {
    execution_limit: Option<Duration>,
    run_tests: bool,
}

struct SharedLibrary {
    #[allow(dead_code)]
    library: Library,
}

#[derive(Clone)]
struct BufferHandle {
    id: KString,
    buffer: Arc<Mutex<String>>,
}

#[derive(Clone)]
struct BufferFile {
    id: KString,
    buffer: Arc<Mutex<String>>,
}

#[repr(C)]
struct RuntimeLibraryApi {
    runtime: *const Runtime,
    register_script: extern "C" fn(*const Runtime, *const c_char) -> bool,
}

impl Runtime {
    pub fn new() -> anyhow::Result<Self> {
        logging::init();

        let stdout = BufferHandle::new("stdout");
        let stderr = BufferHandle::new("stderr");
        let profiling_enabled = Arc::new(AtomicBool::new(false));
        let state = RuntimeState::new(
            RuntimeConfig::default(),
            &stdout,
            &stderr,
            &profiling_enabled,
        )?;

        Ok(Self {
            state: Mutex::new(state),
            stdout,
            stderr,
            profiling_enabled,
        })
    }

    pub fn execute_script(&self, script: &str) -> anyhow::Result<ExecutionOutput> {
        self.execute_script_with_timeout(script, None)
    }

    pub fn execute_script_with_timeout(
        &self,
        script: &str,
        timeout: Option<Duration>,
    ) -> anyhow::Result<ExecutionOutput> {
        logging::with_runtime_subscriber(|| {
            tracing::info!(target: "runtime.vm", len = script.len(), "Evaluating script");
        });

        let mut state = self.lock_state()?;
        if state.config.execution_limit != timeout {
            state.config.execution_limit = timeout;
            state.rebuild_vm(&self.stdout, &self.stderr);
        }

        self.stdout.clear();
        self.stderr.clear();

        let profiling_enabled = state.profiling_flag.load(Ordering::SeqCst);
        let start = Instant::now();
        let result = if profiling_enabled {
            profiling::scope!("koto_script");
            state.koto.compile_and_run(script)
        } else {
            state.koto.compile_and_run(script)
        };
        let duration = start.elapsed();
        let stdout = self.stdout.take();
        let stderr = self.stderr.take();

        match result {
            Ok(value) => {
                let output = if matches!(value, KValue::Null) {
                    None
                } else {
                    Some(state.koto.value_to_string(value.clone())?)
                };
                logging::with_runtime_subscriber(|| {
                    tracing::info!(target: "runtime.vm", elapsed_ms = duration.as_millis() as u64, "Script completed");
                });
                Ok(ExecutionOutput {
                    return_value: output,
                    stdout,
                    stderr,
                    duration,
                })
            }
            Err(error) => {
                logging::with_runtime_subscriber(|| {
                    tracing::error!(target: "runtime.vm", %error, "Script error");
                });
                Err(anyhow!("{error}"))
            }
        }
    }

    pub fn set_execution_timeout(&self, limit: Option<Duration>) -> anyhow::Result<()> {
        let mut state = self.lock_state()?;
        state.config.execution_limit = limit;
        state.rebuild_vm(&self.stdout, &self.stderr);
        logging::with_runtime_subscriber(|| {
            tracing::info!(
                target: "runtime.vm",
                timeout_ms = limit.map(|d| d.as_millis() as u64),
                "Execution timeout updated"
            );
        });
        Ok(())
    }

    pub fn set_profiling_enabled(&self, enabled: bool) {
        self.profiling_enabled.store(enabled, Ordering::SeqCst);
        logging::with_runtime_subscriber(|| {
            tracing::info!(target: "runtime.vm", enabled = enabled, "Profiling updated");
        });
    }

    pub fn register_host_function<F>(&self, name: &str, function: F) -> anyhow::Result<()>
    where
        F: Fn(&mut CallContext) -> KotoRuntimeResult<KValue> + KotoSend + KotoSync + 'static,
    {
        let mut state = self.lock_state()?;
        let value: KValue = KNativeFunction::new(function).into();
        state.register_host_value(name.to_string(), value);
        logging::with_runtime_subscriber(|| {
            tracing::info!(target: "runtime.vm", name = name, "Registered host function");
        });
        Ok(())
    }

    pub fn register_host_module(&self, name: &str, module: KMap) -> anyhow::Result<()> {
        let mut state = self.lock_state()?;
        state.register_host_value(name.to_string(), module.into());
        logging::with_runtime_subscriber(|| {
            tracing::info!(target: "runtime.vm", name = name, "Registered host module");
        });
        Ok(())
    }

    pub fn load_shared_library(&self, path: impl AsRef<Path>) -> anyhow::Result<()> {
        let path = path.as_ref();
        let library = unsafe { Library::new(path) }
            .with_context(|| format!("Failed to load shared library {path:?}"))?;
        let register: libloading::Symbol<unsafe extern "C" fn(RuntimeLibraryApi) -> bool> = unsafe {
            library
                .get(b"koto_register")
                .with_context(|| format!("Library {path:?} is missing koto_register"))?
        };

        let api = RuntimeLibraryApi {
            runtime: self as *const Runtime,
            register_script: register_script_trampoline,
        };

        let success = unsafe { register(api) };
        if !success {
            return Err(anyhow!("Library {path:?} reported registration failure"));
        }

        let mut state = self.lock_state()?;
        state.shared_libraries.push(SharedLibrary { library });
        logging::with_runtime_subscriber(|| {
            tracing::info!(target: "runtime.vm", path = %path.display(), "Loaded shared library");
        });
        Ok(())
    }

    fn lock_state(&self) -> anyhow::Result<std::sync::MutexGuard<'_, RuntimeState>> {
        self.state
            .lock()
            .map_err(|error| anyhow!("Failed to lock runtime state: {error}"))
    }
}

impl RuntimeState {
    fn new(
        config: RuntimeConfig,
        stdout: &BufferHandle,
        stderr: &BufferHandle,
        profiling_flag: &Arc<AtomicBool>,
    ) -> anyhow::Result<Self> {
        let mut state = Self {
            koto: Self::build_koto(&config, stdout, stderr),
            config,
            host_bindings: HashMap::new(),
            shared_libraries: Vec::new(),
            profiling_flag: profiling_flag.clone(),
        };
        state.register_builtin_modules()?;
        Ok(state)
    }

    fn build_koto(config: &RuntimeConfig, stdout: &BufferHandle, stderr: &BufferHandle) -> Koto {
        let mut settings = KotoSettings::default();
        settings.run_tests = config.run_tests;
        if let Some(limit) = config.execution_limit {
            settings = settings.with_execution_limit(limit);
        }
        settings = settings
            .with_stdout(stdout.file())
            .with_stderr(stderr.file());
        Koto::with_settings(settings)
    }

    fn rebuild_vm(&mut self, stdout: &BufferHandle, stderr: &BufferHandle) {
        self.koto = Self::build_koto(&self.config, stdout, stderr);
        self.apply_host_bindings();
    }

    fn register_builtin_modules(&mut self) -> anyhow::Result<()> {
        self.register_host_value("host".to_string(), host_module(self.profiling_flag.clone()));
        self.register_host_value("serde".to_string(), serialization_module()?);
        Ok(())
    }

    fn register_host_value(&mut self, name: String, value: KValue) {
        self.host_bindings.insert(name.clone(), value.clone());
        let mut prelude = self.koto.prelude().data_mut();
        prelude.insert(name.as_str().into(), value);
    }

    fn apply_host_bindings(&mut self) {
        let mut prelude = self.koto.prelude().data_mut();
        for (name, value) in &self.host_bindings {
            prelude.insert(name.as_str().into(), value.clone());
        }
    }
}

impl BufferHandle {
    fn new(id: &str) -> Self {
        Self {
            id: KString::from(id),
            buffer: Arc::new(Mutex::new(String::new())),
        }
    }

    fn file(&self) -> BufferFile {
        BufferFile {
            id: self.id.clone(),
            buffer: Arc::clone(&self.buffer),
        }
    }

    fn clear(&self) {
        if let Ok(mut guard) = self.buffer.lock() {
            guard.clear();
        }
    }

    fn take(&self) -> String {
        if let Ok(mut guard) = self.buffer.lock() {
            let output = guard.clone();
            guard.clear();
            output
        } else {
            String::new()
        }
    }
}

impl KotoFile for BufferFile {
    fn id(&self) -> KString {
        self.id.clone()
    }
}

impl KotoWrite for BufferFile {
    fn write(&self, bytes: &[u8]) -> KotoRuntimeResult<()> {
        let text = String::from_utf8_lossy(bytes);
        if let Ok(mut guard) = self.buffer.lock() {
            guard.push_str(&text);
        }
        Ok(())
    }

    fn write_line(&self, text: &str) -> KotoRuntimeResult<()> {
        self.write(format!("{text}\n").as_bytes())?;
        Ok(())
    }

    fn flush(&self) -> KotoRuntimeResult<()> {
        Ok(())
    }
}

impl KotoRead for BufferFile {}

fn host_module(profiling_flag: Arc<AtomicBool>) -> KValue {
    let module = KMap::default();
    module.insert("version", env!("CARGO_PKG_VERSION"));
    module.insert(
        "echo",
        KNativeFunction::new(|ctx: &mut CallContext| {
            Ok(ctx.args().first().cloned().unwrap_or(KValue::Null))
        }),
    );
    module.insert(
        "profiling_enabled",
        KNativeFunction::new(move |_ctx: &mut CallContext| {
            Ok(profiling_flag.load(Ordering::SeqCst).into())
        }),
    );
    module.insert(
        "now",
        KNativeFunction::new(|_ctx: &mut CallContext| {
            let now = match SystemTime::now().duration_since(SystemTime::UNIX_EPOCH) {
                Ok(duration) => duration,
                Err(error) => return runtime_error!("System time error: {error}"),
            };
            Ok(format!("{}", now.as_secs()).into())
        }),
    );
    module.into()
}

fn serialization_module() -> anyhow::Result<KValue> {
    let module = KMap::default();
    module.insert(
        "to_json",
        KNativeFunction::new(|ctx: &mut CallContext| {
            let value = ctx.args().first().cloned().unwrap_or(KValue::Null);
            let json: JsonValue = match koto::serde::from_koto_value(value) {
                Ok(json) => json,
                Err(error) => return runtime_error!("Serialization error: {error}"),
            };
            match serde_json::to_string_pretty(&json) {
                Ok(text) => Ok(text.into()),
                Err(error) => runtime_error!("Serialization error: {error}"),
            }
        }),
    );
    module.insert(
        "from_json",
        KNativeFunction::new(|ctx: &mut CallContext| match ctx.args() {
            [KValue::Str(text), ..] => {
                let parsed: JsonValue = match serde_json::from_str(text) {
                    Ok(parsed) => parsed,
                    Err(error) => return runtime_error!("Failed to parse JSON: {error}"),
                };
                match koto::serde::to_koto_value(parsed) {
                    Ok(value) => Ok(value),
                    Err(error) => runtime_error!("Failed to convert JSON: {error}"),
                }
            }
            other => runtime_error!("Expected JSON string, found {other:?}"),
        }),
    );
    Ok(module.into())
}

extern "C" fn register_script_trampoline(runtime: *const Runtime, script: *const c_char) -> bool {
    if runtime.is_null() || script.is_null() {
        return false;
    }

    let runtime = unsafe { &*runtime };
    let script = unsafe { CStr::from_ptr(script) };
    match script.to_str() {
        Ok(source) => runtime.execute_script(source).is_ok(),
        Err(_) => false,
    }
}

pub mod logging {
    use super::*;
    use once_cell::sync::OnceCell;
    use tracing_appender::non_blocking::WorkerGuard;

    static DISPATCH: OnceCell<tracing::Dispatch> = OnceCell::new();
    static GUARD: OnceCell<WorkerGuard> = OnceCell::new();

    pub fn init() {
        let _ = dispatcher();
    }

    pub fn with_runtime_subscriber<F, R>(f: F) -> R
    where
        F: FnOnce() -> R,
    {
        let dispatch = dispatcher();
        tracing::dispatcher::with_default(dispatch, f)
    }

    fn dispatcher() -> &'static tracing::Dispatch {
        DISPATCH.get_or_init(|| {
            let logs_dir = PathBuf::from("logs");
            if let Err(error) = fs::create_dir_all(&logs_dir) {
                eprintln!("Failed to create log directory {logs_dir:?}: {error}");
            }
            let file_appender = tracing_appender::rolling::never(&logs_dir, "runtime.log");
            let (writer, guard) = tracing_appender::non_blocking(file_appender);
            let dispatch = tracing_subscriber::fmt()
                .with_ansi(false)
                .with_max_level(Level::TRACE)
                .with_writer(writer)
                .finish()
                .into();
            let _ = GUARD.get_or_init(|| guard);
            dispatch
        })
    }
}
