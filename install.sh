#!/bin/sh
# Install the pup CLI (https://github.com/DataDog/pup).
#
#   curl -fsSL https://raw.githubusercontent.com/DataDog/pup/main/install.sh | sh
#
# On macOS with Homebrew available, installs via `brew install datadog-labs/pack/pup`
# unless PUP_VERSION or PUP_INSTALL_DIR is set, since Homebrew cannot honor them.
# Everywhere else (Linux, macOS without Homebrew, Windows via Git Bash/MSYS2/Cygwin),
# downloads the release archive, verifies its SHA-256 checksum, and installs the binary.
#
# Environment variables:
#   PUP_VERSION      Version to install, e.g. 1.27.0 (default: latest release)
#   PUP_INSTALL_DIR  Directory for the binary (default: $HOME/.local/bin)
#   PUP_NO_BREW      Set to 1 to skip Homebrew on macOS and download directly

set -eu

REPO="DataDog/pup"
BREW_FORMULA="datadog-labs/pack/pup"

say() { printf 'pup-install: %s\n' "$*" >&2; }
die() {
  say "error: $*"
  exit 1
}

need() {
  command -v "$1" >/dev/null 2>&1 || die "required command not found: $1"
}

detect_os() {
  case "$(uname -s)" in
    Darwin) echo Darwin ;;
    Linux) echo Linux ;;
    MINGW* | MSYS* | CYGWIN*) echo Windows ;;
    *) die "unsupported operating system: $(uname -s)" ;;
  esac
}

detect_arch() {
  os="$1"
  arch="$(uname -m)"
  case "$arch" in
    x86_64 | amd64) arch=x86_64 ;;
    arm64 | aarch64) arch=arm64 ;;
    *) die "unsupported architecture: $arch" ;;
  esac
  # A shell running under Rosetta reports x86_64 on Apple Silicon.
  if [ "$os" = Darwin ] && [ "$arch" = x86_64 ] &&
    [ "$(sysctl -n sysctl.proc_translated 2>/dev/null || true)" = 1 ]; then
    arch=arm64
  fi
  # Only an x86_64 Windows build is published; Windows on ARM runs it under emulation.
  if [ "$os" = Windows ]; then
    arch=x86_64
  fi
  echo "$arch"
}

latest_version() {
  # Follow the /releases/latest redirect instead of the rate-limited GitHub API.
  url="$(curl -fsSLI -o /dev/null -w '%{url_effective}' "https://github.com/${REPO}/releases/latest")" ||
    die "could not resolve the latest release"
  case "$url" in
    */tag/v*) echo "${url##*/tag/v}" ;;
    *) die "unexpected latest release URL: $url" ;;
  esac
}

sha256_of() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | awk '{print $1}'
  elif command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "$1" | awk '{print $1}'
  else
    die "sha256sum or shasum is required to verify the download"
  fi
}

verify_checksum() {
  file="$1"
  checksums="$2"
  name="$(basename "$file")"
  expected="$(awk -v f="$name" '$2 == f {print $1}' "$checksums")"
  [ -n "$expected" ] || die "no checksum listed for $name"
  actual="$(sha256_of "$file")"
  [ "$expected" = "$actual" ] || die "checksum mismatch for $name (expected $expected, got $actual)"
}

extract() {
  archive="$1"
  dest="$2"
  case "$archive" in
    *.tar.gz) tar -xzf "$archive" -C "$dest" ;;
    *.zip)
      if command -v unzip >/dev/null 2>&1; then
        unzip -q "$archive" -d "$dest"
      else
        powershell.exe -NoProfile -Command \
          "Expand-Archive -LiteralPath '$(cygpath -w "$archive")' -DestinationPath '$(cygpath -w "$dest")'"
      fi
      ;;
  esac
}

install_with_brew() {
  say "Homebrew detected; running: brew install $BREW_FORMULA"
  brew install "$BREW_FORMULA" </dev/null
  say "installed $(command -v pup)"
}

install_direct() {
  os="$1"
  arch="$(detect_arch "$os")"
  version="${PUP_VERSION:-$(latest_version)}"
  version="${version#v}"
  ext=tar.gz
  bin=pup
  if [ "$os" = Windows ]; then
    ext=zip
    bin=pup.exe
  fi
  asset="pup_${version}_${os}_${arch}.${ext}"
  base="https://github.com/${REPO}/releases/download/v${version}"
  install_dir="${PUP_INSTALL_DIR:-$HOME/.local/bin}"

  tmp="$(mktemp -d)"
  # shellcheck disable=SC2064 # expand $tmp now, not when the trap fires
  trap "rm -rf '$tmp'" EXIT

  say "downloading $asset"
  curl -fsSL -o "$tmp/$asset" "$base/$asset" ||
    die "download failed: $base/$asset (does version $version exist for $os/$arch?)"
  curl -fsSL -o "$tmp/checksums.txt" "$base/pup_${version}_checksums.txt" ||
    die "download failed: $base/pup_${version}_checksums.txt"
  verify_checksum "$tmp/$asset" "$tmp/checksums.txt"

  mkdir -p "$tmp/extract"
  extract "$tmp/$asset" "$tmp/extract"
  [ -f "$tmp/extract/$bin" ] || die "$bin not found in $asset"

  mkdir -p "$install_dir"
  # Replace via mv so a running pup binary is not overwritten in place.
  cp "$tmp/extract/$bin" "$install_dir/$bin.tmp"
  chmod 755 "$install_dir/$bin.tmp"
  mv -f "$install_dir/$bin.tmp" "$install_dir/$bin"
  say "installed pup $version to $install_dir/$bin"

  case ":$PATH:" in
    *":$install_dir:"*) ;;
    *) say "note: $install_dir is not on your PATH; add it with: export PATH=\"$install_dir:\$PATH\"" ;;
  esac
}

main() {
  need uname
  os="$(detect_os)"
  if [ "$os" = Darwin ] && [ "${PUP_NO_BREW:-0}" != 1 ] &&
    [ -z "${PUP_VERSION:-}" ] && [ -z "${PUP_INSTALL_DIR:-}" ] &&
    command -v brew >/dev/null 2>&1; then
    install_with_brew
    return
  fi
  need curl
  need tar
  need awk
  install_direct "$os"
}

# POSIX sh: no `local`, so function variables are script-global.
# Everything runs from main so a truncated `curl | sh` download executes nothing.
main
