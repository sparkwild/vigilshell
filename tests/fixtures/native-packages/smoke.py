from fault_package import *
from protocol import Operation
name,calls,op=build('after-delete')
Operation({'kind':'packages_list'}).finish()
Operation(op).finish();assert calls.read_text().splitlines()==['once']
Operation({'kind':'packages_remove','packages':[{'kind':'repository','name':name,'version':None}]}).finish()
assert not (pathlib.Path('/root/.local/share/vigilshell')/'runtime').exists()
print('PASS after old-executor deletion: package query/install/remove through SSH, without deployed runtime',flush=True)
