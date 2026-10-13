# meownix

A tiny Debian-based live system for the projector's Minix NGC-1. It boots
straight into a console-style crossbar menu (no login screen, no desktop) that
launches Flex, FreeTube and YouTube fullscreen.

## How it fits together

```
UEFI → GRUB → Debian live (trixie, amd64)
  └─ getty autologin on tty1 as `tv`
       └─ ~/.profile → meownix-session
            └─ cage (Wayland kiosk compositor)
                 └─ meownix-menu (Rust + egui crossbar)
                      ├─ meownix-browser flex …     Firefox kiosk, own profile
                      ├─ freetube
                      └─ meownix-browser youtube …  youtube.com/tv with a TV user agent
```

cage shows its newest window fullscreen. The menu launches an app, the app's
window covers the menu, and when the app closes the menu is visible again.

**Getting back to the menu:** hold Back (or Esc) for a second, or press Home.
The menu reads input devices directly, so this works whatever app is open.
The keys are set in `menu/menu.toml`.

## Layout

| Path | What |
|---|---|
| `menu/` | The crossbar menu (Rust/egui) and its `menu.toml` |
| `live/` | live-build config: package list, build hook, files copied into the image |
| `builder/` | Docker images for building (live-build, Rust cross-compiler) |
| `vm/` | Run the ISO in QEMU |
| `build.sh` | Builds `out/meownix-amd64.iso` |

## Working on it

```sh
# Menu only, in a window on the Mac (Esc quits; apps aren't really launched)
cd menu && cargo run -- --windowed --dry-run --config menu.toml

# Full image (needs Docker running; takes a while because x86 is emulated)
./build.sh

# Boot it
vm/run.sh
```

To put it on a USB stick (check the disk number with `diskutil list` first,
since this overwrites the whole disk):

```sh
diskutil unmountDisk /dev/diskN
sudo dd if=out/meownix-amd64.iso of=/dev/rdiskN bs=4m status=progress
```

## Debugging on the box

Ctrl+Alt+F2 gives a console: log in as `tv`, no password (also passwordless
sudo while we're developing; Ctrl+Alt+F1 goes back). `evtest` shows what keys the remote sends. The session log is in
`~/.local/state/meownix/session.log`.
