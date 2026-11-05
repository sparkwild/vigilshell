"""Host relay for a labelled mount-free disposable QEMU container."""
import argparse,json,os,pathlib,subprocess
p=argparse.ArgumentParser();p.add_argument('scenario');p.add_argument('--container',required=True);p.add_argument('--user',choices=['root','fixture'],default='root');p.add_argument('--resume-unit');a=p.parse_args()
if a.resume_unit:
 import re
 assert re.fullmatch(r'vigil-native-[0-9a-f]{8}\.service',a.resume_unit)
root=pathlib.Path(__file__).resolve().parents[3]
info=json.loads(subprocess.check_output(['docker','inspect',a.container]))[0]
assert info['Config']['Labels'].get('vigilshell.acceptance')=='native-services' and not info['Mounts']
binding=info['NetworkSettings']['Ports']['2222/tcp'][0];assert binding['HostIp']=='127.0.0.1'
ssh=['ssh','-i','/vm/identity','-o','BatchMode=yes','-o','ConnectTimeout=5','-o','StrictHostKeyChecking=accept-new','-o','UserKnownHostsFile=/vm/known_hosts','-p','2222','root@127.0.0.1']
def remote(command,data=None):return subprocess.run(['docker','exec','-i',a.container,*ssh,command],input=data,check=True)
remote('test "$(systemd-detect-virt --vm)" = qemu && command -v getfattr && mkdir -p /tmp/native-fixtures && chmod 755 /tmp/native-fixtures')
for path in pathlib.Path(__file__).parent.glob('*.py'):
 if path.name not in ['run.py','start_vm.py','power.py']:
  remote('cat > /tmp/native-fixtures/'+path.name,path.read_bytes())
env={**os.environ,'VIGILSHELL_TEST_NATIVE_PORT':binding['HostPort'],'VIGILSHELL_TEST_NATIVE_USER':a.user}
helper=subprocess.Popen([str(root/'target/debug/examples/native_fixture')],stdin=subprocess.PIPE,stdout=subprocess.PIPE,text=True,env=env)
guest=subprocess.Popen(['docker','exec','-i',a.container,*ssh,'runuser -u '+a.user+' -- env '+('NATIVE_RESUME_UNIT='+a.resume_unit+' ' if a.resume_unit else '')+'python3 -u /tmp/native-fixtures/'+a.scenario+'.py'],stdin=subprocess.PIPE,stdout=subprocess.PIPE,text=True)
try:
 for line in guest.stdout:
  if line.startswith('VIGIL_REQUEST '):
   helper.stdin.write(line[len('VIGIL_REQUEST '):]);helper.stdin.flush();response=helper.stdout.readline();assert response,'host bridge closed'
   value=json.loads(response);print(json.loads(line[len('VIGIL_REQUEST '):])['operation']['kind'],value.get('error'),value.get('state'),flush=True)
   guest.stdin.write(response);guest.stdin.flush()
  else:print(line,end='',flush=True)
 assert guest.wait()==0,'guest scenario failed'
finally:
 if guest.poll() is None:guest.terminate();guest.wait()
 helper.stdin.close();assert helper.wait(timeout=30)==0,'host bridge failed'
