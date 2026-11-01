import copy
import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

from check_release_request import InvalidRequest, select_request
from legacy_release_warning import MARKER, prepend_warning


class ReleaseRequestTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.repo = Path(self.temp.name)
        self.git("init", "--quiet")
        self.git("config", "user.email", "release-test@example.invalid")
        self.git("config", "user.name", "Release test")
        self.git("remote", "add", "origin", str(self.repo))
        self.write_versions("0.1.5")
        self.before = self.commit("previous version")
        self.write_versions("0.1.6")
        self.write("docs/releases/v0.1.6.md", "Release notes\n")
        self.source = self.commit("frozen source")
        self.request = {
            "schema": 1,
            "version": "0.1.6",
            "publish": True,
            "source_sha": self.source,
            "source_tree": self.git("rev-parse", f"{self.source}^{{tree}}"),
            "authorization": {
                "publish_release": True,
                "requested_by": "repository owner",
                "statement": "Publish this validated release.",
            },
            "desktop_validation": {
                "passed": True,
                "environment": "Ubuntu / GNOME X11 / NVIDIA",
                "deb": self.package("deb"),
                "appimage": self.package("AppImage"),
            },
        }

    def git(self, *args):
        return subprocess.check_output(
            ["git", "-C", str(self.repo), *args], text=True, stderr=subprocess.PIPE
        ).strip()

    def write(self, path, value):
        target = self.repo / path
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(value)

    def write_versions(self, version):
        self.write("package.json", json.dumps({"version": version}))
        self.write("src-tauri/tauri.conf.json", json.dumps({"version": version}))
        self.write("src-tauri/Cargo.toml", f'[package]\nname = "quotamate"\nversion = "{version}"\n')
        self.write("src-tauri/Cargo.lock", f'[[package]]\nname = "quotamate"\nversion = "{version}"\n')

    def package(self, extension):
        return {
            "file": f"QuotaMate_0.1.6_amd64.{extension}",
            "sha256": "a" * 64,
            "passed": 32,
            "total": 32,
            "report": f"artifacts/physical/{extension}/results.json",
        }

    def commit(self, message):
        self.git("add", "-A")
        self.git("commit", "--quiet", "-m", message)
        return self.git("rev-parse", "HEAD")

    def request_commit(self, request=None, path=None):
        request = self.request if request is None else request
        path = path or f'.github/release-requests/v{request["version"]}-{request["source_sha"][:12]}.json'
        self.write(path, json.dumps(request))
        return self.commit("explicit publication request")

    def select(self, head, before=None, event="push", ref="refs/heads/main"):
        return select_request(self.repo, event, ref, before or self.before, head)

    def test_new_request_pins_source_before_request_commit(self):
        head = self.request_commit()
        self.assertEqual(self.source, self.select(head))
        self.assertNotEqual(head, self.select(head))

    def test_existing_request_does_not_publish_later_push(self):
        old = self.request_commit()
        self.write("README.md", "ordinary update")
        head = self.commit("ordinary push")
        self.assertIsNone(self.select(head, before=old))

    def test_modified_request_is_not_new_authorization(self):
        old = self.request_commit()
        self.request["authorization"]["statement"] += " Updated wording."
        head = self.request_commit()
        self.assertIsNone(self.select(head, before=old))

    def test_tag_pull_request_manual_and_other_branch_do_not_publish(self):
        head = self.request_commit()
        for event, ref in (("push", "refs/tags/v0.1.6"), ("pull_request", "refs/heads/main"),
                           ("workflow_dispatch", "refs/heads/main"), ("push", "refs/heads/other")):
            with self.subTest(event=event, ref=ref):
                self.assertIsNone(self.select(head, event=event, ref=ref))

    def test_ordinary_push_without_request(self):
        self.assertIsNone(self.select(self.source))

    def test_unknown_push_base_cannot_publish(self):
        self.assertIsNone(self.select(self.request_commit(), before="0" * 40))

    def test_wrong_tree_or_unrelated_source_is_rejected(self):
        self.request["source_tree"] = "b" * 40
        with self.assertRaisesRegex(InvalidRequest, "tree mismatch"):
            self.select(self.request_commit())
        self.git("reset", "--hard", self.source)
        tree = self.git("rev-parse", f"{self.source}^{{tree}}")
        unrelated = self.git("commit-tree", tree, "-m", "unrelated root")
        self.request["source_sha"] = unrelated
        self.request["source_tree"] = tree
        with self.assertRaisesRegex(InvalidRequest, "ancestor"):
            self.select(self.request_commit())

    def test_tracked_changes_after_frozen_source_are_rejected(self):
        self.write("README.md", "not in the attested tree")
        with self.assertRaisesRegex(InvalidRequest, "outside the request directory"):
            self.select(self.request_commit())

    def test_all_four_versions_must_match(self):
        for path in ("package.json", "src-tauri/tauri.conf.json", "src-tauri/Cargo.toml", "src-tauri/Cargo.lock"):
            with self.subTest(path=path):
                self.git("reset", "--hard", self.source)
                self.write(path, (self.repo / path).read_text().replace("0.1.6", "0.1.7"))
                bad_source = self.commit("mismatched version")
                request = copy.deepcopy(self.request)
                request["source_sha"] = bad_source
                request["source_tree"] = self.git("rev-parse", f"{bad_source}^{{tree}}")
                with self.assertRaisesRegex(InvalidRequest, "four application versions"):
                    self.select(self.request_commit(request))

    def test_existing_remote_tag_is_rejected(self):
        self.git("tag", "v0.1.6", self.source)
        with self.assertRaisesRegex(InvalidRequest, "existing tag"):
            self.select(self.request_commit())

    def test_cannot_assume_absence_when_remote_unavailable(self):
        self.git("remote", "set-url", "origin", str(self.repo / "missing"))
        with self.assertRaisesRegex(InvalidRequest, "verify remote tag absence"):
            self.select(self.request_commit())

    def test_missing_authorization_or_incomplete_desktop_checks_rejected(self):
        mutations = (
            lambda req: req.pop("authorization"),
            lambda req: req.update(publish=False),
            lambda req: req["desktop_validation"].pop("appimage"),
            lambda req: req["desktop_validation"]["deb"].update(passed=31),
            lambda req: req["desktop_validation"]["deb"].update(sha256="short"),
            lambda req: req["desktop_validation"]["deb"].update(passed=True),
        )
        for mutate in mutations:
            with self.subTest(mutation=mutate):
                self.git("reset", "--hard", self.source)
                request = copy.deepcopy(self.request)
                mutate(request)
                with self.assertRaises(InvalidRequest):
                    self.select(self.request_commit(request))

    def test_missing_notes_are_rejected(self):
        self.git("rm", "docs/releases/v0.1.6.md")
        source = self.commit("missing notes")
        self.request.update(source_sha=source, source_tree=self.git("rev-parse", f"{source}^{{tree}}"))
        with self.assertRaises(InvalidRequest):
            self.select(self.request_commit())

    def test_request_path_and_source_must_match(self):
        with self.assertRaisesRegex(InvalidRequest, "path must match"):
            self.select(self.request_commit(path=".github/release-requests/v0.1.7-wrong.json"))

    def test_multiple_added_requests_are_ambiguous(self):
        self.request_commit()
        self.write(".github/release-requests/other.json", "{}")
        head = self.commit("second request in one push")
        with self.assertRaisesRegex(InvalidRequest, "exactly one"):
            self.select(head)

    def test_corrected_source_can_request_same_unpublished_version(self):
        old = self.request_commit()
        self.write("README.md", "a correction")
        source = self.commit("new frozen source")
        self.request.update(source_sha=source, source_tree=self.git("rev-parse", f"{source}^{{tree}}"))
        self.assertEqual(source, self.select(self.request_commit(), before=old))

    def test_prepublication_cli_requires_the_prepared_source(self):
        head = self.request_commit()
        event = self.repo / "event.json"
        event.write_text(json.dumps({"before": self.before}))
        command = [
            sys.executable, str(Path(__file__).with_name("check_release_request.py")),
            "--repo", str(self.repo), "--event-name", "push", "--ref", "refs/heads/main",
            "--head", head, "--event-path", str(event), "--expected-source",
        ]
        accepted = subprocess.run(command + [self.source], text=True, capture_output=True)
        self.assertEqual(0, accepted.returncode, accepted.stderr)
        self.assertEqual(self.source, accepted.stdout.strip())
        rejected = subprocess.run(command + [self.before], text=True, capture_output=True)
        self.assertNotEqual(0, rejected.returncode)
        self.assertIn("no longer matches", rejected.stderr)


class LegacyNoticeTests(unittest.TestCase):
    def test_preserves_entire_original_body_and_is_idempotent(self):
        original = "# Original release\n\nWindows/macOS downloads.\n"
        updated = prepend_warning(original)
        self.assertTrue(updated.startswith(MARKER))
        self.assertTrue(updated.endswith(original))
        self.assertEqual(updated, prepend_warning(updated))
        self.assertIn("https://github.com/yueyisui/QuotaMate/releases/latest", updated)


if __name__ == "__main__":
    unittest.main()
