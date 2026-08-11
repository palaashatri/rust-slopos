#!/usr/bin/env python3
"""Inject bounded QA input through Linux uinput inside the isolated UTM VM."""

from __future__ import annotations

import argparse
import fcntl
import json
import os
import struct
import time
from collections.abc import Iterable
from dataclasses import dataclass


EV_SYN = 0x00
EV_KEY = 0x01
EV_ABS = 0x03
SYN_REPORT = 0x00
ABS_X = 0x00
ABS_Y = 0x01
BTN_LEFT = 0x110
BUS_USB = 0x03
ABS_CNT = 0x40

IOC_NRBITS = 8
IOC_TYPEBITS = 8
IOC_SIZEBITS = 14
IOC_NRSHIFT = 0
IOC_TYPESHIFT = IOC_NRSHIFT + IOC_NRBITS
IOC_SIZESHIFT = IOC_TYPESHIFT + IOC_TYPEBITS
IOC_DIRSHIFT = IOC_SIZESHIFT + IOC_SIZEBITS
IOC_WRITE = 1

UINPUT_IOCTL_BASE = ord("U")


def _ioc(direction: int, ioctl_type: int, number: int, size: int) -> int:
    return (
        (direction << IOC_DIRSHIFT)
        | (ioctl_type << IOC_TYPESHIFT)
        | (number << IOC_NRSHIFT)
        | (size << IOC_SIZESHIFT)
    )


def _io(ioctl_type: int, number: int) -> int:
    return _ioc(0, ioctl_type, number, 0)


def _iow(ioctl_type: int, number: int, size: int) -> int:
    return _ioc(IOC_WRITE, ioctl_type, number, size)


UI_DEV_CREATE = _io(UINPUT_IOCTL_BASE, 1)
UI_DEV_DESTROY = _io(UINPUT_IOCTL_BASE, 2)
UI_SET_EVBIT = _iow(UINPUT_IOCTL_BASE, 100, struct.calcsize("i"))
UI_SET_KEYBIT = _iow(UINPUT_IOCTL_BASE, 101, struct.calcsize("i"))
UI_SET_ABSBIT = _iow(UINPUT_IOCTL_BASE, 103, struct.calcsize("i"))

INPUT_EVENT = struct.Struct("@llHHi")
INPUT_ID_AND_NAME = struct.Struct("=80sHHHHI")


KEY_CODES = {
    "esc": 1,
    "1": 2,
    "2": 3,
    "3": 4,
    "4": 5,
    "5": 6,
    "6": 7,
    "7": 8,
    "8": 9,
    "9": 10,
    "0": 11,
    "minus": 12,
    "equal": 13,
    "backspace": 14,
    "tab": 15,
    "q": 16,
    "w": 17,
    "e": 18,
    "r": 19,
    "t": 20,
    "y": 21,
    "u": 22,
    "i": 23,
    "o": 24,
    "p": 25,
    "leftbrace": 26,
    "rightbrace": 27,
    "enter": 28,
    "leftctrl": 29,
    "a": 30,
    "s": 31,
    "d": 32,
    "f": 33,
    "g": 34,
    "h": 35,
    "j": 36,
    "k": 37,
    "l": 38,
    "semicolon": 39,
    "apostrophe": 40,
    "grave": 41,
    "leftshift": 42,
    "backslash": 43,
    "z": 44,
    "x": 45,
    "c": 46,
    "v": 47,
    "b": 48,
    "n": 49,
    "m": 50,
    "comma": 51,
    "dot": 52,
    "slash": 53,
    "rightshift": 54,
    "leftalt": 56,
    "space": 57,
    "left": 105,
    "right": 106,
    "up": 103,
    "down": 108,
    "leftmeta": 125,
}

CHAR_KEYS: dict[str, tuple[int, bool]] = {}
for letter in "abcdefghijklmnopqrstuvwxyz":
    CHAR_KEYS[letter] = (KEY_CODES[letter], False)
    CHAR_KEYS[letter.upper()] = (KEY_CODES[letter], True)
