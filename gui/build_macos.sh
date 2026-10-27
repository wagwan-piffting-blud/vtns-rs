#!/bin/bash
# Build VTNS.app and VTNS.dmg for macOS as one universal bundle (Apple silicon
# + Intel), signed with the Developer ID certificate pinned in tauri.conf.json
# and notarized with the App Store Connect API key.
#
#     ./build_macos.sh
#     NOTARY_ISSUER= ./build_macos.sh      # sign only
#
# Run on the Mac, from this folder. Over SSH the login keychain is locked and
# codesign fails with errSecInternalComponent; run `security unlock-keychain`
# first.

set -euo pipefail

NOTARY_KEY="${NOTARY_KEY:-$HOME/Desktop/AuthKey_RF5UGDDCG8.p8}"
NOTARY_KEY_ID="${NOTARY_KEY_ID:-RF5UGDDCG8}"
NOTARY_ISSUER="${NOTARY_ISSUER:-cf1db9e5-65cc-4af6-a1a5-a6516e4b1d88}"

export PATH="$HOME/.cargo/bin:$PATH"
# the source sits on an SMB share; building there is slow and would share
# target/ with the Windows builds
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$HOME/.cache/vtns-gui-target}"
export MACOSX_DEPLOYMENT_TARGET=11.0

# rust-toolchain.toml pins the compiler, so it goes in before anything asks it
# for targets
rustup toolchain install >/dev/null
for t in aarch64-apple-darwin x86_64-apple-darwin; do
    rustup target list --installed | grep -qx "$t" || rustup target add "$t"
done
cargo tauri --version >/dev/null 2>&1 || cargo install tauri-cli --version "^2" --locked

# tauri notarizes and staples the .app itself when these are set
if [ -n "$NOTARY_ISSUER" ]; then
    export APPLE_API_KEY="$NOTARY_KEY_ID"
    export APPLE_API_ISSUER="$NOTARY_ISSUER"
    export APPLE_API_KEY_PATH="$NOTARY_KEY"
fi

bundle="$CARGO_TARGET_DIR/universal-apple-darwin/release/bundle"
rm -rf "$bundle"
cargo tauri build --target universal-apple-darwin --bundles app

app="$bundle/macos/VTNS.app"
codesign --verify --deep --strict --verbose=2 "$app"
codesign -dvv "$app" 2>&1 | grep -E "Authority=Developer ID Application|Timestamp"

# Not tauri's dmg bundler: its bundle_dmg.sh lays the window out by scripting
# Finder, which fails over SSH. hdiutil needs no session. ditto keeps the
# stapled ticket and extended attributes that cp -R can drop.
version=$(sed -n 's/^  "version": "\(.*\)",$/\1/p' tauri.conf.json)
dmg="$bundle/VTNS_${version}_universal.dmg"
stage=$(mktemp -d)
ditto "$app" "$stage/VTNS.app"
ln -s /Applications "$stage/Applications"
hdiutil create -volname VTNS -srcfolder "$stage" -format ULFO -ov "$dmg"
rm -rf "$stage"

if [ -z "$NOTARY_ISSUER" ]; then
    echo "signed $app and $dmg; set NOTARY_ISSUER to notarize"
    exit 0
fi

# the app inside is stapled already; this staples the image too, so it opens
# offline on first launch
codesign --force --timestamp --sign "303D624AEF460613E64D9F807DBB15B7939369B9" "$dmg"
xcrun notarytool submit "$dmg" --wait \
    --key "$NOTARY_KEY" --key-id "$NOTARY_KEY_ID" --issuer "$NOTARY_ISSUER"
xcrun stapler staple "$dmg"
spctl --assess --type open --context context:primary-signature --verbose=2 "$dmg"

mkdir -p dist
cp "$dmg" dist/
echo "notarized dist/$(basename "$dmg")"
