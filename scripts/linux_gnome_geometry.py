#!/usr/bin/python3
"""Exercise native GNOME/Mutter pet geometry using a fake account and private bus.

Run inside the Ubuntu GNOME X11 session (or set its DISPLAY/XAUTHORITY):
  /usr/bin/python3 scripts/linux_gnome_geometry.py /path/to/quotamate --output artifacts/gnome-geometry

Requires xprop and the same Python/native dependencies as linux_visual.py. The
existing GNOME window manager remains in control. This starts no WM, takes no
screenshots, sends no pointer input, and never unlocks the desktop. Its temporary
HOME/config/Codex account and service-free D-Bus are isolated from the user.
Only the probe-exported window whose native PID equals our child PID is moved.
Passing native geometry checks does not constitute visual acceptance.
"""
import argparse, datetime, hashlib, json, os, re, signal, subprocess, sys, tempfile, time, traceback
from pathlib import Path
sys.dont_write_bytecode = True
SCRIPTS = Path(__file__).resolve().parent
sys.path.insert(0, str(SCRIPTS))
from linux_visual import Commands, compile_probe, geometry, pump, wait, xdo
from linux_smoke import MOCK_CODEX, Watcher
import dbus
from dbus.mainloop.glib import DBusGMainLoop


def execute(options):
    output = options.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    report = {'status':'failed', 'purpose':'native GNOME geometry only; no visual acceptance',
        'screenshots':False, 'pointerInput':False, 'windowManagerStarted':False,
        'binary':str(options.binary.resolve()), 'sha256':hashlib.sha256(options.binary.read_bytes()).hexdigest(),
        'timestamp':datetime.datetime.now(datetime.timezone.utc).isoformat(), 'checks':[], 'observations':[]}
    app = None
    log = (output / 'app.log').open('w+')
    try:
        wm_property = subprocess.check_output(['xprop', '-root', '_NET_SUPPORTING_WM_CHECK'], text=True)
        wm_xid = re.search(r'0x[0-9a-fA-F]+', wm_property)
        if not wm_xid:
            raise RuntimeError('No existing X11 window manager; this diagnostic never starts one')
        wm_name = subprocess.check_output(['xprop', '-id', wm_xid.group(), '_NET_WM_NAME'], text=True).strip()
        report['windowManager'] = wm_name
        if not any(name in wm_name.lower() for name in ('gnome', 'mutter')):
            raise RuntimeError(f'This diagnostic requires existing GNOME/Mutter: {wm_name}')
        with tempfile.TemporaryDirectory(prefix='quotamate-gnome-native-') as directory:
            home = Path(directory)
            config = home / '.config/com.quotamate.app/config.json'
            config.parent.mkdir(parents=True)
            config.write_text(json.dumps({'configVersion':5,'language':'en','petPreset':'car',
                'petEnabled':True,'compactWidgetEnabled':False,'petScale':0.7,'opacity':0.65,
                'petPosition':{'x':400,'y':300},'petAlwaysOnTop':True,'schedulerEnabled':False,
                'launchAtStartup':False,'triggers':[]}))
            mock = home / 'codex'
            mock.write_text(MOCK_CODEX)
            mock.chmod(0o755)
            probe = home / 'visual-probe.so'
            compile_probe(probe)
            trace = home / 'x11-probe.so'
            subprocess.run(['cc','-shared','-fPIC','-Wall','-Wextra','-Werror',
                str(SCRIPTS / 'linux_x11_probe.c'),'-ldl','-o',str(trace)],check=True)
            env = {**os.environ,'HOME':directory,'XDG_CONFIG_HOME':str(home/'.config'),
                'XDG_DATA_HOME':str(home/'data'),'XDG_CACHE_HOME':str(home/'cache'),
                'CODEX_HOME':str(home/'codex-home'),'CODEX_CLI_PATH':str(mock),
                'QUOTAMATE_SMOKE_RUNS':str(home/'runs'),'QUOTAMATE_SMOKE_WEEKLY':str(home/'weekly'),
                'QUOTAMATE_VISUAL_COMMAND':str(home/'command.js'),'NO_AT_BRIDGE':'1',
                'LD_PRELOAD':str(probe)+':'+str(trace)}
            for name in ('LIBGL_ALWAYS_SOFTWARE','WEBKIT_DISABLE_DMABUF_RENDERER',
                         'QUOTAMATE_VISUAL_FORCE_REDRAW','QUOTAMATE_VISUAL_HARDWARE_POLICY',
                         'QUOTAMATE_VISUAL_EXTRA_PRELOAD'):
                env.pop(name,None)
            DBusGMainLoop(set_as_default=True)
            bus = dbus.SessionBus()
            watcher = Watcher(bus)
            app = subprocess.Popen([str(options.binary.resolve()),'--minimized'],env=env,
                stdout=log,stderr=log,start_new_session=True)
            report['appPid'] = app.pid
            wait(lambda:watcher.item,'private indicator registration',app)
            def own_pet():
                try:
                    xid = str(int((home/'command.js.window').read_text()))
                    owner = xdo('getwindowpid',xid,check=False)
                    return xid if owner.returncode==0 and int(owner.stdout.strip())==app.pid else None
                except (FileNotFoundError,ValueError):
                    return None
            pet = wait(own_pet,'probe XID owned by only this app PID',app)
            report['nativeXid'] = pet
            commands = Commands(home/'command.js',app,pet)
            properties = dbus.Interface(bus.get_object(*watcher.item),'org.freedesktop.DBus.Properties')
            menu_path = properties.Get('org.kde.StatusNotifierItem','Menu')
            menu = dbus.Interface(bus.get_object(watcher.item[0],menu_path),'com.canonical.dbusmenu')
            def click(text):
                def flatten(node):
                    yield int(node[0]),str(node[1].get('label',''))
                    for child in node[2]:
                        yield from flatten(child)
                identifier = next(i for i,title in flatten(menu.GetLayout(0,-1,['label'])[1]) if text in title)
                menu.Event(dbus.Int32(identifier),'clicked',dbus.Int32(0),dbus.UInt32(0))
            def visible():
                result = xdo('search','--onlyvisible','--pid',app.pid,'--name','^QuotaMate Pet$',check=False)
                return pet in result.stdout.splitlines()
            def saved():
                return json.loads(config.read_text())
            def stable(position,size,description,save_required=True):
                since = None
                last = None
                def matches():
                    nonlocal since,last
                    actual = geometry(pet)
                    persisted = saved().get('petPosition')
                    observation = {'phase':description,'native':actual,'savedPosition':persisted,
                        'expectedPosition':position,'expectedSize':list(size),'monotonic':round(time.monotonic(),3)}
                    report['lastObservation'] = observation
                    if last != (actual,persisted):
                        report['observations'].append(observation)
                        last = (actual,persisted)
                    valid = (actual['X'],actual['Y'])==(position['x'],position['y']) and (actual['WIDTH'],actual['HEIGHT'])==size
                    valid = valid and (not save_required or persisted==position)
                    if not valid:
                        since = None
                        return False
                    since = since or time.monotonic()
                    return time.monotonic()-since >= 0.65
                wait(matches,description,app)
                report['checks'].append({'name':description,'pass':True,'native':geometry(pet),'savedPosition':saved().get('petPosition')})
                print('PASS:',description,flush=True)
            def expanded(value):
                commands.run("await window.__TAURI_INTERNALS__.invoke('set_pet_expanded',{expanded:VALUE}); return true;".replace('VALUE','true' if value else 'false'))
            def patch(**changes):
                return commands.run("const invoke=window.__TAURI_INTERNALS__.invoke;const config=await invoke('get_config');return await invoke('save_config',{next:{...config,...CHANGES}});".replace('CHANGES',json.dumps(changes)))
            wait(lambda:commands.run("return !!document.querySelector('.pet-character');"),'test pet DOM without a frame/pixel barrier',app)
            wait(visible,'native mapped test pet',app)
            stable({'x':400,'y':300},(200,200),'initial 200x200 native pet restored')
            for cycle in range(8):
                anchor = {'x':400+cycle*8,'y':400+cycle*5}
                xdo('windowmove',pet,anchor['x'],anchor['y'])
                stable(anchor,(200,200),f'cycle {cycle+1}: native window move saved exactly')
                expanded(True)
                stable({'x':anchor['x']-110,'y':anchor['y']-190},(310,390),f'cycle {cycle+1}: expanded bottom-right anchor',save_required=False)
                expanded(False)
                stable(anchor,(200,200),f'cycle {cycle+1}: collapsed bottom-right anchor')
                click('Hidden')
                wait(lambda:not visible(),'own test pet hidden',app)
                wait(lambda:saved().get('petPosition')==anchor,'collapsed saved position persists while hidden',app)
                click('Desktop pet')
                wait(visible,'own test pet mapped again',app)
                stable(anchor,(200,200),f'cycle {cycle+1}: reopened native and saved position unchanged')
            anchor = {'x':400,'y':400}
            xdo('windowmove',pet,400,400)
            stable(anchor,(200,200),'scale stress native anchor')
            patch(petScale=1.5)
            stable({'x':270,'y':195},(330,405),'scale 0.7 to 1.5 keeps bottom-right anchor')
            patch(petScale=0.7)
            stable(anchor,(200,200),'scale 1.5 to 0.7 restores native and saved anchor')
            # Remain idle beyond the production two-second acknowledgement
            # deadline so a no-op GTK move cannot silently leave a stuck target.
            pump(2.3)
            if app.poll() is not None:
                raise RuntimeError('App exited during the final acknowledgement observation')
            report['finalConfig'] = saved()
            report['checks'].append({'name':'app remains alive through eight native GNOME cycles','pass':True})
            report['status'] = 'passed'
    except Exception as error:
        report['error'] = str(error)
        traceback.print_exc()
    finally:
        if app and app.poll() is None:
            os.killpg(app.pid,signal.SIGTERM)
            try:
                app.wait(timeout=10)
            except subprocess.TimeoutExpired:
                os.killpg(app.pid,signal.SIGKILL)
                app.wait(timeout=5)
        log.flush()
        log.close()
        content = (output/'app.log').read_text(errors='replace')
        timeouts = [line for line in content.splitlines() if 'geometry acknowledgement timed out' in line]
        report['geometryWatchdogTimeouts'] = timeouts
        report['checks'].append({'name':'no native geometry acknowledgement watchdog timeout','pass':not timeouts})
        if timeouts:
            report['status'] = 'failed'
        lines = content.splitlines()
        same_moves = []
        for index,line in enumerate(lines):
            request = re.search(r'pet request=move values=(-?\d+),(-?\d+)',line)
            if not request or index+1>=len(lines):
                continue
            native = re.search(r'native=(-?\d+),(-?\d+),(\d+),(\d+)',lines[index+1])
            if native and request.groups()==native.groups()[:2]:
                following = []
                for after in lines[index+2:]:
                    if 'pet request=move ' in after:
                        break
                    following.append(after)
                ack = any('pet configure-payload='+','.join(native.groups()) in after for after in following)
                same_moves.append({'requestLine':index+1,'target':[int(v) for v in native.groups()],
                    'matchingConfigureAckBeforeNextMove':ack})
        report['sameCoordinateNativeMoves'] = same_moves
        # Record the ACK trace separately from the strict position/watchdog
        # assertions; some GLib paths can complete from an already current
        # native geometry without emitting another ConfigureNotify.
        report['sameCoordinateMoveAckCount'] = sum(item['matchingConfigureAckBeforeNextMove'] for item in same_moves)
        acknowledged = report['sameCoordinateMoveAckCount'] > 0
        report['checks'].append({'name':'GNOME acknowledges a same-coordinate native move',
                                 'pass':acknowledged, 'acknowledgedMoves':report['sameCoordinateMoveAckCount']})
        if not acknowledged:
            report['status'] = 'failed'
        (output/'results.json').write_text(json.dumps(report,indent=2)+'\n')
        print('Native geometry report:',output/'results.json',flush=True)
    return 0 if report['status']=='passed' else 1


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('binary',type=Path)
    parser.add_argument('--output',type=Path,required=True)
    options = parser.parse_args()
    if not os.environ.get('QUOTAMATE_GNOME_PRIVATE_BUS'):
        with tempfile.TemporaryDirectory(prefix='quotamate-gnome-bus-') as directory:
            config = Path(directory)/'session.conf'
            config.write_text('<busconfig><type>session</type><listen>unix:tmpdir=/tmp</listen><auth>EXTERNAL</auth><policy context="default"><allow send_destination="*"/><allow receive_sender="*"/><allow own="*"/></policy></busconfig>')
            env = {**os.environ,'QUOTAMATE_GNOME_PRIVATE_BUS':'1'}
            return subprocess.run(['dbus-run-session',f'--config-file={config}','--',sys.executable,str(Path(__file__).resolve()),*sys.argv[1:]],env=env).returncode
    return execute(options)

if __name__=='__main__':
    sys.exit(main())
