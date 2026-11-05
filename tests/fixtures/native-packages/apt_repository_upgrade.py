"""Continue the successful signed APT repository case with a requested-version upgrade."""
import functools,hashlib,http.server,pathlib,subprocess,threading,uuid
from email.utils import formatdate
from protocol import Operation
run=lambda *a,**kw:subprocess.check_output(a,stderr=subprocess.STDOUT,**kw)
assert run('dpkg-query','-W','-f=${Version}','vigil-repository-fixture')==b'1.0'
root=next(p for p in pathlib.Path('/srv').glob('vigil-source-*') if (p/'repository').exists())
repo=root/'repository';stage=root/'package';home=root/'key-1';gpg='/usr/bin/gpg'
control=stage/'DEBIAN/control';control.write_text(control.read_text().replace('Version: 1.0','Version: 2.0'))
run('dpkg-deb','--build',str(stage),str(repo/'pool/fixture.deb'))
# dpkg-scanpackages diagnostics must not enter Packages.
packages=subprocess.check_output(['dpkg-scanpackages','pool','/dev/null'],cwd=repo,stderr=subprocess.DEVNULL)
index=repo/'dists/stable/main/binary-arm64/Packages';index.write_bytes(packages)
release=repo/'dists/stable/Release';release.write_text('Origin: Vigil fixture\nLabel: Vigil fixture\nSuite: stable\nCodename: stable\nDate: '+formatdate(usegmt=True)+'\nArchitectures: arm64\nComponents: main\nSHA256:\n '+hashlib.sha256(packages).hexdigest()+' '+str(len(packages))+' main/binary-arm64/Packages\n')
run(gpg,'--batch','--yes','--homedir',str(home),'--clearsign','--output',str(release.with_name('InRelease')),str(release))
class Handler(http.server.SimpleHTTPRequestHandler):
 def log_message(self,*args):pass
server=http.server.ThreadingHTTPServer(('127.0.0.1',0),functools.partial(Handler,directory=str(repo)));threading.Thread(target=server.serve_forever,daemon=True).start()
try:
 public=run(gpg,'--homedir',str(home),'--armor','--export').decode()
 source=Operation({'kind':'package_sources_create','definition':{'format':'apt','name':'upgrade-'+uuid.uuid4().hex[:8],'url':'http://127.0.0.1:'+str(server.server_port),'suite':'stable','components':['main'],'public_key':public}}).finish().result()
 source=Operation({'kind':'package_sources_manage','source_id':source['sourceId'],'expected_hash':source['hash'],'enabled':True}).finish().result()
 Operation({'kind':'packages_update','packages':[{'kind':'repository','name':'vigil-repository-fixture','version':'2.0'}]}).finish()
 assert run('dpkg-query','-W','-f=${Version}','vigil-repository-fixture')==b'2.0'
 Operation({'kind':'packages_remove','packages':[{'kind':'repository','name':'vigil-repository-fixture','version':None}]}).finish()
 Operation({'kind':'package_sources_remove','source_id':source['sourceId'],'expected_hash':source['hash']}).finish()
 print('PASS APT requested-version repository upgrade and removal after native readback',flush=True)
finally:server.shutdown();server.server_close()
