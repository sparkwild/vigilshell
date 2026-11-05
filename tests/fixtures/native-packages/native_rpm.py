"""Real signed RPM/DNF acceptance; only run in a disposable root container."""
import hashlib
import os
from pathlib import Path
import shutil
import subprocess
from protocol import Operation

assert os.geteuid() == 0 and Path('/.dockerenv').exists()
root = Path('/srv/vigil-rpm-fixture'); root.mkdir(parents=True, exist_ok=True)
for file in Path('/etc/yum.repos.d').glob('*.repo'):
    file.rename(file.with_suffix('.repo.disabled'))
key = root / 'keys'; key.mkdir(mode=0o700)
uid = 'Vigil native fixture <fixture@invalid>'
gpg = shutil.which('gpg') or shutil.which('gpg2')
def run(*args):
    return subprocess.check_output(args, stderr=subprocess.STDOUT)
run(gpg, '--batch', '--homedir', str(key), '--passphrase', '', '--quick-generate-key', uid, 'rsa2048', 'sign', '1d')
public = run(gpg, '--batch', '--homedir', str(key), '--armor', '--export', uid).decode()
source = Operation({'kind': 'package_sources_create', 'definition': {'format': 'rpm', 'name': 'vigil-fixture', 'url': 'https://fixture.invalid/repository', 'public_key': public}}).finish().result()
assert source['globalKeyImported'] is True, source

def build(name, version, main=False, signed=True):
    top = root / (name + '-' + version); top.mkdir()
    spec = top / 'fixture.spec'
    dependency = 'Requires: vigil-native-dependency >= ' + version + '\n' if main else ''
    spec.write_text(f'''Name: {name}
Version: {version}
Release: 1
Summary: Isolated native RPM fixture
License: MIT
BuildArch: noarch
{dependency}%description
Disposable fixture
%install
mkdir -p %{{buildroot}}/usr/share/{name}
printf '{version}' > %{{buildroot}}/usr/share/{name}/version
%files
/usr/share/{name}/version
''' + ('''
%post
echo native > /etc/vigil-rpm-script
%triggerin -- vigil-native-dependency
echo triggered > /etc/vigil-rpm-trigger
''' if main else ''))
    run('rpmbuild', '--define', '_topdir ' + str(top), '-bb', str(spec))
    archive = next((top / 'RPMS/noarch').glob('*.rpm'))
    if signed:
        run('rpmsign', '--define', '_gpg_name ' + uid, '--define', '_gpg_path ' + str(key),
            '--define', '__gpg ' + gpg, '--define', '_openpgp_sign gpg',
            '--define', '_openpgp_sign_id ' + uid, '--addsign', str(archive))
    return {'kind': 'local', 'path': str(archive), 'sha256': hashlib.sha256(archive.read_bytes()).hexdigest()}

unsigned = build('vigil-native-unsigned', '1.0', signed=False)
Operation({'kind': 'packages_install', 'packages': [unsigned]}).finish(False)
assert subprocess.run(['rpm', '-q', 'vigil-native-unsigned'], stdout=subprocess.DEVNULL).returncode != 0
for version, action in [('1.0', 'packages_install'), ('2.0', 'packages_update')]:
    dependency = build('vigil-native-dependency', version)
    main = build('vigil-native-rpm', version, main=True)
    result = Operation({'kind': action, 'packages': [dependency, main]}).finish()
    assert run('rpm', '-q', '--qf', '%{VERSION}', 'vigil-native-rpm') == version.encode()
    assert run('rpm', '-q', '--qf', '%{VERSION}', 'vigil-native-dependency') == version.encode()
    assert Path('/etc/vigil-rpm-script').read_text().strip() == 'native'
    assert Path('/etc/vigil-rpm-trigger').read_text().strip() == 'triggered'
    print('PASS', result.result()['nativePackageResult']['backend'], action, version, 'signed packages, dependencies, scripts, triggers', flush=True)
Operation({'kind': 'packages_remove', 'packages': [{'kind': 'repository', 'name': 'vigil-native-rpm', 'version': None}]}).finish()
assert subprocess.run(['rpm', '-q', 'vigil-native-rpm'], stdout=subprocess.DEVNULL).returncode != 0
assert run('rpm', '-q', 'vigil-native-dependency')
assert Path('/etc/vigil-rpm-script').exists()
print('PASS native removal preserves unrelated dependency/data and unsigned RPM is rejected', flush=True)
