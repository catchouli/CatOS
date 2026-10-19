#!/bin/sh
# Boots a catos ISO in a VM with UEFI, like the real box.
#
#   vm/run.sh [arm64|amd64] [window|headless]
#
#   arm64      out/catos-arm64.iso with Apple's Virtualization framework (via vfkit):
#              near-native speed on Apple Silicon. The default on an ARM Mac; use it
#              for testing. Always opens a window (Ctrl+Cmd+F for full screen);
#              quit vfkit to stop it. 1280x720: the guest draws in software.
#   amd64      out/catos-amd64.iso, the image for the real box. x86 is emulated
#              on an ARM Mac, so it's very slow: only for checking that it boots.
#
#   (default)  full screen, scaled to fit (Ctrl+Opt+F leaves full screen,
#              Ctrl+Opt+G releases the mouse)
#   window     in a resizable window
#   headless   no window; control it with vm/ctl.py
set -eu
cd "$(dirname "$0")/.."

arch=amd64
[ "$(uname -m)" = arm64 ] && arch=arm64
case "${1:-}" in amd64|arm64) arch=$1; shift ;; esac

iso="out/catos-$arch.iso"
[ -f "$iso" ] || { echo "No $iso yet: run ./build.sh $arch"; exit 1; }
share="$(brew --prefix qemu)/share/qemu"
state=out/vm
mkdir -p "$state"

# zoom-to-fit scales the guest screen to the window, so it isn't tiny on Retina screens.
case "${1:-}" in
    headless) display="-display none -vnc 127.0.0.1:1" ;;
    window)   display="-display cocoa,zoom-to-fit=on" ;;
    *)        display="-display cocoa,zoom-to-fit=on,full-screen=on" ;;
esac

common="-smp 4 -m 4G
    -device qemu-xhci -device usb-kbd -device usb-tablet
    -nic user,model=virtio-net-pci,ipv6=off
    -qmp unix:$state/qmp.sock,server,nowait"

# shellcheck disable=SC2086
case $arch in
    arm64)
        # Homebrew's QEMU can't use Apple's hypervisor on macOS 14 (it needs the
        # macOS 15 SDK), so this uses Apple's Virtualization framework directly.
        command -v vfkit >/dev/null || { echo "Needs vfkit: brew install vfkit"; exit 1; }
        exec vfkit --cpus 4 --memory 4096 \
            --bootloader efi,variable-store="$state/efi-vars-vfkit",create \
            --device usb-mass-storage,path="$iso",readonly \
            --device virtio-net,nat \
            --device virtio-gpu,width=1280,height=720 \
            --device virtio-input,keyboard \
            --device virtio-input,pointing \
            --device virtio-rng \
            --gui
        ;;
    amd64)
        vars="$state/efi-vars.fd"
        [ -f "$vars" ] || cp "$share/edk2-i386-vars.fd" "$vars"
        exec qemu-system-x86_64 \
            -machine q35 -accel tcg,thread=multi -cpu max $common \
            -drive if=pflash,format=raw,readonly=on,file="$share/edk2-x86_64-code.fd" \
            -drive if=pflash,format=raw,file="$vars" \
            -cdrom "$iso" -boot d \
            -device virtio-vga,xres=1280,yres=720 \
            $display
        ;;
esac
