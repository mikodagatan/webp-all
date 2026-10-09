//! Every conversion is appended to a CSV, so savings survive restarts and
//! originals kept by Trial mode aren't converted a second time.

use std::error::Error;
use std::fs::{self, OpenOptions};
use std::path::{Path, PathBuf};

const HEADER: [&str; 8] = [
    "converted_at",
    "mode",
    "original",
    "original_bytes",
    "webp",
    "webp_bytes",
    "saved_bytes",
    "saved_percent",
];

/// `~/Library/Application Support`, `%APPDATA%`, or `~/.local/share`, plus `WebP All`.
pub fn data_dir() -> PathBuf {
    dirs::data_dir().unwrap_or_default().join("WebP All")
}

pub fn path() -> PathBuf {
    data_dir().join("history.csv")
}

pub struct Entry {
    pub converted_at: String,
    pub kept_original: bool,
    pub original: PathBuf,
    pub original_bytes: u64,
    pub webp: PathBuf,
    pub webp_bytes: u64,
}

impl Entry {
    pub fn saved_bytes(&self) -> i64 {
        self.original_bytes as i64 - self.webp_bytes as i64
    }

    fn to_record(&self) -> [String; 8] {
        [
            self.converted_at.clone(),
            if self.kept_original { "trial" } else { "delete" }.to_string(),
            self.original.to_string_lossy().into_owned(),
            self.original_bytes.to_string(),
            self.webp.to_string_lossy().into_owned(),
            self.webp_bytes.to_string(),
            self.saved_bytes().to_string(),
            format!("{:.1}", percent(self.saved_bytes(), self.original_bytes)),
        ]
    }

    fn from_record(record: &csv::StringRecord) -> Option<Self> {
        Some(Self {
            converted_at: record.get(0)?.to_string(),
            kept_original: record.get(1)? == "trial",
            original: record.get(2)?.into(),
            original_bytes: record.get(3)?.parse().ok()?,
            webp: record.get(4)?.into(),
            webp_bytes: record.get(5)?.parse().ok()?,
        })
    }
}

#[derive(Default)]
pub struct History {
    entries: Vec<Entry>,
}

pub struct Totals {
    pub count: usize,
    pub original_bytes: u64,
    pub webp_bytes: u64,
    pub saved_bytes: i64,
}

impl History {
    pub fn load() -> Self {
        let mut reader = match csv::Reader::from_path(path()) {
            Ok(reader) => reader,
            Err(e) => {
                if path().exists() {
                    crate::log!("could not read history: {e}");
                }
                return Self::default();
            }
        };
        let entries = reader
            .records()
            .filter_map(|r| Entry::from_record(&r.ok()?))
            .collect();
        Self { entries }
    }

    pub fn record(&mut self, entry: Entry) -> Result<(), Box<dyn Error>> {
        fs::create_dir_all(data_dir())?;
        let is_new = !path().exists();
        let file = OpenOptions::new().create(true).append(true).open(path())?;
        let mut writer = csv::Writer::from_writer(file);
        if is_new {
            writer.write_record(HEADER)?;
        }
        writer.write_record(entry.to_record())?;
        writer.flush()?;
        self.entries.push(entry);
        Ok(())
    }

    /// True if `original` is an image Trial mode already converted and left in place.
    pub fn is_kept_original(&self, original: &Path, bytes: u64) -> bool {
        self.entries
            .iter()
            .any(|e| e.kept_original && e.original == original && e.original_bytes == bytes)
    }

    /// Originals kept by Trial mode that are still on disk, and whose WebP still exists.
    pub fn kept_originals(&self) -> Vec<(PathBuf, u64)> {
        self.entries
            .iter()
            .filter(|e| e.kept_original && e.webp.is_file())
            .filter(|e| fs::metadata(&e.original).is_ok_and(|m| m.len() == e.original_bytes))
            .map(|e| (e.original.clone(), e.original_bytes))
            .collect()
    }

    pub fn totals(&self) -> Totals {
        Totals {
            count: self.entries.len(),
            original_bytes: self.entries.iter().map(|e| e.original_bytes).sum(),
            webp_bytes: self.entries.iter().map(|e| e.webp_bytes).sum(),
            saved_bytes: self.entries.iter().map(Entry::saved_bytes).sum(),
        }
    }
}

pub fn percent(part: i64, whole: u64) -> f64 {
    if whole == 0 { 0.0 } else { part as f64 * 100.0 / whole as f64 }
}

/// Decimal units, matching Finder: 1 KB = 1000 bytes.
pub fn human_bytes(bytes: i64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let sign = if bytes < 0 { "-" } else { "" };
    let mut value = bytes.unsigned_abs() as f64;
    let mut unit = 0;
    while value >= 1000.0 && unit < UNITS.len() - 1 {
        value /= 1000.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{sign}{value} B")
    } else {
        format!("{sign}{value:.1} {}", UNITS[unit])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(original: &str, original_bytes: u64, webp_bytes: u64, kept: bool) -> Entry {
        Entry {
            converted_at: "2026-10-08 23:00:00".into(),
            kept_original: kept,
            original: original.into(),
            original_bytes,
            webp: format!("{original}.webp").into(),
            webp_bytes,
        }
    }

    #[test]
    fn record_round_trips_through_csv() {
        let original = entry("/tmp/a, \"weird\".jpg", 1000, 250, true);
        let mut writer = csv::Writer::from_writer(vec![]);
        writer.write_record(original.to_record()).unwrap();
        let bytes = writer.into_inner().unwrap();
        let mut reader = csv::ReaderBuilder::new().has_headers(false).from_reader(&bytes[..]);
        let record = reader.records().next().unwrap().unwrap();
        let parsed = Entry::from_record(&record).unwrap();
        assert_eq!(parsed.original, original.original);
        assert_eq!(parsed.saved_bytes(), 750);
        assert!(parsed.kept_original);
        assert_eq!(&record[7], "75.0");
    }

    #[test]
    fn only_trial_originals_with_matching_size_are_skipped() {
        let history = History {
            entries: vec![entry("/d/kept.jpg", 100, 40, true), entry("/d/deleted.jpg", 100, 40, false)],
        };
        assert!(history.is_kept_original(Path::new("/d/kept.jpg"), 100));
        assert!(!history.is_kept_original(Path::new("/d/kept.jpg"), 101));
        assert!(!history.is_kept_original(Path::new("/d/deleted.jpg"), 100));
    }

    #[test]
    fn totals_include_files_that_grew() {
        let history = History {
            entries: vec![entry("/d/a.jpg", 1000, 200, false), entry("/d/b.png", 100, 150, false)],
        };
        let totals = history.totals();
        assert_eq!(totals.count, 2);
        assert_eq!(totals.original_bytes, 1100);
        assert_eq!(totals.webp_bytes, 350);
        assert_eq!(totals.saved_bytes, 750);
    }

    #[test]
    fn formats_bytes_like_finder() {
        assert_eq!(human_bytes(999), "999 B");
        assert_eq!(human_bytes(1_234_567), "1.2 MB");
        assert_eq!(human_bytes(-50_000), "-50.0 KB");
    }
}
