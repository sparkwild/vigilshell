"""Native file entry semantics through the host executor and a disposable SSH guest."""
import argparse,hashlib,json,os,pathlib,subprocess,uuid
p=argparse.ArgumentParser();p.add_argument('--container',required=True);a=p.parse_args()
repo=pathlib.Path(__file__).resolve().parents[3]
info=json.loads(subprocess.check_output(['docker','inspect',a.container]))[0]
assert info['Config']['Labels'].get('vigilshell.acceptance')=='global-refactor' and not info['Mounts']
binding=info['NetworkSettings']['Ports']['22/tcp'][0];assert binding['HostIp']=='127.0.0.1'
root='/srv/fixture/scripts-native-'+uuid.uuid4().hex[:8]
def guest(code,data=None):return subprocess.check_output(['docker','exec','-i',a.container,'python3','-c',code],input=json.dumps(data).encode() if data is not None else b'')
def write(name,text):guest('import sys,json,pathlib;p,t=json.load(sys.stdin);f=pathlib.Path(p);f.parent.mkdir(parents=True,exist_ok=True);f.write_text(t)',[root+'/'+name,text])
def exists(name):return guest('import sys,json,pathlib;print(int(pathlib.Path(json.load(sys.stdin)).exists()))',root+'/'+name).strip()==b'1'
def remote(path):return {'scopeId':'__account__','path':path.lstrip('/')}
class Bridge:
 def __init__(self,review):
  env={**os.environ,'VIGILSHELL_TEST_NATIVE_PORT':binding['HostPort'],'VIGILSHELL_TEST_NATIVE_USER':'root','VIGILSHELL_TEST_SCRIPT_REVIEW':str(review).lower()}
  self.p=subprocess.Popen([str(repo/'target/debug/examples/native_fixture')],stdin=subprocess.PIPE,stdout=subprocess.PIPE,text=True,env=env)
 def call(self,**data):
  self.p.stdin.write(json.dumps({'includeOutput':True,**data})+'\n');self.p.stdin.flush();line=self.p.stdout.readline();assert line,'bridge ended';return json.loads(line)
 def close(self):self.p.stdin.close();assert self.p.wait(timeout=30)==0
 def script(self,interpreter,text=None,path=None,args=None,**options):
  source={'kind':'inline','text':text} if path is None else {'kind':'remoteFile','target':remote(path),'expected_hash':None}
  return self.call(operation={'kind':'script','source':source,'interpreter':interpreter,'arguments':args or [],'directory':remote(root)},**options)
def success(t):assert t['state']['outcome']=={'kind':'completed','exitCode':0},{k:t.get(k) for k in ['id','state','error']}
def output(t):return ''.join(r['body'].get('text','') for r in t['recordedOutput']['records'] if r['body']['kind']=='output')
write('placeholder','fixture')
b=Bridge(False)
try:
 t=b.script('sh',text='printf DEFAULT_OFF');success(t);assert t['state']['review']['kind']=='notRequired' and 'DEFAULT_OFF' in output(t)
 print('PASS default-off native file entry without review',flush=True)
 write('direct-large.sh','#'+('x'*300000)+'\nprintf DIRECT_LARGE_OK\n')
 path=root+'/direct-large.sh'
 guest('import pathlib,sys,json;p=pathlib.Path(json.load(sys.stdin));p.write_bytes(b"#\\xff\\n"+p.read_bytes())',path)
 direct=b.script('bash',path=path);success(direct);assert direct['scriptSnapshot'] is None and 'DIRECT_LARGE_OK' in output(direct)
 print('PASS review off executes original file without UTF-8 snapshot or review size restriction',flush=True)
