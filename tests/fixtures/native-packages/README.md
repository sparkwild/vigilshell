# Native package SSH fixtures

These tests use the production Rust `LocalExecutor`, SQLite receipts/output storage and Russh SSH/SFTP adapters. The Python scripts run only package builders and assertions inside disposable Linux containers; JSON requests are relayed to a **local test-only** Rust example. No VigilShell executable is deployed to the server. Synthetic policy has system protection and standalone-script review enabled. Public MCP/Dev integration is not exercised.

Build the bridge with `cargo build --locked -p vigilshell-application --example native_fixture`. `run.py <apt|dnf4|dnf5> <scenario> --container <name>` accepts only containers labeled `vigilshell.acceptance=native-packages`, without mounts, whose SSH port is published on `127.0.0.1`. It does not create or restart Docker/Dev, read saved connections, or access host credentials. The container needs password SSH for synthetic `root` and `fixture` accounts (`native-fixture-only`), SFTP, attr, Python3, GPG and its package toolchain. The shared helper accepts named native operations; package scenarios submit package operations only; source checks use the same task path as source maintenance, with no private source-picker branch.

The first argument is the expected backend, not a selector. Before copying fixtures or starting the guest Python module, the runner submits a read-only `packages_list` through the same Rust bridge and checks its completed task/report and selected `backend`. Missing, failed, unknown or mismatched results stop the run and close the bridge before guest setup. This uses production selection, including `dnf` aliases to DNF5 and environments containing multiple managers; the presence of a manager alone is insufficient. The extra probe task belongs only to the bridge's temporary SQLite database. `query_without_manager` starts with a valid backend, then deliberately removes the installer inside the scenario and continues to expect `backend=null`; subsequent replies are relayed unchanged.

Task deadlines are passed when constructing `Operation(operation, timeout=8)`. `finish(success=True)` only asserts the already returned native report; it has no separate waiting timeout. The Rust bridge retains its default task timeout, cancellation/disconnection options and independent 240-second settlement guard. Script approval tests can still inject a wrong content hash. The unused `sourceVersion` approval override is removed; the production snapshot field, historical reads and current `sourceVersion=null` assertion remain.

APT fixtures need dpkg-dev/apt-utils. DNF fixtures need rpm-build/rpm-sign/createrepo_c, and must separately use actual DNF4 and DNF5 environments. Do not add host mounts or use production hosts. Each scenario may modify repository files, trust keys or installed packages inside its disposable container. `faults` temporarily replaces the native query executable to simulate readback failure; always discard these containers after acceptance.

Scenarios:

- `environment_probe`: real SSH/SFTP account and manager probing with a temporary fixture systemctl; checks system/user environments, unsafe user directory rejection and failed-query handling. Restores the executable in finally. This is not a real systemd lifecycle test.

- `native_apt_dependencies`: local multi-package dependency, upgrade, trigger, changed conffile and removal preservation.
- `native_sources`: source lifecycle/key rotation/conflict; on APT also signed repository install and tampered index rejection.
- `apt_repository_upgrade`: follows `native_sources` in the same APT container; requested-version upgrade and removal.
- `native_rpm_repository`: signed local lifecycle, dependencies/scripts/triggers, repository versions and unsigned rejection. Fresh container required because its package build paths are fixed. It does not exercise service management.
- `source_contracts`: unsupported format, conservative update, source-query task/receipt identity, independent public-key fingerprint and configuration checks, and retained keys.
- `faults`: actual native lock, unavailable result query and cancellation after an install script starts; no replay.
- `permission`: uses the synthetic unprivileged `fixture` account; installation must fail without elevation.
- `disconnect`: closes the SSH connection after an install script starts; result remains unknown and the script runs once.
- `untrusted_key`: DNF4/5 signed local package with an unknown key; neither execution nor implicit import is allowed.
- `smoke`: query/install/remove after deletion of the old executor.
- `query_without_manager`: installed database queries remain usable without an installer; unavailable backends are not advertised.

The bridge submits each request twice with the same key to verify receipt reuse. Test packages are unique except the documented shared scenarios. Verification output belongs in the ignored `.cache`, not fixture source or Dev data. Cleanup must identify only the exact containers created for the run; never prune other containers, images or volumes.

Request/response and runner-order regressions use only the Python standard library:

```sh
python3 -B -m unittest discover -s tests/development -p 'test_native_fixtures.py' -v
```

These tests mock the bridge/process interfaces and do not use Docker or application data. They are included in Backend CI. Real SSH backend matching, mismatch-before-setup, package smoke, missing-installer and script cases remain separate integration evidence. Package and service fixture success rules remain distinct: services can acknowledge `requestAccepted`, while packages require a completed zero exit status.
