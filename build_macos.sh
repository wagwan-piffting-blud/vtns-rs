#!/bin/sh
# Build vtpaul for macOS as ONE universal binary (Apple silicon + Intel), sign
# it with the Developer ID and notarize it. No single-architecture binary is
# ever shipped.
# Runs on a Mac; build_targets.ps1 drives it over ssh from Windows.
#
#     sh build_macos.sh                build, sign, check both slices, notarize
#     sh build_macos.sh --no-check     skip the slice check
#     sh build_macos.sh --no-notarize  sign only
#     sh build_macos.sh --no-sign      leave the linker's ad-hoc signature
#
# Needs rustup and the Xcode command line tools (linker, lipo, codesign,
# notarytool). The x86_64 slice is checked under Rosetta on Apple silicon.
#
# The login keychain is unlocked before anything is built, when it is locked:
# over ssh it always is, and codesign then fails with errSecInternalComponent.
# That asks for the Mac password, so it needs a terminal (ssh -t). Its
# auto-lock timeout is lifted for the run and put back on exit, so a long build
# cannot relock it before codesign. Notarization uses the API key below and
# needs no keychain.
#
# The target directory stays on the Mac's own disk: the source may be an SMB
# share of the Windows checkout, whose target/ belongs to Windows.

set -eu

# ---- per-app -----------------------------------------------------------------

APP=vtpaul
IDENT=vtpaul
CRATE_DIR=.
CARGO_SEL="--bin vtpaul"
TARGET_CACHE=vtpaul-target
MACOS_MIN=11.0

# By SHA-1, not name: the expiring G1 and the new G2 Developer ID Application
# certificates share a name, and codesign refuses an ambiguous one.
SIGN_ID=303D624AEF460613E64D9F807DBB15B7939369B9
NOTARY_KEY_ID=RF5UGDDCG8
NOTARY_ISSUER=cf1db9e5-65cc-4af6-a1a5-a6516e4b1d88
NOTARY_KEY="$HOME/Desktop/AuthKey_RF5UGDDCG8.p8"

# rust-toolchain.toml pins the compiler, so it goes in before anything asks it
# for targets. The remaps make the binary path-free and, unsigned,
# byte-reproducible; a signed one after `codesign --remove-signature` differs
# in one byte per slice, __LINKEDIT's vmsize, which signing grows and removal
# keeps.
app_env() {
    rustup toolchain install >/dev/null
    sysroot=$(rustc --print sysroot)
    export RUSTFLAGS="--remap-path-prefix=$sysroot=/rustc-sysroot --remap-path-prefix=$ROOT=vtpaul --remap-path-prefix=$HOME=~"
}

# $1 the signed universal binary, $2 the slices this Mac can run.
# Every voice through every slice; the SHA256s must agree. build_targets.ps1
# -Check holds both slices to the Windows reference as well.
app_check() {
    work=$(mktemp -d)
    TMPS="$TMPS $work"
    data="$ROOT/../all_voices"
    printf '%s' "The National Weather Service in Norman has issued a severe thunderstorm warning for Cleveland County until 5:45 PM CDT." > "$work/p0.txt"
    printf '%s' "On 10/07/2026 the total was \$1,234.56; call 1-800-555-0199 or write to wx@example.gov." > "$work/p1.txt"
    printf '%s' "Hola a todos. El servicio meteorologico nacional ha emitido un aviso de tormenta severa." > "$work/p2.txt"
    fail=0
    for c in paul:0 paul:1 kate:0 kate:1 julie:0 julie:1 james:0 james:1 violeta:2; do
        v=${c%:*}
        i=${c#*:}
        for s in $2; do
            arch -$s "$1" --data "$data" --voice $v -o "$work/$s-$v-$i.wav" -f "$work/p$i.txt"
        done
        if [ "$2" != "x86_64" ] && \
           [ "$(shasum -a 256 < "$work/arm64-$v-$i.wav")" != "$(shasum -a 256 < "$work/x86_64-$v-$i.wav")" ]; then
            echo "$v/p$i: arm64 and x86_64 slices differ" >&2
            fail=1
        fi
    done
    [ $fail = 0 ] || return 1
    echo "slices agree on all phrases"
}

# ---- end per-app -------------------------------------------------------------

ROOT=$(cd "$(dirname "$0")" && pwd -P)
export PATH="$HOME/.cargo/bin:$PATH"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$HOME/.cache/$TARGET_CACHE}"
if [ -n "$MACOS_MIN" ]; then
    export MACOSX_DEPLOYMENT_TARGET="$MACOS_MIN"
fi

CHECK=1
SIGN=1
NOTARIZE=1
for a in "$@"; do
    case "$a" in
        --no-check) CHECK=0 ;;
        --no-notarize) NOTARIZE=0 ;;
        --no-sign) SIGN=0; NOTARIZE=0 ;;
        *) echo "unknown option $a" >&2; exit 2 ;;
    esac
done

ARM=aarch64-apple-darwin
X86=x86_64-apple-darwin
OUT="$ROOT/dist/$APP-macos-universal"

