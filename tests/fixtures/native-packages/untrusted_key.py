import os
os.environ['NATIVE_FAULT_NO_IMPORT']='1'
from fault_package import *
from protocol import Operation
assert not apt
name,calls,op=build('untrusted')
before=run('rpm','-qa','gpg-pubkey').splitlines()
result=Operation(op).finish(False)
assert not calls.exists() and before==run('rpm','-qa','gpg-pubkey').splitlines(),result.task
print('PASS signed RPM with unknown key rejected without automatic key import',flush=True)
