"""Build unique disposable packages; it never writes outside the container."""
import hashlib,os,pathlib,shutil,subprocess,uuid
assert os.geteuid()==0 and pathlib.Path('/.dockerenv').exists()
root=pathlib.Path('/srv/native-fault-'+uuid.uuid4().hex[:8]);root.mkdir()
apt=shutil.which('apt-get') is not None
# All source writes here are confined to the disposable container.
for directory,extensions in [('/etc/apt/sources.list.d',['.sources','.list']),('/etc/yum.repos.d',['.repo'])]:
 p=pathlib.Path(directory)
 if p.exists():
  for f in p.iterdir():
   if f.suffix in extensions:f.rename(f.with_suffix(f.suffix+'.disabled'))
def run(*args):return subprocess.check_output(args,stderr=subprocess.STDOUT)
if not apt:
 key=root/'keys';key.mkdir(mode=0o700);gpg=shutil.which('gpg') or shutil.which('gpg2');uid='Fault <fault@invalid>'
 run(gpg,'--homedir',str(key),'--batch','--passphrase','','--quick-generate-key',uid,'rsa2048','sign','1d')
 public=root/'key.asc';public.write_bytes(run(gpg,'--homedir',str(key),'--armor','--export',uid))
 if os.environ.get('NATIVE_FAULT_NO_IMPORT')!='1':run('rpm','--import',str(public))
def build(scenario,slow=False):
 name='vigil-'+scenario+'-'+uuid.uuid4().hex[:8];stage=root/name;stage.mkdir()
 calls=root/(name+'.calls')
 script='echo once >> '+str(calls)+'\n'+('sleep 10\n' if slow else '')
 if apt:
  (stage/'DEBIAN').mkdir();(stage/'DEBIAN/control').write_text(f'Package: {name}\nVersion: 1.0\nArchitecture: all\nMaintainer: test <t@invalid>\nDescription: fault\n')
  post=stage/'DEBIAN/postinst';post.write_text('#!/bin/sh\n'+script);post.chmod(0o755)
  archive=root/(name+'.deb');run('dpkg-deb','--build',str(stage),str(archive))
 else:
  spec=stage/'fixture.spec';spec.write_text(f'Name: {name}\nVersion: 1.0\nRelease: 1\nSummary: fault\nLicense: MIT\nBuildArch: noarch\n%description\nfault\n%install\nmkdir -p %{{buildroot}}/usr/share/{name}\necho x > %{{buildroot}}/usr/share/{name}/value\n%files\n/usr/share/{name}/value\n%post\n'+script)
  run('rpmbuild','--define','_topdir '+str(stage),'-bb',str(spec));archive=next((stage/'RPMS/noarch').glob('*.rpm'))
  run('rpmsign','--define','_gpg_name '+uid,'--define','_gpg_path '+str(key),'--define','__gpg '+gpg,'--define','_openpgp_sign gpg','--define','_openpgp_sign_id '+uid,'--addsign',str(archive))
 return name,calls,{'kind':'packages_install','packages':[{'kind':'local','path':str(archive),'sha256':hashlib.sha256(archive.read_bytes()).hexdigest()}]}
