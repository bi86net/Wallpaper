#!/bin/sh
set -eu
cd "$(dirname "$0")/.."
./scripts/package-macos.sh --release
version=$(sed -n 's/^version = "\([^"]*\)"/\1/p' Cargo.toml | head -n 1)
date_version=${version%%.*}
architecture=$(uname -m)
output=${1:-"target/release/Wallpaper-${date_version}-macOS-${architecture}.dmg"}
mkdir -p "$(dirname "$output")"
staging=$(mktemp -d "$PWD/target/dmg-stage.XXXXXX")
trap 'rm -rf "$staging"' EXIT HUP INT TERM

ditto target/release/Wallpaper.app "$staging/Wallpaper.app"
codesign --force --sign - --timestamp=none "$staging/Wallpaper.app"
codesign --verify --deep --strict "$staging/Wallpaper.app"
ln -s /Applications "$staging/Applications"
cat > "$staging/安装说明.txt" <<'TEXT'
光景 Wallpaper

将 Wallpaper.app 拖到 Applications 文件夹后，从“应用程序”打开。
不要直接从安装镜像启用开机启动。若此前运行过开发版，安装后关闭再开启一次自启，以更新程序路径。

适用：Apple Silicon Mac（M 系列芯片）。
本包使用本地临时签名，尚未经过 Apple 开发者签名及公证。
TEXT
hdiutil create -volname "Wallpaper $date_version" -srcfolder "$staging" -format UDZO -ov "$output"
hdiutil verify "$output"
echo "DMG: $output"
