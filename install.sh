#!/bin/sh
# readingbuddy installer.
#
#   curl -fsSL https://raw.githubusercontent.com/bongofongo/readingbuddy/main/install.sh | sh
#
# Downloads the release archive for this machine, checks it against the
# release's SHA256SUMS, and puts `readingbuddy-tui` and `readingbuddy` in
# ~/.local/bin. Nothing else is touched: no shell profile is edited, no package
# manager is invoked, nothing is installed system-wide.
#
# Knobs, all environment variables (the script is usually read from a pipe, so
# it cannot take flags on the shell's own command line):
#
#   RB_VERSION      pin a version, e.g. v0.1.0   (default: the latest release)
#   RB_INSTALL_DIR  where the binaries go        (default: ~/.local/bin)
#   RB_NO_VERIFY=1  skip the checksum            (default: verify, and refuse
#                                                 to install if it fails)
#
# POSIX sh on purpose — /bin/sh is dash on Debian and Ubuntu, and a bashism
# here is a syntax error on the machines most likely to run this.

set -eu

REPO="bongofongo/readingbuddy"
BASE="https://github.com/${REPO}"

INSTALL_DIR="${RB_INSTALL_DIR:-${XDG_BIN_HOME:-${HOME}/.local/bin}}"
BINS="readingbuddy-tui readingbuddy"

# ---------------------------------------------------------------- plumbing --

say()  { printf '%s\n' "$*"; }
warn() { printf 'warning: %s\n' "$*" >&2; }
die()  { printf 'error: %s\n' "$*" >&2; exit 1; }
have() { command -v "$1" >/dev/null 2>&1; }

if have curl; then
    fetch()      { curl -fsSL "$1" -o "$2"; }
    fetch_out()  { curl -fsSL "$1"; }
    # /releases/latest is a redirect to /releases/tag/vX.Y.Z, so the tag can be
    # read off the final URL. Deliberately not the JSON API: that is rate
    # limited to 60 requests an hour per IP, which a shared network or a CI
    # runner can exhaust without anyone doing anything wrong.
    latest_tag() {
        _u=$(curl -fsSLI -o /dev/null -w '%{url_effective}' "${BASE}/releases/latest")
        printf '%s\n' "${_u##*/}"
    }
elif have wget; then
    fetch()      { wget -qO "$2" "$1"; }
    fetch_out()  { wget -qO- "$1"; }
    # No redirect-URL reporting in wget, so this path does pay the API's rate
    # limit. Pin RB_VERSION if that ever bites.
    latest_tag() {
        wget -qO- "https://api.github.com/repos/${REPO}/releases/latest" \
            | sed -n 's/.*"tag_name": *"\([^"]*\)".*/\1/p' \
            | head -n 1
    }
else
    die "need curl or wget."
fi

# ------------------------------------------------------------- what am I? --

detect_target() {
    _os=$(uname -s)
    _arch=$(uname -m)

    # Under Rosetta 2 a shell reports x86_64 on an arm64 Mac. Believing it
    # installs the Intel build on Apple silicon: it runs, slowly, for ever, and
    # nothing on screen says why.
    if [ "$_os" = "Darwin" ] && [ "$_arch" = "x86_64" ]; then
        if [ "$(sysctl -n sysctl.proc_translated 2>/dev/null || echo 0)" = "1" ]; then
            _arch="arm64"
        fi
    fi

    case "$_os" in
        Linux)  _sys="unknown-linux-gnu" ;;
        Darwin) _sys="apple-darwin" ;;
        *) die "no prebuilt binaries for ${_os}. Build from source: cargo install --git ${BASE}.git readingbuddy-tui" ;;
    esac

    case "$_arch" in
        x86_64|amd64)  _cpu="x86_64" ;;
        aarch64|arm64) _cpu="aarch64" ;;
        *) die "no prebuilt binaries for ${_arch}. Build from source: cargo install --git ${BASE}.git readingbuddy-tui" ;;
    esac

    printf '%s-%s\n' "$_cpu" "$_sys"
}

