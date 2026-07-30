#!/bin/sh

set -eu

repository="LVTD-LLC/reviewgate"
install_dir="${REVIEWGATE_INSTALL_DIR:-${HOME}/.local/bin}"
version="${REVIEWGATE_VERSION:-latest}"

fail() {
  printf 'reviewgate installer: %s\n' "$1" >&2
  exit 1
}

command -v curl >/dev/null 2>&1 || fail "curl is required"
command -v tar >/dev/null 2>&1 || fail "tar is required"

case "$(uname -s):$(uname -m)" in
  Linux:x86_64 | Linux:amd64)
    target="x86_64-unknown-linux-gnu"
    ;;
  Darwin:arm64 | Darwin:aarch64)
    target="aarch64-apple-darwin"
    ;;
  Darwin:x86_64 | Darwin:amd64)
    target="x86_64-apple-darwin"
    ;;
  *)
    fail "unsupported platform $(uname -s) $(uname -m); use cargo install --git https://github.com/${repository} --locked reviewgate-cli"
    ;;
esac

archive="reviewgate-${target}.tar.gz"
if [ "$version" = "latest" ]; then
  release_url="https://github.com/${repository}/releases/latest/download"
else
  case "$version" in
    v[0-9]*)
      ;;
    *)
      fail "REVIEWGATE_VERSION must be latest or a release tag such as v0.8.0"
      ;;
  esac
  release_url="https://github.com/${repository}/releases/download/${version}"
fi

temporary_dir="$(mktemp -d 2>/dev/null || mktemp -d -t reviewgate)"
temporary_target=""
cleanup() {
  if [ -n "$temporary_target" ]; then
    rm -f "$temporary_target"
  fi
  rm -rf "$temporary_dir"
}
trap cleanup EXIT HUP INT TERM

printf 'Downloading ReviewGate %s for %s...\n' "$version" "$target"
curl --proto '=https' --tlsv1.2 -LsSf \
  "${release_url}/${archive}" \
  -o "${temporary_dir}/${archive}"
curl --proto '=https' --tlsv1.2 -LsSf \
  "${release_url}/${archive}.sha256" \
  -o "${temporary_dir}/${archive}.sha256"

expected_checksum="$(awk 'NR == 1 { print $1 }' "${temporary_dir}/${archive}.sha256")"
case "$expected_checksum" in
  *[!0-9a-fA-F]* | "")
    fail "release checksum is invalid"
    ;;
esac
[ "${#expected_checksum}" -eq 64 ] || fail "release checksum is invalid"

if command -v sha256sum >/dev/null 2>&1; then
  actual_checksum="$(sha256sum "${temporary_dir}/${archive}" | awk '{ print $1 }')"
elif command -v shasum >/dev/null 2>&1; then
  actual_checksum="$(shasum -a 256 "${temporary_dir}/${archive}" | awk '{ print $1 }')"
else
  fail "sha256sum or shasum is required to verify the download"
fi

[ "$actual_checksum" = "$expected_checksum" ] || fail "release checksum verification failed"

tar -xzf "${temporary_dir}/${archive}" -C "$temporary_dir"
[ -f "${temporary_dir}/reviewgate" ] || fail "release archive does not contain reviewgate"

mkdir -p "$install_dir"
install_target="${install_dir}/reviewgate"
temporary_target="$(mktemp "${install_dir}/.reviewgate-install.XXXXXX")"
cp "${temporary_dir}/reviewgate" "$temporary_target"
chmod 0755 "$temporary_target"
mv -f "$temporary_target" "$install_target"
temporary_target=""

printf 'Installed ReviewGate to %s\n' "$install_target"
case ":${PATH}:" in
  *":${install_dir}:"*)
    ;;
  *)
    printf 'Add %s to PATH to run reviewgate from any directory.\n' "$install_dir"
    ;;
esac
