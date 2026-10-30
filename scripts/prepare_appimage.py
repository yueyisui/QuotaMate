#!/usr/bin/env python3
"""Cache Tauri's AppRun with permissions that survive mounting as another user."""

import hashlib
import os
from pathlib import Path
import platform
import sys
import tempfile
from urllib.request import urlopen


# Tauri CLI 2.11.4 downloads these launchers with mode 0770, then copies that
# mode into AppRun.wrapped. Pre-populate its default cache with mode 0755.
# See tauri-bundler/src/bundle/linux/appimage/{mod,linuxdeploy}.rs.
LAUNCHERS = {
    "x86_64": "f30140a43a0a59e46db21bdefdf749b9e9f2c6946e92afabbacf98b8ae73fb4f",
    "aarch64": "072f17c0895a85c490282fe5395c5007e5fc75da727e553b3b8fb680feb11578",
}
BASE_URL = "https://github.com/tauri-apps/binary-releases/releases/download/apprun-old"


def prepare_launcher(cache_dir, arch):
    expected = LAUNCHERS[arch]
    launcher = cache_dir / f"AppRun-{arch}"
    cache_dir.mkdir(parents=True, exist_ok=True)
    if not launcher.is_file() or hashlib.sha256(launcher.read_bytes()).hexdigest() != expected:
        with urlopen(f"{BASE_URL}/AppRun-{arch}", timeout=60) as response:
            data = response.read()
        if hashlib.sha256(data).hexdigest() != expected:
            raise ValueError(f"SHA-256 mismatch for the official AppRun-{arch}")
        # Download and validate completely before replacing an existing cache.
        with tempfile.NamedTemporaryFile(dir=cache_dir, delete=False) as temporary:
            temporary.write(data)
            temporary_path = Path(temporary.name)
        try:
            temporary_path.chmod(0o755)
            temporary_path.replace(launcher)
        finally:
            temporary_path.unlink(missing_ok=True)
    launcher.chmod(0o755)
    return launcher


def main():
    arch = os.environ.get("TAURI_ENV_ARCH") or platform.machine()
    if sys.platform != "linux" or arch not in LAUNCHERS:
        raise SystemExit(f"Unsupported AppImage build platform: {sys.platform}/{arch}")
    cache_root = Path(os.environ.get("XDG_CACHE_HOME") or Path.home() / ".cache")
    launcher = prepare_launcher(cache_root / "tauri", arch)
    print(f"Prepared {launcher} with mode 0755")


if __name__ == "__main__":
    main()
