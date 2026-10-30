#!/usr/bin/env python3
"""Check original SquashFS permissions, which extract-and-run can change."""

import argparse
from pathlib import Path
import subprocess
import sys


REQUIRED_EXECUTABLES = ("AppRun", "AppRun.wrapped", "usr/bin/quotamate")


def permission_errors(listing):
    entries = {}
    errors = []
    for line in listing.splitlines():
        fields = line.split(maxsplit=5)
        if len(fields) != 6 or len(fields[0]) != 10:
            continue
        mode, path = fields[0], fields[5].split(" -> ", 1)[0]
        if path != "squashfs-root" and not path.startswith("squashfs-root/"):
            continue
        path = path.removeprefix("squashfs-root").lstrip("/") or "."
        entries[path] = mode
        if mode[0] not in "-d":
            continue
        readable = all(mode[index] == "r" for index in (1, 4, 7))
        executable = all(mode[index] in "xst" for index in (3, 6, 9))
        has_execute_bit = any(mode[index] in "xst" for index in (3, 6, 9))
        if not readable:
            errors.append(f"{path}: {mode} is not readable by every user")
        if (mode[0] == "d" or has_execute_bit or path in REQUIRED_EXECUTABLES) and not executable:
            errors.append(f"{path}: {mode} is not executable/traversable by every user")
    if "." not in entries:
        errors.append("Could not read the SquashFS root directory metadata")
    for path in REQUIRED_EXECUTABLES:
        if path not in entries:
            errors.append(f"Missing required executable: {path}")
        elif entries[path][0] != "-":
            errors.append(f"Expected a regular executable: {path}")
    return errors


def check_appimage(image):
    image = image.resolve(strict=True)
    # Only run the AppImage runtime's metadata command; do not extract the app.
    offset = int(subprocess.check_output([str(image), "--appimage-offset"], text=True).strip())
    listing = subprocess.check_output(
        ["unsquashfs", "-lln", "-o", str(offset), str(image)], text=True
    )
    errors = permission_errors(listing)
    if errors:
        raise ValueError("AppImage permission check failed:\n" + "\n".join(errors))
    print(f"PASS: original AppImage permissions allow other users to run {image.name}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("image", type=Path)
    args = parser.parse_args()
    try:
        check_appimage(args.image)
    except (OSError, ValueError, subprocess.CalledProcessError) as error:
        print(error, file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
