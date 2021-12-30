#!/bin/bash

# This script will be deprecated soon and replaced with cargo make.

echo "Building with a fat binary with release profile for Mac.."
# silence errors
rm -r build 2>/dev/null
mkdir build
cargo build --target=aarch64-apple-darwin --release
cargo build --target=x86_64-apple-darwin --release
lipo -create target/aarch64-apple-darwin/release/pollenwall target/x86_64-apple-darwin/release/pollenwall -output build/pollenwall
zip -r build/pollenwall-x86_x64-aarch64-apple-darwin.zip build/pollenwall
rm build/pollenwall 2>/dev/null
echo "Done!"

echo "Building for windows.."
rm pollenwall-x86_64-pc-windows-gnu.zip 2>/dev/null
rustup run stable cargo rustc --release --target=x86_64-pc-windows-gnu -- -C linker=x86_64-w64-mingw32-gcc
zip -r build/pollenwall-x86_64-pc-windows-gnu.zip target/x86_64-pc-windows-gnu/release/pollenwall.exe
echo "Done!"

# echo "Building for linux.."
# rm pollenwall-x86_64-unknown-linux-gnu.zip 2>/dev/null
# rustup run stable cargo rustc --release --target=x86_64-unknown-linux-gnu
# zip -r build/pollenwall-x86_64-unknown-linux-gnu.zip target/x86_64-unknown-linux-gnu/release/pollenwall
# echo "Done!"
