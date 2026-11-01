import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import Mock

from visual_config_snapshot import read_config_snapshot, stable_config_snapshot


class VisualConfigSnapshotTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.config = Path(self.temp.name) / "config.json"
        self.expected = {"x": 400, "y": 300}
        self.now = 0.0

    def save(self, position=None):
        self.config.write_text(json.dumps({"petPosition": self.expected if position is None else position}))

    def poller(self):
        return stable_config_snapshot(
            self.config, lambda value: value.get("petPosition") == self.expected,
            stable_seconds=0.6, clock=lambda: self.now,
        )

    def test_real_truncate_then_complete_write_waits_for_full_stability(self):
        self.save()
        check = self.poller()
        # Interleave external reads with the actual truncate/write sequence.
        with self.config.open("w") as saving:
            self.assertFalse(check())
            saving.write('{"petPosition":')
            saving.flush()
            self.assertFalse(check())
            saving.write('{"x":400,"y":300}}')
            saving.flush()
            self.assertFalse(check())
        self.now = 0.59
        self.assertFalse(check())
        self.now = 0.61
        self.assertTrue(check())

    def test_permanently_bad_json_never_satisfies_the_condition(self):
        self.config.write_text('{"petPosition":')
        matches = Mock(return_value=True)
        check = stable_config_snapshot(self.config, matches, 0.6, clock=lambda: self.now)
        for self.now in (0, 0.6, 1, 30, 60):
            self.assertFalse(check())
        matches.assert_not_called()

    def test_missing_or_incomplete_read_resets_continuous_stability(self):
        for incomplete in ("missing", "partial"):
            with self.subTest(incomplete=incomplete):
                self.save()
                self.now = 0
                check = self.poller()
                self.assertFalse(check())
                self.now = 0.5
                self.assertFalse(check())
                if incomplete == "missing":
                    self.config.unlink()
                else:
                    self.config.write_text("")
                self.now = 0.55
                self.assertFalse(check())
                self.save()
                self.now = 0.7
                self.assertFalse(check())
                self.now = 1.2
                self.assertFalse(check())
                self.now = 1.31
                self.assertTrue(check())

    def test_valid_but_wrong_position_still_fails_and_resets_stability(self):
        self.save()
        check = self.poller()
        self.assertFalse(check())
        self.now = 0.7
        self.assertTrue(check())
        self.save({"x": 290, "y": 110})
        self.now = 30
        self.assertFalse(check())
        self.now = 60
        self.assertFalse(check())
        self.save()
        self.assertFalse(check())
        self.now = 60.61
        self.assertTrue(check())

    def test_other_io_errors_and_wrong_json_shape_are_not_swallowed(self):
        path = Mock()
        path.read_text.side_effect = PermissionError("unreadable")
        with self.assertRaises(PermissionError):
            read_config_snapshot(path)
        self.config.write_text("[]")
        with self.assertRaises(TypeError):
            self.poller()()


if __name__ == "__main__":
    unittest.main()
