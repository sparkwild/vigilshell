"""Inside a labelled disposable, mount-free Docker container only; no host power commands."""
import json,os,pathlib,subprocess
assert pathlib.Path('/.dockerenv').exists() and os.geteuid()==0
root=pathlib.Path('/vm');assert (root/'base.qcow2').exists() and not (root/'guest.qcow2').exists()
subprocess.run(['ssh-keygen','-q','-t','ed25519','-N','','-f',str(root/'identity')],check=True)
public=(root/'identity.pub').read_text().strip()
(root/'user-data').write_text('''#cloud-config
disable_root: false
ssh_pwauth: true
users:
  - name: root
    lock_passwd: false
    ssh_authorized_keys:
      - '''+public+'''
  - name: fixture
    shell: /bin/bash
    lock_passwd: false
chpasswd:
  expire: false
  users:
    - name: root
      password: native-fixture-only
      type: text
    - name: fixture
      password: native-fixture-only
      type: text
write_files:
  - path: /etc/ssh/sshd_config.d/00-fixture.conf
    permissions: '0644'
    content: |
      PermitRootLogin yes
      PasswordAuthentication yes
runcmd:
  - [systemctl, reload, ssh]
  - [sh, -c, "apt-get update -qq && apt-get install -y attr dbus-user-session >/var/log/native-fixture-setup.log 2>&1"]
  - [loginctl, enable-linger, fixture]
  - [sh, -c, "mkdir -p /var/log/journal; systemctl restart systemd-journald; touch /run/native-fixture-ready"]
''')
(root/'meta-data').write_text('instance-id: vigil-native-services\nlocal-hostname: vigil-native-services\n')
subprocess.run(['cloud-localds',str(root/'seed.img'),str(root/'user-data'),str(root/'meta-data')],check=True)
subprocess.run(['qemu-img','create','-f','qcow2','-F','qcow2','-b',str(root/'base.qcow2'),str(root/'guest.qcow2')],check=True)
args=['qemu-system-aarch64','-machine','virt','-cpu','cortex-a72','-accel','tcg','-m','1024','-smp','2','-nographic','-bios','/usr/share/qemu-efi-aarch64/QEMU_EFI.fd','-drive','file=/vm/guest.qcow2,format=qcow2,if=virtio','-drive','file=/vm/seed.img,format=raw,if=virtio','-netdev','user,id=net,hostfwd=tcp:0.0.0.0:2222-:22','-device','virtio-net-pci,netdev=net,romfile=']
with (root/'console.log').open('w') as output:
 p=subprocess.Popen(args,stdout=output,stderr=subprocess.STDOUT,start_new_session=True)
(root/'process.json').write_text(json.dumps({'pid':p.pid,'args':args}))
print('QEMU started',p.pid,flush=True)
