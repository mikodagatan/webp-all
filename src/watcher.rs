use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::thread;
use std::time::Duration;

use notify_debouncer_mini::notify::{RecommendedWatcher, RecursiveMode};
use notify_debouncer_mini::{DebouncedEventKind, Debouncer, new_debouncer};

use crate::convert;
use crate::state::State;

// A file must see no filesystem events for this long before it's considered fully downloaded.
const SETTLE_TIME: Duration = Duration::from_secs(3);
const PAUSE_POLL: Duration = Duration::from_millis(500);

/// Watches one folder until dropped. Dropping also stops an in-progress backlog scan.
pub struct Watcher {
    _debouncer: Debouncer<RecommendedWatcher>,
    stop: Arc<AtomicBool>,
}

impl Drop for Watcher {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

/// Converts images already in `dir`, then converts new ones as they arrive, calling
/// `on_converted` after each conversion.
pub fn start(
    dir: PathBuf,
    state: Arc<State>,
    on_converted: impl Fn() + Send + 'static,
) -> Result<Watcher, Box<dyn Error>> {
    let (tx, rx) = mpsc::channel();
    let mut debouncer = new_debouncer(SETTLE_TIME, tx)?;
    debouncer.watcher().watch(&dir, RecursiveMode::NonRecursive)?;
    let backlog = fs::read_dir(&dir)?;
    crate::log!("watching {}", dir.display());

    let stop = Arc::new(AtomicBool::new(false));
    let stopped = stop.clone();
    thread::spawn(move || {
        let process = |path: &Path| {
            if convert::process(path, &state) {
                on_converted();
            }
        };
        // Returns false once the watcher is dropped. Events arriving meanwhile queue up in `rx`.
        // ponytail: polls the pause flag; switch to a Condvar if resume latency ever matters.
        let wait_while_paused = || {
            while state.paused.load(Ordering::Relaxed) && !stopped.load(Ordering::Relaxed) {
                thread::sleep(PAUSE_POLL);
            }
            !stopped.load(Ordering::Relaxed)
        };

        for entry in backlog.flatten() {
            if !wait_while_paused() {
                return;
            }
            process(&entry.path());
        }

        // Ends when the debouncer is dropped, since that drops its sender.
        for result in rx {
            if !wait_while_paused() {
                return;
            }
            match result {
                Ok(events) => events
                    .iter()
                    .filter(|e| e.kind == DebouncedEventKind::Any)
                    .for_each(|e| process(&e.path)),
                Err(e) => crate::log!("watch error: {e}"),
            }
        }
    });

    Ok(Watcher {
        _debouncer: debouncer,
        stop,
    })
}
