//! 保护按对象和效果定义；业务数据不会因为位于系统目录下而自动成为核心内容。
use super::*;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub enum ProtectionCategory {
    Core,
    CriticalConfiguration,
    Credential,
    Control,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SystemProtection {
    pub path: String,
    /// Directory 包含全部后代；File 只保护该对象，不能推导为保护整个父目录。
    pub kind: ScopeKind,
    pub category: ProtectionCategory,
    pub maintenance: String,
    pub reason: String,
}
impl SystemProtection {
    pub fn contains(&self, path: &str) -> bool {
        self.path == path || self.kind == ScopeKind::Directory && beneath(&self.path, path)
    }
    pub fn blocks(&self, path: &str, effect: FileEffect, recursive: bool) -> bool {
        (self.contains(path) || recursive && beneath(path, &self.path))
            && effect != FileEffect::Read
    }
}

/// 支持 Linux 的公共基线；执行端追加服务器实际账号入口、凭据和组件私有路径。
pub fn system_protections() -> Vec<SystemProtection> {
    use ProtectionCategory::*;
    let mut rules = Vec::new();
    let mut add = |path: &str, kind, category, maintenance: &str| {
        rules.push(SystemProtection {
            path: path.into(),
            kind,
            category,
            maintenance: maintenance.into(),
            reason: match category {
                Core => "system_core_protected",
                CriticalConfiguration => "controlled_configuration_required",
                Credential | Control => "protected_system_resource",
            }
            .into(),
        });
    };
    for path in [
        "/usr",
        "/var",
        "/var/lib",
        "/var/cache",
        "/var/spool",
        "/etc",
        "/home",
        "/root",
    ] {
        add(path, ScopeKind::File, Core, "preserve_container");
    }
    for path in [
        "/usr/bin",
        "/usr/sbin",
        "/usr/lib",
        "/usr/lib64",
        "/usr/libexec",
        "/usr/share",
        "/bin",
        "/sbin",
        "/lib",
        "/lib64",
        "/boot",
        "/var/lib/dpkg",
        "/var/lib/rpm",
        "/var/lib/dnf",
        "/var/lib/apt",
        "/var/cache/apt",
        "/var/cache/dnf",
    ] {
        add(path, ScopeKind::Directory, Core, "packages");
    }
    for path in [
        "/etc/passwd",
        "/etc/group",
        "/etc/sudoers",
        "/etc/sudoers.d",
        "/etc/pam.d",
        "/etc/security",
        "/etc/login.defs",
        "/etc/polkit-1",
        "/var/lib/polkit-1",
        "/var/lib/AccountsService",
        "/etc/shells",
        "/etc/nsswitch.conf",
    ] {
        add(
            path,
            ScopeKind::Directory,
            CriticalConfiguration,
            "accounts",
        );
    }
    for path in [
        "/etc/systemd",
        "/etc/init.d",
        "/etc/rc.local",
        "/etc/init",
        "/var/lib/systemd",
        "/etc/rc0.d",
        "/etc/rc1.d",
        "/etc/rc2.d",
        "/etc/rc3.d",
        "/etc/rc4.d",
        "/etc/rc5.d",
        "/etc/rc6.d",
        "/etc/rcS.d",
    ] {
        add(
            path,
            ScopeKind::Directory,
            CriticalConfiguration,
            "services",
        );
    }
    for path in [
        "/etc/cron.d",
        "/etc/crontab",
        "/etc/cron.daily",
        "/etc/cron.hourly",
        "/etc/cron.weekly",
        "/etc/cron.monthly",
        "/var/spool/cron",
        "/var/spool/at",
    ] {
        add(
            path,
            ScopeKind::Directory,
            CriticalConfiguration,
            "scheduled_tasks",
        );
    }
    for path in [
        "/etc/profile",
        "/etc/profile.d",
        "/etc/bash.bashrc",
        "/etc/environment",
        "/etc/ld.so.preload",
        "/etc/ld.so.cache",
        "/etc/ld.so.conf",
        "/etc/ld.so.conf.d",
        "/etc/alternatives",
        "/etc/fstab",
        "/etc/crypttab",
        "/etc/default/grub",
        "/etc/grub.d",
        "/etc/modules",
        "/etc/modules-load.d",
        "/etc/modprobe.d",
        "/etc/udev",
        "/etc/dbus-1",
        "/etc/selinux",
        "/etc/apparmor.d",
        "/etc/sysctl.conf",
        "/etc/sysctl.d",
    ] {
        add(
            path,
            ScopeKind::Directory,
            CriticalConfiguration,
            "validated_configuration",
        );
    }
    for path in ["/etc/apt", "/etc/dnf", "/etc/yum.repos.d", "/etc/rpm"] {
        add(
            path,
            ScopeKind::Directory,
            CriticalConfiguration,
            "package_sources",
        );
    }
    for path in [
        "/etc/hostname",
        "/etc/machine-info",
        "/etc/localtime",
        "/etc/timezone",
        "/etc/locale.conf",
        "/etc/default/locale",
    ] {
        add(
            path,
            ScopeKind::Directory,
            CriticalConfiguration,
            "system_settings",
        );
    }
    for path in [
        "/etc/hosts",
        "/etc/resolv.conf",
        "/etc/NetworkManager",
        "/etc/network",
        "/etc/netplan",
        "/etc/sysconfig/network-scripts",
        "/etc/firewalld",
        "/etc/nftables.conf",
        "/etc/iptables",
    ] {
        add(
            path,
            ScopeKind::Directory,
            CriticalConfiguration,
            "network_settings",
        );
    }
    add(
        "/etc/ssh",
        ScopeKind::Directory,
        CriticalConfiguration,
        "login_policy",
    );
    for path in [
        "/etc/ssh/ssh_host_rsa_key",
        "/etc/ssh/ssh_host_ecdsa_key",
        "/etc/ssh/ssh_host_ed25519_key",
        "/etc/ssh/ssh_host_dsa_key",
        "/etc/shadow",
        "/etc/gshadow",
        "/etc/ssl/private",
    ] {
        add(
            path,
            ScopeKind::Directory,
            Credential,
            "public_settings_only",
        );
    }
    for path in ["/proc", "/sys", "/dev", "/run"] {
        add(path, ScopeKind::Directory, Control, "application_control");
    }
    rules
}
