FROM ubuntu:24.04
RUN apt-get update && DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends openssh-server python3 nodejs bash coreutils procps attr findutils systemd systemd-sysv dbus dbus-user-session gnupg dpkg-dev apt-utils && rm -rf /var/lib/apt/lists/*
RUN useradd -m -u 1000 fixture && echo 'root:native-fixture-only' | chpasswd && echo 'fixture:native-fixture-only' | chpasswd && mkdir -p /run/sshd /srv/fixture && chown fixture:fixture /srv/fixture
# Never unregister Docker VM emulators when an optional systemd fixture shuts down.
RUN systemctl mask systemd-binfmt.service proc-sys-fs-binfmt_misc.mount proc-sys-fs-binfmt_misc.automount
STOPSIGNAL SIGRTMIN+3
CMD ["/sbin/init"]
