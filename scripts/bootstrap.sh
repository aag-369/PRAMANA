#!/usr/bin/env bash
# Provision a Rust toolchain. Prefers rustup; falls back to extracting distribution
# packages into a user prefix when rustup is unreachable and root is unavailable.
set -euo pipefail
PREFIX="${PRAMANA_RUST_PREFIX:-$HOME/.pramana-rust}"

if command -v cargo >/dev/null 2>&1; then
  echo "cargo already present: $(cargo --version)"
  exit 0
fi

if curl -sSf https://sh.rustup.rs -o /tmp/rustup.sh 2>/dev/null; then
  sh /tmp/rustup.sh -y
  echo "Installed via rustup. Run: source \$HOME/.cargo/env"
  exit 0
fi

echo "rustup unreachable; falling back to distribution packages"
mkdir -p /tmp/pramana_debs "$PREFIX"
cd /tmp/pramana_debs
apt-get download rustc cargo libstd-rust-dev libstd-rust-1.75 libhttp-parser2.9 libssh2-1
for f in *.deb; do dpkg-deb -x "$f" "$PREFIX"; done
cat > "$PREFIX/env" <<ENVEOF
export PATH=$PREFIX/usr/bin:\$PATH
export LD_LIBRARY_PATH=$PREFIX/usr/lib/x86_64-linux-gnu:$PREFIX/usr/lib:\${LD_LIBRARY_PATH:-}
ENVEOF
echo "Toolchain installed. Run: source $PREFIX/env"
