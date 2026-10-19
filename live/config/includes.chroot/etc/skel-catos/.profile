# Start the TV session on the first console. Other consoles (Ctrl+Alt+F2)
# still give a normal shell for debugging.
if [ "$(tty)" = /dev/tty1 ] && [ -z "$WAYLAND_DISPLAY" ]; then
    exec /usr/local/bin/catos-session
fi
