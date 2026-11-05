from fault_package import *
from protocol import Operation
name,calls,op=build('permission')
result=Operation(op).finish(False)
assert not calls.exists(),result.task
assert result.task['error']=='native_package_operation_failed',result.task
print('PASS non-root native operation denied without escalation',flush=True)
