import os,pathlib,subprocess,uuid
from protocol import Operation
assert os.geteuid()==0 and subprocess.check_output(['systemd-detect-virt','--vm']).strip()==b'qemu'
name='vigil-disconnect-'+uuid.uuid4().hex[:8]+'.service';path=pathlib.Path('/etc/systemd/system')/name;marker=pathlib.Path('/tmp/'+name+'.calls')
text='[Service]\nType=oneshot\nRemainAfterExit=yes\nExecStart=/bin/sh -c "echo once >> '+str(marker)+'; sleep 60"\n'
Operation({'kind':'services_register','definition':{'target':{'manager':'system','name':name},'content':{'kind':'unit','text':text},'expectedHash':None}}).finish()
result=Operation({'kind':'services_control','name':name,'action':'start'},disconnectAfterMs=20000).finish(False)
assert result.task['state']['outcome']['kind']=='unknown' and marker.read_text().splitlines()==['once'],result.task
assert subprocess.check_output(['systemctl','show','--property=ActiveState','--value',name]).strip() in (b'activating',b'active')
subprocess.run(['systemctl','stop',name],check=True);path.unlink();subprocess.run(['systemctl','daemon-reload'],check=True)
print('PASS SSH loss retains unknown task, does not stop systemd job or replay request',flush=True)
