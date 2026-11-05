"""Exercise account/manager probing over real SSH/SFTP with a fixture systemctl.

This validates probe arguments and environment, not a running systemd lifecycle.
The runner permits only disposable, labelled containers without host mounts.
"""
import json
import os
import pathlib
import shutil
import socket
import sys
import tempfile

assert os.geteuid() == 0 and pathlib.Path('/.dockerenv').exists()
systemctl = pathlib.Path(shutil.which('systemctl')).resolve()
run = pathlib.Path('/run/user/0')
assert not run.exists(), 'requires a fresh fixture without a user manager'
backup = pathlib.Path(tempfile.mkdtemp(prefix='vigil-probe-', dir='/srv'))
original = backup / 'systemctl'
denied = backup / 'deny'

def probe():
    print('VIGIL_REQUEST ' + json.dumps({'environment': True}), flush=True)
    value = json.loads(sys.stdin.readline())
    assert value['accountUid'] == 0 and value['sftp'], value
    return value

systemctl.rename(original)
bus = socket.socket(socket.AF_UNIX)
try:
    systemctl.write_text('''#!/usr/bin/python3
import os, pathlib, pwd, sys
user = '--user' in sys.argv
expected = ['--no-ask-password', '--user' if user else '--system', 'show', '--property=Version', '--value']
assert sys.argv[1:] == expected, sys.argv
assert os.environ['HOME'] == pwd.getpwuid(os.getuid()).pw_dir
assert os.environ['LC_ALL'] == 'C'
assert os.environ['SYSTEMD_PAGER'] == ''
if user:
    assert os.environ['XDG_RUNTIME_DIR'] == '/run/user/0'
    assert os.environ['DBUS_SESSION_BUS_ADDRESS'] == 'unix:path=/run/user/0/bus'
else:
    assert 'XDG_RUNTIME_DIR' not in os.environ
    assert 'DBUS_SESSION_BUS_ADDRESS' not in os.environ
if pathlib.Path(%r).exists():
    sys.exit(1)
print('fixture-version')
''' % str(denied))
    systemctl.chmod(0o755)
    run.mkdir(mode=0o700, parents=True)
    bus.bind(str(run / 'bus'))
    ready = probe()
    assert ready['systemServiceManager'] and ready['userServiceManager'], ready
    assert 'services_query.status' in ready['supportedTools']
    run.chmod(0o755)
    unsafe = probe()
    assert unsafe['systemServiceManager'] and not unsafe['userServiceManager'], unsafe
    run.chmod(0o700)
    denied.touch()
    unavailable = probe()
    assert not unavailable['systemServiceManager'] and not unavailable['userServiceManager'], unavailable
    assert 'services_query.status' not in unavailable['supportedTools']
    print('PASS SSH/SFTP account probe: system/user environments, unsafe user directory and failed manager query', flush=True)
finally:
    bus.close()
    shutil.rmtree(run)
    systemctl.unlink()
    original.rename(systemctl)
    shutil.rmtree(backup)
