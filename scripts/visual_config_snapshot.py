"""Read live test configuration without mistaking a partial save for a snapshot."""

import json
import time


def read_config_snapshot(path):
    try:
        snapshot = json.loads(path.read_text())
    except (FileNotFoundError, json.JSONDecodeError):
        return None
    if not isinstance(snapshot, dict):
        raise TypeError("Configuration snapshot must be a JSON object")
    return snapshot


def stable_config_snapshot(path, matches, stable_seconds, clock=time.monotonic):
    """Return a polling callback; its caller retains the existing deadline."""
    stable_since = None

    def check():
        nonlocal stable_since
        snapshot = read_config_snapshot(path)
        if snapshot is None or not matches(snapshot):
            stable_since = None
            return False
        now = clock()
        if stable_since is None:
            stable_since = now
        return now - stable_since >= stable_seconds

    return check
