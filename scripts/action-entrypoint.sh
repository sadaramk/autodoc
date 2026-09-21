#!/usr/bin/env bash
# Download a released nunki binary for this runner and run one subcommand.
#
# A checksum from the release's own SHA256SUMS is verified before the binary is
# executed: the download is fetched over the network into a job that may hold
# repository credentials, so "it unpacked" is not enough.
set -euo pipefail

REPO="${NUNKI_ACTION_REPO:-sadaramk/nunki}"
version="${NUNKI_VERSION:-}"

# Pinning the action has to pin the binary. `uses: …/nunki@v0.2.6` sets
# GITHUB_ACTION_REF to `v0.2.6`, and a workflow that pinned that ref expects
# that build — not whatever was released since. Only an exact vX.Y.Z is used:
# a moving major tag (`@v0`) or a branch (`@main`) names no single release, so
# those fall through to the newest one.
if [ -z "$version" ]; then
  case "${GITHUB_ACTION_REF:-}" in
    v[0-9]*.[0-9]*.[0-9]*) version="$GITHUB_ACTION_REF" ;;
    *) version=latest ;;
  esac
fi

case "$(uname -s)" in
  Linux) os=unknown-linux-musl ;;
  Darwin) os=apple-darwin ;;
  MINGW* | MSYS* | CYGWIN* | Windows_NT) os=pc-windows-msvc ;;
  *)
    echo "nunki: unsupported operating system $(uname -s)" >&2
    exit 1
    ;;
esac

case "$(uname -m)" in
  x86_64 | amd64) arch=x86_64 ;;
  arm64 | aarch64) arch=aarch64 ;;
  *)
    echo "nunki: unsupported architecture $(uname -m)" >&2
    exit 1
    ;;
esac

target="$arch-$os"
if [ "$target" = "aarch64-pc-windows-msvc" ]; then
  echo "nunki: no release binary for $target yet" >&2
  exit 1
fi

api() {
  if [ -n "${GH_TOKEN:-}" ]; then
    curl -fsSL -H "Authorization: Bearer $GH_TOKEN" -H "X-GitHub-Api-Version: 2022-11-28" "$@"
  else
    curl -fsSL "$@"
  fi
}

if [ "$version" = "latest" ]; then
  version=$(api "https://api.github.com/repos/$REPO/releases/latest" |
    sed -n 's/.*"tag_name"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' | head -1)
  if [ -z "$version" ]; then
    echo "nunki: could not resolve the latest release of $REPO" >&2
    exit 1
  fi
fi
plain="${version#v}"

ext=tar.gz
if [ "$os" = "pc-windows-msvc" ]; then
  ext=zip
fi
name="nunki-$plain-$target"
base="https://github.com/$REPO/releases/download/$version"

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
echo "nunki: downloading $name.$ext"
curl -fsSL -o "$work/$name.$ext" "$base/$name.$ext"
curl -fsSL -o "$work/SHA256SUMS" "$base/SHA256SUMS"

want=$(awk -v f="$name.$ext" '$2 == f || $2 == "*" f { print $1 }' "$work/SHA256SUMS" | head -1)
if [ -z "$want" ]; then
  echo "nunki: $name.$ext is not listed in the release's SHA256SUMS" >&2
  exit 1
fi
if command -v sha256sum >/dev/null 2>&1; then
  got=$(sha256sum "$work/$name.$ext" | cut -d' ' -f1)
else
  got=$(shasum -a 256 "$work/$name.$ext" | cut -d' ' -f1)
fi
if [ "$want" != "$got" ]; then
  echo "nunki: checksum mismatch for $name.$ext (expected $want, got $got)" >&2
  exit 1
fi

if [ "$ext" = zip ]; then
  unzip -q "$work/$name.$ext" -d "$work"
else
  tar xzf "$work/$name.$ext" -C "$work"
fi

src="$work/$name/nunki"
if [ -f "$src.exe" ]; then
  src="$src.exe"
fi
# `outputs.binary` is a path a later step may use, so the binary is moved out of
# the directory this script cleans up. Previously it survived only because the
# script ended in `exec`, which skips the trap — commenting means it no longer does.
keep="${RUNNER_TEMP:-/tmp}/nunki-$plain-$target"
mkdir -p "$keep"
bin="$keep/$(basename "$src")"
mv -f "$src" "$bin"
chmod +x "$bin"
echo "binary=$bin" >>"${GITHUB_OUTPUT:-/dev/null}"

set -- "${NUNKI_COMMAND:-check}" "${NUNKI_PATH:-.}"
if [ -n "${NUNKI_OUT:-}" ]; then
  set -- "$@" --out "$NUNKI_OUT"
fi
# Word-split deliberately: `args` is a command line the caller wrote.
if [ -n "${NUNKI_ARGS:-}" ]; then
  # shellcheck disable=SC2086
  set -- "$@" $NUNKI_ARGS
fi

"$bin" --version

# Posting the diff on a pull request is its own script: it decides what to say
# from `diff --exit-code` and which comment to edit, none of which needs the
# network dance above, so it is testable on its own.
if [ "${NUNKI_COMMENT:-false}" = "true" ]; then
  if [ "${NUNKI_COMMAND:-check}" != "diff" ]; then
    echo "nunki: comment: true only applies to 'command: diff' (got ${NUNKI_COMMAND:-check})" >&2
    exit 1
  fi
  shift # the subcommand; action-comment.sh supplies `diff` itself
  exec bash "$GITHUB_ACTION_PATH/scripts/action-comment.sh" "$bin" "$@"
fi

echo "nunki: $*"
exec "$bin" "$@"
