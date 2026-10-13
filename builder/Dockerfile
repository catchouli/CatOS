# Build environment for meownix: a plain Debian trixie with live-build.
# Run as linux/amd64 so the image we produce is native x86_64 for the NGC-1.
FROM debian:trixie

RUN apt-get update \
 && apt-get install -y --no-install-recommends \
      live-build debootstrap ca-certificates curl \
      xorriso squashfs-tools mtools dosfstools \
      grub-efi-amd64-bin grub-pc-bin syslinux syslinux-common isolinux \
 && rm -rf /var/lib/apt/lists/*

WORKDIR /build
