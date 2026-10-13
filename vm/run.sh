#!/bin/sh
# Boots out/meownix-amd64.iso in QEMU, the way the NGC-1 will: UEFI, x86_64.
#
#   vm/run.sh             in a window (click into it; Ctrl+Opt+G releases the mouse)
#   vm/run.sh headless    no window; control it with vm/ctl.py
#
# x86 is emulated on Apple Silicon, so it's slow. Fine for checking that things
# boot and launch, not for judging video playback.
set -eu
cd "$(dirname "$0")/.."

iso=out/meownix-amd64.iso
share="$(brew --prefix qemu)/share/qemu"
state=out/vm
mkdir -p "$state"
[ -f "$state/efi-vars.fd" ] || cp "$share/edk2-i386-vars.fd" "$state/efi-vars.fd"

if [ "${1:-}" = headless ]; then
    display="-display none -vnc 127.0.0.1:1"
else
    display="-display cocoa"
fi

# shellcheck disable=SC2086
exec qemu-system-x86_64 \
    -machine q35 -accel tcg,thread=multi -cpu max -smp 4 -m 4G \
    -drive if=pflash,format=raw,readonly=on,file="$share/edk2-x86_64-code.fd" \
    -drive if=pflash,format=raw,file="$state/efi-vars.fd" \
    -cdrom "$iso" -boot d \
    -device virtio-vga \
    -device qemu-xhci -device usb-kbd -device usb-tablet \
    -nic user,model=virtio-net-pci \
    -qmp unix:"$state/qmp.sock",server,nowait \
    $display
