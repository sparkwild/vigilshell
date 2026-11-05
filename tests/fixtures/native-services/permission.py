import os,pathlib
from protocol import Operation
assert os.geteuid()!=0
path=pathlib.Path('/etc/systemd/system/vigil-permission-test.service')
request={'kind':'services_register','definition':{'target':{'manager':'system','name':path.name},'content':{'kind':'unit','text':'[Service]\nExecStart=/usr/bin/sleep infinity\n'},'expectedHash':None}}
result=Operation(request).finish(False);assert not path.exists() and result.task['error']=='native_account_permission_denied',result.task
print('PASS actual unprivileged account cannot create a system definition; no elevation',flush=True)
