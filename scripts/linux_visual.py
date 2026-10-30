#!/usr/bin/env python3
"""Check real composited pixels from the packaged Linux pet window.

Default: private Xvfb + Openbox + xcompmgr, private D-Bus and fake Codex.
  /usr/bin/python3 scripts/linux_visual.py /path/to/quotamate --output artifacts/visual
Physical desktop (explicit opt-in, uses its actual GPU/compositor):
  /usr/bin/python3 scripts/linux_visual.py /path/to/quotamate --no-wm --output artifacts/visual-gpu

Requires python3-dbus, python3-gi, python3-gi-cairo, python3-pil, GTK3/WebKitGTK4.1 headers,
pkg-config, a C compiler, xdotool, dbus-run-session, and (default mode) Xvfb,
Openbox and xcompmgr. No real account, config, scheduler or autostart is used.
The C preload is test-only; the app itself exposes no test control interface.
"""
import argparse
import datetime
import hashlib
import json
import os
from pathlib import Path
import shlex
import shutil
import signal
import subprocess
import sys
import tempfile
import time
import traceback

sys.dont_write_bytecode = True

import dbus
from dbus.mainloop.glib import DBusGMainLoop
import gi
gi.require_version("Gtk", "3.0")
gi.require_version("Gdk", "3.0")
gi.require_version("GdkX11", "3.0")
gi.require_version("GdkPixbuf", "2.0")
from gi.repository import GLib
from PIL import Image, ImageChops, ImageDraw

from linux_smoke import MOCK_CODEX, Watcher


BACKGROUND = (22, 39, 58)


def pump(seconds=0):
    deadline = time.monotonic() + seconds
    while True:
        while GLib.MainContext.default().iteration(False):
            pass
        if time.monotonic() >= deadline:
            return
        time.sleep(min(0.03, max(0, deadline - time.monotonic())))


def wait(check, description, app=None, timeout=30):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        pump()
        if app and app.poll() is not None:
            raise RuntimeError(f"App exited with {app.returncode} while waiting for {description}")
        result = check()
        if result:
            return result
        pump(0.08)
    raise TimeoutError(f"Timed out waiting for {description}")


def xdo(*args, check=True):
    return subprocess.run(["xdotool", *map(str, args)], capture_output=True, text=True, check=check)


def geometry(window):
    return {key: int(value) for key, value in
            (line.split("=", 1) for line in xdo("getwindowgeometry", "--shell", window).stdout.splitlines())
            if key in {"X", "Y", "WIDTH", "HEIGHT"}}


def capture(rect, path):
    root = Gdk.get_default_root_window()
    pixbuf = Gdk.pixbuf_get_from_window(root, rect["X"], rect["Y"], rect["WIDTH"], rect["HEIGHT"])
    if pixbuf is None:
        raise RuntimeError("Composited root-window capture failed")
    pixbuf.savev(str(path), "png", [], [])
    with Image.open(path) as image:
        return image.convert("RGB")


