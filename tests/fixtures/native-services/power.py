"""Observe actual reboot/power-off only in the exact disposable QEMU guest."""
import argparse,json,os,pathlib,subprocess,time
p=argparse.ArgumentParser();p.add_argument('--container',required=True);p.add_argument('--output',required=True);a=p.parse_args()
root=pathlib.Path(__file__).resolve().parents[3]
info=json.loads(subprocess.check_output(['docker','inspect',a.container]))[0]
assert info['Config']['Labels'].get('vigilshell.acceptance')=='native-services' and not info['Mounts']
binding=info['NetworkSettings']['Ports']['2222/tcp'][0];assert binding['HostIp']=='127.0.0.1'
ssh=['docker','exec',a.container,'ssh','-i','/vm/identity','-o','BatchMode=yes','-o','ConnectTimeout=3','-o','UserKnownHostsFile=/vm/known_hosts','-p','2222','root@127.0.0.1']
def probe(command):return subprocess.run(ssh+[command],capture_output=True,text=True,timeout=15)
assert probe('systemd-detect-virt --vm').stdout.strip()=='qemu'
pid=json.loads(subprocess.check_output(['docker','exec',a.container,'cat','/vm/process.json']))['pid']
def qemu_state():
 script='import pathlib,json; p=pathlib.Path("/proc/'+str(pid)+'/stat"); print(json.dumps(None if not p.exists() else p.read_text().split(") ",1)[1].split()))'
 return json.loads(subprocess.check_output(['docker','exec',a.container,'python3','-c',script]))
def wait_boot(previous=None):
 deadline=time.monotonic()+300
 while time.monotonic()<deadline:
  state=qemu_state();assert state and state[0]!='Z','QEMU ended during reboot'
  try:
   r=probe('cat /proc/sys/kernel/random/boot_id')
   if r.returncode==0 and r.stdout.strip() and r.stdout.strip()!=previous:return r.stdout.strip()
  except subprocess.TimeoutExpired:pass
  time.sleep(2)
 raise AssertionError('guest did not become ready')
def request(action):
 env={**os.environ,'VIGILSHELL_TEST_NATIVE_PORT':binding['HostPort'],'VIGILSHELL_TEST_NATIVE_USER':'root'}
 p=subprocess.Popen([str(root/'target/debug/examples/native_fixture')],stdin=subprocess.PIPE,stdout=subprocess.PIPE,text=True,env=env)
 try:
  p.stdin.write(json.dumps({'operation':{'kind':'power_control','action':action}})+'\n');p.stdin.flush()
  task=json.loads(p.stdout.readline())
  assert task['state']['outcome']['kind']=='requestAccepted',task
  report=task['nativeReport'];assert json.loads(report['json'])['effectConfirmed'] is False and report['effects']=='unknown',task
  return task
 finally:p.stdin.close();assert p.wait(timeout=30)==0
first=wait_boot();print('Guest boot',first,flush=True)
reboot=request('reboot');print('Reboot request accepted; waiting for independent boot evidence',flush=True)
second=wait_boot(first);print('PASS actual QEMU reboot changed boot ID',second,flush=True)
off=request('powerOff');deadline=time.monotonic()+90
while time.monotonic()<deadline:
 state=qemu_state()
 if state is None or state[0]=='Z':break
 time.sleep(1)
else:raise AssertionError('guest did not power off')
# A child of the container's idle PID 1 can remain a zombie; /proc field 52 is its exit status.
exit_status=None if state is None else int(state[49])
assert exit_status in (None,0),state
result={'firstBootId':first,'secondBootId':second,'rebootTask':reboot,'powerOffTask':off,'qemuStopped':True,'qemuExitStatus':exit_status}
pathlib.Path(a.output).write_text(json.dumps(result,indent=2)+'\n')
print('PASS power-off ended QEMU; tasks report accepted requests rather than completed power effects',flush=True)