for digit in "0123456789":
    CHAR_KEYS[digit] = (KEY_CODES[digit], False)

CHAR_KEYS.update(
    {
        " ": (KEY_CODES["space"], False),
        "\n": (KEY_CODES["enter"], False),
        "-": (KEY_CODES["minus"], False),
        "_": (KEY_CODES["minus"], True),
        "=": (KEY_CODES["equal"], False),
        "+": (KEY_CODES["equal"], True),
        "[": (KEY_CODES["leftbrace"], False),
        "{": (KEY_CODES["leftbrace"], True),
        "]": (KEY_CODES["rightbrace"], False),
        "}": (KEY_CODES["rightbrace"], True),
        "\\": (KEY_CODES["backslash"], False),
        "|": (KEY_CODES["backslash"], True),
        ";": (KEY_CODES["semicolon"], False),
        ":": (KEY_CODES["semicolon"], True),
        "'": (KEY_CODES["apostrophe"], False),
        '"': (KEY_CODES["apostrophe"], True),
        "`": (KEY_CODES["grave"], False),
        "~": (KEY_CODES["grave"], True),
        ",": (KEY_CODES["comma"], False),
        "<": (KEY_CODES["comma"], True),
        ".": (KEY_CODES["dot"], False),
        ">": (KEY_CODES["dot"], True),
        "/": (KEY_CODES["slash"], False),
        "?": (KEY_CODES["slash"], True),
        "!": (KEY_CODES["1"], True),
        "@": (KEY_CODES["2"], True),
        "#": (KEY_CODES["3"], True),
        "$": (KEY_CODES["4"], True),
        "%": (KEY_CODES["5"], True),
        "^": (KEY_CODES["6"], True),
        "&": (KEY_CODES["7"], True),
        "*": (KEY_CODES["8"], True),
        "(": (KEY_CODES["9"], True),
        ")": (KEY_CODES["0"], True),
    }
)


@dataclass(frozen=True)
class DeviceConfig:
    width: int
    height: int
    key_codes: frozenset[int]
    absolute_pointer: bool


class UInputDevice:
    """Minimal transient uinput device with explicit event capabilities."""

    def __init__(self, config: DeviceConfig) -> None:
        if config.width <= 0 or config.height <= 0:
            raise ValueError("uinput bounds must be positive")
        self._config = config
        self._fd = os.open("/dev/uinput", os.O_WRONLY | os.O_NONBLOCK)
        self._created = False
        try:
            self._configure()
        except Exception:
            os.close(self._fd)
            raise

    def _configure(self) -> None:
        fcntl.ioctl(self._fd, UI_SET_EVBIT, EV_SYN)
        if self._config.key_codes:
            fcntl.ioctl(self._fd, UI_SET_EVBIT, EV_KEY)
            for key_code in sorted(self._config.key_codes):
                fcntl.ioctl(self._fd, UI_SET_KEYBIT, key_code)
        if self._config.absolute_pointer:
            fcntl.ioctl(self._fd, UI_SET_EVBIT, EV_ABS)
            fcntl.ioctl(self._fd, UI_SET_ABSBIT, ABS_X)
            fcntl.ioctl(self._fd, UI_SET_ABSBIT, ABS_Y)

        name = b"SLOPOS-I isolated QA input"
        header = INPUT_ID_AND_NAME.pack(
            name,
            BUS_USB,
            0x1D6B,
            0x0104,
            1,
            0,
        )
        abs_max = [0] * ABS_CNT
        abs_min = [0] * ABS_CNT
        abs_fuzz = [0] * ABS_CNT
        abs_flat = [0] * ABS_CNT
        if self._config.absolute_pointer:
            abs_max[ABS_X] = self._config.width - 1
            abs_max[ABS_Y] = self._config.height - 1
        arrays = struct.pack(
            f"={ABS_CNT * 4}i",
            *(abs_max + abs_min + abs_fuzz + abs_flat),
        )
        os.write(self._fd, header + arrays)
        fcntl.ioctl(self._fd, UI_DEV_CREATE)
        self._created = True
        time.sleep(0.8)

    def emit(self, event_type: int, code: int, value: int) -> None:
        os.write(self._fd, INPUT_EVENT.pack(0, 0, event_type, code, value))

    def sync(self) -> None:
        self.emit(EV_SYN, SYN_REPORT, 0)

    def close(self) -> None:
        if self._created:
            fcntl.ioctl(self._fd, UI_DEV_DESTROY)
            self._created = False
        os.close(self._fd)

    def __enter__(self) -> UInputDevice:
        return self

    def __exit__(self, exc_type: object, exc: object, traceback: object) -> None:
        self.close()