def difference(actual, expected, tolerance=3):
    if actual.size != expected.size:
        return {"pass": False, "size": list(actual.size), "expectedSize": list(expected.size)}
    pixels = list(ImageChops.difference(actual, expected).getdata())
    bad = sum(max(pixel) > tolerance for pixel in pixels)
    return {"pass": bad <= max(12, len(pixels) // 1000), "differentPixels": bad,
            "totalPixels": len(pixels), "differentRatio": round(bad / len(pixels), 6),
            "maxChannelError": max(max(pixel) for pixel in pixels)}


class Commands:
    def __init__(self, path, app):
        self.path, self.app, self.sequence = path, app, 0

    def run(self, body, timeout=20):
        self.sequence += 1
        sequence = self.sequence
        script = f"""// quotamate-visual-request: {sequence}
(async () => {{
  try {{
    const result = await (async () => {{ {body} }})();
    window.webkit.messageHandlers.quotamateVisual.postMessage(JSON.stringify({{id:{sequence},ok:true,result}}));
  }} catch (error) {{
    window.webkit.messageHandlers.quotamateVisual.postMessage(JSON.stringify({{id:{sequence},ok:false,error:String(error.stack || error)}}));
  }}
}})(); void 0;
"""
        temporary = self.path.with_suffix(".next")
        temporary.write_text(script)
        temporary.replace(self.path)

        def reply():
            try:
                value = json.loads(Path(str(self.path) + ".ack").read_text())
                return value if value.get("id") == sequence else None
            except (FileNotFoundError, json.JSONDecodeError):
                return None

        response = wait(reply, f"WebKit visual command {sequence}", self.app, timeout)
        if not response.get("ok"):
            raise RuntimeError(response.get("error", "WebKit command failed"))
        return response.get("result")

    def patch(self, **changes):
        return self.run("""
const invoke = window.__TAURI_INTERNALS__.invoke;
const config = await invoke('get_config');
const next = await invoke('save_config', {next: {...config, ...CHANGES}});
await new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve)));
return next;
""".replace("CHANGES", json.dumps(changes)))

    def hide(self, hidden):
        self.run("""
let style = document.getElementById('quotamate-visual-hidden');
if (!style) { style = document.createElement('style'); style.id='quotamate-visual-hidden'; document.head.append(style); }
style.textContent = HIDDEN ? '.pet-window { visibility: hidden !important; }' : '';
await new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve)));
return {hidden:HIDDEN};
""".replace("HIDDEN", "true" if hidden else "false"))

    def pause(self, paused):
        self.run("""
let style = document.getElementById('quotamate-visual-static');
if (!style) { style = document.createElement('style'); style.id='quotamate-visual-static'; document.head.append(style); }
style.textContent = PAUSED ? '*, *::before, *::after { animation: none !important; transition: none !important; } .pet-toolbar { opacity:0 !important; }' : '';
await new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve)));
return {paused:PAUSED};
""".replace("PAUSED", "true" if paused else "false"))


def compile_probe(path):
    flags = subprocess.check_output(["pkg-config", "--cflags", "webkit2gtk-4.1"], text=True)
    # Resolve GTK/WebKit functions from the app itself, lazily. Linking them
    # here could preload host libraries into AppImage's launcher before AppRun
    # selects its own bundled runtime, invalidating that packaging test.
    subprocess.run(["cc", "-shared", "-fPIC", "-Wall", "-Wextra", "-Werror",
                    "-Wno-deprecated-declarations", str(Path(__file__).with_name("linux_visual_probe.c")),
                    "-o", str(path), *shlex.split(flags), "-Wl,-z,lazy", "-ldl"], check=True)


def process_tree_resources(root_pid):
    """Read only numeric /proc statistics, never command lines/environments."""
    records = {}
    for entry in Path("/proc").iterdir():
        if not entry.name.isdigit():
            continue
        try:
            fields = (entry / "stat").read_text().rsplit(")", 1)[1].split()
            records[int(entry.name)] = {"parent": int(fields[1]),
                                        "ticks": int(fields[11]) + int(fields[12]),
                                        "rss": int(fields[21]) * os.sysconf("SC_PAGE_SIZE")}
        except (OSError, ValueError, IndexError):
            continue
    descendants = {root_pid}
    while True:
        found = {pid for pid, value in records.items() if value["parent"] in descendants}
        if found <= descendants:
            break
        descendants.update(found)
    return {pid: records[pid] for pid in descendants if pid in records}


def sample_resources(app, seconds=10):
    previous = process_tree_resources(app.pid)
    previous_time = time.monotonic()
    cpu, rss = [], [sum(value["rss"] for value in previous.values())]
    for _ in range(seconds):
        pump(1)
        if app.poll() is not None:
            raise RuntimeError("App exited during resource sampling")
        current = process_tree_resources(app.pid)
        current_time = time.monotonic()
        ticks = sum(max(0, value["ticks"] - previous.get(pid, value)["ticks"])
                    for pid, value in current.items())
        cpu.append(100 * ticks / os.sysconf("SC_CLK_TCK") / (current_time - previous_time))
        rss.append(sum(value["rss"] for value in current.values()))
        previous, previous_time = current, current_time
    return {"sampleSeconds": seconds, "processesAtEnd": len(previous),
            "averageCpuPercentOneCore": round(sum(cpu) / len(cpu), 2),
            "peakCpuPercentOneCore": round(max(cpu), 2),
            "startAggregateRssMiB": round(rss[0] / 1024 ** 2, 2),
            "endAggregateRssMiB": round(rss[-1] / 1024 ** 2, 2),
            "peakAggregateRssMiB": round(max(rss) / 1024 ** 2, 2),
            "note": "RSS summed across app descendants, so shared pages can be counted more than once."}


def generate_transparent_gif(path):
    palette = [0, 0, 0, 255, 77, 116, 43, 211, 196, 255, 211, 96] + [0] * (768 - 12)
    frames = []
    for index, (x, y) in enumerate(((12, 20), (40, 12), (75, 25), (72, 72), (35, 80), (10, 65))):
        frame = Image.new("P", (128, 128), 0)
        frame.putpalette(palette)
        ImageDraw.Draw(frame).rounded_rectangle((x, y, x + 35, y + 35), radius=7, fill=1 + index % 3)
        frames.append(frame)
    frames[0].save(path, save_all=True, append_images=frames[1:], duration=150,
                   loop=0, transparency=0, background=0, disposal=2, optimize=False)


def run(args):
    global Gdk, Gtk
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    report = {"status": "failed", "binary": str(args.binary.resolve()),
              "sha256": hashlib.sha256(args.binary.read_bytes()).hexdigest(),
              "physicalDesktop": args.no_wm, "timestamp": datetime.datetime.now(datetime.timezone.utc).isoformat(),
              "checks": []}
    app = wm = compositor = xvfb = background = None
    original_mouse = None
    log = (output / "app.log").open("w+")
    try:
        with tempfile.TemporaryDirectory(prefix="quotamate-visual-") as directory:
            home = Path(directory)
            env = {**os.environ, "HOME": directory, "XDG_CONFIG_HOME": str(home / ".config"),
                   "XDG_DATA_HOME": str(home / "data"), "XDG_CACHE_HOME": str(home / "cache"),
                   "CODEX_HOME": str(home / "codex-home"), "CODEX_CLI_PATH": str(home / "codex"),
                   "QUOTAMATE_SMOKE_RUNS": str(home / "runs"),
                   "QUOTAMATE_SMOKE_WEEKLY": str(home / "weekly"),
                   "NO_AT_BRIDGE": "1", "QUOTAMATE_VISUAL_COMMAND": str(home / "command.js")}
            if not args.no_wm:
                read_fd, write_fd = os.pipe()
                xvfb = subprocess.Popen(["Xvfb", "-displayfd", str(write_fd), "-screen", "0", "1280x900x24",
                                         "-nolisten", "tcp", "-ac"], pass_fds=(write_fd,), stdout=log, stderr=log)
                os.close(write_fd)
                with os.fdopen(read_fd) as pipe:
                    display = pipe.readline().strip()
                if not display:
                    raise RuntimeError("Xvfb failed to allocate a private display")
                env["DISPLAY"] = f":{display}"
                os.environ["DISPLAY"] = env["DISPLAY"]
                # Virtual X servers have no GPU. Physical mode deliberately
                # inherits the real graphics configuration without these flags.
                env["LIBGL_ALWAYS_SOFTWARE"] = "1"
                env["WEBKIT_DISABLE_DMABUF_RENDERER"] = "1"
                rc = home / ".config/openbox/rc.xml"
                for candidate in (Path("/etc/xdg/openbox/rc.xml"), Path("/tmp/quotamate-wm/root/etc/xdg/openbox/rc.xml")):
                    if candidate.exists():
                        rc.parent.mkdir(parents=True, exist_ok=True)
                        shutil.copyfile(candidate, rc)
                        break
                wm = subprocess.Popen(["openbox", "--sm-disable"], env=env, stdout=log, stderr=log)
                time.sleep(0.5)
                compositor = subprocess.Popen(["xcompmgr", "-n"], env=env, stdout=log, stderr=log)
            elif not env.get("DISPLAY"):
                raise RuntimeError("--no-wm requires the physical desktop DISPLAY")
            # PyGObject initializes GTK when its override module is imported,
            # so import it only after selecting our private display.
            os.environ["NO_AT_BRIDGE"] = "1"
            from gi.repository import Gdk, GdkX11, Gtk
            if not Gtk.init_check()[0]:
                raise RuntimeError("GTK could not connect to the test display")
            wait(lambda: Gdk.Screen.get_default().is_composited(), "active X11 compositor", timeout=10)
            report["compositor"] = True
            report["display"] = env["DISPLAY"]
            report["renderingEnvironment"] = {key: env.get(key) for key in (
                "WEBKIT_DISABLE_DMABUF_RENDERER", "WEBKIT_DISABLE_COMPOSITING_MODE", "LIBGL_ALWAYS_SOFTWARE",
                "QUOTAMATE_VISUAL_FORCE_REDRAW", "QUOTAMATE_VISUAL_EXTRA_PRELOAD",
                "QUOTAMATE_VISUAL_HARDWARE_POLICY")}
            mouse = dict(line.split("=", 1) for line in xdo("getmouselocation", "--shell").stdout.splitlines())
            original_mouse = (int(mouse["X"]), int(mouse["Y"]))
            background = Gtk.Window(title="QuotaMate Visual Test Background")
            background.set_decorated(False)
            background.set_skip_taskbar_hint(True)
            background.set_accept_focus(False)
            background.set_default_size(700, 650)
            background.move(100, 100)
            area = Gtk.DrawingArea()

            def draw(_widget, context):
                context.set_source_rgb(*(value / 255 for value in BACKGROUND))
                context.paint()

            area.connect("draw", draw)
            background.add(area)
            background.show_all()
            pump(0.7)
            background_rect = geometry(str(GdkX11.X11Window.get_xid(background.get_window())))
            background_reference = capture(background_rect, output / "background-reference.png")
            solid = Image.new("RGB", background_reference.size, BACKGROUND)
            if not difference(background_reference, solid)["pass"]:
                raise RuntimeError("The reference background is occluded or not a uniform test color")
            DBusGMainLoop(set_as_default=True)
            bus = dbus.SessionBus()
            watcher = Watcher(bus)
            mock = home / "codex"
            mock.write_text(MOCK_CODEX)
            mock.chmod(0o755)
            config = home / ".config/com.quotamate.app/config.json"
            config.parent.mkdir(parents=True, exist_ok=True)
            config.write_text(json.dumps({"configVersion": 5, "language": "en", "petPreset": "car",
                                         "petEnabled": True, "compactWidgetEnabled": False,
                                         "petScale": 1, "opacity": 0.65, "petPosition": {"x": 400, "y": 300},
                                         "petAlwaysOnTop": True, "schedulerEnabled": False,
                                         "launchAtStartup": False, "triggers": []}))
            probe = home / "visual-probe.so"
            compile_probe(probe)
            env["LD_PRELOAD"] = str(probe)
            if env.get("QUOTAMATE_VISUAL_EXTRA_PRELOAD"):
                env["LD_PRELOAD"] += ":" + env["QUOTAMATE_VISUAL_EXTRA_PRELOAD"]
            app = subprocess.Popen([str(args.binary.resolve()), "--minimized"], env=env,
                                   stdout=log, stderr=log, start_new_session=True)
            wait(lambda: watcher.item, "private indicator registration", app)
            def test_pet_window():
                try:
                    xid = str(int((home / "command.js.window").read_text()))
                    return xid if xdo("getwindowgeometry", xid, check=False).returncode == 0 else None
                except (FileNotFoundError, ValueError):
                    return None

            pet = wait(test_pet_window, "isolated test pet's native window", app)
            commands = Commands(home / "command.js", app)
            wait(lambda: commands.run("return !!document.querySelector('.pet-character');"), "rendered pet DOM", app)
            commands.patch(petPreset="car", petScale=1, opacity=0.65)
            commands.pause(True)
            xdo("windowmove", pet, 400, 300)
            xdo("mousemove", 0, 0)
            pump(0.9)
            pet_rect = geometry(pet)
            report["petGeometry"] = pet_rect

            def check(name, result):
                report["checks"].append({"name": name, **result})
                print(f"{'PASS' if result['pass'] else 'FAIL'}: {name} {json.dumps(result)}", flush=True)

            reference = capture(pet_rect, output / "car-reference.png")
            check("visible pet produces non-background pixels", {"pass": not difference(reference, Image.new("RGB", reference.size, BACKGROUND))["pass"]})
            for index, preset in enumerate(("tiga", "cat", "robot", "dog", "rocket", "car")):
                commands.patch(petPreset=preset)
                pump(0.45)
                capture(geometry(pet), output / f"switch-{index + 1}-{preset}.png")
            final_car = capture(geometry(pet), output / "car-after-switches.png")
            check("car is unchanged after switching through other pets", difference(final_car, reference))
            commands.hide(True)
            pump(0.7)
            cleared = capture(background_rect, output / "clear-after-switches.png")
            check("switching pets leaves no pixels when content is hidden", difference(cleared, background_reference))
            commands.hide(False)
            commands.pause(False)
            commands.patch(petPreset="cat")
            animation_frames = []
            for frame in range(7):
                pump(1)
                animation_frames.append(capture(geometry(pet), output / f"animation-{frame + 1}.png"))
            distinct = len({hashlib.sha256(frame.tobytes()).hexdigest() for frame in animation_frames})
            max_changed = max(difference(frame, animation_frames[0])["differentPixels"] for frame in animation_frames)
            check("built-in CSS animation continues changing visible pixels", {
                "pass": distinct >= 3 and max_changed > 100, "distinctFrames": distinct,
                "maxChangedPixels": max_changed})
            commands.hide(True)
            pump(0.7)
            cleared = capture(background_rect, output / "clear-after-animation.png")
            check("animation leaves no pixels when content is hidden", difference(cleared, background_reference))
            commands.hide(False)
            for position in ((250, 220), (450, 320), (350, 250), (400, 300)):
                xdo("windowmove", pet, *position)
                pump(0.5)
            commands.hide(True)
            pump(0.7)
            cleared = capture(background_rect, output / "clear-after-moving.png")
            check("moving the animated pet leaves no residual pixels", difference(cleared, background_reference))
            commands.hide(False)
            commands.pause(True)
            commands.patch(petPreset="car", petScale=1, opacity=0.65)
            pump(0.5)

            # Every size/opacity pair receives a fresh reference. This catches
            # alpha accumulation at low opacity and stale regions after scale
            # changes, while keeping exact comparisons at the same dimensions.
            for scale in (0.7, 1, 1.5):
                for opacity in (0.35, 0.65, 1):
                    suffix = f"scale-{scale}-opacity-{opacity}"
                    commands.hide(False)
                    commands.patch(petPreset="car", petScale=scale, opacity=opacity)
                    xdo("windowmove", pet, 400, 300)
                    pump(0.5)
                    matrix_reference = capture(geometry(pet), output / f"{suffix}-reference.png")
                    for preset in ("tiga", "cat", "robot", "dog", "rocket", "car"):
                        commands.patch(petPreset=preset)
                        pump(0.12)
                    pump(0.3)
                    actual = capture(geometry(pet), output / f"{suffix}-roundtrip.png")
                    check(f"preset roundtrip at scale={scale}, opacity={opacity}", difference(actual, matrix_reference))
                    commands.hide(True)
                    pump(0.4)
                    clear = capture(background_rect, output / f"{suffix}-clear.png")
                    check(f"alpha clears at scale={scale}, opacity={opacity}", difference(clear, background_reference))

            commands.hide(False)
            commands.patch(petPreset="car", petScale=1, opacity=0.65)
            xdo("windowmove", pet, 400, 300)
            pump(0.5)
            rapid_reference = capture(geometry(pet), output / "rapid-reference.png")
            commands.run("""
const invoke = window.__TAURI_INTERNALS__.invoke;
for (let cycle=0; cycle<3; cycle++) {
  for (const petPreset of ['tiga','cat','robot','dog','rocket','car']) {
    const config = await invoke('get_config');
    await invoke('save_config', {next:{...config,petPreset}});
    await new Promise(resolve => setTimeout(resolve,30));
  }
}
await new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve)));
return {switches:18};
""")
            pump(0.5)
            rapid_final = capture(geometry(pet), output / "rapid-roundtrip.png")
            check("18 rapid preset changes preserve the final car pixels", difference(rapid_final, rapid_reference))
            commands.hide(True)
            pump(0.4)
            rapid_clear = capture(background_rect, output / "rapid-clear.png")
            check("rapid preset changes leave no residual pixels", difference(rapid_clear, background_reference))
            commands.hide(False)

            gif = output / "transparent-moving-pet.gif"
            generate_transparent_gif(gif)
            commands.run("await window.__TAURI_INTERNALS__.invoke('import_pet_image', {source:SOURCE}); return true;"
                         .replace("SOURCE", json.dumps(str(gif))))
            wait(lambda: commands.run("return !!document.querySelector('.custom-pet-stage');"),
                 "imported transparent animated GIF", app)
            pump(0.5)
            gif_frames = []
            for frame in range(8):
                pump(0.17)
                gif_frames.append(capture(geometry(pet), output / f"gif-animation-{frame + 1}.png"))
            distinct = len({hashlib.sha256(frame.tobytes()).hexdigest() for frame in gif_frames})
            max_changed = max(difference(frame, gif_frames[0])["differentPixels"] for frame in gif_frames)
            check("custom transparent GIF continues animating", {"pass": distinct >= 3 and max_changed > 100,
                  "distinctFrames": distinct, "maxChangedPixels": max_changed})
            commands.hide(True)
            pump(0.5)
            gif_clear = capture(background_rect, output / "clear-after-gif.png")
            check("custom transparent GIF leaves no residual pixels", difference(gif_clear, background_reference))
            commands.hide(False)
            report["resources"] = sample_resources(app)
            commands.patch(petPreset="car")
            # A real screenshot is retained for every check, together with the
            # app log and exact executable digest for reproducibility.
            check("app remains alive after visual stress", {"pass": app.poll() is None})
            report["status"] = "passed" if all(item["pass"] for item in report["checks"]) else "failed"
    except Exception as error:
        report["error"] = str(error)
        traceback.print_exc()
    finally:
        if original_mouse:
            try:
                xdo("mousemove", *original_mouse, check=False)
            except OSError:
                pass
        if app and app.poll() is None:
            os.killpg(app.pid, signal.SIGTERM)
            try:
                app.wait(timeout=10)
            except subprocess.TimeoutExpired:
                os.killpg(app.pid, signal.SIGKILL)
                app.wait(timeout=5)
        if background:
            background.destroy()
            pump()
        for process in (compositor, wm, xvfb):
            if process and process.poll() is None:
                process.terminate()
                try:
                    process.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait(timeout=5)
        log.close()
        (output / "results.json").write_text(json.dumps(report, indent=2) + "\n")
        print(f"Visual report: {output / 'results.json'}", flush=True)
    return 0 if report["status"] == "passed" else 1


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("binary", type=Path)
    parser.add_argument("--output", type=Path, default=Path("artifacts/linux-visual"))
    parser.add_argument("--no-wm", action="store_true", help="Use the current physical display and compositor; creates no Xvfb/WM")
    args = parser.parse_args()
    if not os.environ.get("QUOTAMATE_VISUAL_PRIVATE_BUS"):
        env = {**os.environ, "QUOTAMATE_VISUAL_PRIVATE_BUS": "1"}
        # No session service directories: the test must not auto-start host
        # portals/keyrings using the real user's home or desktop settings.
        with tempfile.TemporaryDirectory(prefix="quotamate-visual-bus-") as directory:
            config = Path(directory) / "session.conf"
            config.write_text('''<!DOCTYPE busconfig PUBLIC "-//freedesktop//DTD D-Bus Bus Configuration 1.0//EN"
"http://www.freedesktop.org/standards/dbus/1.0/busconfig.dtd">
<busconfig><type>session</type><listen>unix:tmpdir=/tmp</listen><auth>EXTERNAL</auth>
<policy context="default"><allow send_destination="*"/><allow receive_sender="*"/><allow own="*"/></policy>
</busconfig>''')
            return subprocess.run(["dbus-run-session", f"--config-file={config}", "--", sys.executable,
                                   str(Path(__file__).resolve()), *sys.argv[1:]], env=env).returncode
    return run(args)


if __name__ == "__main__":
    sys.exit(main())
