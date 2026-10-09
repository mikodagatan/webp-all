use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;

use image::{DynamicImage, ImageDecoder, ImageReader};

use crate::history::{Entry, human_bytes, percent};
use crate::state::State;

const QUALITY: f32 = 80.0;
// GIF is excluded on purpose: encoding would keep only the first frame of an animation.
const EXTENSIONS: &[&str] = &["jpg", "jpeg", "png", "bmp", "tif", "tiff"];

/// Converts `path` if it's an image that hasn't been handled yet. Returns true if it converted.
pub fn process(path: &Path, state: &State) -> bool {
    if !is_convertible(path) {
        return false;
    }
    let Ok(original_bytes) = fs::metadata(path).map(|m| m.len()) else {
        return false;
    };
    if state.history.lock().unwrap().is_kept_original(path, original_bytes) {
        return false;
    }

    let (webp, webp_bytes) = match convert(path) {
        Ok(result) => result,
        Err(e) => {
            crate::log!("failed to convert {}: {e}", path.display());
            return false;
        }
    };
    // An original that couldn't be deleted is recorded as kept, so it isn't converted again
    // and "Delete Kept Originals" can retry it.
    let kept_original = state.trial.load(Ordering::Relaxed)
        || match fs::remove_file(path) {
            Ok(()) => false,
            Err(e) => {
                crate::log!("could not delete {}: {e}", path.display());
                true
            }
        };
    let entry = Entry {
        converted_at: chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
        kept_original,
        original: path.to_path_buf(),
        original_bytes,
        webp,
        webp_bytes,
    };
    crate::log!(
        "converted {} -> {} ({} -> {}, saved {} / {:.0}%){}",
        path.display(),
        entry.webp.display(),
        human_bytes(original_bytes as i64),
        human_bytes(webp_bytes as i64),
        human_bytes(entry.saved_bytes()),
        percent(entry.saved_bytes(), original_bytes),
        if kept_original { ", original kept" } else { "" },
    );
    if let Err(e) = state.history.lock().unwrap().record(entry) {
        crate::log!("could not save history: {e}");
    }
    true
}

fn is_convertible(path: &Path) -> bool {
    let is_hidden = path
        .file_name()
        .and_then(|n| n.to_str())
        .is_none_or(|n| n.starts_with('.'));
    let has_image_ext = path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()));
    !is_hidden && has_image_ext && path.is_file()
}

/// Encodes `path` to a sibling .webp file. Returns the WebP's path and size.
fn convert(path: &Path) -> Result<(PathBuf, u64), Box<dyn Error>> {
    let mut decoder = ImageReader::open(path)?.with_guessed_format()?.into_decoder()?;
    let orientation = decoder.orientation()?;
    let mut img = DynamicImage::from_decoder(decoder)?;
    img.apply_orientation(orientation);

    let encoded = if img.color().has_alpha() {
        let rgba = img.to_rgba8();
        webp::Encoder::from_rgba(&rgba, rgba.width(), rgba.height()).encode(QUALITY)
    } else {
        let rgb = img.to_rgb8();
        webp::Encoder::from_rgb(&rgb, rgb.width(), rgb.height()).encode(QUALITY)
    };

    let out = available_output_path(path);
    let tmp = out.with_file_name(format!(
        ".{}.part",
        out.file_name().unwrap().to_string_lossy()
    ));
    fs::write(&tmp, &*encoded)?;
    fs::File::options()
        .write(true)
        .open(&tmp)?
        .set_times(original_times(path)?)?;
    fs::rename(&tmp, &out)?;
    Ok((out, encoded.len() as u64))
}

/// Modified and (where the OS allows setting it) created time of `path`, so the WebP sorts
/// by the photo's original dates instead of the conversion date.
fn original_times(path: &Path) -> std::io::Result<fs::FileTimes> {
    let meta = fs::metadata(path)?;
    #[allow(unused_mut)]
    let mut times = fs::FileTimes::new().set_modified(meta.modified()?);
    #[cfg(target_os = "macos")]
    {
        use std::os::macos::fs::FileTimesExt;
        if let Ok(created) = meta.created() {
            times = times.set_created(created);
        }
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::FileTimesExt;
        if let Ok(created) = meta.created() {
            times = times.set_created(created);
        }
    }
    Ok(times)
}

/// `photo.jpg` -> `photo.webp`, or `photo-1.webp`, `photo-2.webp`, ... if taken.
fn available_output_path(path: &Path) -> PathBuf {
    let stem = path.file_stem().unwrap().to_string_lossy();
    let mut out = path.with_extension("webp");
    let mut n = 1;
    while out.exists() {
        out = path.with_file_name(format!("{stem}-{n}.webp"));
        n += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, SystemTime};

    #[test]
    fn webp_keeps_original_dates() {
        let dir = std::env::temp_dir().join(format!("webp-all-test-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let png = dir.join("old.png");
        image::RgbImage::new(4, 4).save(&png).unwrap();
        let old = SystemTime::UNIX_EPOCH + Duration::from_secs(1_500_000_000);
        fs::File::options()
            .write(true)
            .open(&png)
            .unwrap()
            .set_times(fs::FileTimes::new().set_modified(old))
            .unwrap();
        let original = fs::metadata(&png).unwrap();

        let (webp, _) = convert(&png).unwrap();
        let meta = fs::metadata(&webp).unwrap();
        fs::remove_dir_all(&dir).unwrap();
        assert_eq!(meta.modified().unwrap(), old);
        #[cfg(any(target_os = "macos", windows))]
        assert_eq!(meta.created().unwrap(), original.created().unwrap());
    }
}
