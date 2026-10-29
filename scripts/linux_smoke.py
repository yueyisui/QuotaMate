#!/usr/bin/env python3
"""Exercise the packaged Linux app on a private D-Bus session with a fake Codex.

Run: dbus-run-session -- xvfb-run -a /usr/bin/python3 scripts/linux_smoke.py /path/to/quotamate
Requires python3-dbus, python3-gi and Xvfb. No real Codex account is used.
"""
import datetime
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import tempfile
import time

import dbus
import dbus.service
from dbus.mainloop.glib import DBusGMainLoop
from gi.repository import GLib


MOCK_CODEX = r'''#!/usr/bin/python3
import json, os, pathlib, sys
if "--version" in sys.argv:
    print("codex-cli 0.0.0-smoke")
    sys.exit(0)
if "exec" in sys.argv:
    with open(os.environ["QUOTAMATE_SMOKE_RUNS"], "a") as runs:
        runs.write(json.dumps(sys.argv[1:]) + "\n")
    print("OK")
    sys.exit(0)
for line in sys.stdin:
    message = json.loads(line)
    if "id" not in message:
        continue
    method = message.get("method")
    if method == "account/read":
        result = {"account": {"type": "chatgpt", "email": "smoke@example.invalid", "planType": "plus"}}
    elif method == "account/rateLimits/read":
        weekly_only = pathlib.Path(os.environ["QUOTAMATE_SMOKE_WEEKLY"]).exists()
        weekly = {"windowDurationMins": 10080, "usedPercent": 35}
        result = {"rateLimits": {"primary": weekly if weekly_only else {"windowDurationMins": 300, "usedPercent": 20}, "secondary": None if weekly_only else weekly}}
    else:
        result = {"userAgent": "quotamate-smoke"}
    print(json.dumps({"id": message["id"], "result": result}), flush=True)
'''


class Watcher(dbus.service.Object):
    def __init__(self, bus):
        self.name = dbus.service.BusName("org.kde.StatusNotifierWatcher", bus)
        super().__init__(bus, "/StatusNotifierWatcher")
        self.item = None

    @dbus.service.method("org.kde.StatusNotifierWatcher", in_signature="s", out_signature="", sender_keyword="sender")
    def RegisterStatusNotifierItem(self, item, sender=None):
        self.item = (sender, str(item)) if item.startswith("/") else (str(item), "/StatusNotifierItem")

    @dbus.service.method("org.kde.StatusNotifierWatcher", in_signature="s", out_signature="")
    def RegisterStatusNotifierHost(self, host):
        pass

    @dbus.service.method("org.freedesktop.DBus.Properties", in_signature="ss", out_signature="v")
    def Get(self, interface, prop):
        return self.GetAll(interface)[prop]

    @dbus.service.method("org.freedesktop.DBus.Properties", in_signature="s", out_signature="a{sv}")
    def GetAll(self, interface):
        return {"IsStatusNotifierHostRegistered": True, "RegisteredStatusNotifierItems": dbus.Array([], signature="s"), "ProtocolVersion": dbus.Int32(0)}


def wait_for(check, description, process, timeout=35):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        while GLib.MainContext.default().iteration(False):
            pass
        if process.poll() is not None:
            raise AssertionError(f"App exited with {process.returncode} while waiting for {description}")
        try:
            result = check()
            if result:
                print(f"PASS: {description}", flush=True)
                return result
        except dbus.DBusException:
            pass
        time.sleep(0.1)
    raise AssertionError(f"Timed out waiting for {description}")


