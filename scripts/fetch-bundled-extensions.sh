#!/usr/bin/env bash
# Downloads the pinned first-party extensions that ship inside pup release
# archives and verifies them against pinned SHA-256 sums. Output layout matches
# the archive `files:` entries in .goreleaser-*.yaml:
#   dist-bundled/<goos>-<goarch>/pup-setup
# Kept to bash 3.2 features because the macOS release runner uses /bin/bash.
set -euo pipefail

SETUP_REPO="DataDog/pup-setup"
# Placeholder pin: no pup-setup release exists yet.
SETUP_TAG="v0.0.0"

setup_sha256() {
  case "$1" in
    linux-amd64) echo "0000000000000000000000000000000000000000000000000000000000000000" ;;
    linux-arm64) echo "0000000000000000000000000000000000000000000000000000000000000000" ;;
    darwin-amd64) echo "0000000000000000000000000000000000000000000000000000000000000000" ;;
    darwin-arm64) echo "0000000000000000000000000000000000000000000000000000000000000000" ;;
    *) return 1 ;;
  esac
}

asset_arch() {
  case "$1" in
    amd64) echo "x86_64" ;;
    arm64) echo "aarch64" ;;
    *) return 1 ;;
  esac
}

out_root="${1:-dist-bundled}"

for platform in linux-amd64 linux-arm64 darwin-amd64 darwin-arm64; do
  goos="${platform%-*}"
  goarch="${platform#*-}"
  asset="pup-setup-${goos}-$(asset_arch "$goarch")"
  expected="$(setup_sha256 "$platform")"
  dest_dir="${out_root}/${platform}"
  dest="${dest_dir}/pup-setup"

  mkdir -p "$dest_dir"
  curl --fail --silent --show-error --location \
    --output "$dest" \
    "https://github.com/${SETUP_REPO}/releases/download/${SETUP_TAG}/${asset}"

  actual="$(shasum -a 256 "$dest" | cut -d' ' -f1)"
  if [[ "$actual" != "$expected" ]]; then
    echo "checksum mismatch for ${asset}: expected ${expected}, got ${actual}" >&2
    exit 1
  fi
  chmod 0755 "$dest"
done
