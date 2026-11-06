"""Provision a stable CI signing identity; never import it into local signing keychains."""
import base64
import json
import os
from pathlib import Path
import secrets
import subprocess
import tempfile

REPO = 'sparkwild/vigilshell'
BACKUP = Path.home() / 'Library/Application Support/VigilShell Signing/release-backup'
SERVICE = 'com.sparkwild.vigilshell.release-signing.p12'


def run(args, data=None):
    result = subprocess.run(args, input=data, capture_output=True)
    if result.returncode:
        raise RuntimeError(f'{args[0]} operation failed (exit {result.returncode})')
    return result.stdout


def api(path, method='GET', body=None):
    args = ['gh', 'api', path, '--method', method]
    if body is not None:
        args += ['--input', '-']
    result = subprocess.run(args, input=json.dumps(body).encode() if body is not None else None, capture_output=True)
    if result.returncode:
        message = json.loads(result.stdout or b'{}').get('message', '')
        # Only explicit feature/plan restrictions permit fallback.
        if method == 'PUT' and ('upgrade' in message.lower() or 'plan does not support' in message.lower()) and ('public' in message.lower() or 'environment' in message.lower()):
            return None
        raise RuntimeError(f'GitHub {method} {path}: {message}')
    return json.loads(result.stdout) if result.stdout.strip() else {}


def main():
    os.umask(0o077)
    BACKUP.mkdir(parents=True, exist_ok=True, mode=0o700)
    p12 = BACKUP / 'release.p12'
    cert = BACKUP / 'certificate.pem'
    if p12.exists():
        password = run(['security', 'find-generic-password', '-s', SERVICE, '-a', 'release', '-w']).strip()
    else:
        if cert.exists():
            raise RuntimeError('Partial backup exists; refusing to replace an identity')
        existing = subprocess.run(['security', 'find-generic-password', '-s', SERVICE, '-a', 'release', '-w'], capture_output=True)
        if existing.returncode == 0:
            password = existing.stdout.strip()
        else:
            password = secrets.token_urlsafe(48).encode()
            run(['security', 'add-generic-password', '-s', SERVICE, '-a', 'release', '-w', password.decode()])
        with tempfile.TemporaryDirectory(prefix='vigilshell-release-') as temporary:
            temp = Path(temporary)
            config = temp / 'certificate.conf'
            config.write_text('[req]\ndistinguished_name=subject\nx509_extensions=signing\nprompt=no\n[subject]\nCN=VigilShell Release Signing\n[signing]\nbasicConstraints=critical,CA:false\nkeyUsage=critical,digitalSignature\nextendedKeyUsage=critical,codeSigning\n')
            run(['openssl', 'req', '-x509', '-newkey', 'rsa:3072', '-sha256', '-nodes', '-days', '3650', '-config', str(config), '-keyout', str(temp/'key.pem'), '-out', str(cert)])
            run(['openssl', 'pkcs12', '-export', '-inkey', str(temp/'key.pem'), '-in', str(cert), '-out', str(p12), '-keypbe', 'PBE-SHA1-3DES', '-certpbe', 'PBE-SHA1-3DES', '-macalg', 'sha1', '-passout', 'stdin'], password+b'\n')
    fingerprint = run(['openssl', 'x509', '-in', str(cert), '-noout', '-fingerprint', '-sha1']).decode().strip().split('=')[1].replace(':', '')
    # Confirm the backup password works before sending anything to GitHub.
    extracted = run(['openssl', 'pkcs12', '-in', str(p12), '-clcerts', '-nokeys', '-passin', 'stdin'], password+b'\n')
    actual = run(['openssl', 'x509', '-noout', '-fingerprint', '-sha1'], extracted).decode().strip().split('=')[1].replace(':', '')
    if actual != fingerprint:
        raise RuntimeError('Backup certificate mismatch')
    (BACKUP/'fingerprint.txt').write_text(fingerprint+'\n')
    environment = api(f'repos/{REPO}/environments/macos-release', 'PUT', {'deployment_branch_policy': {'protected_branches': False, 'custom_branch_policies': True}})
    scope = 'environment' if environment is not None else 'repository'
    prefix = f'repos/{REPO}/environments/macos-release' if scope == 'environment' else f'repos/{REPO}/actions'
    variables = api(prefix+'/variables')['variables']
    old = next((v['value'] for v in variables if v['name']=='MACOS_SIGNING_IDENTITY'), None)
    if old and old != fingerprint:
        raise RuntimeError('Existing GitHub identity differs; refusing replacement')
    if scope == 'environment':
        policies = api(prefix+'/deployment-branch-policies')['branch_policies']
        for kind, name in [('branch','main'),('branch','develop'),('tag','v*-beta.*')]:
            if not any(p['name']==name and p.get('type')==kind for p in policies):
                api(prefix+'/deployment-branch-policies','POST',{'name':name,'type':kind})
    extra = ['--env', 'macos-release'] if scope == 'environment' else []
    for name, value in [('MACOS_SIGNING_P12_BASE64',base64.b64encode(p12.read_bytes())),('MACOS_SIGNING_P12_PASSWORD',password)]:
        run(['gh','secret','set',name,'--repo',REPO,*extra],value)
    run(['gh','variable','set','MACOS_SIGNING_IDENTITY','--repo',REPO,*extra],fingerprint.encode())
    names = {s['name'] for s in api(prefix+'/secrets')['secrets']}
    if not {'MACOS_SIGNING_P12_BASE64','MACOS_SIGNING_P12_PASSWORD'} <= names:
        raise RuntimeError('Signing secrets were not stored')
    if api(prefix+'/variables/MACOS_SIGNING_IDENTITY')['value'] != fingerprint:
        raise RuntimeError('Stored fingerprint mismatch')
    run(['gh','variable','set','MACOS_SIGNING_STORAGE','--repo',REPO],scope.encode())
    print(json.dumps({'storage':scope,'backup':str(p12),'identity_verified':True,'secrets_present':True}))

if __name__ == '__main__':
    main()
