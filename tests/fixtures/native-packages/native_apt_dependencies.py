"""Real APT dependency, trigger and conffile handling in a disposable container."""
import hashlib
import os
from pathlib import Path
import subprocess
import uuid
from protocol import Operation

assert os.geteuid()==0 and Path('/.dockerenv').exists()
suffix=uuid.uuid4().hex[:8]
root=Path('/srv/vigil-apt-'+suffix);root.mkdir()
main='vigil-app-'+suffix;dependency='vigil-dep-'+suffix
configuration=Path('/etc/'+main+'.conf')
trigger=Path('/var/lib/'+main+'.trigger')
def build(name,version):
    stage=root/(name+'-'+version);(stage/'DEBIAN').mkdir(parents=True)
    control=f'Package: {name}\nVersion: {version}\nArchitecture: all\nMaintainer: fixture <fixture@invalid>\nDescription: APT native dependency fixture\n'
    if name==main:
        control+=f'Depends: {dependency} (>= 1.0)\n'
        (stage/'etc').mkdir();(stage/'etc'/configuration.name).write_text('packaged='+version+'\n')
        (stage/'DEBIAN/conffiles').write_text(str(configuration)+'\n')
        (stage/'DEBIAN/triggers').write_text('interest-noawait /usr/share/'+dependency+'\n')
        post=stage/'DEBIAN/postinst';post.write_text(f'#!/bin/sh\nif [ "$1" = triggered ]; then echo native-trigger > {trigger}; fi\n');post.chmod(0o755)
    else:
        (stage/'usr/share'/dependency).mkdir(parents=True)
        (stage/'usr/share'/dependency/'value').write_text(version)
    (stage/'DEBIAN/control').write_text(control)
    path=root/(name+'-'+version+'.deb')
    subprocess.run(['dpkg-deb','--build',str(stage),str(path)],check=True)
    return {'kind':'local','path':str(path),'sha256':hashlib.sha256(path.read_bytes()).hexdigest()}

first=build(main,'1.0');dep=build(dependency,'1.0')
Operation({'kind':'packages_install','packages':[first,dep]}).finish()
assert configuration.read_text()=='packaged=1.0\n'
# The interested package is now installed; updating its dependency activates the native file trigger.
Operation({'kind':'packages_update','packages':[build(dependency,'2.0')]}).finish()
assert trigger.read_text()=='native-trigger\n'
configuration.write_text('operator-owned\n')
Operation({'kind':'packages_update','packages':[build(main,'2.0')]}).finish()
assert configuration.read_text()=='operator-owned\n'
def remove(name):
    Operation({'kind':'packages_remove','packages':[{'kind':'repository','name':name,'version':None}]}).finish()
remove(main)
assert subprocess.check_output(['dpkg-query','-W','-f=${Status}',dependency]).strip()==b'install ok installed'
assert configuration.read_text()=='operator-owned\n'
remove(dependency)
print('PASS APT multi-package dependency, native trigger, upgrade preserves changed conffile, remove retains configuration/dependency',flush=True)
