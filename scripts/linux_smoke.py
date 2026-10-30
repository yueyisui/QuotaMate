#!/usr/bin/env python3
"""Exercise the packaged Linux app on a private D-Bus session with a fake Codex.

Run: xvfb-run -a /usr/bin/python3 scripts/linux_smoke.py /path/to/quotamate
Requires python3-dbus, python3-gi, Xvfb, xdotool, Openbox and a C compiler.
No real Codex account is used.
"""
import datetime
import argparse
import json
import os
from pathlib import Path
import re
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
        except (dbus.DBusException, json.JSONDecodeError):
            pass
        time.sleep(0.1)
    raise AssertionError(f"Timed out waiting for {description}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=Path)
    parser.add_argument("--first-open-only", action="store_true", help="Diagnose a fresh tray-to-pet opening without later cycles")
    parser.add_argument("--menu-about-to-show", action="store_true", help="Send the tray host's AboutToShow lifecycle call before each menu event")
    parser.add_argument("--trace-native", action="store_true", help="Retain test-probe GTK map/configure diagnostics on success too")
    parser.add_argument("--rapid-display-switches", type=int, default=0, metavar="N", help="Send N rapid native-menu mode sequences and assert that the final selection wins")
    parser.add_argument("--suppress-pet-configure-handlers", action="store_true", help="Test geometry observation without configure callbacks or external dragging")
    options = parser.parse_args()
    if not os.environ.get("QUOTAMATE_SMOKE_PRIVATE_BUS"):
        env = {**os.environ, "QUOTAMATE_SMOKE_PRIVATE_BUS": "1"}
        # A private session must not auto-activate host portals or keyrings,
        # which would otherwise inherit the real user's HOME from an outer
        # dbus-run-session. Keep those unrelated services outside this test.
        with tempfile.TemporaryDirectory(prefix="quotamate-smoke-bus-") as directory:
            bus_config = Path(directory) / "session.conf"
            bus_config.write_text('''<!DOCTYPE busconfig PUBLIC "-//freedesktop//DTD D-Bus Bus Configuration 1.0//EN"
"http://www.freedesktop.org/standards/dbus/1.0/busconfig.dtd">
<busconfig><type>session</type><listen>unix:tmpdir=/tmp</listen><auth>EXTERNAL</auth>
<policy context="default"><allow send_destination="*"/><allow receive_sender="*"/><allow own="*"/></policy>
</busconfig>''')
            return subprocess.run(["dbus-run-session", f"--config-file={bus_config}", "--", sys.executable,
                                   str(Path(__file__).resolve()), *sys.argv[1:]], env=env).returncode
    binary = str(options.binary.resolve())
    # Give the scheduler a full minute even if CI starts at a minute boundary.
    second = datetime.datetime.now().second
    if second >= 50:
        time.sleep(61 - second)
    DBusGMainLoop(set_as_default=True)
    bus = dbus.SessionBus()
    watcher = Watcher(bus)
    with tempfile.TemporaryDirectory(prefix="quotamate-smoke-") as directory:
        root = Path(directory)
        probe = root / "x11-probe.so"
        subprocess.run(["cc", "-shared", "-fPIC", "-Wall", "-Wextra", "-Werror",
                        str(Path(__file__).with_name("linux_x11_probe.c")), "-ldl", "-o", str(probe)], check=True)
        mock = root / "codex"
        mock.write_text(MOCK_CODEX)
        mock.chmod(0o755)
        config_path = root / ".config/com.quotamate.app/config.json"
        config_path.parent.mkdir(parents=True)
        config_path.write_text(json.dumps({"language": "en", "petPreset": "car", "petScale": 0.7, "triggers": [{
            "id": "smoke", "enabled": True, "time": datetime.datetime.now().strftime("%H:%M"),
            "label": "Smoke", "lastRun": None, "lastRunDate": None, "lastStatus": None, "lastResult": None,
        }]}))
        env = {**os.environ, "HOME": directory, "XDG_CONFIG_HOME": str(root / ".config"),
               "XDG_DATA_HOME": str(root / "data"), "XDG_CACHE_HOME": str(root / "cache"),
               "CODEX_HOME": str(root / "codex-home"), "CODEX_CLI_PATH": str(mock),
               "QUOTAMATE_SMOKE_RUNS": str(root / "runs"), "QUOTAMATE_SMOKE_WEEKLY": str(root / "weekly"),
               # Xvfb has no GPU/DRI display. Use its software rendering path;
               # this is a test environment setting, independent of real GPUs.
               "WEBKIT_DISABLE_DMABUF_RENDERER": "1",
               "NO_AT_BRIDGE": "1",
               "LD_PRELOAD": str(probe), "LIBGL_ALWAYS_SOFTWARE": "1"}
        suppress_marker = root / "suppress-configure-handlers"
        if options.suppress_pet_configure_handlers:
            env["QUOTAMATE_TEST_SUPPRESS_CONFIGURE_MARKER"] = str(suppress_marker)
        with (root / "app.log").open("w+") as log:
            wm = subprocess.Popen(["openbox", "--sm-disable"], env=env, stdout=log, stderr=log)
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
                    if options.menu_about_to_show:
                        menu.AboutToShow(dbus.Int32(0))
                    entry = next(identifier for identifier, title in entries() if text in title)
                    print(f"MENU: {text} id={entry} monotonic={time.monotonic():.6f} aboutToShow={options.menu_about_to_show}", flush=True)
                    menu.Event(dbus.Int32(entry), "clicked", dbus.Int32(0), dbus.UInt32(0))

                def xdo(*args):
                    return subprocess.run(["xdotool", *map(str, args)], capture_output=True, text=True, check=True).stdout

                def pet_window():
                    result = subprocess.run(["xdotool", "search", "--onlyvisible", "--name", "^QuotaMate Pet$"], capture_output=True, text=True)
                    return result.stdout.strip().splitlines()[0] if result.returncode == 0 else None

                def geometry(window):
                    return {key: int(value) for key, value in
                            (line.split("=", 1) for line in xdo("getwindowgeometry", "--shell", window).splitlines())
                            if key in {"X", "Y", "WIDTH", "HEIGHT"}}

                def saved_position():
                    return json.loads(config_path.read_text()).get("petPosition") or {}

                def stable_position(expected, window=None):
                    # Observe a full debounce interval, so a later configure or
                    # hide event cannot silently overwrite the correct anchor.
                    stable_since = None

                    def check():
                        nonlocal stable_since
                        matches = saved_position() == expected
                        if window is not None:
                            actual = geometry(window)
                            matches = matches and actual["X"] == expected["x"] and actual["Y"] == expected["y"]
                        if not matches:
                            stable_since = None
                            return False
                        stable_since = stable_since or time.monotonic()
                        return time.monotonic() - stable_since >= 0.5

                    return check

                wait_for(lambda: any("5h 80% · W 65%" in title for _, title in entries()), "live quota in native menu", app)
                if options.first_open_only:
                    click("Desktop pet")
                    pet = wait_for(pet_window, "first pet visible", app)
                    wait_for(lambda: label() == "", "first pet mode clears panel label", app)
                    print(f"PASS: first pet geometry {geometry(pet)}", flush=True)
                    click("Exit")
                    assert app.wait(timeout=15) == 0
                    log.flush()
                    log.seek(0)
                    for line in log:
                        if "QUOTAMATE_X11_PROBE pet " in line:
                            print(line.rstrip(), flush=True)
                    return
                if options.rapid_display_switches > 0:
                    def settled_mode(mode):
                        stable_since = None
                        def check():
                            nonlocal stable_since
                            current = json.loads(config_path.read_text())
                            compact = mode == "Top bar quota"
                            pet = mode == "Desktop pet"
                            title = next(title for _, title in entries() if mode in title)
                            matches = (current["compactWidgetEnabled"] == compact
                                       and current["petEnabled"] == pet
                                       and label() == ("5h 80% · W 65%" if compact else "")
                                       and bool(pet_window()) == pet
                                       and title.startswith("●"))
                            if not matches:
                                stable_since = None
                                return False
                            stable_since = stable_since or time.monotonic()
                            return time.monotonic() - stable_since >= 0.5
                        return check

                    click("Desktop pet")
                    wait_for(settled_mode("Desktop pet"), "pet mode before rapid native-menu switches", app)
                    sequences = [
                        ("Top bar quota", ("Hidden", "Top bar quota")),
                        ("Desktop pet", ("Hidden", "Top bar quota", "Desktop pet")),
                        ("Hidden", ("Top bar quota", "Desktop pet", "Hidden")),
                    ]
                    for final_mode, sequence in sequences:
                        # No condition wait or artificial pause between menu
                        # events: older work must not override the last choice.
                        for _ in range(options.rapid_display_switches):
                            for mode in sequence:
                                click(mode)
                        wait_for(settled_mode(final_mode),
                                 f"{options.rapid_display_switches} rapid sequences keep final {final_mode} across config/menu/window/label", app)
                last_position = None
                for cycle in range(8):
                    click("Desktop pet")
                    pet = wait_for(pet_window, f"pet visible (cycle {cycle + 1})", app)
                    wait_for(lambda: (geometry(pet)["WIDTH"], geometry(pet)["HEIGHT"]) == (200, 200),
                             "scale 0.7 opens at the actual GTK minimum 200x200", app)
                    if last_position is not None:
                        wait_for(stable_position(last_position, pet),
                                 "reopening preserves the exact saved native position", app)
                    assert json.loads(config_path.read_text())["petScale"] == 0.7
                    wait_for(lambda: label() == "", "pet mode clears panel label", app)
                    # Window managers differ in whether they constrain an
                    # initial GTK window to the screen. Keep test input visible.
                    xdo("windowmove", pet, 400, 400)
                    wait_for(stable_position({"x": 400, "y": 400}, pet),
                             "pointer-input position is saved as exact native X/Y", app)
                    if options.suppress_pet_configure_handlers and cycle == 0:
                        # Arm only after the fully constructed pet is ready.
                        # The probe blocks existing handlers at the next resize,
                        # leaving GTK's class/default configure handler intact.
                        suppress_marker.touch()
                    # Real XTest pointer input exercises IPC, native resizing and
                    # the transparent WebKit window, beyond merely mapping it.
                    def expand():
                        size = geometry(pet)
                        if size["WIDTH"] == 310:
                            return True
                        xdo("mousemove", "--window", pet, size["WIDTH"] // 2, size["HEIGHT"] // 2, "click", 1)
                        time.sleep(1)
                        return geometry(pet)["WIDTH"] == 310
                    wait_for(expand, "pet click expands rendered quota details", app)
                    xdo("mousemove", "--window", pet, 155, 195)
                    time.sleep(0.2)
                    size = geometry(pet)
                    # An expansion anchored at the bottom right can cover
                    # (0, 0). Pick a screen corner outside the actual window.
                    corner = (0, 0)
                    if size["X"] <= 0 < size["X"] + size["WIDTH"] and size["Y"] <= 0 < size["Y"] + size["HEIGHT"]:
                        screen = tuple(map(int, xdo("getdisplaygeometry").split()))
                        corner = (screen[0] - 1, screen[1] - 1)
                    xdo("mousemove", *corner)
                    wait_for(lambda: (geometry(pet)["WIDTH"], geometry(pet)["HEIGHT"]) == (200, 200),
                             "pointer exit collapses pet to 200x200", app)
                    wait_for(stable_position({"x": 400, "y": 400}, pet),
                             "expanding and collapsing preserves the native anchor", app)
                    if options.suppress_pet_configure_handlers:
                        # Blocking every user handler also removes Tao's Moved
                        # notifications. This fault scenario covers controller
                        # geometry only; default runs retain real drag coverage.
                        last_position = {"x": 400, "y": 400}
                        wait_for(stable_position(last_position, pet),
                                 "collapsed anchor survives missing configure callbacks", app)
                    else:
                        # Moving the native window also exercises the debounced
                        # position persistence while WebKit animations are running.
                        xdo("windowmove", pet, 600 + cycle, 500)
                        last_position = {"x": 600 + cycle, "y": 500}
                        wait_for(stable_position(last_position, pet),
                                 "dragged position is persisted as exact native X/Y", app)
                    click("Hidden")
                    wait_for(lambda: not pet_window(), "pet can be hidden", app)
                    wait_for(stable_position(last_position), "hiding does not overwrite the saved native position", app)
                click("Desktop pet")
                pet = wait_for(pet_window, "pet can be reopened after repeated resizing", app)
                wait_for(stable_position(last_position, pet),
                         "final reopening preserves the exact saved position after eight cycles", app)
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
                log.flush()
                log.seek(0)
                output = log.read()
                assert "QUOTAMATE_X11_PROBE init=1" in output, "Xlib thread support must be initialized"
                assert "QUOTAMATE_X11_PROBE open initialized=1" in output, "Probe must observe a real display connection"
                assert "QUOTAMATE_X11_PROBE open initialized=0" not in output, "XInitThreads must precede every QuotaMate display connection"
                assert "geometry acknowledgement timed out" not in output, "Controlled geometry transitions must complete without watchdog recovery"
                if options.suppress_pet_configure_handlers:
                    summary = re.search(r"QUOTAMATE_X11_PROBE suppression-summary blocked=(\d+) callbacks=(\d+)", output)
                    assert summary and 0 < int(summary[1]) < 128, "Configure-handler suppression must be installed"
                    assert int(summary[2]) == 0, "Suppressed configure handlers must receive no later callbacks"
                    print(f"PASS: suppressed {summary[1]} configure handlers with zero later probe callbacks", flush=True)
                if options.trace_native:
                    for line in output.splitlines():
                        if "QUOTAMATE_X11_PROBE pet " in line:
                            print(line, flush=True)
                print("PASS: Xlib thread support initialized before opening displays", flush=True)
                print("PASS: geometry transitions complete without watchdog recovery", flush=True)
                print("PASS: clean exit from indicator menu", flush=True)
            except Exception:
                print(f"Window manager diagnostic: pid={wm.pid} returncode={wm.poll()}", file=sys.stderr)
                try:
                    config = json.loads(config_path.read_text())
                    fields = ("petEnabled", "compactWidgetEnabled", "petPreset", "petScale", "petPosition", "petAlwaysOnTop")
                    print("Fake configuration diagnostic: " + json.dumps({key: config.get(key) for key in fields}), file=sys.stderr)
                except (OSError, json.JSONDecodeError) as diagnostic_error:
                    print(f"Configuration diagnostic failed: {diagnostic_error}", file=sys.stderr)
                try:
                    found = subprocess.run(["xdotool", "search", "--name", "QuotaMate"], capture_output=True, text=True)
                    print(f"All QuotaMate X windows (including hidden): {found.stdout.strip() or '(none)'}", file=sys.stderr)
                    for window in found.stdout.splitlines():
                        for command in (["xdotool", "getwindowname", window], ["xdotool", "getwindowgeometry", "--shell", window],
                                        ["xwininfo", "-id", window]):
                            result = subprocess.run(command, capture_output=True, text=True)
                            output = result.stdout
                            if command[0] == "xwininfo":
                                output = "\n".join(line.strip() for line in output.splitlines() if "Map State:" in line or "Override Redirect State:" in line)
                            print(f"X window {window}: {output.strip()}", file=sys.stderr)
                    if watcher.item:
                        print(f"Panel label diagnostic: {label()!r}", file=sys.stderr)
                        print(f"Tray menu diagnostic: {entries()}", file=sys.stderr)
                except (OSError, NameError, dbus.DBusException) as diagnostic_error:
                    print(f"X/menu diagnostic failed: {diagnostic_error}", file=sys.stderr)
                try:
                    if pet_window():
                        print(f"Pet geometry at failure: {geometry(pet_window())}", file=sys.stderr)
                except (NameError, subprocess.CalledProcessError):
                    pass
                log.flush()
                log.seek(0)
                print(log.read(), file=sys.stderr)
                raise
            finally:
                if app.poll() is None:
                    os.killpg(app.pid, signal.SIGTERM)
                    app.wait(timeout=10)
                wm.terminate()
                wm.wait(timeout=10)


if __name__ == "__main__":
    sys.exit(main())
