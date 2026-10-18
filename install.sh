#!/bin/sh
# SPDX-License-Identifier: MPL-2.0
#
# wjson installer for Linux and macOS.
#
#   curl -fsSL https://raw.githubusercontent.com/potatosl/wjson/main/install.sh | sh
#
# Downloads the wjson sources, builds its small C++ support library and writes a
# small wrapper, `wjson-wavec`, that compiles a Wave program against it:
#
#   ~/.wjson/bin/wjson-wavec hello.wave --run
#
# It does not install the tools wjson needs (the Wave compiler and a
# C++ compiler); if one is missing it says how to get it and stops.
#
# Read it before piping it into a shell; it is short. Settings (environment):
#   WJSON_REF      git tag, branch or commit to install            (default: main)
#   WJSON_DIR      where to install                                (default: ~/.wjson)
#   WJSON_SHA256   expected SHA-256 of the source tarball; the install stops
#                  if the download differs (the actual value is always printed)
#   WJSON_TARBALL  use this local file (or URL) instead of downloading a tag
#   WAVE_STD       Wave's standard library folder, if wavec cannot find it
#   WJSON_OPT      optimization flag for programs built with the wrapper
#                  (default: -O2, or -O0 on macOS arm64, see README)
#
# To remove wjson, delete the install folder.

set -eu

REPO="potatosl/wjson"
REF="${WJSON_REF:-main}"
DIR="${WJSON_DIR:-$HOME/.wjson}"

say() { printf '%s\n' "$*"; }
die() { printf 'wjson install: %s\n' "$*" >&2; exit 1; }
need() { command -v "$1" > /dev/null 2>&1; }

case "${1:-}" in
    -h | --help)
        sed -n '4,28p' "$0" 2> /dev/null | sed 's/^# \{0,1\}//' || true
        exit 0
        ;;
esac

case "$(uname -s)" in
    Linux | Darwin) ;;
    *) die "this installer supports Linux and macOS. On Windows run setup-windows.cmd from a clone (see the README)." ;;
esac
OS_ARCH="$(uname -s)-$(uname -m)"

[ -n "$DIR" ] && [ "$DIR" != "/" ] || die "WJSON_DIR must be a real folder"

# ---- tools ------------------------------------------------------------------------

need curl || die "curl is required"
need tar || die "tar is required"
if ! need wavec; then
    die "the Wave compiler (wavec) is not on PATH. Install it with
    curl -fsSL https://wave-lang.dev/install.sh | bash
and open a new shell."
fi
need "${CXX:-c++}" || die "a C++20 compiler (${CXX:-c++}) is required (Xcode command line tools on macOS, g++ or clang++ on Linux)"

sha256_of() {
    if need sha256sum; then
        sha256sum "$1" | cut -d ' ' -f 1
    elif need shasum; then
        shasum -a 256 "$1" | cut -d ' ' -f 1
    else
        echo "unavailable"
    fi
}

# Wave's standard library: wavec looks in a fixed place; installers do not
# always put it there, so find it when the default is missing.
STD_ROOT=""
if [ -n "${WAVE_STD:-}" ]; then
    STD_ROOT="$WAVE_STD"
else
    default_std="$(wavec print std-path 2> /dev/null || true)"
    if [ -z "$default_std" ] || [ ! -d "$default_std" ]; then
        for candidate in "$HOME/.wave/bin/std" "$HOME/wave/std" "$HOME/.wave/std" /usr/local/lib/wave/std; do
            if [ -d "$candidate" ]; then
                STD_ROOT="$candidate"
                break
            fi
        done
        [ -n "$STD_ROOT" ] || die "wavec cannot find Wave's standard library. Set WAVE_STD to the folder that holds it (often ~/.wave/bin/std)."
    fi
fi

# ---- download -----------------------------------------------------------------------

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT INT TERM

