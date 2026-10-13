#!/usr/bin/env python3
"""Drive the headless VM over QMP.

    vm/ctl.py shot out/vm/screen.png     screenshot
    vm/ctl.py key right right ret        press keys (QEMU key names)
    vm/ctl.py hold esc 1.5               hold a key down for N seconds
    vm/ctl.py type 'ls -la'              type text and press Enter
"""

import json
import socket
import sys
import time
from pathlib import Path

SOCK = Path(__file__).resolve().parent.parent / "out/vm/qmp.sock"


def qmp():
    s = socket.socket(socket.AF_UNIX)
    s.connect(str(SOCK))
    f = s.makefile("rw")
    json.loads(f.readline())  # greeting

    def cmd(name, **args):
        f.write(json.dumps({"execute": name, "arguments": args}) + "\n")
        f.flush()
        while True:
            msg = json.loads(f.readline())
            if "event" in msg:
                continue
            if "error" in msg:
                sys.exit(f"QMP {name}: {msg['error']['desc']}")
            return msg.get("return")

    cmd("qmp_capabilities")
    return cmd


PLAIN = {" ": "spc", "\n": "ret", "-": "minus", "=": "equal", "/": "slash", ".": "dot",
         ",": "comma", ";": "semicolon", "'": "apostrophe", "[": "bracket_left",
         "]": "bracket_right", "\\": "backslash", "`": "grave_accent", "\t": "tab"}
SHIFTED = {"_": "minus", "+": "equal", "?": "slash", ">": "dot", "<": "comma", ":": "semicolon",
           '"': "apostrophe", "{": "bracket_left", "}": "bracket_right", "|": "backslash",
           "~": "grave_accent", "!": "1", "@": "2", "#": "3", "$": "4", "%": "5", "^": "6",
           "&": "7", "*": "8", "(": "9", ")": "0"}


def qcodes(ch):
    """QEMU key codes for one character on a US keyboard."""
    if ch.isalpha():
        return ["shift", ch.lower()] if ch.isupper() else [ch]
    if ch.isdigit():
        return [ch]
    if ch in PLAIN:
        return [PLAIN[ch]]
    return ["shift", SHIFTED[ch]]


def main():
    cmd = qmp()
    action, args = sys.argv[1], sys.argv[2:]
    if action == "shot":
        cmd("screendump", filename=str(Path(args[0]).resolve()), format="png")
    elif action == "key":
        for k in args:
            cmd("send-key", keys=[{"type": "qcode", "data": part} for part in k.split("-")])
            time.sleep(0.3)
    elif action == "type":
        for ch in " ".join(args) + "\n":
            # Short hold: the emulated guest is slow enough to auto-repeat otherwise.
            cmd("send-key", keys=[{"type": "qcode", "data": k} for k in qcodes(ch)], **{"hold-time": 50})
            time.sleep(0.05)
    elif action == "hold":
        key = {"type": "key", "data": {"down": True, "key": {"type": "qcode", "data": args[0]}}}
        cmd("input-send-event", events=[key])
        time.sleep(float(args[1]) if len(args) > 1 else 1.5)
        key["data"]["down"] = False
        cmd("input-send-event", events=[key])
    else:
        sys.exit(__doc__)


if __name__ == "__main__":
    main()
