"""Requests are executed by the host production SSH adapter, not a guest runtime."""
import json,sys
class Operation:
 def __init__(self,operation,**options):
  self.operation=operation
  print('VIGIL_REQUEST '+json.dumps({'operation':operation,**options}),flush=True)
  self.task=json.loads(sys.stdin.readline())
 def finish(self,success=True):
  report=self.task.get('nativeReport') or {};outcome=report.get('outcome') or {}
  ok=outcome.get('kind') in ('completed','requestAccepted') and (outcome.get('kind')=='requestAccepted' or outcome.get('exitCode')==0) and not report.get('error')
  assert ok==success,(self.operation,self.task)
  return self
 def result(self):return json.loads(self.task['nativeReport']['json'])
