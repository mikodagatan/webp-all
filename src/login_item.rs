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

/// Shows up in Task Manager > Startup apps.
#[cfg(windows)]
mod windows {
    use std::os::windows::process::CommandExt;
    use std::process::{Command, Output};

    const RUN_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";
    const NAME: &str = "WebP All";
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

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

#[cfg(target_os = "linux")]
mod linux {
    use std::fs;
    use std::io::ErrorKind;
    use std::path::PathBuf;

    fn desktop_file() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_default()
            .join("autostart/webp-all.desktop")
    }

    pub fn is_enabled() -> bool {
        desktop_file().exists()
    }

    pub fn set_enabled(enabled: bool) -> Result<(), String> {
        let path = desktop_file();
        if !enabled {
            return match fs::remove_file(&path) {
                Err(e) if e.kind() != ErrorKind::NotFound => Err(e.to_string()),
                _ => Ok(()),
            };
        }
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        // ponytail: Exec isn't escaped beyond quoting; breaks only if the path contains `"`, `$`, `` ` `` or `\`.
        let entry = format!(
            "[Desktop Entry]\nType=Application\nName=WebP All\nExec=\"{}\"\n",
            exe.display()
        );
        fs::create_dir_all(path.parent().unwrap())
            .and_then(|()| fs::write(&path, entry))
            .map_err(|e| e.to_string())
    }
}
