#!/bin/sh
# Writes out/catos-amd64.iso to a USB stick, keeping its saved settings.
#
#   ./flash.sh /dev/diskN [image.iso]     (Linux: /dev/sdX)
#
# The settings live in a partition after the image (see catos-persistence).
# Writing an image replaces the stick's partition table, so this copies that
# partition's entry first and puts it back afterwards. Its data is never touched,
# as long as the new image ends before it starts.
set -eu
cd "$(dirname "$0")"

disk=${1:?usage: ./flash.sh /dev/diskN [image.iso]}
iso=${2:-out/catos-amd64.iso}
[ -f "$iso" ] || { echo "No $iso: run ./build.sh"; exit 1; }

case $(uname) in
Darwin)
    info=$(diskutil info "$disk") || exit 1
    echo "$info" | grep -Eq 'Device Location: +External|Removable Media: +Removable' ||
        { echo "$disk doesn't look like a USB stick; refusing."; exit 1; }
    name=$(echo "$info" | sed -n 's/^ *Device \/ Media Name: *//p')
    raw=$(echo "$disk" | sed 's|/dev/disk|/dev/rdisk|')
    bs=4m
    unmount() { diskutil unmountDisk "$disk" >/dev/null; }
    finish() { diskutil eject "$disk" >/dev/null; }
    ;;
*)
    [ "$(lsblk -n -d -o TRAN "$disk")" = usb ] ||
        { echo "$disk isn't a USB stick; refusing."; exit 1; }
    name=$(lsblk -n -d -o MODEL "$disk")
    raw=$disk
    bs=4M
    unmount() { for p in $(lsblk -n -l -o PATH "$disk" | tail -n +2); do umount "$p" 2>/dev/null || true; done; }
    finish() { sync; }
    ;;
esac

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

# The settings partition is the third entry of the MBR (bytes 478-493 of sector 0).
sudo dd if="$raw" of="$tmp/mbr" bs=512 count=1 2>/dev/null
dd if="$tmp/mbr" of="$tmp/entry" bs=1 skip=478 count=16 2>/dev/null
type=$(od -A n -t x1 -j 4 -N 1 "$tmp/entry" | tr -d ' ')
start=$(od -A n -t u4 -j 8 -N 4 "$tmp/entry" | tr -d ' ')
iso_sectors=$(( $(wc -c < "$iso") / 512 ))

keep=
if [ "$type" = 83 ] && [ "$start" -gt 0 ]; then
    if [ "$iso_sectors" -le "$start" ]; then
        keep=1
        echo "Keeping the saved settings ($(( start / 2048 )) MiB in)."
    else
        echo "The image is now too big to fit before the saved settings, which will be lost."
    fi
else
    echo "No saved settings on this stick yet; catos makes them on its first boot."
fi

printf 'Write %s to %s (%s)? Everything else on it is erased. [y/N] ' "$iso" "$disk" "$name"
read -r answer
[ "$answer" = y ] || [ "$answer" = Y ] || exit 1

unmount
sudo dd if="$iso" of="$raw" bs=$bs status=progress
if [ -n "$keep" ]; then
    # Put the entry back in the new image's (empty) third slot.
    sudo dd if="$raw" of="$tmp/mbr" bs=512 count=1 2>/dev/null
    dd if="$tmp/entry" of="$tmp/mbr" bs=1 seek=478 count=16 conv=notrunc 2>/dev/null
    sudo dd if="$tmp/mbr" of="$raw" bs=512 count=1 2>/dev/null
fi
finish
echo "Done."
