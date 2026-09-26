#!/bin/sh
# Builds the web version of the game into `site/`, ready to be served as it is: the page, the game
# compiled to WebAssembly, and the data it loads. Needs the wasm32-unknown-unknown target and
# wasm-bindgen-cli at the version in Cargo.lock. Try it with `python3 -m http.server -d site`.
# Arguments are passed on to `cargo build`.
set -eu
cd "$(dirname "$0")/.."

cargo build --profile web --target wasm32-unknown-unknown "$@"
rm -rf site
# Without the function names: they are a fifth of the file, and only name functions in a panic's
# stack trace.
wasm-bindgen --target web --no-typescript --remove-name-section --out-dir site/pkg \
    target/wasm32-unknown-unknown/web/maze.wasm
cp web/index.html site/
# Only what is checked in, so a build here is the build GitHub makes.
git ls-files data | while read -r file; do
    mkdir -p "site/$(dirname "$file")"
    cp "$file" "site/$file"
done
