#!/bin/sh
set -eu
cd "$(dirname "$0")/.."
# Freeze the release date once, before compilation. All artifacts read this version.
release_date=$(TZ=Asia/Shanghai date +%y%m%d)
release_version="$release_date.0.0"
temporary=$(mktemp "$PWD/.Cargo.toml.XXXXXX")
trap 'rm -f "$temporary"' EXIT HUP INT TERM
awk -v version="$release_version" '
    /^\[package\][[:space:]]*$/ { in_package = 1; print; next }
    /^\[/ { in_package = 0 }
    in_package && /^[[:space:]]*version[[:space:]]*=/ {
        print "version = \"" version "\""
        updated++
        next
    }
    { print }
    END { if (updated != 1) exit 1 }
' Cargo.toml > "$temporary"
if ! cmp -s Cargo.toml "$temporary"; then
    cat "$temporary" > Cargo.toml
fi
echo "Release version: $release_version (Asia/Shanghai)"
