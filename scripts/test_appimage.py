"""Regression checks for launchers that work only as the build user."""

import hashlib
import io
import os
from pathlib import Path
import shutil
import stat
import subprocess
import tempfile
import unittest
from unittest.mock import patch

from check_appimage import permission_errors
from prepare_appimage import prepare_launcher


LISTING = """Parallel unsquashfs: Using 2 processors
drwxr-xr-x 0/0 100 2026-10-30 00:00 squashfs-root
-rwxr-xr-x 0/0 100 2026-10-30 00:00 squashfs-root/AppRun
-rwxr-xr-x 0/0 100 2026-10-30 00:00 squashfs-root/AppRun.wrapped
drwxr-xr-x 0/0 100 2026-10-30 00:00 squashfs-root/usr
drwxr-xr-x 0/0 100 2026-10-30 00:00 squashfs-root/usr/bin
-rwxr-xr-x 0/0 100 2026-10-30 00:00 squashfs-root/usr/bin/quotamate
-rw-r--r-- 0/0 100 2026-10-30 00:00 squashfs-root/file with spaces
lrwxrwxrwx 0/0 100 2026-10-30 00:00 squashfs-root/.DirIcon -> icon.png
"""


class PermissionTests(unittest.TestCase):
    def test_public_permissions_pass(self):
        self.assertEqual(permission_errors(LISTING), [])

    def test_0770_launcher_fails_even_when_build_user_can_execute(self):
        listing = LISTING.replace(
            "-rwxr-xr-x 0/0 100 2026-10-30 00:00 squashfs-root/AppRun.wrapped",
            "-rwxrwx--- 0/0 100 2026-10-30 00:00 squashfs-root/AppRun.wrapped",
        )
        self.assertTrue(any("AppRun.wrapped" in error for error in permission_errors(listing)))

    def test_private_parent_directory_fails(self):
        self.assertTrue(permission_errors(LISTING.replace("drwxr-xr-x", "drwx------", 1)))

    def test_unreadable_resource_fails(self):
        self.assertTrue(permission_errors(LISTING.replace("-rw-r--r--", "-rw-r-----")))

    def test_non_executable_launcher_fails(self):
        self.assertTrue(permission_errors(LISTING.replace("-rwxr-xr-x", "-rw-r--r--")))

    def test_empty_or_incomplete_listing_fails(self):
        self.assertTrue(permission_errors(""))
        self.assertTrue(permission_errors(LISTING.replace("AppRun.wrapped", "another-file")))

    @unittest.skipUnless(os.name == "posix" and shutil.which("mksquashfs"), "needs squashfs-tools on Linux")
    def test_actual_squashfs_keeps_private_launcher_permissions(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            stage = root / "app"
            (stage / "usr/bin").mkdir(parents=True)
            for path in (stage, stage / "usr", stage / "usr/bin"):
                path.chmod(0o755)
            for name in ("AppRun", "AppRun.wrapped", "usr/bin/quotamate"):
                path = stage / name
                path.write_bytes(b"launcher")
                path.chmod(0o755)
            wrapped = stage / "AppRun.wrapped"
            for mode in (0o770, 0o755):
                wrapped.chmod(mode)
                image = root / f"{mode:o}.squashfs"
                subprocess.run(
                    ["mksquashfs", str(stage), str(image), "-noappend", "-processors", "1", "-no-progress"],
                    check=True, stdout=subprocess.DEVNULL,
                )
                listing = subprocess.check_output(["unsquashfs", "-lln", str(image)], text=True)
                self.assertEqual(bool(permission_errors(listing)), mode == 0o770)


class LauncherTests(unittest.TestCase):
    def setUp(self):
        self.data = b"test launcher"
        self.hashes = patch.dict("prepare_appimage.LAUNCHERS", {"x86_64": hashlib.sha256(self.data).hexdigest()})
        self.hashes.start()
        self.addCleanup(self.hashes.stop)

    def assert_public_executable(self, launcher):
        self.assertEqual(launcher.read_bytes(), self.data)
        if os.name == "posix":
            self.assertEqual(stat.S_IMODE(launcher.stat().st_mode), 0o755)

    def test_existing_cache_is_repaired_without_network(self):
        with tempfile.TemporaryDirectory() as directory:
            cache = Path(directory)
            launcher = cache / "AppRun-x86_64"
            launcher.write_bytes(self.data)
            launcher.chmod(0o770)
            with patch("prepare_appimage.urlopen") as download:
                self.assert_public_executable(prepare_launcher(cache, "x86_64"))
                download.assert_not_called()

    def test_fresh_cache_is_ready_before_tauri_bundles(self):
        with tempfile.TemporaryDirectory() as directory:
            with patch("prepare_appimage.urlopen", return_value=io.BytesIO(self.data)):
                self.assert_public_executable(prepare_launcher(Path(directory), "x86_64"))

    def test_bad_download_does_not_replace_cache(self):
        with tempfile.TemporaryDirectory() as directory:
            cache = Path(directory)
            launcher = cache / "AppRun-x86_64"
            launcher.write_bytes(b"old cache")
            with patch("prepare_appimage.urlopen", return_value=io.BytesIO(b"bad download")):
                with self.assertRaisesRegex(ValueError, "SHA-256 mismatch"):
                    prepare_launcher(cache, "x86_64")
            self.assertEqual(launcher.read_bytes(), b"old cache")


if __name__ == "__main__":
    unittest.main()
