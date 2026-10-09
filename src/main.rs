#![windows_subsystem = "windows"]

mod convert;
mod history;
mod icon;
mod log;
mod login_item;
mod state;
mod watcher;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use tao::event::{Event, StartCause};
use tao::event_loop::{ControlFlow, EventLoopBuilder, EventLoopProxy};
#[cfg(target_os = "macos")]
use tao::platform::macos::{ActivationPolicy, EventLoopExtMacOS};
use tray_icon::TrayIconBuilder;
use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem};

use history::{human_bytes, percent};
use state::State;
use watcher::Watcher;

enum UserEvent {
    Menu(MenuEvent),
    Converted,
}

fn main() {
    // A folder passed on the command line wins over the one picked in the menu.
    let dir = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .or_else(state::load_folder)
        .or_else(dirs::download_dir)
        .or_else(|| dirs::home_dir().map(|home| home.join("Downloads")))
        .expect("could not determine home directory");

    #[cfg_attr(not(target_os = "macos"), allow(unused_mut))]
    let mut event_loop = EventLoopBuilder::<UserEvent>::with_user_event().build();
    #[cfg(target_os = "macos")]
    event_loop.set_activation_policy(ActivationPolicy::Accessory);
    {
        let proxy = event_loop.create_proxy();
        MenuEvent::set_event_handler(Some(move |event| {
            let _ = proxy.send_event(UserEvent::Menu(event));
        }));
    }

    let mut app = App::new(Arc::new(State::load()), event_loop.create_proxy());
    app.watch(dir);

    let mut tray = None;
    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;
        match event {
            Event::NewEvents(StartCause::Init) => {
                #[cfg(target_os = "macos")]
                let builder = TrayIconBuilder::new().with_icon_templated(icon::tray_icon());
                #[cfg(not(target_os = "macos"))]
                let builder = TrayIconBuilder::new().with_icon(icon::tray_icon());
                tray = Some(
                    builder
                        .with_tooltip("WebP All")
                        .with_menu(Box::new(app.menu.clone()))
                        .build()
                        .expect("failed to create menu bar icon"),
                );
            }
            Event::UserEvent(UserEvent::Converted) => app.refresh_savings(),
            Event::UserEvent(UserEvent::Menu(event)) => {
                if event.id == app.quit.id() {
                    tray.take();
                    *control_flow = ControlFlow::Exit;
                } else {
                    app.handle(&event);
                }
            }
            _ => {}
        }
    });
}

struct App {
    state: Arc<State>,
    proxy: EventLoopProxy<UserEvent>,
    dir: PathBuf,
    watcher: Option<Watcher>,
    status_text: String,

    menu: Menu,
    status: MenuItem,
    change_folder: MenuItem,
    savings: MenuItem,
    pause: MenuItem,
    trial_mode: CheckMenuItem,
    delete_mode: CheckMenuItem,
    delete_kept: MenuItem,
    open_history: MenuItem,
    open_log: MenuItem,
    start_at_login: CheckMenuItem,
    quit: MenuItem,
}

impl App {
    fn new(state: Arc<State>, proxy: EventLoopProxy<UserEvent>) -> Self {
        let trial = state.trial.load(Ordering::Relaxed);
        let app = Self {
            state,
            proxy,
            dir: PathBuf::new(),
            watcher: None,
            status_text: String::new(),

            menu: Menu::new(),
            status: MenuItem::new("", false, None),
            change_folder: MenuItem::new("Change Folder…", true, None),
            savings: MenuItem::new("", false, None),
            pause: MenuItem::new("Pause", true, None),
            trial_mode: CheckMenuItem::new("Trial: Keep Originals", true, trial, None),
            delete_mode: CheckMenuItem::new("Delete Originals", true, !trial, None),
            delete_kept: MenuItem::new("", false, None),
            open_history: MenuItem::new("Open Savings History", true, None),
            open_log: MenuItem::new("Open Log", true, None),
            start_at_login: CheckMenuItem::new(
                "Start at Login",
                true,
                login_item::is_enabled(),
                None,
            ),
            quit: MenuItem::new("Quit WebP All", true, None),
        };
        app.menu
            .append_items(&[
                &app.status,
                &app.change_folder,
                &PredefinedMenuItem::separator(),
                &app.savings,
                &app.pause,
                &PredefinedMenuItem::separator(),
                &app.trial_mode,
                &app.delete_mode,
                &app.delete_kept,
                &PredefinedMenuItem::separator(),
                &app.open_history,
                &app.open_log,
                &app.start_at_login,
                &PredefinedMenuItem::separator(),
                &app.quit,
            ])
            .expect("failed to build menu");
        app.refresh_savings();
        app
    }

