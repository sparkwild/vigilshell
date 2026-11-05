"""Native systemd edge cases omitted from the basic lifecycle acceptance."""
import hashlib
from pathlib import Path
import subprocess
import time
from protocol import Operation
assert subprocess.check_output(["systemd-detect-virt","--vm"]).strip()==b"qemu"
root=Path('/srv/vigil-service-edges');root.mkdir()
def call(op, success=True):return Operation(op).finish(success)
def define(name,text):
    return call({'kind':'services_register','definition':{'target':{'manager':'system','name':name},'content':{'kind':'unit','text':text},'expectedHash':None}}).result()
def control(name,action,success=True):return call({'kind':'services_control','name':name,'action':action},success)
def prop(name,key):return subprocess.check_output(['systemctl','show','-p',key,'--value',name],text=True).strip()
def wait_file(path):
    for _ in range(100):
        if path.exists():return
        time.sleep(.05)
    raise AssertionError(str(path))
dep='vigil-edge-dependency.service';template='vigil-edge@.service';instance='vigil-edge@one.service'
define(dep,'[Service]\nType=oneshot\nExecStart=/usr/bin/touch '+str(root/'dependency')+'\nRemainAfterExit=yes\n')
program=root/'program';program.write_text('#!/bin/sh\nprintf "%s" "$VALUE" > '+str(root/'observed')+'\nexec /usr/bin/sleep infinity\n');program.chmod(0o755)
(root/'env').write_text('VALUE=first\n')
text='[Unit]\nRequires='+dep+'\nAfter='+dep+'\n[Service]\nEnvironmentFile='+str(root/'env')+'\nExecStartPre=/usr/bin/touch '+str(root/'pre-%i')+'\nExecStart='+str(program)+'\nExecStopPost=/usr/bin/touch '+str(root/'stopped')+'\n'
created=define(template,text)
control(instance,'start');wait_file(root/'observed')
assert (root/'dependency').exists() and (root/'pre-one').exists()
assert (root/'observed').read_text()=='first'
pid=prop(instance,'MainPID');assert pid!='0'
control(instance,'reload',False)
assert prop(instance,'MainPID')==pid and not (root/'stopped').exists()
path=Path('/etc/systemd/system')/template
call({'kind':'services_update','definition':{'target':{'manager':'system','name':template},'content':{'kind':'unit','text':'[Service]\nExecStart=/not-existing/vigil-program\n'},'expectedHash':created['hash']}},False)
assert path.read_text()==text and prop(instance,'MainPID')==pid
(root/'env').write_text('VALUE=second\n');program.write_text('#!/bin/sh\nprintf "updated-%s" "$VALUE" > '+str(root/'observed')+'\nexec /usr/bin/sleep infinity\n')
control(instance,'restart')
for _ in range(100):
    if (root/'observed').read_text()=='updated-second':break
    time.sleep(.05)
assert (root/'observed').read_text()=='updated-second' and (root/'stopped').exists()
pid=prop(instance,'MainPID')
call({'kind':'services_unregister','name':template,'drop_in':None,'expected_hash':created['hash']})
assert not path.exists() and prop(instance,'MainPID')==pid and prop(instance,'ActiveState')=='active'
control(instance,'stop');control(dep,'stop')
vendor=Path('/lib/systemd/system/dbus.service');before=hashlib.sha256(vendor.read_bytes()).hexdigest()
call({'kind':'services_unregister','name':'dbus.service','drop_in':None,'expected_hash':before},False)
assert hashlib.sha256(vendor.read_bytes()).hexdigest()==before
print('PASS template instance, dependencies, EnvironmentFile, start/stop hooks, updated executable, reload refusal without restart, invalid definition preserves file/process, definition removal keeps running service, vendor unit deletion denied',flush=True)
