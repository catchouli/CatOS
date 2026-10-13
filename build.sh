#!/bin/sh
# Builds out/meownix-<arch>.iso.
#
#   ./build.sh                 x86_64 image, for the real box
#   ./build.sh arm64           ARM image, for fast VMs on Apple Silicon
#   ./build.sh [arch] menu     just compile the menu
#
# Everything runs in Docker. live-build works inside a Docker volume rather than
# this folder, because a debootstrap chroot needs a real Linux filesystem.
set -eu
cd "$(dirname "$0")"

arch=amd64
case "${1:-}" in amd64|arm64) arch=$1; shift ;; esac
case $arch in
    amd64) rust_target=x86_64-unknown-linux-gnu ;;
    arm64) rust_target=aarch64-unknown-linux-gnu ;;
esac

FREETUBE_VERSION=0.25.3
FREETUBE_URL="https://github.com/FreeTubeApp/FreeTube/releases/download/v${FREETUBE_VERSION}-beta/freetube_${FREETUBE_VERSION}_beta_${arch}.deb"

mkdir -p out cache

echo "==> Building the menu (Rust, $rust_target)"
docker build -q -t meownix-rust -f builder/rust.Dockerfile builder >/dev/null
docker run --rm \
    -v "$PWD/menu:/src" \
    -v meownix-cargo:/usr/local/cargo/registry \
    -e CARGO_TARGET_DIR=/src/target/linux \
    meownix-rust \
    cargo build --release --target "$rust_target"
cp "menu/target/linux/$rust_target/release/meownix-menu" "out/meownix-menu-$arch"
[ "${1:-}" = menu ] && exit 0

echo "==> Fetching FreeTube ${FREETUBE_VERSION} ($arch)"
deb="cache/freetube_${FREETUBE_VERSION}_${arch}.deb"
[ -f "$deb" ] || curl -fL --progress-bar -o "$deb" "$FREETUBE_URL"

echo "==> Building the image (Debian live-build, $arch)"
docker build -q --platform "linux/$arch" -t "meownix-builder-$arch" builder >/dev/null
docker run --rm --privileged --platform "linux/$arch" \
    -e ARCH="$arch" \
    -v "meownix-build-$arch:/build" \
    -v "$PWD/live:/src/live:ro" \
    -v "$PWD/menu/menu.toml:/src/menu.toml:ro" \
    -v "$PWD/out:/out" \
    -v "$PWD/cache:/cache:ro" \
    "meownix-builder-$arch" sh -euc '
        # Start from a clean config each time, but keep the apt cache between builds.
        cd /build
        lb clean --purge >/dev/null 2>&1 || true
        find /build -mindepth 1 -maxdepth 1 ! -name cache -exec rm -rf {} +
        cp -a /src/live/. /build/

        inc=config/includes.chroot
        install -Dm755 /out/meownix-menu-$ARCH  $inc/usr/local/bin/meownix-menu
        install -Dm644 /src/menu.toml           $inc/etc/meownix/menu.toml
        mkdir -p config/packages.chroot
        cp /cache/freetube_*_$ARCH.deb config/packages.chroot/

        lb config
        lb build 2>&1 | tee /out/build-$ARCH.log | grep --line-buffered -E "^(P:|E:|W:)" || true
        iso=$(ls -t /build/*.iso 2>/dev/null | head -1)
        [ -n "$iso" ] || { echo "No ISO produced, see out/build-$ARCH.log"; exit 1; }
        cp "$iso" /out/meownix-$ARCH.iso
    '
ls -lh "out/meownix-$arch.iso"
