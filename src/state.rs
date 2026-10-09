use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::AtomicBool;

use crate::history::{self, History};

/// Shared between the menu bar (main thread) and the watcher thread.
pub struct State {
    pub paused: AtomicBool,
    /// Trial mode keeps originals next to their WebP instead of deleting them.
    pub trial: AtomicBool,
    pub history: Mutex<History>,
}

impl State {
    pub fn load() -> Self {
        Self {
            paused: AtomicBool::new(false),
            trial: AtomicBool::new(load_trial_mode()),
            history: Mutex::new(History::load()),
        }
    }
}

fn folder_path() -> PathBuf {
    history::data_dir().join("folder")
}

/// The folder the user picked from the menu, if any.
pub fn load_folder() -> Option<PathBuf> {
    fs::read_to_string(folder_path()).ok().map(PathBuf::from)
}

pub fn save_folder(dir: &Path) {
    let result = fs::create_dir_all(history::data_dir())
        .and_then(|()| fs::write(folder_path(), dir.as_os_str().as_encoded_bytes()));
    if let Err(e) = result {
        crate::log!("could not save folder: {e}");
    }
}

fn mode_path() -> PathBuf {
    history::data_dir().join("mode")
}

/// Defaults to Trial so nothing is deleted until the user opts in.
fn load_trial_mode() -> bool {
    fs::read_to_string(mode_path()).map_or(true, |s| s.trim() != "delete")
}

pub fn save_trial_mode(trial: bool) {
    let result = fs::create_dir_all(history::data_dir())
        .and_then(|()| fs::write(mode_path(), if trial { "trial" } else { "delete" }));
    if let Err(e) = result {
        crate::log!("could not save mode: {e}");
    }
}
