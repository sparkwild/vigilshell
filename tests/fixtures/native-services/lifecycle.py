import os,pwd,subprocess,pathlib,uuid,hashlib,json
from protocol import Operation
assert subprocess.check_output(['systemd-detect-virt','--vm']).strip()==b'qemu'
user=os.geteuid()!=0;home=pathlib.Path(pwd.getpwuid(os.geteuid()).pw_dir)
resume=os.environ.get('NATIVE_RESUME_UNIT');name=resume or 'vigil-native-'+uuid.uuid4().hex[:8]+'.service';identifier=('user:' if user else '')+name
manager='currentUser' if user else 'system';target={'manager':manager,'name':name}
unit='[Unit]\nDescription=SSH fixture\n[Service]\nExecStart=/bin/sh -c "echo fixture-service-start; exec /usr/bin/sleep infinity"\nExecReload=/usr/bin/touch '+str(home/(name+'.reloaded'))+'\n[Install]\nWantedBy='+('default.target' if user else 'multi-user.target')+'\n'
def call(op,success=True,**options):return Operation(op,**options).finish(success)
def state():return call({'kind':'services_status','name':identifier}).result()['state']
if not resume:
 call({'kind':'services_list'})
 created=call({'kind':'services_register','definition':{'target':target,'content':{'kind':'unit','text':unit},'expectedHash':None}}).result()
 assert 'ActiveState=inactive' in state()
 call({'kind':'services_control','name':identifier,'action':'start'});assert 'ActiveState=active' in state()
 call({'kind':'services_control','name':identifier,'action':'reload'});assert (home/(name+'.reloaded')).exists()
else:
 path=(home/'.config/systemd/user'/name) if user else pathlib.Path('/etc/systemd/system')/name
 created={'hash':hashlib.sha256(path.read_bytes()).hexdigest(),'path':str(path)}
logs=call({'kind':'services_logs','name':identifier,'lines':20},includeOutput=True)
assert 'fixture-service-start' in json.dumps(logs.task['recordedOutput']),logs.task
info=call({'kind':'services_definition','name':identifier}).result();assert any(f['hash']==created['hash'] for f in info['editableFiles'])
pid=next(l for l in state().splitlines() if l.startswith('MainPID='))
drop=call({'kind':'services_register','definition':{'target':target,'content':{'kind':'dropIn','name':'environment.conf','text':'[Service]\nEnvironment=VIGIL=one\n'},'expectedHash':None}}).result()
assert pid in state()
changed=call({'kind':'services_update','definition':{'target':target,'content':{'kind':'dropIn','name':'environment.conf','text':'[Service]\nEnvironment=VIGIL=two\n'},'expectedHash':drop['hash']}}).result()
assert pid in state() and pathlib.Path(changed['backup']).read_text()=='[Service]\nEnvironment=VIGIL=one\n'
call({'kind':'services_set_enabled','name':identifier,'enabled':True,'expected_version':info['definitionHash']},False)
for enabled in [True,False]:
 call({'kind':'services_set_enabled','name':identifier,'enabled':enabled,'expected_version':None});assert pid in state()
call({'kind':'services_control','name':identifier,'action':'restart'});assert 'ActiveState=active' in state()
call({'kind':'services_unregister','name':identifier,'drop_in':'environment.conf','expected_hash':changed['hash']});assert 'ActiveState=active' in state()
call({'kind':'services_control','name':identifier,'action':'stop'});assert 'ActiveState=inactive' in state()
call({'kind':'services_unregister','name':identifier,'drop_in':None,'expected_hash':created['hash']});assert not pathlib.Path(created['path']).exists()
print('PASS',manager,'query/logs/start/reload/restart/stop/enable; unit/drop-in/version/backup; definitions never restart or stop service',flush=True)
