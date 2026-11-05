"""Container-side business fixture bridge; production execution stays in the host SSH adapter."""
import json
import sys
class Operation:
    def __init__(self, operation, **options):
        self.operation = operation
        print('VIGIL_REQUEST '+json.dumps({'operation':operation,**options}), flush=True)
        self.task=json.loads(sys.stdin.readline())
    def finish(self, success=True):
        report=self.task.get('nativeReport') or {}
        outcome=report.get('outcome') or {}
        ok=outcome.get('kind')=='completed' and outcome.get('exitCode')==0 and not report.get('error')
        assert ok==success,(self.operation,self.task)
        return self
    def result(self):
        return json.loads(self.task['nativeReport']['json'])
