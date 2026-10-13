#!/bin/sh
# Builds out/meownix-amd64.iso.
#
#   ./build.sh          menu + ISO
#   ./build.sh menu     just cross-compile the menu
#
# Everything runs in Docker. live-build works inside a Docker volume rather than
# this folder, because a debootstrap chroot needs a real Linux filesystem.
set -eu
cd "$(dirname "$0")"

FREETUBE_VERSION=0.25.3
FREETUBE_URL="https://github.com/FreeTubeApp/FreeTube/releases/download/v${FREETUBE_VERSION}-beta/freetube_${FREETUBE_VERSION}_beta_amd64.deb"

mkdir -p out cache

echo "==> Building the menu (Rust, cross-compiled to x86_64)"
docker build -q -t meownix-rust -f builder/rust.Dockerfile builder >/dev/null
docker run --rm \
    -v "$PWD/menu:/src" \
    -v meownix-cargo:/usr/local/cargo/registry \
    -e CARGO_TARGET_DIR=/src/target/linux \
    meownix-rust \
    cargo build --release --target x86_64-unknown-linux-gnu
cp menu/target/linux/x86_64-unknown-linux-gnu/release/meownix-menu out/meownix-menu
[ "${1:-}" = menu ] && exit 0

echo "==> Fetching FreeTube ${FREETUBE_VERSION}"
deb="cache/freetube_${FREETUBE_VERSION}_amd64.deb"
[ -f "$deb" ] || curl -fL --progress-bar -o "$deb" "$FREETUBE_URL"

echo "==> Building the image (Debian live-build, x86_64 under emulation; slow)"
docker build -q --platform linux/amd64 -t meownix-builder builder >/dev/null
docker run --rm --privileged --platform linux/amd64 \
    -v meownix-build:/build \
    -v "$PWD/live:/src/live:ro" \
    -v "$PWD/menu/menu.toml:/src/menu.toml:ro" \
    -v "$PWD/out:/out" \
    -v "$PWD/cache:/cache:ro" \
    meownix-builder sh -euc '
        # Start from a clean config each time, but keep the apt cache between builds.
        cd /build
        lb clean --purge >/dev/null 2>&1 || true
        find /build -mindepth 1 -maxdepth 1 ! -name cache -exec rm -rf {} +
        cp -a /src/live/. /build/

        inc=config/includes.chroot
        install -Dm755 /out/meownix-menu        $inc/usr/local/bin/meownix-menu
        install -Dm644 /src/menu.toml           $inc/etc/meownix/menu.toml
        mkdir -p config/packages.chroot
        cp /cache/freetube_*_amd64.deb config/packages.chroot/

        lb config
        lb build 2>&1 | tee /out/build.log | grep -E "^(P:|E:|W:)" || true
        iso=$(ls -t /build/*.iso 2>/dev/null | head -1)
        [ -n "$iso" ] || { echo "No ISO produced, see out/build.log"; exit 1; }
        cp "$iso" /out/meownix-amd64.iso
    '
ls -lh out/meownix-amd64.iso
