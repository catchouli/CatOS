#!/bin/sh
# Builds out/meownix-<arch>.iso.
#
#   ./build.sh                 x86_64 image, for the real box
#   ./build.sh arm64           ARM image, for fast VMs on Apple Silicon
#   ./build.sh [arch] menu     just compile the menu
#
# Optional extras, from cache/ (not in the repo):
#   cache/CubismSdkForNative-*/   Live2D's Cubism SDK for Native, unzipped
#   cache/avatar/                 a Live2D model (its .model3.json and files)
#   cache/ssh/authorized_keys     SSH keys that can log in as tv (otherwise
#                                 SSH has no way in: password logins are off)
#   cache/flex/                   a Linux build of Flex (unpacked) to use instead
#                                 of the FLEX_VERSION release, e.g. a CI build
#   cache/shares                  network shares to mount (see /etc/meownix/shares)
#   cache/smb-credentials         the login for them (username=, password=)
# With both, the menu shows the avatar; see [avatar] in menu/menu.toml.
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

# Flex, installed at /opt/flex (x86_64 only: there's no ARM build). To update,
# change both lines; the checksum is on the release page.
FLEX_VERSION=0.10.0
FLEX_SHA256=0d4c41db2b712aa9fb070ceb76c0c7a8ca0b5eac1b572fe285aa7d142f674e25
FLEX_URL="https://github.com/catchouli/flex/releases/download/v${FLEX_VERSION}/flex-v${FLEX_VERSION}-linux-x64.tar.gz"

mkdir -p out cache

# Live2D's SDK, if it's been downloaded (newest first).
cubism_sdk=$(ls -d cache/CubismSdkForNative-*/ 2>/dev/null | sort -V | tail -1)
if [ -n "$cubism_sdk" ]; then
    sdk_args="-v $PWD/$cubism_sdk:/sdk:ro -e MEOWNIX_CUBISM_SDK=/sdk"
    echo "==> Building the menu (Rust, $rust_target, with Live2D from $cubism_sdk)"
else
    sdk_args=
    echo "==> Building the menu (Rust, $rust_target)"
fi
docker build -q -t meownix-rust -f builder/rust.Dockerfile builder >/dev/null
# shellcheck disable=SC2086 # sdk_args is a list of arguments
docker run --rm \
    -v "$PWD/menu:/src" \
    -v meownix-cargo:/usr/local/cargo/registry \
    -e CARGO_TARGET_DIR=/src/target/linux \
    $sdk_args \
    meownix-rust \
    cargo build --release --target "$rust_target"
cp "menu/target/linux/$rust_target/release/meownix-menu" "out/meownix-menu-$arch"
[ "${1:-}" = menu ] && exit 0

echo "==> Fetching FreeTube ${FREETUBE_VERSION} ($arch)"
deb="cache/freetube_${FREETUBE_VERSION}_${arch}.deb"
[ -f "$deb" ] || curl -fL --progress-bar -o "$deb" "$FREETUBE_URL"

if [ "$arch" = amd64 ] && [ ! -d cache/flex ]; then
    echo "==> Fetching Flex ${FLEX_VERSION}"
    flex_tar="cache/flex-v${FLEX_VERSION}-linux-x64.tar.gz"
    [ -f "$flex_tar" ] || curl -fL --progress-bar -o "$flex_tar" "$FLEX_URL"
    if ! echo "$FLEX_SHA256  $flex_tar" | shasum -a 256 -c - >/dev/null 2>&1 &&
       ! echo "$FLEX_SHA256  $flex_tar" | sha256sum -c - >/dev/null 2>&1; then
        echo "$flex_tar doesn't match FLEX_SHA256; delete it to download it again"
        exit 1
    fi
fi

echo "==> Building the image (Debian live-build, $arch)"
docker build -q --platform "linux/$arch" -t "meownix-builder-$arch" builder >/dev/null
docker run --rm --privileged --platform "linux/$arch" \
    -e ARCH="$arch" \
    -e FLEX_VERSION="$FLEX_VERSION" \
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

        # The Live2D avatar, if there is one: the model, and the shaders
        # the Live2D renderer loads at run time.
        if [ -d /cache/avatar ]; then
            mkdir -p $inc/usr/share/meownix
            cp -r /cache/avatar $inc/usr/share/meownix/avatar
        fi
        if [ "$ARCH" = amd64 ]; then
            mkdir -p $inc/opt/flex
            if [ -x /cache/flex/flex ]; then
                cp -a /cache/flex/. $inc/opt/flex/
            else
                tar -xzf /cache/flex-v$FLEX_VERSION-linux-x64.tar.gz -C $inc/opt/flex --strip-components=1
            fi
        fi
        [ -f /cache/shares ] && install -m644 /cache/shares $inc/etc/meownix/shares
        [ -f /cache/smb-credentials ] && install -m600 /cache/smb-credentials $inc/etc/meownix/smb-credentials
        if [ -f /cache/ssh/authorized_keys ]; then
            install -d -m700 $inc/etc/skel-meownix/.ssh
            install -m600 /cache/ssh/authorized_keys $inc/etc/skel-meownix/.ssh/authorized_keys
        fi
        shaders=$(ls -d /cache/CubismSdkForNative-*/Framework/src/Rendering/OpenGL/Shaders/StandardES 2>/dev/null | sort -V | tail -1)
        if [ -n "$shaders" ]; then
            mkdir -p $inc/usr/share/meownix
            cp -r "$shaders" $inc/usr/share/meownix/live2d-shaders
        fi

        lb config
        lb build 2>&1 | tee /out/build-$ARCH.log | grep --line-buffered -E "^(P:|E:|W:)" || true
        iso=$(ls -t /build/*.iso 2>/dev/null | head -1)
        [ -n "$iso" ] || { echo "No ISO produced, see out/build-$ARCH.log"; exit 1; }
        cp "$iso" /out/meownix-$ARCH.iso
    '
ls -lh "out/meownix-$arch.iso"
