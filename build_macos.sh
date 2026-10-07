#!/bin/bash
# Build vtpaul for macOS (Apple silicon and Intel) and a universal binary, sign them with
# the Developer ID identity and notarize them.
#
#     VTPAUL_NOTARY_PROFILE=profile ./build_macos.sh
#     VTPAUL_SIGN_IDENTITY=<certificate SHA-1 or unique name> VTPAUL_NOTARY_PROFILE=profile ./build_macos.sh
#     VTPAUL_UNSIGNED=1 ./build_macos.sh          # reproducible unsigned build, no signing or notarizing
#
# Output goes to dist/: vtpaul-macos-arm64, vtpaul-macos-x86_64, vtpaul-macos-universal and
# vtpaul-macos.zip (the three binaries, as submitted for notarization).
#
# Notarizing needs a notarytool profile stored once in the login keychain:
#     xcrun notarytool store-credentials <profile> --apple-id <id> --team-id 48SH89HN4S
# The script never reads or asks for the keychain password; codesign and notarytool do.
#
# Bare Mach-O executables cannot carry a stapled ticket (stapler only accepts app bundles,
# disk images and installer packages), so Gatekeeper looks the ticket up online on first run.
# `spctl --assess` is no check for them: it rejects every bare tool as "not an app".
#
# The unsigned binaries are byte-reproducible. A signed one, after `codesign --remove-signature`,
# differs from them in one byte per architecture: __LINKEDIT's vmsize, which signing grows and
# removal keeps.

set -euo pipefail

cd "$(dirname "$0")"
crate="$(pwd -P)"
dist="${DIST:-dist}"
# Keep build output off network shares.
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$HOME/.cache/vtpaul-target}"
export MACOSX_DEPLOYMENT_TARGET=11.0

# The SHA-1 of the Developer ID Application certificate issued by Apple's Developer ID G2 CA
# (valid to 2031). The name alone also matches the older certificate that expires on
# 2027-02-01, and codesign refuses an ambiguous name.
identity="${VTPAUL_SIGN_IDENTITY:-303D624AEF460613E64D9F807DBB15B7939369B9}"
unsigned="${VTPAUL_UNSIGNED:-}"
profile="${VTPAUL_NOTARY_PROFILE:-"vtpaul-notary"}"
if [ -z "$unsigned" ] && [ -z "${profile}" ]; then
    echo "VTPAUL_NOTARY_PROFILE is not set (a stored notarytool profile name)." >&2
    echo "Set it, or set VTPAUL_UNSIGNED=1 for an unsigned build." >&2
    exit 2
fi

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

if [ -z "$unsigned" ]; then
    # codesign fails with errSecInternalComponent when the keychain holding the key is locked
    # (typical over SSH). `security` prompts for the password itself; this script never sees it.
    kc="$(security default-keychain -d user | sed 's/^ *"//; s/"$//')"
    if ! security show-keychain-info "$kc" >/dev/null 2>&1; then
        echo "=== $kc is locked; unlocking it for codesign"
        security unlock-keychain "$kc"
    fi
    echo "=== signing with: $identity"
    for b in "${bins[@]}"; do
        codesign --force --options runtime --timestamp --identifier vtpaul --sign "$identity" "$b"
        codesign --verify --strict --verbose=1 "$b"
    done

    echo "=== notarizing (profile $profile)"
    zip="$dist/vtpaul-macos.zip"
    rm -f "$zip"
    # ditto keeps the signatures intact the way Apple's notary service expects.
    stage="$(mktemp -d)"
    trap 'rm -rf "$stage"' EXIT
    cp "${bins[@]}" "$stage/"
    ditto -c -k --keepParent "$stage" "$zip"

    result="$(xcrun notarytool submit "$zip" --keychain-profile "$profile" --wait --output-format json)"
    echo "$result"
    id="$(printf '%s' "$result" | plutil -extract id raw -o - -)"
    status="$(printf '%s' "$result" | plutil -extract status raw -o - -)"
    if [ "$status" != "Accepted" ]; then
        echo "notarization $status (submission $id); log follows" >&2
        xcrun notarytool log "$id" --keychain-profile "$profile" >&2 || true
        exit 1
    fi
    # spctl rejects every bare command-line tool as "not an app", notarized or not, and stapler
    # cannot staple one; the ticket lives on Apple's servers and Gatekeeper fetches it on first run.
    echo "notarization Accepted (submission $id); ticket is online-only for bare executables"
fi

for b in "${bins[@]}"; do
    printf '%-32s %s\n' "$(basename "$b")" "$(lipo -archs "$b")"
done
