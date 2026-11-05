import json,os,pathlib,shutil,subprocess,sys,uuid
from protocol import Operation
assert os.geteuid()==0 and pathlib.Path('/.dockerenv').exists()
apt=shutil.which('apt-get') is not None
base=pathlib.Path('/etc/apt/sources.list.d' if apt else '/etc/yum.repos.d')
invalid=base/('vigil-invalid.sources' if apt else 'vigil-invalid.repo');invalid.write_text('not a supported configuration\n')
listing=Operation({'kind':'package_sources_list'}).finish().result()
row=next(r for r in listing['sources'] if r['path']==str(invalid));assert row['supported'] is False and row['id'] is None,row
invalid.unlink()
name='inspect-'+uuid.uuid4().hex[:8];root=pathlib.Path('/srv/'+name);root.mkdir(mode=0o700)
gpg=shutil.which('gpg') or shutil.which('gpg2');uid='Inspect <inspect@invalid>'
subprocess.run([gpg,'--homedir',str(root),'--batch','--passphrase','','--quick-generate-key',uid,'rsa2048','sign','1d'],check=True,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
key=subprocess.check_output([gpg,'--homedir',str(root),'--armor','--export',uid]).decode()
definition={'format':'apt' if apt else 'rpm','name':name,'url':'https://example.invalid/fixture','public_key':key}
if apt:definition.update(suite='stable',components=['main'])
source=Operation({'kind':'package_sources_create','definition':definition}).finish().result()
listing=Operation({'kind':'package_sources_list'}).finish().result()
row=next(r for r in listing['sources'] if r['id']==source['sourceId'])
assert row['supported'] and row['path']==source['path'] and row['hash']==source['hash'],row
actual_key=subprocess.check_output([gpg,'--homedir',str(root),'--batch','--with-colons','--show-keys',source['keyPath']]).decode()
primary=False;fingerprints=[]
for line in actual_key.splitlines():
 fields=line.split(':')
 if fields[0]=='pub':primary=True
 if primary and fields[0]=='fpr':fingerprints.append(fields[9]);primary=False
assert source['fingerprints'] and source['fingerprints']==fingerprints,source
assert ('URIs: https://example.invalid/fixture' if apt else 'baseurl=https://example.invalid/fixture') in pathlib.Path(source['path']).read_text()
Operation({'kind':'packages_list'}).finish()
# Only the explicit managed format can be rewritten; unknown options are retained and rejected.
path=pathlib.Path(source['path']);path.write_text(path.read_text()+('Architectures: arm64\n' if apt else 'cost=500\n'))
row=next(r for r in Operation({'kind':'package_sources_list'}).finish().result()['sources'] if r['path']==str(path))
failed=Operation({'kind':'package_sources_update','source_id':row['id'],'expected_hash':row['hash'],'patch':{'url':'https://example.invalid/other'}}).finish(False)
assert failed.task['error']=='package_source_contract_unsupported',failed.task
assert ('Architectures:' if apt else 'cost=500') in path.read_text()
Operation({'kind':'package_sources_remove','source_id':row['id'],'expected_hash':row['hash']}).finish()
assert pathlib.Path(source['keyPath']).exists()
print('PASS unsupported formats reject; native source task matches receipt, actual key and location; unknown options not rewritten; keys retained',flush=True)
