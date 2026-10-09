#!/bin/sh
# Builds "WebP All.app" into target/. With --install, also copies it to /Applications and launches it.
set -eu
cd "$(dirname "$0")/.."

APP="target/WebP All.app"

cargo build --release
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS"
cp target/release/webp-all "$APP/Contents/MacOS/"
cp macos/Info.plist "$APP/Contents/"
codesign --force --sign - "$APP"
echo "Built $APP"

if [ "${1:-}" = "--install" ]; then
    pkill -x webp-all || true
    rm -rf "/Applications/WebP All.app"
    cp -R "$APP" /Applications/
    open "/Applications/WebP All.app"
    echo "Installed and launched /Applications/WebP All.app"
fi
