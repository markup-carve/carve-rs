#!/bin/sh
# Packages carve as a downstream receives it (`cargo package`), unpacks the
# tarball in a scratch dir and runs consumer-smoke against the unpacked crate.
set -eu
here=$(cd "$(dirname "$0")/.." && pwd)
work=${1:-$(mktemp -d)}
target=${CARGO_TARGET_DIR:-$here/target}
version=$(sed -n 's/^version = "\(.*\)"/\1/p' "$here/Cargo.toml" | head -n 1)
cd "$here"
cargo package --quiet --allow-dirty --no-verify
mkdir -p "$work/unpacked" "$work/consumer/src"
tar -xzf "$target/package/carve-lang-$version.crate" -C "$work/unpacked"
cp consumer-smoke/src/main.rs "$work/consumer/src/main.rs"
sed "s|path = \"..\"|path = \"$work/unpacked/carve-lang-$version\"|" consumer-smoke/Cargo.toml > "$work/consumer/Cargo.toml"
cd "$work/consumer"
cargo run --quiet
