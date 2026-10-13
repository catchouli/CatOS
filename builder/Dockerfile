# Build environment for meownix: a plain Debian trixie with live-build.
# Built per target architecture (--platform linux/amd64 or linux/arm64), so the
# image we produce is native to it.
FROM debian:trixie

RUN case "$(dpkg --print-architecture)" in \
      amd64) boot="grub-efi-amd64-bin grub-pc-bin syslinux syslinux-common isolinux" ;; \
      arm64) boot="grub-efi-arm64-bin" ;; \
    esac \
 && apt-get update \
 && apt-get install -y --no-install-recommends \
      live-build debootstrap ca-certificates curl \
      xorriso squashfs-tools mtools dosfstools $boot \
 && rm -rf /var/lib/apt/lists/*

WORKDIR /build