tarball="$tmp/wjson.tar.gz"
if [ -n "${WJSON_TARBALL:-}" ]; then
    case "$WJSON_TARBALL" in
        http://* | https://*) curl -fsSL -o "$tarball" "$WJSON_TARBALL" ;;
        *) cp "$WJSON_TARBALL" "$tarball" ;;
    esac
    say "using $WJSON_TARBALL"
else
    url="https://codeload.github.com/$REPO/tar.gz/$REF"
    say "downloading $url"
    curl -fsSL -o "$tarball" "$url" || die "could not download $url (is '$REF' a tag, branch or commit of $REPO?)"
fi

actual="$(sha256_of "$tarball")"
say "sha256 $actual"
if [ -n "${WJSON_SHA256:-}" ] && [ "$WJSON_SHA256" != "$actual" ]; then
    die "the download does not match WJSON_SHA256 (expected $WJSON_SHA256)"
fi

# ---- unpack and build -------------------------------------------------------------

SRC="$DIR/src"
mkdir -p "$DIR/bin"
rm -rf "${SRC:?}"
mkdir -p "$SRC"
tar -xzf "$tarball" -C "$SRC" --strip-components=1 || die "could not unpack the download"
[ -f "$SRC/build.wave" ] || die "the download does not look like wjson (no build.wave)"

say "building the library"
log="$DIR/build.log"
(
    cd "$SRC"
    if [ -n "$STD_ROOT" ]; then
        WJSON_TASK=libs wavec --std-root="$STD_ROOT" run build.wave
    else
        WJSON_TASK=libs wavec run build.wave
    fi
) > "$log" 2>&1 || {
    tail -n 30 "$log" >&2
    die "the build failed; the full log is in $log"
}

# The build prints the library paths and the linker flags for this machine.
link_flags="$(sed -n 's/^link[[:space:]]*:[[:space:]]*//p' "$log" | tail -n 1)"
native_lib="$SRC/$(sed -n 's/^native[[:space:]]*:[[:space:]]*//p' "$log" | tail -n 1)"
[ -f "$native_lib" ] || die "the build did not produce the library (see $log)"

# ---- the wrapper ----------------------------------------------------------------------

wrapper="$DIR/bin/wjson-wavec"
{
    printf '#!/bin/sh\n'
    printf '# Written by the wjson installer. Compiles a Wave program against wjson:\n'
    printf '#   wjson-wavec hello.wave --run\n'
    printf '# Extra arguments go to `wavec build`. Optimization: WJSON_OPT, default -O2\n'
    printf '# (-O0 on macOS arm64, where Wave issue 933 breaks optimized builds).\n'
    printf 'opt="${WJSON_OPT:-}"\n'
    printf 'if [ -z "$opt" ]; then\n'
    printf '    if [ "$(uname -s)-$(uname -m)" = "Darwin-arm64" ]; then opt=-O0; else opt=-O2; fi\n'
    printf 'fi\n'
    if [ -n "$STD_ROOT" ]; then
        printf 'std="${WAVE_STD:-%s}"\n' "$STD_ROOT"
        printf 'exec wavec --std-root="$std" $opt build "$@" "%s" --dep=wjson="%s" %s\n' \
            "$native_lib" "$SRC" "$link_flags"
    else
        printf 'exec wavec $opt build "$@" "%s" --dep=wjson="%s" %s\n' \
            "$native_lib" "$SRC" "$link_flags"
    fi
} > "$wrapper"
chmod +x "$wrapper"

say ""
say "wjson is installed in $DIR ($OS_ARCH, ref $REF)."
say ""
say "Compile and run a program that uses wjson:"
say "    $wrapper hello.wave --run"
say ""
say "Add the wrapper to your PATH to call it as plain 'wjson-wavec':"
say "    export PATH=\"$DIR/bin:\$PATH\""
say ""
say "In your program:  import(\"wjson\")::{ json_parse, json_root, ... };  (see $SRC/README.md)"