TMPS=""
KC=""
KC_RESTORE=""
cleanup() {
    if [ -n "$KC_RESTORE" ]; then
        security set-keychain-settings $KC_RESTORE "$KC" || true
    fi
    if [ -n "$TMPS" ]; then
        rm -rf $TMPS
    fi
}
trap cleanup EXIT

unlock_keychain() {
    KC=$(security default-keychain -d user 2>/dev/null | sed 's/^ *"//; s/"$//')
    [ -n "$KC" ] || KC="$HOME/Library/Keychains/login.keychain-db"
    if ! info=$(security show-keychain-info "$KC" 2>&1); then
        echo "=== $KC is locked; unlocking it for codesign"
        security unlock-keychain "$KC"
        info=$(security show-keychain-info "$KC" 2>&1)
    fi
    t=$(printf '%s' "$info" | sed -n 's/.*timeout=\([0-9][0-9]*\)s.*/\1/p')
    if [ -n "$t" ]; then
        los=""
        case "$info" in *lock-on-sleep*) los="-l" ;; esac
        security set-keychain-settings $los "$KC"
        KC_RESTORE="$los -t $t"
    fi
    if ! security find-identity -v -p codesigning | grep -q "$SIGN_ID"; then
        echo "signing identity $SIGN_ID is not in the keychain search list; these are:" >&2
        security find-identity -v -p codesigning >&2
        exit 1
    fi
}

# The stored path first; then wherever notarytool and altool look for keys,
# and the usual drop spots. A hit elsewhere is used, and the stored path should
# then be updated above.
find_notary_key() {
    if [ -f "$NOTARY_KEY" ]; then
        return 0
    fi
    f="AuthKey_$NOTARY_KEY_ID.p8"
    for d in "$HOME/Desktop" "$HOME/Downloads" "$HOME/Documents" \
             "$HOME/.appstoreconnect/private_keys" "$HOME/.private_keys" "$HOME/private_keys"; do
        if [ -f "$d/$f" ]; then
            echo "=== notary key is not at $NOTARY_KEY but at $d/$f; using it (update NOTARY_KEY in build_macos.sh)"
            NOTARY_KEY="$d/$f"
            return 0
        fi
    done
    echo "no $f at $NOTARY_KEY or in ~/Desktop, ~/Downloads, ~/Documents," >&2
    echo "~/.appstoreconnect/private_keys, ~/.private_keys, ~/private_keys; use --no-notarize to sign only" >&2
    return 1
}

# Everything that can ask a question or fail on credentials happens before the
# build, so the run is unattended from here on.
if [ $SIGN = 1 ]; then
    unlock_keychain
fi
if [ $NOTARIZE = 1 ]; then
    find_notary_key
fi

cd "$ROOT/$CRATE_DIR"
app_env
for t in $ARM $X86; do
    rustup target list --installed | grep -qx "$t" || rustup target add "$t"
done
cargo build --release $CARGO_SEL --target $ARM --target $X86

mkdir -p "$ROOT/dist"
rm -f "$ROOT/dist/$APP-macos-arm64" "$ROOT/dist/$APP-macos-x86_64" "$ROOT/dist/$APP-macos.zip"
lipo -create -output "$OUT" \
    "$CARGO_TARGET_DIR/$ARM/release/$APP" \
    "$CARGO_TARGET_DIR/$X86/release/$APP"
ARCHS=$(lipo -archs "$OUT")
case "$ARCHS" in
    "arm64 x86_64"|"x86_64 arm64") echo "slices: $ARCHS" ;;
    *) echo "expected exactly arm64 and x86_64, got: $ARCHS" >&2; exit 1 ;;
esac

# Signed before the check, so the check runs the file that ships.
if [ $SIGN = 1 ]; then
    codesign --force --options runtime --timestamp \
        --identifier "$IDENT" --sign "$SIGN_ID" "$OUT"
    codesign --verify --strict --verbose=2 "$OUT"
fi
ls -l "$OUT"

if [ $CHECK = 1 ]; then
    if [ "$(uname -m)" = "arm64" ]; then
        app_check "$OUT" "arm64 x86_64"
    else
        app_check "$OUT" "x86_64"
    fi
fi

[ $NOTARIZE = 1 ] || exit 0

# A bare Mach-O cannot be stapled, so the ticket lives with Apple and
# Gatekeeper fetches it on first run. notarytool only takes zip, pkg or dmg.
zdir=$(mktemp -d)
TMPS="$TMPS $zdir"
ZIP="$zdir/$APP-macos-universal.zip"
ditto -c -k --keepParent "$OUT" "$ZIP"
RESULT=$(xcrun notarytool submit "$ZIP" --wait --output-format plist \
    --key "$NOTARY_KEY" --key-id "$NOTARY_KEY_ID" --issuer "$NOTARY_ISSUER")
STATUS=$(printf '%s' "$RESULT" | plutil -extract status raw -)
SUBMISSION=$(printf '%s' "$RESULT" | plutil -extract id raw -)
echo "notarization $SUBMISSION: $STATUS"
if [ "$STATUS" != "Accepted" ]; then
    xcrun notarytool log "$SUBMISSION" \
        --key "$NOTARY_KEY" --key-id "$NOTARY_KEY_ID" --issuer "$NOTARY_ISSUER"
    exit 1
fi