verify() {
    # $1 = archive name, run from the directory holding it and SHA256SUMS.
    if [ "${RB_NO_VERIFY:-0}" = "1" ]; then
        warn "skipping checksum verification (RB_NO_VERIFY=1)."
        return 0
    fi
    grep " ${1}\$" SHA256SUMS > SHA256SUMS.one \
        || die "${1} is not listed in the release's SHA256SUMS."
    if have sha256sum; then
        sha256sum -c SHA256SUMS.one >/dev/null || die "checksum mismatch on ${1}. Not installing."
    elif have shasum; then
        shasum -a 256 -c SHA256SUMS.one >/dev/null || die "checksum mismatch on ${1}. Not installing."
    else
        warn "no sha256sum or shasum here; the download was not verified."
    fi
}

# ------------------------------------------------------------------- main --

target=$(detect_target)

version="${RB_VERSION:-}"
if [ -z "$version" ]; then
    version=$(latest_tag) || true
    [ -n "$version" ] || die "could not work out the latest version. Set RB_VERSION, e.g. RB_VERSION=v0.1.0."
fi
case "$version" in v*) ;; *) version="v${version}" ;; esac

archive="readingbuddy-${version}-${target}.tar.gz"
url="${BASE}/releases/download/${version}/${archive}"

say "readingbuddy ${version} (${target})"

tmp=$(mktemp -d "${TMPDIR:-/tmp}/readingbuddy.XXXXXX") || die "could not make a temp directory."
# `trap ... EXIT` alone leaves the directory behind when the shell is killed;
# naming the signals too is what makes a Ctrl-C tidy up after itself.
trap 'rm -rf "$tmp"' EXIT INT TERM HUP

say "downloading ${archive}"
fetch "$url" "${tmp}/${archive}" \
    || die "no such release asset. Check ${BASE}/releases for ${version}."
fetch "${BASE}/releases/download/${version}/SHA256SUMS" "${tmp}/SHA256SUMS" \
    || die "could not fetch SHA256SUMS for ${version}."

( cd "$tmp" && verify "$archive" )
say "checksum ok"

tar -xzf "${tmp}/${archive}" -C "$tmp" || die "could not unpack ${archive}."
unpacked="${tmp}/readingbuddy-${version}-${target}"

mkdir -p "$INSTALL_DIR" || die "could not create ${INSTALL_DIR}."
[ -w "$INSTALL_DIR" ] || die "${INSTALL_DIR} is not writable. Set RB_INSTALL_DIR to somewhere you own."

for bin in $BINS; do
    [ -f "${unpacked}/${bin}" ] || die "${bin} is missing from the archive."
    # Copy beside the destination and rename over it. A plain `cp` onto a
    # binary that is currently running fails with ETXTBSY on Linux, which is
    # exactly what happens when you upgrade from inside the TUI's own pane.
    cp "${unpacked}/${bin}" "${INSTALL_DIR}/${bin}.new"
    chmod 755 "${INSTALL_DIR}/${bin}.new"
    mv -f "${INSTALL_DIR}/${bin}.new" "${INSTALL_DIR}/${bin}"
    say "installed ${INSTALL_DIR}/${bin}"
done

say ""
case ":${PATH}:" in
    *":${INSTALL_DIR}:"*) ;;
    *)
        say "${INSTALL_DIR} is not on your PATH. Add it:"
        say ""
        say "    export PATH=\"${INSTALL_DIR}:\$PATH\""
        say ""
        ;;
esac

# Not a footnote. The data root defaults to the current directory, so without
# this every directory you launch from quietly becomes a separate library —
# and the first symptom is an empty shelf on a machine that has one.
say "Set your library's home once, in your shell profile:"
say ""
say "    export READINGBUDDY_DATA_DIR=\"\$HOME/.local/share/readingbuddy\""
say ""
say "Then: readingbuddy-tui   (the reading room)"
say "      readingbuddy       (the command line — try 'readingbuddy --help')"
