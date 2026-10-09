//! "Start at Login": SMAppService on macOS, the registry Run key on Windows, and an XDG
//! autostart entry on Linux.

#[cfg(target_os = "linux")]
pub use linux::*;
#[cfg(target_os = "macos")]
pub use macos::*;
#[cfg(windows)]
pub use windows::*;

/// Shows up in System Settings > General > Login Items. Only works from inside the .app bundle.
#[cfg(target_os = "macos")]
mod macos {
    use objc2_service_management::{SMAppService, SMAppServiceStatus};

    pub fn is_enabled() -> bool {
        // SAFETY: takes no arguments and documents no preconditions; objc2 marks its generated
        // bindings `unsafe` only because they're unaudited.
        unsafe { SMAppService::mainAppService().status() == SMAppServiceStatus::Enabled }
    }

    pub fn set_enabled(enabled: bool) -> Result<(), String> {
        // SAFETY: same as `is_enabled`; every SMAppService call here is argument-free with no preconditions.
        unsafe {
            let service = SMAppService::mainAppService();
            if enabled {
                service.registerAndReturnError()
            } else {
                service.unregisterAndReturnError()
            }
            .map_err(|e| e.localizedDescription().to_string())?;

            if service.status() == SMAppServiceStatus::RequiresApproval {
                SMAppService::openSystemSettingsLoginItems();
            }
        }
        Ok(())
    }
}

/// Start at Login shows up in Task Manager > Startup apps. A Start menu shortcut makes the
/// app show up in Windows search.
#[cfg(windows)]
mod windows {
    use std::fs;
    use std::os::windows::process::CommandExt;
    use std::path::Path;
    use std::process::{Command, Output};
    use std::thread;

    const RUN_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";
    const NAME: &str = "WebP All";
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    const ICON_SIZE: u32 = 128;

    /// Rewritten on every launch so the shortcut follows the .exe if it's moved.
    pub fn register_launcher() -> Result<(), String> {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let icon = crate::history::data_dir().join("icon.ico");
        fs::create_dir_all(icon.parent().unwrap()).map_err(|e| e.to_string())?;
        let pixels = crate::icon::rgba(ICON_SIZE);
        image::save_buffer(&icon, &pixels, ICON_SIZE, ICON_SIZE, image::ExtendedColorType::Rgba8)
            .map_err(|e| e.to_string())?;
        let shortcut = dirs::data_dir()
            .unwrap_or_default()
            .join(r"Microsoft\Windows\Start Menu\Programs\WebP All.lnk");

        let quote = |path: &Path| format!("'{}'", path.display().to_string().replace('\'', "''"));
        let script = format!(
            "$s = (New-Object -ComObject WScript.Shell).CreateShortcut({}); \
             $s.TargetPath = {}; $s.IconLocation = {}; $s.Save()",
            quote(&shortcut),
            quote(&exe),
            quote(&icon),
        );
        // PowerShell takes about a second to start, so don't hold up the tray icon for it.
        thread::spawn(move || {
            let result = Command::new("powershell")
                .args(["-NoProfile", "-NonInteractive", "-Command", &script])
                .creation_flags(CREATE_NO_WINDOW)
                .output();
            match result {
                Ok(out) if out.status.success() => {}
                Ok(out) => crate::log!(
                    "could not create Start menu shortcut: {}",
                    String::from_utf8_lossy(&out.stderr).trim()
                ),
                Err(e) => crate::log!("could not create Start menu shortcut: {e}"),
            }
        });
        Ok(())
    }

    fn reg(args: &[&str]) -> std::io::Result<Output> {
        Command::new("reg").args(args).creation_flags(CREATE_NO_WINDOW).output()
    }

    pub fn is_enabled() -> bool {
        reg(&["query", RUN_KEY, "/v", NAME]).is_ok_and(|out| out.status.success())
    }

    pub fn set_enabled(enabled: bool) -> Result<(), String> {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let command = format!("\"{}\"", exe.display());
        let out = if enabled {
            reg(&["add", RUN_KEY, "/v", NAME, "/t", "REG_SZ", "/d", &command, "/f"])
        } else {
            reg(&["delete", RUN_KEY, "/v", NAME, "/f"])
        }
        .map_err(|e| e.to_string())?;
        if out.status.success() {
            Ok(())
        } else {
            Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
        }
    }
}

/// The same .desktop entry goes in `~/.config/autostart` for Start at Login and in
/// `~/.local/share/applications` so the app shows up in the launcher's search.
#[cfg(target_os = "linux")]
mod linux {
    use std::fs;
    use std::io::ErrorKind;
    use std::path::{Path, PathBuf};

    const ICON_SIZE: u32 = 128;

    fn autostart_file() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_default()
            .join("autostart/webp-all.desktop")
    }

    fn icon_file() -> PathBuf {
        crate::history::data_dir().join("icon.png")
    }

    fn desktop_entry() -> Result<String, String> {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        // ponytail: Exec isn't escaped beyond quoting; breaks only if the path contains `"`, `$`, `` ` `` or `\`.
        Ok(format!(
            "[Desktop Entry]\nType=Application\nName=WebP All\nComment=Convert images in a folder to WebP\n\
             Exec=\"{}\"\nIcon={}\nCategories=Graphics;\nTerminal=false\n",
            exe.display(),
            icon_file().display(),
        ))
    }

    fn write(path: &Path, contents: &str) -> Result<(), String> {
        fs::create_dir_all(path.parent().unwrap())
            .and_then(|()| fs::write(path, contents))
            .map_err(|e| e.to_string())
    }

    pub fn is_enabled() -> bool {
        autostart_file().exists()
    }

    pub fn set_enabled(enabled: bool) -> Result<(), String> {
        if enabled {
            return write(&autostart_file(), &desktop_entry()?);
        }
        match fs::remove_file(autostart_file()) {
            Err(e) if e.kind() != ErrorKind::NotFound => Err(e.to_string()),
            _ => Ok(()),
        }
    }

    /// Rewritten on every launch so the entry follows the binary if it's moved.
    pub fn register_launcher() -> Result<(), String> {
        let icon = icon_file();
        fs::create_dir_all(icon.parent().unwrap()).map_err(|e| e.to_string())?;
        let pixels = crate::icon::rgba(ICON_SIZE);
        image::save_buffer(&icon, &pixels, ICON_SIZE, ICON_SIZE, image::ExtendedColorType::Rgba8)
            .map_err(|e| e.to_string())?;
        let launcher = dirs::data_dir()
            .unwrap_or_default()
            .join("applications/webp-all.desktop");
        write(&launcher, &desktop_entry()?)
    }
}
