"""Native source lifecycle and APT repository verification in disposable containers."""
import functools
import hashlib
import http.server
import os
from pathlib import Path
import shutil
import subprocess
import threading
import time
import uuid
from email.utils import formatdate
from protocol import Operation
assert os.geteuid()==0 and Path('/.dockerenv').exists()
root=Path('/srv/vigil-source-'+uuid.uuid4().hex[:8]);root.mkdir()
fmt='apt' if shutil.which('apt-get') else 'rpm'
gpg=shutil.which('gpg') or shutil.which('gpg2')
def run(*args,**kwargs):return subprocess.check_output(args,stderr=subprocess.STDOUT,**kwargs)
def key(number):
    home=root/('key-'+str(number));home.mkdir(mode=0o700)
    uid='Vigil source '+str(number)+' <fixture@invalid>'
    run(gpg,'--batch','--homedir',str(home),'--passphrase','','--quick-generate-key',uid,'rsa2048','sign','1d')
    return home,run(gpg,'--batch','--homedir',str(home),'--armor','--export',uid).decode()
def invoke(op,success=True):return Operation(op).finish(success)
home,public=key(1);name='source-'+uuid.uuid4().hex[:8]
definition={'format':fmt,'name':name,'url':'http://127.0.0.1:18081/repo','public_key':public}
if fmt=='apt':definition.update({'suite':'stable','components':['main']})
created=invoke({'kind':'package_sources_create','definition':definition}).result()
assert len(created['sourceId'])==64,created
state=invoke({'kind':'package_sources_manage','source_id':created['sourceId'],'expected_hash':created['hash'],'enabled':True}).result()
changed=invoke({'kind':'package_sources_update','source_id':state['sourceId'],'expected_hash':state['hash'],'patch':{'url':'http://127.0.0.1:18082/repo'}}).result()
assert changed['enabled'] is True,changed
text=Path(created['path']).read_text();assert ('Enabled: yes' if fmt=='apt' else 'enabled=1') in text
invoke({'kind':'package_sources_update','source_id':state['sourceId'],'expected_hash':state['hash'],'patch':{'url':'http://127.0.0.1:18083/repo'}},False)
_,second=key(2)
rotated=invoke({'kind':'package_sources_update','source_id':changed['sourceId'],'expected_hash':changed['hash'],'patch':{'public_key':second}}).result()
assert rotated['enabled'] is True
if fmt=='rpm':assert rotated['globalKeyImported'] is True
state=invoke({'kind':'package_sources_manage','source_id':rotated['sourceId'],'expected_hash':rotated['hash'],'enabled':False}).result()
invoke({'kind':'package_sources_remove','source_id':state['sourceId'],'expected_hash':state['hash']})
assert not Path(created['path']).exists()
print('PASS',fmt,'source create/enable/update preserves enabled/key rotation/conflict/disable/remove',flush=True)
if fmt!='apt':raise SystemExit(0)
# Isolate the fixture's software sources; these files exist only in this disposable container.
for path in Path('/etc/apt/sources.list.d').iterdir():
    if path.suffix in ('.sources','.list'):path.rename(path.with_suffix(path.suffix+'.disabled'))
if Path('/etc/apt/sources.list').exists():Path('/etc/apt/sources.list').rename('/etc/apt/sources.list.disabled')
repo=root/'repository';(repo/'pool').mkdir(parents=True);index=repo/'dists/stable/main/binary-arm64';index.mkdir(parents=True)
staging=root/'package';(staging/'DEBIAN').mkdir(parents=True);(staging/'usr/share/vigil-repository-fixture').mkdir(parents=True)
(staging/'DEBIAN/control').write_text('Package: vigil-repository-fixture\nVersion: 1.0\nArchitecture: all\nMaintainer: fixture <fixture@invalid>\nDescription: signed repository fixture\n')
(staging/'usr/share/vigil-repository-fixture/value').write_text('repository-install')
run('dpkg-deb','--build',str(staging),str(repo/'pool/fixture.deb'))
packages=subprocess.check_output(['dpkg-scanpackages','pool','/dev/null'],cwd=repo,stderr=subprocess.PIPE)
(index/'Packages').write_bytes(packages)
release=repo/'dists/stable/Release'
release.write_text('Origin: Vigil fixture\nLabel: Vigil fixture\nSuite: stable\nCodename: stable\nDate: '+formatdate(usegmt=True)+'\nArchitectures: arm64\nComponents: main\nSHA256:\n '+hashlib.sha256(packages).hexdigest()+' '+str(len(packages))+' main/binary-arm64/Packages\n')
inrelease=release.with_name('InRelease')
run(gpg,'--batch','--yes','--homedir',str(home),'--clearsign','--output',str(inrelease),str(release))
class Handler(http.server.SimpleHTTPRequestHandler):
    def log_message(self,*args):pass
    def do_GET(self):
        if 'If-Modified-Since' in self.headers:del self.headers['If-Modified-Since']
        return super().do_GET()
server=http.server.ThreadingHTTPServer(('127.0.0.1',0),functools.partial(Handler,directory=str(repo)))
threading.Thread(target=server.serve_forever,daemon=True).start()
try:
    definition={'format':'apt','name':'repository-'+uuid.uuid4().hex[:8],'url':'http://127.0.0.1:'+str(server.server_port),'suite':'stable','components':['main'],'public_key':public}
    source=invoke({'kind':'package_sources_create','definition':definition}).result()
    source=invoke({'kind':'package_sources_manage','source_id':source['sourceId'],'expected_hash':source['hash'],'enabled':True}).result()
    target={'kind':'repository','name':'vigil-repository-fixture','version':'1.0'}
    installed=invoke({'kind':'packages_install','packages':[target]})
    assert installed.result()['nativePackageResult']['packages'][0]['installed'] is True
    assert run('dpkg-query','-W','-f=${Version}','vigil-repository-fixture')==b'1.0'
    inrelease.write_text(inrelease.read_text().replace('Label: Vigil fixture','Label: Tampered fixture'))
    invoke({'kind':'packages_install','packages':[target]},False)
    assert run('dpkg-query','-W','-f=${Version}','vigil-repository-fixture')==b'1.0'
    print('PASS APT signed repository refresh/install over SSH without a remote runner; tampered repository signature rejected',flush=True)
finally:server.shutdown();server.server_close()