    /// Stops watching the current folder (if any) and starts watching `dir`.
    fn watch(&mut self, dir: PathBuf) {
        // Filesystem events report resolved paths; match them so history lookups line up.
        // Not on Windows, where that would add a `\\?\` prefix the watcher doesn't use.
        #[cfg(not(windows))]
        let dir = dir.canonicalize().unwrap_or(dir);
        self.watcher = None;
        let proxy = self.proxy.clone();
        let on_converted = move || {
            let _ = proxy.send_event(UserEvent::Converted);
        };
        match watcher::start(dir.clone(), self.state.clone(), on_converted) {
            Ok(watcher) => {
                self.watcher = Some(watcher);
                self.status_text = format!("Watching {}", display_path(&dir));
            }
            Err(e) => {
                log!("can't watch {}: {e}", dir.display());
                self.status_text = format!("Can't watch {}", display_path(&dir));
            }
        }
        self.dir = dir;
        self.refresh_status();
    }

    fn handle(&mut self, event: &MenuEvent) {
        let id = &event.id;
        if id == self.change_folder.id() {
            self.change_folder();
        } else if id == self.pause.id() {
            let paused = !self.state.paused.fetch_xor(true, Ordering::Relaxed);
            log!("{}", if paused { "paused" } else { "resumed" });
            self.pause.set_text(if paused { "Resume" } else { "Pause" });
            self.refresh_status();
        } else if id == self.trial_mode.id() || id == self.delete_mode.id() {
            let trial = id == self.trial_mode.id();
            self.state.trial.store(trial, Ordering::Relaxed);
            state::save_trial_mode(trial);
            self.trial_mode.set_checked(trial);
            self.delete_mode.set_checked(!trial);
            log!("mode: {}", if trial { "trial (keep originals)" } else { "delete originals" });
        } else if id == self.delete_kept.id() {
            delete_kept_originals(&self.state);
            self.refresh_savings();
        } else if id == self.open_history.id() {
            open(&history::path());
        } else if id == self.open_log.id() {
            open(&log::path());
        } else if id == self.start_at_login.id() {
            if let Err(e) = login_item::set_enabled(self.start_at_login.is_checked()) {
                log!("could not change Start at Login: {e}");
            }
            self.start_at_login.set_checked(login_item::is_enabled());
        }
    }

    fn change_folder(&mut self) {
        let Some(dir) = choose_folder(&self.dir) else {
            return;
        };
        if !self.state.trial.load(Ordering::Relaxed) {
            let message = format!(
                "WebP All will convert every image in {} and permanently delete the originals.",
                display_path(&dir),
            );
            if !confirm(&message, "Watch Folder") {
                return;
            }
        }
        state::save_folder(&dir);
        log!("folder changed to {}", dir.display());
        self.watch(dir);
    }

    fn refresh_status(&self) {
        let paused = self.state.paused.load(Ordering::Relaxed);
        self.status.set_text(if paused { "Paused" } else { &self.status_text });
    }

    fn refresh_savings(&self) {
        let history = self.state.history.lock().unwrap();
        let totals = history.totals();
        self.savings.set_text(if totals.count == 0 {
            "No images converted yet".to_string()
        } else {
            format!(
                "Saved {} across {} image{} ({:.0}%): {} → {}",
                human_bytes(totals.saved_bytes),
                totals.count,
                if totals.count == 1 { "" } else { "s" },
                percent(totals.saved_bytes, totals.original_bytes),
                human_bytes(totals.original_bytes as i64),
                human_bytes(totals.webp_bytes as i64),
            )
        });

        let kept = history.kept_originals();
        let kept_bytes: u64 = kept.iter().map(|(_, bytes)| bytes).sum();
        self.delete_kept.set_enabled(!kept.is_empty());
        self.delete_kept.set_text(if kept.is_empty() {
            "Delete Kept Originals".to_string()
        } else {
            format!(
                "Delete {} Kept Original{} (frees {})…",
                kept.len(),
                if kept.len() == 1 { "" } else { "s" },
                human_bytes(kept_bytes as i64),
            )
        });
    }
}

