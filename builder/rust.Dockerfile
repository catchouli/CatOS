# Cross-compiles the menu for x86_64 Linux. Runs natively on the Mac's ARM CPU
# (much faster than emulating x86) and links with Debian's x86_64 cross gcc.
FROM rust:1-trixie

RUN dpkg --add-architecture amd64 \
 && apt-get update \
 && apt-get install -y --no-install-recommends gcc-x86-64-linux-gnu libc6-dev-amd64-cross \
    # For the Live2D avatar bridge (C++ and OpenGL), when the SDK is present.
    g++-x86-64-linux-gnu mesa-common-dev libopengl-dev libopengl-dev:amd64 \
 && rm -rf /var/lib/apt/lists/* \
 && rustup target add x86_64-unknown-linux-gnu

ENV CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER=x86_64-linux-gnu-gcc
WORKDIR /src
