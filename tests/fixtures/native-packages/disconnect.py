from fault_package import *
from protocol import Operation
import time
name,calls,op=build('disconnect',True)
result=Operation(op,disconnectAfterMs=3000).finish(False)
assert result.task['state']['outcome']['kind']=='unknown',result.task
assert calls.exists()
time.sleep(12)
assert calls.read_text().splitlines()==['once']
print('PASS SSH disconnect during native script remains unknown without replay',flush=True)