def main():
    binary = str(Path(sys.argv[1]).resolve())
    # Give the scheduler a full minute even if CI starts at a minute boundary.
    second = datetime.datetime.now().second
    if second >= 50:
        time.sleep(61 - second)
    DBusGMainLoop(set_as_default=True)
    bus = dbus.SessionBus()
    watcher = Watcher(bus)
    with tempfile.TemporaryDirectory(prefix="quotamate-smoke-") as directory:
        root = Path(directory)
        mock = root / "codex"
        mock.write_text(MOCK_CODEX)
        mock.chmod(0o755)
        config_path = root / ".config/com.quotamate.app/config.json"
        config_path.parent.mkdir(parents=True)
        config_path.write_text(json.dumps({"language": "en", "triggers": [{
            "id": "smoke", "enabled": True, "time": datetime.datetime.now().strftime("%H:%M"),
            "label": "Smoke", "lastRun": None, "lastRunDate": None, "lastStatus": None, "lastResult": None,
        }]}))
        env = {**os.environ, "HOME": directory, "XDG_CONFIG_HOME": str(root / ".config"),
               "XDG_DATA_HOME": str(root / "data"), "XDG_CACHE_HOME": str(root / "cache"),
               "CODEX_HOME": str(root / "codex-home"), "CODEX_CLI_PATH": str(mock),
               "QUOTAMATE_SMOKE_RUNS": str(root / "runs"), "QUOTAMATE_SMOKE_WEEKLY": str(root / "weekly"),
               "WEBKIT_DISABLE_DMABUF_RENDERER": "1", "LIBGL_ALWAYS_SOFTWARE": "1"}
        with (root / "app.log").open("w+") as log:
            app = subprocess.Popen([binary, "--minimized"], env=env, stdout=log, stderr=log, start_new_session=True)
            try:
                wait_for(lambda: watcher.item, "native AppIndicator registration", app)
                item = bus.get_object(*watcher.item)
                properties = dbus.Interface(item, "org.freedesktop.DBus.Properties")
                label = lambda: str(properties.Get("org.kde.StatusNotifierItem", "XAyatanaLabel"))
                wait_for(lambda: label() == "5h 80% · W 65%", "live quota in panel label", app)
                menu_path = properties.Get("org.kde.StatusNotifierItem", "Menu")
                menu = dbus.Interface(bus.get_object(watcher.item[0], menu_path), "com.canonical.dbusmenu")

                def entries():
                    def flatten(node):
                        yield int(node[0]), str(node[1].get("label", ""))
                        for child in node[2]:
                            yield from flatten(child)
                    return list(flatten(menu.GetLayout(0, -1, ["label"])[1]))

                def click(text):
                    entry = next(identifier for identifier, title in entries() if text in title)
                    menu.Event(dbus.Int32(entry), "clicked", dbus.Int32(0), dbus.UInt32(0))

                wait_for(lambda: any("5h 80% · W 65%" in title for _, title in entries()), "live quota in native menu", app)
                click("Hidden")
                wait_for(lambda: label() == "", "hidden mode clears panel label", app)
                click("Top bar quota")
                wait_for(lambda: label() == "5h 80% · W 65%", "top bar mode restores quota immediately", app)
                click("Launch at startup")
                startup_files = lambda: list((root / ".config/autostart").glob("*.desktop"))
                wait_for(startup_files, "autostart creates a desktop entry", app)
                assert "--minimized" in startup_files()[0].read_text()
                click("Launch at startup")
                wait_for(lambda: not startup_files(), "disabling autostart removes the entry", app)
                (root / "weekly").touch()
                click("Refresh usage")
                wait_for(lambda: label() == "W 65%", "refresh removes absent 5h quota", app)
                wait_for(lambda: json.loads(config_path.read_text())["triggers"][0]["lastStatus"] == "success", "scheduled read-only session completes", app)
                runs = (root / "runs").read_text().splitlines()
                assert len(runs) == 1, "Scheduled session must execute exactly once"
                args = json.loads(runs[0])
                assert args[args.index("--sandbox") + 1] == "read-only"
                assert "--ephemeral" in args and "--ignore-rules" in args
                assert Path(args[args.index("--cd") + 1]).is_relative_to(root)
                print("PASS: one ephemeral session in an isolated directory", flush=True)
                click("Exit")
                assert app.wait(timeout=15) == 0
                print("PASS: clean exit from indicator menu", flush=True)
            except Exception:
                log.flush()
                log.seek(0)
                print(log.read(), file=sys.stderr)
                raise
            finally:
                if app.poll() is None:
                    os.killpg(app.pid, signal.SIGTERM)
                    app.wait(timeout=10)


if __name__ == "__main__":
    main()
