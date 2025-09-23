use std::sync::Mutex;

use anyhow::{Result, anyhow};
use koto::Koto;
use once_cell::sync::Lazy;

pub static RUNTIME: Lazy<RuntimeState> = Lazy::new(RuntimeState::new);

pub struct RuntimeState {
    koto: Mutex<Koto>,
}

impl RuntimeState {
    fn new() -> Self {
        Self {
            koto: Mutex::new(Koto::default()),
        }
    }

    pub fn evaluate_script(&self, source: &str) -> Result<String> {
        let mut koto = self
            .koto
            .lock()
            .map_err(|error| anyhow!("Failed to lock Koto runtime: {error}"))?;
        let value = koto
            .compile_and_run(source)
            .map_err(|error| anyhow!("Script error: {error}"))?;
        koto.value_to_string(value)
            .map_err(|error| anyhow!("Failed to stringify value: {error}"))
    }
}
