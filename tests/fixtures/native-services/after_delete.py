import os,pathlib,subprocess,uuid,hashlib
from protocol import Operation
assert os.geteuid()==0 and subprocess.check_output(['systemd-detect-virt','--vm']).strip()==b'qemu'
def call(op,success=True):return Operation(op).finish(success)
name='vigil-after-delete-'+uuid.uuid4().hex[:8]+'.service'
text='[Service]\nExecStart=/usr/bin/sleep infinity\n'
unit=call({'kind':'services_register','definition':{'target':{'manager':'system','name':name},'content':{'kind':'unit','text':text},'expectedHash':None}}).result()
changed=call({'kind':'services_update','definition':{'target':{'manager':'system','name':name},'content':{'kind':'unit','text':text+'Environment=AFTER_DELETE=1\n'},'expectedHash':unit['hash']}}).result()
assert pathlib.Path(changed['backup']).read_text()==text
call({'kind':'services_control','name':name,'action':'start'});assert 'ActiveState=active' in call({'kind':'services_status','name':name}).result()['state']
call({'kind':'services_unregister','name':name,'drop_in':None,'expected_hash':unit['hash']},False)
call({'kind':'services_control','name':name,'action':'stop'})
call({'kind':'services_unregister','name':name,'drop_in':None,'expected_hash':changed['hash']})
# Shared native infrastructure still executes package queries without a runtime artifact.
call({'kind':'packages_list'})
assert not pathlib.Path('/root/.local/share/vigilshell/runtime').exists()
print('PASS post-deletion SSH unit update/version/backup/start/status/stop/removal; shared package query; no runtime deployed',flush=True)
