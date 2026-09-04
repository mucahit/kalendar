#!/bin/sh
set -eu

target="${1:-$(rustc -vV | sed -n 's/^host: //p')}"
version=$(sed -n 's/^version = "\([^"]*\)"/\1/p' Cargo.toml)
name="kalendar-v${version}-${target}"
stage="dist/${name}"
archive="dist/${name}.tar.gz"

cargo build --release --target "$target"
rm -rf "$stage"
mkdir -p "$stage/bin" "$stage/libexec/kalendar"
install -m 755 "target/${target}/release/kalendar" "$stage/bin/kalendar"

bridge=""
for candidate in target/"${target}"/release/build/kalendar-macos-*/out/kalendar-eventkit; do
    if [ -f "$candidate" ]; then
        bridge="$candidate"
        break
    fi
done
if [ -z "$bridge" ]; then
    echo "EventKit helper was not produced for ${target}." >&2
    exit 1
fi
install -m 755 "$bridge" "$stage/libexec/kalendar/kalendar-eventkit"

# EventKit privacy authorization is associated with the helper's code identity.
# Swift signs native builds, but cross-compiled helpers need an explicit ad-hoc
# signature before distribution.
codesign --force --sign - --identifier dev.kalendar.cli "$stage/bin/kalendar"
codesign --force --sign - --identifier dev.kalendar.eventkit "$stage/libexec/kalendar/kalendar-eventkit"

tar -C dist -czf "$archive" "$name"
shasum -a 256 "$archive" > "${archive}.sha256"
echo "$archive"
