"""Explicit-version repository targets and dependency resolution, independently on DNF4/5."""
import http.server
import os
import socketserver
from pathlib import Path
import shutil
import threading
import native_rpm as fixture
from protocol import Operation

root=fixture.root
repo=root/'repository';repo.mkdir()
for pattern in ['vigil-native-rpm-*/RPMS/noarch/*.rpm','vigil-native-dependency-*/RPMS/noarch/*.rpm','vigil-native-unsigned-*/RPMS/noarch/*.rpm']:
    for file in root.glob(pattern):shutil.copyfile(file,repo/file.name)
fixture.run('createrepo_c',str(repo))
class Handler(http.server.SimpleHTTPRequestHandler):
    def log_message(self,*args):pass
class Server(socketserver.ThreadingMixIn,http.server.HTTPServer):
    daemon_threads=True
os.chdir(repo)
server=Server(('127.0.0.1',0),Handler)
threading.Thread(target=server.serve_forever,daemon=True).start()
try:
    Operation({'kind':'packages_remove','packages':[{'kind':'repository','name':'vigil-native-dependency','version':None}]}).finish()
    source=Operation({'kind':'package_sources_create','definition':{'format':'rpm','name':'vigil-repository-test','url':'http://127.0.0.1:'+str(server.server_port),'public_key':fixture.public}}).finish().result()
    source=Operation({'kind':'package_sources_manage','source_id':source['sourceId'],'expected_hash':source['hash'],'enabled':True}).finish().result()
    for version,action in [('1.0-1','packages_install'),('2.0-1','packages_update')]:
        result=Operation({'kind':action,'packages':[{'kind':'repository','name':'vigil-native-rpm','version':version}]}).finish()
        assert fixture.run('rpm','-q','--qf','%{VERSION}-%{RELEASE}','vigil-native-rpm')==version.encode()
        assert fixture.run('rpm','-q','vigil-native-dependency')
        assert Path('/etc/vigil-rpm-trigger').read_text().strip()=='triggered'
        print('PASS',result.result()['nativePackageResult']['backend'],'repository',action,version,'automatic dependency resolution',flush=True)
    Operation({'kind':'packages_install','packages':[{'kind':'repository','name':'vigil-native-unsigned','version':'1.0-1'}]}).finish(False)
    print('PASS repository unsigned package rejected without implicit trust import',flush=True)
    Operation({'kind':'packages_remove','packages':[{'kind':'repository','name':'vigil-native-rpm','version':None}]}).finish()
    Operation({'kind':'package_sources_remove','source_id':source['sourceId'],'expected_hash':source['hash']}).finish()
finally:
    server.shutdown();server.server_close()
