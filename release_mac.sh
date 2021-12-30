#!/bin/bash

echo "Building with a fat binary with release profile for Mac.."
# silence errors
rm pollenwall 2>/dev/null
rm pollenwall.zip 2>/dev/null
cargo build --target=aarch64-apple-darwin --release
cargo build --target=x86_64-apple-darwin --release
lipo -create target/aarch64-apple-darwin/release/pollenwall target/x86_64-apple-darwin/release/pollenwall -output pollenwall
zip -r pollenwall-x86_x64-aarch64-apple-darwin.zip pollenwall
echo "Done!"