def click(x: int, y: int, width: int, height: int) -> dict[str, object]:
    """Move an absolute pointer and send one bounded left-button click."""
    if not 0 <= x < width or not 0 <= y < height:
        raise ValueError(f"click coordinate {x},{y} is outside {width}x{height}")
    config = DeviceConfig(width, height, frozenset({BTN_LEFT}), True)
    with UInputDevice(config) as device:
        device.emit(EV_ABS, ABS_X, x)
        device.emit(EV_ABS, ABS_Y, y)
        device.sync()
        time.sleep(0.1)
        device.emit(EV_KEY, BTN_LEFT, 1)
        device.sync()
        time.sleep(0.1)
        device.emit(EV_KEY, BTN_LEFT, 0)
        device.sync()
        time.sleep(0.2)
    return {"action": "click", "x": x, "y": y, "width": width, "height": height}


def key_combo(names: Iterable[str], width: int, height: int) -> dict[str, object]:
    """Press the named Linux input keys together, then release in reverse."""
    normalized = [name.strip().lower() for name in names]
    if not normalized:
        raise ValueError("at least one key name is required")
    unknown = [name for name in normalized if name not in KEY_CODES]
    if unknown:
        raise ValueError(f"unknown key names: {', '.join(unknown)}")
    codes = [KEY_CODES[name] for name in normalized]
    config = DeviceConfig(width, height, frozenset(codes), False)
    with UInputDevice(config) as device:
        for code in codes:
            device.emit(EV_KEY, code, 1)
        device.sync()
        time.sleep(0.1)
        for code in reversed(codes):
            device.emit(EV_KEY, code, 0)
        device.sync()
        time.sleep(0.2)
    return {"action": "combo", "keys": normalized}


def type_text(text: str, width: int, height: int) -> dict[str, object]:
    """Type text through a transient US-layout uinput keyboard."""
    unsupported = sorted(
        {character for character in text if character not in CHAR_KEYS}
    )
    if unsupported:
        raise ValueError(f"unsupported characters: {unsupported!r}")
    key_codes = {code for code, _shift in CHAR_KEYS.values()}
    key_codes.add(KEY_CODES["leftshift"])
    config = DeviceConfig(width, height, frozenset(key_codes), False)
    with UInputDevice(config) as device:
        for character in text:
            code, shifted = CHAR_KEYS[character]
            if shifted:
                device.emit(EV_KEY, KEY_CODES["leftshift"], 1)
            device.emit(EV_KEY, code, 1)
            device.sync()
            device.emit(EV_KEY, code, 0)
            if shifted:
                device.emit(EV_KEY, KEY_CODES["leftshift"], 0)
            device.sync()
            time.sleep(0.015)
        time.sleep(0.2)
    return {"action": "type", "characters": len(text)}


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--width", type=int, default=1280)
    parser.add_argument("--height", type=int, default=800)
    subparsers = parser.add_subparsers(dest="action", required=True)

    click_parser = subparsers.add_parser("click")
    click_parser.add_argument("x", type=int)
    click_parser.add_argument("y", type=int)

    combo_parser = subparsers.add_parser("combo")
    combo_parser.add_argument("keys", nargs="+")

    type_parser = subparsers.add_parser("type")
    type_parser.add_argument("text")
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    if args.action == "click":
        result = click(args.x, args.y, args.width, args.height)
    elif args.action == "combo":
        result = key_combo(args.keys, args.width, args.height)
    else:
        result = type_text(args.text, args.width, args.height)
    print(json.dumps(result, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
