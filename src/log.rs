use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;

pub fn path() -> PathBuf {
    if cfg!(target_os = "macos") {
        std::env::home_dir()
            .unwrap_or_default()
            .join("Library/Logs/webp-all.log")
    } else {
        crate::history::data_dir().join("webp-all.log")
    }
}

pub fn write(msg: &str) {
    let line = format!("{} {msg}\n", chrono::Local::now().format("%Y-%m-%d %H:%M:%S"));
    print!("{line}");
    let path = path();
    if let Some(dir) = path.parent() {
        let _ = fs::create_dir_all(dir);
    }
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = file.write_all(line.as_bytes());
    }
}

#[macro_export]
macro_rules! log {
    ($($arg:tt)*) => { $crate::log::write(&format!($($arg)*)) };
}
