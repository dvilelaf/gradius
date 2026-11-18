#!/usr/bin/env -S uv run python
"""Virtual evdev gamepad on /dev/uinput, stdlib only, for testing pad support without hardware.

usage: virtual-pad.py VENDOR PRODUCT [NAME] < commands
commands (one per line): `key BTN_START 1`, `abs ABS_HAT0X -1`, `sleep 0.5`, `quit`.
The pad has the Xbox 360 buttons and axes (X Y Z RX RY RZ, HAT0X/Y); it disappears on exit.
"""
import fcntl
import os
import struct
import sys
import time

KEYS = {"BTN_SOUTH": 0x130, "BTN_EAST": 0x131, "BTN_NORTH": 0x133, "BTN_WEST": 0x134,
        "BTN_TL": 0x136, "BTN_TR": 0x137, "BTN_SELECT": 0x13A, "BTN_START": 0x13B,
        "BTN_MODE": 0x13C, "BTN_THUMBL": 0x13D, "BTN_THUMBR": 0x13E}
ABS = {"ABS_X": 0, "ABS_Y": 1, "ABS_Z": 2, "ABS_RX": 3, "ABS_RY": 4, "ABS_RZ": 5,
       "ABS_HAT0X": 0x10, "ABS_HAT0Y": 0x11}
UI_SET_EVBIT, UI_SET_KEYBIT, UI_SET_ABSBIT = 0x40045564, 0x40045565, 0x40045567
UI_DEV_CREATE, UI_DEV_DESTROY = 0x5501, 0x5502
EV_SYN, EV_KEY, EV_ABS = 0, 1, 3


def emit(fd, kind, code, value):
    os.write(fd, struct.pack("llHHi", 0, 0, kind, code, value))
    os.write(fd, struct.pack("llHHi", 0, 0, EV_SYN, 0, 0))


def main():
    vendor, product = int(sys.argv[1], 0), int(sys.argv[2], 0)
    name = (sys.argv[3] if len(sys.argv) > 3 else "Virtual Pad").encode()
    fd = os.open("/dev/uinput", os.O_WRONLY | os.O_NONBLOCK)
    for ev in (EV_KEY, EV_ABS):
        fcntl.ioctl(fd, UI_SET_EVBIT, ev)
    for code in KEYS.values():
        fcntl.ioctl(fd, UI_SET_KEYBIT, code)
    absmax, absmin = [0] * 64, [0] * 64
    for code in ABS.values():
        fcntl.ioctl(fd, UI_SET_ABSBIT, code)
        hat = code >= 0x10
        absmin[code], absmax[code] = (-1, 1) if hat else (-32768, 32767)
    setup = struct.pack("80sHHHHi", name, 0x03, vendor, product, 0x110, 0)
    setup += struct.pack("64i", *absmax) + struct.pack("64i", *absmin) + bytes(64 * 4 * 2)
    os.write(fd, setup)
    fcntl.ioctl(fd, UI_DEV_CREATE)
    print("pad created", flush=True)
    try:
        for line in sys.stdin:
            parts = line.split()
            if not parts:
                continue
            if parts[0] == "quit":
                break
            if parts[0] == "sleep":
                time.sleep(float(parts[1]))
            elif parts[0] == "key":
                emit(fd, EV_KEY, KEYS[parts[1]], int(parts[2]))
            elif parts[0] == "abs":
                emit(fd, EV_ABS, ABS[parts[1]], int(parts[2]))
    finally:
        fcntl.ioctl(fd, UI_DEV_DESTROY)


main()
