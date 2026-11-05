import os,pathlib,subprocess,time,uuid
from protocol import Operation
assert os.geteuid()==0 and subprocess.check_output(['systemd-detect-virt','--vm']).strip()==b'qemu'
def call(op,success=True,**options):return Operation(op,**options).finish(success)
name='vigil-fault-'+uuid.uuid4().hex[:8]+'.service';path=pathlib.Path('/etc/systemd/system')/name
text='[Service]\nExecStart=/usr/bin/sleep infinity\n'
unit=call({'kind':'services_register','definition':{'target':{'manager':'system','name':name},'content':{'kind':'unit','text':text},'expectedHash':None}}).result()
# Readback failure after a successful native action must stay unknown.
program=pathlib.Path('/usr/bin/systemctl');original=program.with_name('systemctl.fixture-original');program.rename(original)
program.write_text('#!/bin/sh\nfor arg in "$@"; do if [ "$arg" = show ]; then exit 2; fi; done\nexec '+str(original)+' "$@"\n');program.chmod(0o755)
try:
 result=call({'kind':'services_control','name':name,'action':'start'},False)
 assert result.task['state']['outcome']['kind']=='unknown',result.task
finally:program.unlink();original.rename(program)
assert subprocess.check_output(['systemctl','is-active',name]).strip()==b'active'
print('PASS successful native action with unavailable readback stays unknown; service remains active',flush=True)
# A published definition followed by daemon-reload failure is partial, never rolled back.
program.rename(original);program.write_text('#!/bin/sh\nfor arg in "$@"; do if [ "$arg" = daemon-reload ]; then exit 1; fi; done\nexec '+str(original)+' "$@"\n');program.chmod(0o755)
updated=text+'Environment=VIGIL_PARTIAL=1\n'
try:
 result=call({'kind':'services_update','definition':{'target':{'manager':'system','name':name},'content':{'kind':'unit','text':updated},'expectedHash':unit['hash']}},False)
 assert result.task['nativeReport']['effects']=='partial' and path.read_text()==updated,result.task
finally:program.unlink();original.rename(program);subprocess.run(['systemctl','daemon-reload'],check=True)
print('PASS published definition plus reload failure keeps partial result and backup',flush=True)
# Missing current-account bus is rejected before any management call.
bus=pathlib.Path('/run/user/0/bus');away=bus.with_name('bus.fixture-backup');bus.rename(away)
try:
 result=call({'kind':'services_status','name':'user:example.service'},False)
 assert result.task['error']=='user_service_manager_unavailable',result.task
finally:away.rename(bus)
print('PASS missing actual account user manager is reported unavailable',flush=True)
subprocess.run(['systemctl','stop',name],check=True);path.unlink();subprocess.run(['systemctl','daemon-reload'],check=True)
# Slow activation: stopping the systemctl client does not stop or replay the systemd job.
name='vigil-cancel-'+uuid.uuid4().hex[:8]+'.service';path=pathlib.Path('/etc/systemd/system')/name;marker=pathlib.Path('/tmp/'+name+'.calls')
text='[Service]\nType=oneshot\nRemainAfterExit=yes\nExecStart=/bin/sh -c "echo once >> '+str(marker)+'; sleep 60"\n'
created=call({'kind':'services_register','definition':{'target':{'manager':'system','name':name},'content':{'kind':'unit','text':text},'expectedHash':None}}).result()
result=call({'kind':'services_control','name':name,'action':'start'},False,cancelAfterMs=20000)
assert result.task['state']['outcome']['kind']=='unknown' and marker.read_text().splitlines()==['once'],result.task
state=subprocess.check_output(['systemctl','show','--property=ActiveState','--value',name]).strip();assert state in (b'activating',b'active'),state
print('PASS cancelling native client does not report service stopped or replay activation',flush=True)
subprocess.run(['systemctl','stop',name],check=True);path.unlink();subprocess.run(['systemctl','daemon-reload'],check=True)
