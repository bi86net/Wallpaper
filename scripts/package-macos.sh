#!/bin/sh
set -eu
cd "$(dirname "$0")/.."
if [ "$(uname -s)" != "Darwin" ]; then
    echo 'This script requires macOS.' >&2
    exit 1
fi
./scripts/update-version.sh
profile=debug
if [ "${1:-}" = "--release" ]; then
    profile=release
    cargo build --release --bin wallpaper
else
    cargo build --bin wallpaper
fi
app="target/$profile/Wallpaper.app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp "target/$profile/wallpaper" "$app/Contents/MacOS/wallpaper"
cp assets/icon.icns "$app/Contents/Resources/AppIcon.icns"
version=$(sed -n 's/^version = "\([^"]*\)"/\1/p' Cargo.toml | head -n 1)
cat > "$app/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleExecutable</key><string>wallpaper</string>
<key>CFBundleIdentifier</key><string>local.wallpaper.desktop</string>
<key>CFBundleName</key><string>Wallpaper</string>
<key>CFBundleDisplayName</key><string>光景 Wallpaper</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleIconFile</key><string>AppIcon</string>
<key>CFBundleShortVersionString</key><string>$version</string>
<key>CFBundleVersion</key><string>$version</string>
<key>NSHighResolutionCapable</key><true/>
</dict></plist>
PLIST
plutil -lint "$app/Contents/Info.plist"
echo "Built: $app"
