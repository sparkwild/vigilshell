# Native services and system settings over SSH

Only use a disposable QEMU guest inside a Docker container labelled `vigilshell.acceptance=native-services`, without host mounts, with guest SSH forwarded from Docker port 2222 to `127.0.0.1`. No privileged container, host systemd/socket, Docker restart, saved connection or Dev credential is needed. The Rust `native_fixture` example uses production LocalExecutor/SQLite/Russh with synthetic policy (operate, system protection and script review enabled).

The container needs qemu-system-aarch64, QEMU_EFI, cloud-localds, SSH client, and a verified Debian cloud image copied to `/vm/base.qcow2`. `start_vm.py` creates a fresh overlay, cloud-init seed and fixture-only SSH identity. Root and fixture use the synthetic password `native-fixture-only`; those credentials are never read from application storage. Wait for cloud-init completion, SSH readiness, `getfattr` and the user manager before testing. The QEMU guest has its own kernel, systemd, D-Bus and user manager. Keep the source cloud image immutable.

Build `cargo build --locked -p vigilshell-application --example native_fixture`, then run:

```sh
python3 tests/fixtures/native-services/run.py lifecycle --container <fixture-container>
python3 tests/fixtures/native-services/run.py lifecycle --user fixture --container <fixture-container>
python3 tests/fixtures/native-services/run.py settings --container <fixture-container>
python3 tests/fixtures/native-services/run.py edges --container <fixture-container>
python3 tests/fixtures/native-services/run.py faults --container <fixture-container>
python3 tests/fixtures/native-services/run.py permission --user fixture --container <fixture-container>
python3 tests/fixtures/native-services/run.py disconnect --container <fixture-container>
python3 tests/fixtures/native-services/power.py --container <fixture-container> --output <ignored-result-file>
```

`run.py` uploads only business test scripts; operations are relayed to the local production adapter. It never uploads a VigilShell runtime. The system/user lifecycle validates persisted journal output, definition hashes, backups, and absence of implicit restart/stop. `--resume-unit vigil-native-<8-hex>.service` resumes the definition tail after an initial phase passed; do not replay successful mutations just to rerun a failed assertion.

`faults` temporarily replaces the guest systemctl executable to inject failed readback and daemon-reload, restores it in finally, then cancels a slow native client while independently observing its systemd job. Do not run other service cases concurrently with this injection. `disconnect` closes the helper SSH connection after activation starts. `permission` uses a genuinely unprivileged SSH account. None of these failures grants elevation or causes automatic replay.

Power testing first asserts `systemd-detect-virt --vm == qemu`, uses the labelled container's loopback forwarding, and independently checks boot ID and QEMU process exit. A guest's accepted power request is not reported as completed business effect. Never point this script at another host or use host shutdown commands.

The `after_delete` scenario provides focused unit update/start/stop/delete and shared package-query regression. A powered-off fixture may be resumed with the same validated QEMU arguments and existing overlay, without regenerating credentials, solely for that regression. `native_file_versions` is an ignored Rust integration test enabled only with `VIGILSHELL_TEST_NATIVE_PORT`; it verifies that a stale maintenance hash cannot remove changed content at the final SFTP boundary.

Record results under ignored `.cache`, retain meaningful evidence, and delete only the exact fixture container created by this run after checking ID/label/mounts. Do not prune existing containers, images, volumes or base images.
