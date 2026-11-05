import fcntl,pathlib,time
from fault_package import *
from protocol import Operation
name,calls,op=build('lock')
lockpath=pathlib.Path('/var/lib/dpkg/lock-frontend') if apt else pathlib.Path(run('rpm','--eval','%{_dbpath}').decode().strip())/'.rpm.lock'
with lockpath.open('a') as lock:
 fcntl.lockf(lock,fcntl.LOCK_EX|fcntl.LOCK_NB);inode=lockpath.stat().st_ino
 result=Operation(op,timeout=8).finish(False)
 assert lockpath.stat().st_ino==inode and not calls.exists(),result.task
Operation(op).finish();assert calls.read_text().splitlines()==['once']
print('PASS native lock stays intact; explicit retry only after unlock',flush=True)
name,calls,op=build('readback')
query=pathlib.Path(shutil.which('dpkg-query' if apt else 'rpm'));original=query.with_name(query.name+'.fixture-original');query.rename(original)
query.write_text('#!/bin/sh\n'+('if [ "$1" = -W ]; then exit 2; fi\n' if apt else 'if [ "$1" = -q ]; then exit 2; fi\n')+'exec '+str(original)+' "$@"\n');query.chmod(0o755)
try:
 result=Operation(op).finish(False);assert result.task['state']['outcome']['kind']=='unknown',result.task
finally:query.unlink();original.rename(query)
assert calls.read_text().splitlines()==['once']
print('PASS installed package with unavailable readback stays unknown without replay',flush=True)
name,calls,op=build('cancel',True)
result=Operation(op,cancelAfterMs=3000).finish(False)
assert result.task['state']['outcome']['kind']=='unknown',result.task
assert calls.exists(),'cancellation must happen after native script started'
time.sleep(12)
assert calls.read_text().splitlines()==['once']
print('PASS cancel after native script started remains unknown; script runs once',flush=True)
