#!/bin/bash
# Build vtpaul for macOS (Apple silicon and Intel) and a universal binary.
#
#     ./build_macos.sh
#     VTPAUL_SIGN_IDENTITY="Developer ID Application: Name (TEAMID)" ./build_macos.sh
#     VTPAUL_SIGN_IDENTITY=... VTPAUL_NOTARY_PROFILE=profile ./build_macos.sh
#
# Output goes to dist/: vtpaul-macos-arm64, vtpaul-macos-x86_64 and
# vtpaul-macos-universal. The unsigned binaries are byte-reproducible. A signed
# one, after `codesign --remove-signature`, differs from them in one byte per
# architecture: __LINKEDIT's vmsize, which signing grows and removal keeps.
# Notarizing needs a stored notarytool profile (xcrun notarytool store-credentials).

set -euo pipefail

cd "$(dirname "$0")"
crate="$(pwd -P)"
dist="${DIST:-dist}"
# Keep build output off network shares.
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$HOME/.cache/vtpaul-target}"
export MACOSX_DEPLOYMENT_TARGET=11.0

targets=(aarch64-apple-darwin x86_64-apple-darwin)
rustup toolchain install >/dev/null
rustup target add "${targets[@]}" >/dev/null

sysroot="$(rustc --print sysroot)"
export RUSTFLAGS="--remap-path-prefix=$sysroot=/rustc-sysroot --remap-path-prefix=$crate=vtpaul --remap-path-prefix=$HOME=~"

for t in "${targets[@]}"; do
    echo "=== $t"
    cargo build --release --bin vtpaul --target "$t"
done

mkdir -p "$dist"
cp "$CARGO_TARGET_DIR/aarch64-apple-darwin/release/vtpaul" "$dist/vtpaul-macos-arm64"
cp "$CARGO_TARGET_DIR/x86_64-apple-darwin/release/vtpaul" "$dist/vtpaul-macos-x86_64"
lipo -create -output "$dist/vtpaul-macos-universal" "$dist/vtpaul-macos-arm64" "$dist/vtpaul-macos-x86_64"

bins=("$dist/vtpaul-macos-arm64" "$dist/vtpaul-macos-x86_64" "$dist/vtpaul-macos-universal")
if [ -n "${VTPAUL_SIGN_IDENTITY:-}" ]; then
    for b in "${bins[@]}"; do
        codesign --force --options runtime --timestamp --identifier vtpaul --sign "$VTPAUL_SIGN_IDENTITY" "$b"
        codesign --verify --strict --verbose=1 "$b"
    done
    if [ -n "${VTPAUL_NOTARY_PROFILE:-}" ]; then
        zip -j -q "$dist/vtpaul-macos.zip" "${bins[@]}"
        xcrun notarytool submit "$dist/vtpaul-macos.zip" --keychain-profile "$VTPAUL_NOTARY_PROFILE" --wait
    fi
fi

for b in "${bins[@]}"; do
    printf '%-32s %s\n' "$(basename "$b")" "$(lipo -archs "$b")"
done
