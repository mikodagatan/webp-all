# WebP All

A menu bar / system tray app for macOS, Windows, and Linux that watches a folder (your Downloads folder by default) and converts every JPEG, PNG, BMP, and TIFF image in it to WebP, usually cutting file sizes by more than half.

- Converts the images already in the folder, then each new one as it finishes downloading.
- Starts in **Trial mode**: originals are kept next to their WebP copies, so nothing is deleted until you choose **Delete Originals**.
- Shows how much space you've saved, and keeps a CSV history of every conversion.
- Skips GIFs, since converting one would keep only the first frame of an animation.

## Install

Download the file for your system from the [latest release](../../releases/latest).

### macOS

Requires macOS 13 (Ventura) or later on Apple Silicon. On an Intel Mac, [build from source](#build-from-source).

1. Download `WebP-All-macOS.zip` and unzip it.
2. Drag **WebP All.app** into your **Applications** folder.
3. Open it. The app isn't notarized by Apple, so macOS blocks it on first launch. To allow it, go to **System Settings → Privacy & Security**, scroll down, and click **Open Anyway**. Or run this in Terminal:

   ```sh
   xattr -dr com.apple.quarantine "/Applications/WebP All.app"
   ```

### Windows

Requires Windows 10 or later (64-bit).

1. Download `WebP-All-Windows.exe` and move it somewhere permanent, such as `C:\Program Files\WebP All\`. Start at Login remembers where the file is, so move it before turning that on.
2. Double-click it. The app isn't code-signed, so SmartScreen may warn you: click **More info → Run anyway**.
3. The icon appears in the system tray. If you don't see it, click the **^** arrow next to the clock.

The first run also adds **WebP All** to the Start menu, so after that you can start it from Windows search. If you move the `.exe`, run it once from its new location to update the shortcut.

### Linux

Requires a 64-bit (x86_64) distro with glibc 2.35 or later (Ubuntu 22.04, Debian 12, Fedora 36, or newer), and a desktop that shows tray icons. On GNOME, other than Ubuntu's, that means installing the [AppIndicator extension](https://extensions.gnome.org/extension/615/appindicator-support/).

1. Install the libraries it needs:

   ```sh
   # Debian / Ubuntu
   sudo apt install libgtk-3-0 libayatana-appindicator3-1
   # Fedora
   sudo dnf install gtk3 libayatana-appindicator-gtk3
   # Arch
   sudo pacman -S gtk3 libayatana-appindicator
   ```

2. Download `WebP-All-Linux.tar.gz`, then unpack it and run it:

   ```sh
   mkdir -p ~/.local/bin
   tar -xzf WebP-All-Linux.tar.gz -C ~/.local/bin
   ~/.local/bin/webp-all &
   ```

   The first run adds **WebP All** to your app launcher, so after that you can start it by searching for it. The entry follows the binary, so if you move `webp-all`, run it once from its new location.

### Build from source

Install [Rust](https://rustup.rs) and clone this repo, then:

- **macOS:** `./scripts/bundle.sh --install` builds `WebP All.app`, copies it to `/Applications`, and launches it. Leave off `--install` to only build it into `target/`.
- **Windows:** `cargo build --release` builds `target\release\webp-all.exe`.
- **Linux:** install the development libraries (`sudo apt install libgtk-3-dev libayatana-appindicator3-dev` on Debian/Ubuntu), then `cargo build --release` builds `target/release/webp-all`.

## Usage

Click the photo icon in the menu bar or system tray:

| Menu item | What it does |
| --- | --- |
| Change Folder… | Pick a different folder to watch. |
| Pause / Resume | Stop or restart converting. |
| Trial: Keep Originals | Keep each original next to its WebP copy (the default). |
| Delete Originals | Delete each original after it's converted. |
| Delete Kept Originals… | Delete the originals Trial mode kept. Their WebP copies stay. |
| Open Savings History | Open the CSV of every conversion. |
| Open Log | Open the activity log. |
| Start at Login | Launch WebP All when you log in. |

New WebP files are saved next to the original with the same name (`photo.jpg` becomes `photo.webp`, or `photo-1.webp` if that name is taken), at quality 80.

To watch a folder once without changing the saved setting, run the app with a path, for example:

```sh
"/Applications/WebP All.app/Contents/MacOS/webp-all" ~/Desktop
```

## Files

Settings and `history.csv` live in a `WebP All` folder:

| System | Settings and history | Log |
| --- | --- | --- |
| macOS | `~/Library/Application Support/WebP All/` | `~/Library/Logs/webp-all.log` |
| Windows | `%APPDATA%\WebP All\` | Same folder, `webp-all.log` |
| Linux | `~/.local/share/WebP All/` | Same folder, `webp-all.log` |

Start at Login is stored in System Settings → Login Items on macOS, the registry `Run` key on Windows (shown in Task Manager → Startup apps), and `~/.config/autostart/webp-all.desktop` on Linux.

## Uninstall

Turn off **Start at Login**, quit WebP All from its menu, then delete the app and its settings folder listed above. On Windows, also delete the shortcut at `%APPDATA%\Microsoft\Windows\Start Menu\Programs\WebP All.lnk`. On Linux, delete `~/.local/share/applications/webp-all.desktop`.

Only one copy runs at a time: launching it again while it's running does nothing.

## Publishing a release

Pushing a version tag builds the app for all three systems on GitHub Actions and attaches the downloads to a new GitHub release:

```sh
git tag v0.1.0
git push origin v0.1.0
```