finally:b.close()
b=Bridge(True)
try:
 for interpreter,text in [('sh','printf SH_OK'),('bash','printf BASH_OK'),('python3','print("PYTHON_OK")'),('node','console.log("NODE_OK")')]:
  t=b.script(interpreter,text=text);assert t['state']['phase']=='waitingReview' and t['state']['dispatch']['kind']=='notDispatched'
  result=b.call(approve=t['id']);success(result);assert interpreter.upper().replace('PYTHON3','PYTHON')+'_OK' in output(result)
  print('PASS reviewed inline native',interpreter,flush=True)
 write('lib.py','VALUE="RELATIVE_PY"\n');write('lib.cjs','module.exports="RELATIVE_NODE"\n');write('lib.mjs','export default "RELATIVE_ESM";\n')
 write('sibling.sh','printf "SIBLING:%s:%s:%s\\n" "${BASH_SOURCE[*]}" "${FUNCNAME[*]}" "${BASH_LINENO[*]}"\n')
 bash='printf "ZERO=%s\\nSOURCE=%s\\nPWD=%s\\nARG=%s\\n" "$0" "${BASH_SOURCE[0]}" "$PWD" "$1"\nf(){ printf "STACK:%s:%s:%s:%s\\n" "${BASH_SOURCE[*]}" "${FUNCNAME[*]}" "${LINENO}" "${BASH_LINENO[*]}"; }\nf\n. "${BASH_SOURCE[0]%/*}/sibling.sh"\nif read line; then exit 9; fi\n'
 cases=[('sh','original sh.sh','printf "ORIGINAL:%s:%s:%s" "$0" "$1" "$PWD"; if read line; then exit 9; fi'),('bash','original bash.sh',bash),('python3','original.py','import sys,os,lib\nassert sys.stdin.read()==""\nprint("ORIGINAL",__file__,sys.argv[1],os.getcwd(),lib.VALUE)\n'),('node','original.cjs','if(require("fs").readFileSync(0,"utf8")!=="")throw Error("stdin");console.log("ORIGINAL",__filename,__dirname,process.argv[2],require("./lib.cjs"));'),('node','original.mjs','import value from "./lib.mjs";console.log("ORIGINAL",import.meta.url,process.argv[2],value);')]
 for interpreter,name,text in cases:
  write(name,text);path=root+'/'+name
  program={'sh':'/bin/sh','bash':'/bin/bash','python3':'/usr/bin/python3','node':'/usr/bin/node'}[interpreter]
  native=subprocess.run(['docker','exec','-i','-w',root,a.container,'/usr/bin/env','-i','PATH=/usr/bin:/bin','HOME=/root','LANG=C.UTF-8','LC_ALL=C.UTF-8',program,'--',path,'a b'],input=b'',capture_output=True,check=True).stdout.decode()
  t=b.script(interpreter,path=path,args=['a b']);assert t['state']['phase']=='waitingReview' and t['scriptSnapshot']['sourceVersion'] is None
  result=b.call(approve=t['id']);success(result);assert output(result)==native,(interpreter,output(result),native)
  assert result['scriptStagingPath'] is None and guest('import pathlib,sys,json;print(pathlib.Path(json.load(sys.stdin)).read_text())',path).decode().strip()==text.strip()
  print('PASS native baseline parity: source/stack/lines/relative loading/arguments/cwd/EOF',interpreter,name,flush=True)
 for change in ['content','replacement','mode','removed','symlink']:
  name='changed-'+change+'.sh';path=root+'/'+name;text='printf BAD > '+root+'/bad-'+change;write(name,text)
  key='changed-'+change;t=b.script('bash',path=path,requestKey=key)
  if change=='content':write(name,'printf CHANGED > '+root+'/bad-'+change)
  elif change=='replacement':guest('import pathlib,sys,json;f=pathlib.Path(json.load(sys.stdin));n=f.with_suffix(".swap");n.write_bytes(f.read_bytes());n.replace(f)',path)
  elif change=='mode':guest('import pathlib,sys,json;pathlib.Path(json.load(sys.stdin)).chmod(0o600)',path)
  elif change=='removed':guest('import pathlib,sys,json;pathlib.Path(json.load(sys.stdin)).unlink()',path)
  else:guest('import pathlib,sys,json;f=pathlib.Path(json.load(sys.stdin));n=f.with_suffix(".target");n.write_bytes(f.read_bytes());f.unlink();f.symlink_to(n)',path)
  repeat=b.script('bash',path=path,requestKey=key);assert repeat['id']==t['id'] and repeat['scriptSnapshot']==t['scriptSnapshot']
  result=b.call(approve=t['id'])
  if change in ['content','removed']:
   assert result['state']['outcome']=={'kind':'notExecuted','reason':'denied'} and result['state']['dispatch']['kind']=='notDispatched' and result['error']=='script_source_changed',result
   assert not exists('bad-'+change)
   print('PASS source',change,'denied before dispatch; original receipt reused, no implicit reapproval/replay',flush=True)
  else:
   success(result);assert exists('bad-'+change)
   print('PASS unchanged content accepted after',change,flush=True)
  assert b.call(approve=t['id']).get('rejected')
 t=b.script('sh',text='printf BAD > '+root+'/bad');assert not exists('bad')
 assert b.call(approve=t['id'],hash='0'*64).get('rejected')
 denied=b.call(reject=t['id']);assert denied['state']['outcome']['kind']=='notExecuted' and not exists('bad')
 t=b.script('sh',text='printf BAD > '+root+'/bad');cancelled=b.call(cancel=t['id']);assert cancelled['state']['outcome']['kind']=='notExecuted' and not exists('bad')
 assert b.call(approve=t['id']).get('rejected')
 print('PASS wrong approval/rejection/cancellation/late approval execute nothing',flush=True)

 pending=[b.script('sh',text='printf PENDING') for _ in range(5)]
 command=b.call(operation={'kind':'command','directory':remote(root),'command':'printf CONCURRENCY_OK'});success(command);assert 'CONCURRENCY_OK' in output(command)
 for t in pending:b.call(cancel=t['id'])
 print('PASS waiting reviews do not consume execution concurrency',flush=True)
 large='#'+('x'*180000)+'\nprintf LARGE_SCRIPT_OK\n';t=b.script('bash',text=large);result=b.call(approve=t['id']);success(result);assert 'LARGE_SCRIPT_OK' in output(result)
 print('PASS large inline body runs as a real file, without oversized argv or eval',flush=True)
finally:b.close()
