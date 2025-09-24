use std::{path::PathBuf, time::SystemTime};

use anyhow::Result;
use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher as _};

/// Wraps a [`notify`] watcher and normalizes events with timestamps.
pub struct Watcher {
    _watcher: RecommendedWatcher,
}

/// Events emitted by [`Watcher`].
#[derive(Debug)]
pub enum WatchEvent {
    /// A filesystem event emitted by [`notify`].
    FileEvent { event: Event, timestamp: SystemTime },
    /// An error reported by the underlying watcher.
    Error { error: notify::Error },
}

impl Watcher {
    /// Watches the provided directory recursively and forwards events to the handler.
    pub fn new(
        path: PathBuf,
        mut handler: impl FnMut(WatchEvent) + Send + 'static,
    ) -> Result<Self> {
        let mut watcher = notify::recommended_watcher(move |event| match event {
            Ok(event) => handler(WatchEvent::FileEvent {
                event,
                timestamp: SystemTime::now(),
            }),
            Err(error) => handler(WatchEvent::Error { error }),
        })?;

        watcher.watch(&path, RecursiveMode::Recursive)?;

        Ok(Self { _watcher: watcher })
    }
}
