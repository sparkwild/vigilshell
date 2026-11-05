# Disposable native Linux targets

These images provide existing SSH/SFTP, interpreters and native package/service tools. No VigilShell program or private protocol runs in the guest. Test orchestration uses the local Rust example and explicit mount-free loopback containers; never use saved business connections.

`apt.Dockerfile` supplies Ubuntu/APT; `dnf.Dockerfile` requires an explicit DNF4 or DNF5 base. Package and service scenarios are in their respective sibling directories. systemd images mask binfmt units so optional privileged container shutdown cannot unregister Docker VM emulators. QEMU setup and immutable image verification remain with native-services; no host power commands are used.

Use the current SSH/SFTP package, service and script fixtures for business behavior. Retain required test output and guest recovery objects. Cleanup may remove only the exact targets created by the run; never prune existing host resources.
