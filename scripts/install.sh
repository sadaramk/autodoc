#!/usr/bin/env sh
# Install a released autodoc binary.
#
#   curl -fsSL https://github.com/sadaramk/autodoc/releases/latest/download/install.sh | sh
#
# Environment:
#   AUTODOC_VERSION  release tag to install (default: latest)
#   AUTODOC_BIN_DIR  where to put the binary (default: the first writable of
#                    /usr/local/bin, $HOME/.local/bin)
#
# The checksum published with the release is verified before anything is
# installed. POSIX sh on purpose: this runs before autodoc does, on whatever the
# machine has.
set -eu

REPO="${AUTODOC_REPO:-sadaramk/autodoc}"
version="${AUTODOC_VERSION:-latest}"

die() {
  echo "install: $1" >&2
  exit 1
}

case "$(uname -s)" in
  Linux) os=unknown-linux-musl ;;
  Darwin) os=apple-darwin ;;
  *) die "unsupported operating system $(uname -s); see https://github.com/$REPO/releases" ;;
esac

case "$(uname -m)" in
  x86_64 | amd64) arch=x86_64 ;;
  arm64 | aarch64) arch=aarch64 ;;
  *) die "unsupported architecture $(uname -m)" ;;
esac

target="$arch-$os"

if [ "$version" = latest ]; then
  version=$(curl -fsSL "https://api.github.com/repos/$REPO/releases/latest" |
    sed -n 's/.*"tag_name"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' | head -1)
  [ -n "$version" ] || die "could not resolve the latest release of $REPO"
fi
plain="${version#v}"

name="autodoc-$plain-$target"
base="https://github.com/$REPO/releases/download/$version"

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT HUP INT TERM

echo "install: downloading $name.tar.gz"
curl -fsSL -o "$work/$name.tar.gz" "$base/$name.tar.gz" ||
  die "no binary for $target in $version"
curl -fsSL -o "$work/SHA256SUMS" "$base/SHA256SUMS" || die "$version publishes no SHA256SUMS"

want=$(awk -v f="$name.tar.gz" '$2 == f || $2 == "*" f { print $1 }' "$work/SHA256SUMS" | head -1)
[ -n "$want" ] || die "$name.tar.gz is not listed in SHA256SUMS"
if command -v sha256sum >/dev/null 2>&1; then
  got=$(sha256sum "$work/$name.tar.gz" | cut -d' ' -f1)
else
  got=$(shasum -a 256 "$work/$name.tar.gz" | cut -d' ' -f1)
fi
[ "$want" = "$got" ] || die "checksum mismatch for $name.tar.gz (expected $want, got $got)"

tar xzf "$work/$name.tar.gz" -C "$work"

dir="${AUTODOC_BIN_DIR:-}"
if [ -z "$dir" ]; then
  for candidate in /usr/local/bin "$HOME/.local/bin"; do
    if [ -d "$candidate" ] && [ -w "$candidate" ]; then
      dir="$candidate"
      break
    fi
  done
fi
if [ -z "$dir" ]; then
  dir="$HOME/.local/bin"
  mkdir -p "$dir"
fi

install -m 755 "$work/$name/autodoc" "$dir/autodoc" 2>/dev/null ||
  die "cannot write to $dir; set AUTODOC_BIN_DIR to a directory you own"

echo "install: $("$dir/autodoc" --version) → $dir/autodoc"
case ":$PATH:" in
  *":$dir:"*) ;;
  *) echo "install: add $dir to PATH" ;;
esac
