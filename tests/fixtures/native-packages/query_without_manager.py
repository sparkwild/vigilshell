import os,pathlib,shutil
from protocol import Operation
assert os.geteuid()==0 and pathlib.Path('/.dockerenv').exists()
paths={pathlib.Path(p).resolve() for p in [shutil.which(n) for n in ['apt-get','dnf','dnf5']] if p}
moved=[]
try:
 for path in paths:
  backup=path.with_name(path.name+'.fixture-original');path.rename(backup);moved.append((path,backup))
 result=Operation({'kind':'packages_list'}).finish().result()
 assert result['backend'] is None and result['packages'] and result['queryProgram'] in ('dpkg-query','rpm'),result
 denied=Operation({'kind':'packages_install','packages':[{'kind':'repository','name':'vigil-must-not-install','version':None}]}).finish(False)
 assert denied.task['error']=='package_backend_unavailable',denied.task
 print('PASS query works without installer; absent backend is not advertised or executed',flush=True)
finally:
 for path,backup in moved:backup.rename(path)