fn delete_kept_originals(state: &State) {
    let kept = state.history.lock().unwrap().kept_originals();
    let bytes: u64 = kept.iter().map(|(_, bytes)| bytes).sum();
    let message = format!(
        "Permanently delete {} original image(s) kept by Trial mode ({})? Their WebP versions stay.",
        kept.len(),
        human_bytes(bytes as i64),
    );
    if !confirm(&message, "Delete") {
        return;
    }
    for (path, _) in kept {
        match fs::remove_file(&path) {
            Ok(()) => log!("deleted kept original {}", path.display()),
            Err(e) => log!("could not delete {}: {e}", path.display()),
        }
    }
}

/// Opens `path` in its default app.
fn open(path: &Path) {
    #[cfg(target_os = "macos")]
    let opener = "open";
    #[cfg(windows)]
    let opener = "explorer";
    #[cfg(target_os = "linux")]
    let opener = "xdg-open";
    let _ = Command::new(opener).arg(path).spawn();
}

/// Shows a native folder picker starting at `current`; None if cancelled.
#[cfg(target_os = "macos")]
fn choose_folder(current: &Path) -> Option<PathBuf> {
    let script = format!(
        "POSIX path of (choose folder with prompt \"Choose a folder for WebP All to watch\" \
         default location (POSIX file \"{}\"))",
        applescript_escape(&current.to_string_lossy()),
    );
    let out = Command::new("osascript").args(["-e", &script]).output().ok()?;
    if !out.status.success() {
        return None;
    }
    let path = String::from_utf8(out.stdout).ok()?;
    Some(PathBuf::from(path.trim_end_matches('\n')))
}

/// Shows a native confirmation dialog; true only if the user clicks `action`.
#[cfg(target_os = "macos")]
fn confirm(message: &str, action: &str) -> bool {
    let script = format!(
        "display dialog \"{}\" with title \"WebP All\" buttons {{\"Cancel\", \"{}\"}} \
         default button \"Cancel\" cancel button \"Cancel\" with icon caution",
        applescript_escape(message),
        applescript_escape(action),
    );
    Command::new("osascript")
        .args(["-e", &script])
        .output()
        .is_ok_and(|out| out.status.success())
}

#[cfg(target_os = "macos")]
fn applescript_escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

#[cfg(windows)]
fn choose_folder(current: &Path) -> Option<PathBuf> {
    rfd::FileDialog::new()
        .set_title("Choose a folder for WebP All to watch")
        .set_directory(current)
        .pick_folder()
}

/// Windows only gets OK/Cancel: custom button labels need rfd's common-controls-v6 manifest.
#[cfg(windows)]
fn confirm(message: &str, _action: &str) -> bool {
    rfd::MessageDialog::new()
        .set_title("WebP All")
        .set_description(message)
        .set_level(rfd::MessageLevel::Warning)
        .set_buttons(rfd::MessageButtons::OkCancel)
        .show()
        == rfd::MessageDialogResult::Ok
}

// GTK dialogs are built directly: rfd would run them on a second GTK thread, but tao
// already drives GTK on this one.
#[cfg(target_os = "linux")]
fn choose_folder(current: &Path) -> Option<PathBuf> {
    use gtk::prelude::*;
    let dialog = gtk::FileChooserDialog::with_buttons(
        Some("Choose a folder for WebP All to watch"),
        None::<&gtk::Window>,
        gtk::FileChooserAction::SelectFolder,
        &[("Cancel", gtk::ResponseType::Cancel), ("Choose", gtk::ResponseType::Accept)],
    );
    dialog.set_current_folder(current);
    let dir = (dialog.run() == gtk::ResponseType::Accept)
        .then(|| dialog.filename())
        .flatten();
    dialog.close();
    dir
}

#[cfg(target_os = "linux")]
fn confirm(message: &str, action: &str) -> bool {
    use gtk::prelude::*;
    let dialog = gtk::MessageDialog::new(
        None::<&gtk::Window>,
        gtk::DialogFlags::MODAL,
        gtk::MessageType::Warning,
        gtk::ButtonsType::None,
        message,
    );
    dialog.set_title("WebP All");
    dialog.add_buttons(&[("Cancel", gtk::ResponseType::Cancel), (action, gtk::ResponseType::Accept)]);
    let confirmed = dialog.run() == gtk::ResponseType::Accept;
    dialog.close();
    confirmed
}

/// `/Users/me/Downloads` -> `~/Downloads`
fn display_path(dir: &Path) -> String {
    match std::env::home_dir().and_then(|home| dir.strip_prefix(home).ok().map(PathBuf::from)) {
        Some(rel) => format!("~/{}", rel.display()),
        None => dir.display().to_string(),
    }
}
