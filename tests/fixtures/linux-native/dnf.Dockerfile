ARG BASE_IMAGE
FROM ${BASE_IMAGE}
RUN if rpm -q coreutils-single >/dev/null 2>&1; then dnf -y swap coreutils-single coreutils; fi
RUN if command -v dnf5 >/dev/null; then dnf5 -y install openssh-server python3 nodejs bash coreutils procps-ng attr findutils rpm-build rpm-sign gnupg2 createrepo_c systemd dbus; else dnf -y install openssh-server python3 nodejs bash coreutils procps-ng attr findutils rpm-build rpm-sign gnupg2 createrepo_c systemd dbus; fi
RUN useradd -m -u 1000 fixture && echo 'root:native-fixture-only' | chpasswd && echo 'fixture:native-fixture-only' | chpasswd && mkdir -p /run/sshd /srv/fixture && chown fixture:fixture /srv/fixture
RUN systemctl mask systemd-binfmt.service proc-sys-fs-binfmt_misc.mount proc-sys-fs-binfmt_misc.automount
STOPSIGNAL SIGRTMIN+3
CMD ["/sbin/init"]
