mod editing;
mod transfer_archive;
mod transfer_tree;
use super::*;
use russh_sftp::{
    client::{RawSftpSession, error::Error as SftpError},
    protocol::{OpenFlags, Packet, StatusCode},
};
use std::sync::atomic::Ordering;
use tokio::sync::OnceCell;

use vigilshell_application_contracts::ports::{
    AgentDownload, FileExecutionControl, FutureResult, ProxyRoute, SecretMaterial, ServerTools,
    TransferIo,
};
use vigilshell_domain::tools::*;

pub(super) struct Tools {
    handle: client::Handle<Handler>,
    _guard: SocketGuard,
    sftp: OnceCell<crate::sftp::Session>,
}
pub(super) async fn connect(
    input: ConnectionInput,
    key: HostKey,
    secret: SecretMaterial,
    route: Option<ProxyRoute>,
) -> Result<Arc<dyn ServerTools>, WorkspaceError> {
    let (handle, guard) = authenticated(input, key, secret, route, None, None).await?;
    Ok(Arc::new(Tools {
        handle,
        _guard: guard,
        sftp: OnceCell::new(),
    }))
}
use crate::sftp::sftp_error;
fn local_error(e: std::io::Error) -> WorkspaceError {
    match e.kind() {
        std::io::ErrorKind::AlreadyExists => WorkspaceError::FilesExists,
        std::io::ErrorKind::PermissionDenied => WorkspaceError::FilesPermissionDenied,
        _ => WorkspaceError::StorageUnavailable,
    }
}
impl Tools {
    async fn sftp(&self) -> Result<&crate::sftp::Session, WorkspaceError> {
        self.sftp
            .get_or_try_init(|| crate::sftp::open(&self.handle))
            .await
    }
    async fn exec(&self, command: &str) -> Result<String, WorkspaceError> {
        shared_exec(&self.handle, command).await
    }
}
pub(crate) async fn shared_exec(
    handle: &client::Handle<Handler>,
    command: &str,
) -> Result<String, WorkspaceError> {
    let mut channel = deadline(handle.channel_open_session(), 10).await?;
    let work = async {
        channel.exec(true, command).await.map_err(error)?;
        let mut bytes = Vec::new();
        while let Some(event) = channel.wait().await {
            match event {
                ChannelMsg::Data { data } => {
                    if bytes.len() + data.len() > 512 * 1024 {
                        return Err(WorkspaceError::FilesTooLarge);
                    }
                    bytes.extend_from_slice(&data);
                }
                ChannelMsg::Failure => return Err(WorkspaceError::ToolsUnavailable),
                ChannelMsg::Eof | ChannelMsg::Close => break,
                _ => {}
            }
        }
        String::from_utf8(bytes).map_err(|_| WorkspaceError::ToolsUnavailable)
    };
    let result = tokio::time::timeout(Duration::from_secs(5), work)
        .await
        .map_err(|_| WorkspaceError::Timeout)
        .and_then(|r| r);
    let _ = channel.close().await;
    result
}

pub(crate) const QUICK: &str = "LC_ALL=C; printf '__cpu__\\n'; head -n 1 /proc/stat 2>/dev/null; printf '__memory__\\n'; cat /proc/meminfo 2>/dev/null; printf '__uptime__\\n'; cat /proc/uptime 2>/dev/null; printf '__load__\\n'; cat /proc/loadavg 2>/dev/null; printf '__network_time__\\n'; cat /proc/uptime 2>/dev/null; printf '__network__\\n'; cat /proc/net/dev 2>/dev/null";
pub(crate) const FULL: &str = "LC_ALL=C; export LC_ALL; printf '__hostname__\\n'; hostname 2>/dev/null; printf '__kernel__\\n'; uname -r 2>/dev/null; printf '__architecture__\\n'; uname -m 2>/dev/null; printf '__distribution__\\n'; cat /etc/os-release 2>/dev/null; printf '__cpu__\\n'; head -n 1 /proc/stat 2>/dev/null; printf '__memory__\\n'; cat /proc/meminfo 2>/dev/null; printf '__uptime__\\n'; cat /proc/uptime 2>/dev/null; printf '__load__\\n'; cat /proc/loadavg 2>/dev/null; printf '__network_time__\\n'; cat /proc/uptime 2>/dev/null; printf '__network__\\n'; cat /proc/net/dev 2>/dev/null; printf '__disks__\\n'; df -Pk 2>/dev/null";
pub(crate) fn parse_sample(text: &str) -> RawSample {
    let mut result = RawSample::default();
    let mut section = "";
    for line in text.lines() {
        if line.starts_with("__") && line.ends_with("__") {
            section = line;
            continue;
        }
        let value = line.trim();
        match section {
            "__hostname__" if !value.is_empty() => {
                result.view.hostname = Some(value.chars().take(256).collect())
            }
            "__kernel__" if !value.is_empty() => {
                result.view.kernel = Some(value.chars().take(256).collect())
            }
            "__architecture__" if !value.is_empty() => {
                result.view.architecture = Some(value.chars().take(64).collect())
            }
            "__distribution__" => {
                if let Some(name) = value.strip_prefix("PRETTY_NAME=") {
                    result.view.distribution =
                        Some(name.trim_matches('"').chars().take(256).collect());
                }
            }
            "__uptime__" => {
                result.view.uptime = value
                    .split_whitespace()
                    .next()
                    .and_then(|v| v.parse::<f64>().ok())
                    .filter(|n| n.is_finite() && *n >= 0.0)
            }
            "__cpu__" => {
                let values: Option<Vec<u64>> = value.strip_prefix("cpu ").and_then(|v| {
                    v.split_whitespace()
                        .take(8)
                        .map(str::parse)
                        .collect::<Result<Vec<_>, _>>()
                        .ok()
                });
                if let Some(values) = values.filter(|v| v.len() >= 4) {
                    result.cpu_total = values.iter().try_fold(0u64, |a, b| a.checked_add(*b));
                    result.cpu_idle = values[3].checked_add(values.get(4).copied().unwrap_or(0));
                }
            }
            "__network_time__" => {
                result.network_time = value
                    .split_whitespace()
                    .next()
                    .and_then(|v| v.parse::<f64>().ok())
                    .filter(|v| v.is_finite() && *v >= 0.0);
            }
            "__network__" => {
                if let Some((name, counters)) = value.split_once(':') {
                    let fields: Vec<_> = counters.split_whitespace().collect();
                    if fields.len() >= 16
                        && let (Ok(received), Ok(sent)) =
                            (fields[0].parse::<u64>(), fields[8].parse::<u64>())
                    {
                        let network = result.network.get_or_insert_with(Default::default);
                        if name.trim() != "lo" {
                            network.insert(name.trim().into(), (received, sent));
                        }
                    }
                }
            }
            "__load__" => {
                let values = value
                    .split_whitespace()
                    .take(3)
                    .map(str::parse::<f64>)
                    .collect::<Result<Vec<_>, _>>();
                if let Ok(values) = values
                    && values.len() == 3
                    && values.iter().all(|v| v.is_finite() && *v >= 0.)
                {
                    result.load = Some([values[0], values[1], values[2]]);
                }
            }
            "__memory__" => {
                let mut parts = value.split_whitespace();
                let key = parts.next();
                let bytes = parts
                    .next()
                    .and_then(|v| v.parse::<u64>().ok())
                    .and_then(|n| n.checked_mul(1024));
                match key {
                    Some("MemTotal:") => result.view.memory_total = bytes,
                    Some("MemAvailable:") => result.view.memory_available = bytes,
                    Some("MemFree:") => result.memory_free = bytes,
                    Some("Shmem:") => result.memory_shared = bytes,
                    Some("Buffers:") => result.memory_buffers = bytes,
                    Some("SwapTotal:") => result.swap_total = bytes,
                    Some("SwapFree:") => result.swap_free = bytes,
                    _ => {}
                }
            }
            "__disks__" => {
                let parts: Vec<_> = value.split_whitespace().collect();
                if parts.len() >= 6
                    && ![
                        "Filesystem",
                        "tmpfs",
                        "devtmpfs",
                        "proc",
                        "sysfs",
                        "cgroup",
                        "cgroup2",
                    ]
                    .contains(&parts[0])
                    && let (Ok(total), Ok(available)) =
                        (parts[1].parse::<u64>(), parts[3].parse::<u64>())
                    && let (Some(total), Some(available)) =
                        (total.checked_mul(1024), available.checked_mul(1024))
                    && total > 0
                {
                    result.view.disks.push(DiskUsage {
                        mount: parts[5..].join(" "),
                        total,
                        available,
                    });
                }
            }
            _ => {}
        }
    }
    result
}
impl ServerTools for Tools {
    fn identities(&self, uids: Vec<u32>, gids: Vec<u32>) -> FutureResult<'_, Vec<IdentityName>> {
        Box::pin(async move {
            if uids.len() + gids.len() > 64 {
                return Err(WorkspaceError::InvalidInput);
            }
            let mut command = String::from("LC_ALL=C; export LC_ALL; ");
            for (group, ids) in [(false, uids), (true, gids)] {
                for id in ids {
                    let database = if group { "group" } else { "passwd" };
                    let prefix = if group { "g" } else { "u" };
                    command.push_str(&format!("getent {database} {id} 2>/dev/null | awk -F: '{{print \"{prefix}:\" $3 \":\" $1}}'; "));
                }
            }
            let text = self.exec(&command).await?;
            Ok(text
                .lines()
                .filter_map(|line| {
                    let mut fields = line.splitn(3, ':');
                    let group = match fields.next()? {
                        "g" => true,
                        "u" => false,
                        _ => return None,
                    };
                    let id = fields.next()?.parse().ok()?;
                    let name = fields.next()?;
                    if name.is_empty() || name.len() > 256 {
                        return None;
                    }
                    Some(IdentityName {
                        id,
                        group,
                        name: name.into(),
                    })
                })
                .take(64)
                .collect())
        })
    }
    fn sample(&self, full: bool) -> FutureResult<'_, RawSample> {
        Box::pin(async move {
            Ok(parse_sample(
                &self.exec(if full { FULL } else { QUICK }).await?,
            ))
        })
    }
    fn list(
        &self,
        path: String,
        cancel: Arc<std::sync::atomic::AtomicBool>,
    ) -> FutureResult<'_, (String, Vec<RemoteEntry>)> {
        Box::pin(async move {
            valid_remote_path(&path)?;
            let (sftp, _) = self.sftp().await?;
            crate::sftp::list(sftp, path, &cancel).await
        })
    }
    fn stat(&self, path: String) -> FutureResult<'_, RemoteEntry> {
        Box::pin(async move {
            valid_remote_path(&path)?;
            let attrs = self
                .sftp()
                .await?
                .0
                .lstat(path.clone())
                .await
                .map_err(sftp_error)?
                .attrs;
            Ok(RemoteEntry {
                name: path
                    .rsplit('/')
                    .next()
                    .filter(|s| !s.is_empty())
                    .unwrap_or("/")
                    .into(),
                path,
                kind: if attrs.is_symlink() {
                    "symlink"
                } else if attrs.is_dir() {
                    "directory"
                } else if attrs.is_regular() {
                    "file"
                } else {
                    "other"
                }
                .into(),
                size: attrs.size,
                modified: attrs.mtime.map(u64::from),
                permissions: attrs.permissions,
                uid: attrs.uid,
                gid: attrs.gid,
            })
        })
    }
    fn create_file(&self, path: String) -> FutureResult<'_, ()> {
        Box::pin(async move {
            valid_remote_path(&path)?;
            let sftp = &self.sftp().await?.0;
            if sftp.lstat(path.clone()).await.is_ok() {
                return Err(WorkspaceError::FilesExists);
            }
            let opened = sftp
                .open(
                    path.clone(),
                    OpenFlags::WRITE | OpenFlags::CREATE | OpenFlags::EXCLUDE,
                    crate::sftp::empty_attrs(),
                )
                .await;
            match opened {
                Ok(handle) => {
                    sftp.close(handle.handle).await.map_err(sftp_error)?;
                    Ok(())
                }
                Err(_) if sftp.lstat(path).await.is_ok() => Err(WorkspaceError::FilesExists),
                Err(e) => Err(sftp_error(e)),
            }
        })
    }
    fn permissions(
        &self,
        path: String,
        set: u32,
        clear: u32,
        expected: RemoteEntry,
    ) -> FutureResult<'_, ()> {
        Box::pin(async move {
            if set > 0o777 || clear > 0o777 || set & clear != 0 {
                return Err(WorkspaceError::InvalidInput);
            }
            let current = self.stat(path.clone()).await?;
            if !["directory", "file"].contains(&current.kind.as_str()) {
                return Err(WorkspaceError::FilesUnsupported);
            }
            if current.kind != expected.kind
                || current.permissions != expected.permissions
                || current.uid != expected.uid
                || current.gid != expected.gid
                || (current.kind == "file"
                    && (current.size != expected.size || current.modified != expected.modified))
            {
                return Err(WorkspaceError::Conflict);
            }
            let mode = (current
                .permissions
                .ok_or(WorkspaceError::FilesUnsupported)?
                | set)
                & !clear;
            self.sftp()
                .await?
                .0
                .setstat(
                    path.clone(),
                    russh_sftp::protocol::FileAttributes {
                        permissions: Some(mode),
                        ..crate::sftp::empty_attrs()
                    },
                )
                .await
                .map_err(sftp_error)?;
            let after = self.stat(path).await?;
            if after.permissions.map(|m| m & 0o7777) != Some(mode & 0o7777)
                || after.uid != current.uid
                || after.gid != current.gid
            {
                return Err(WorkspaceError::Conflict);
            }
            Ok(())
        })
    }
    fn mkdir(&self, path: String) -> FutureResult<'_, ()> {
        Box::pin(async move {
            valid_remote_path(&path)?;
            self.sftp()
                .await?
                .0
                .mkdir(path, crate::sftp::empty_attrs())
                .await
                .map_err(sftp_error)?;
            Ok(())
        })
    }
    fn rename(&self, from: String, to: String) -> FutureResult<'_, ()> {
        Box::pin(async move {
            valid_remote_path(&from)?;
            valid_remote_path(&to)?;
            let sftp = &self.sftp().await?.0;
            if sftp.lstat(to.clone()).await.is_ok() {
                return Err(WorkspaceError::FilesExists);
            }
            sftp.rename(from, to).await.map_err(sftp_error)?;
            Ok(())
        })
    }
    fn remove(&self, path: String, directory: bool) -> FutureResult<'_, ()> {
        Box::pin(async move {
            valid_remote_path(&path)?;
            let sftp = &self.sftp().await?.0;
            let attrs = sftp.lstat(path.clone()).await.map_err(sftp_error)?.attrs;
            if directory {
                if !attrs.is_dir() || attrs.is_symlink() {
                    return Err(WorkspaceError::FilesUnsupported);
                }
                match sftp.rmdir(path).await {
                    Ok(_) => {}
                    Err(SftpError::Status(status)) if status.status_code == StatusCode::Failure => {
                        return Err(WorkspaceError::FilesNotEmpty);
                    }
                    Err(e) => return Err(sftp_error(e)),
                }
            } else {
                if !attrs.is_regular() && !attrs.is_symlink() {
                    return Err(WorkspaceError::FilesUnsupported);
                }
                sftp.remove(path).await.map_err(sftp_error)?;
            }
            Ok(())
        })
    }
    fn transfer_tree(
        &self,
        tree: vigilshell_application_contracts::ports::TransferTreeIo,
    ) -> FutureResult<'_, ()> {
        Box::pin(async move { transfer_tree::run(self, tree).await })
    }
    fn archive(
        &self,
        archive: vigilshell_application_contracts::ports::ArchiveIo,
    ) -> FutureResult<'_, ()> {
        Box::pin(async move { transfer_archive::run(self, archive).await })
    }
    fn transfer(&self, io: TransferIo) -> FutureResult<'_, ()> {
        Box::pin(async move {
            let (sftp, replace) = self.sftp().await?;
            transfer_file(sftp, *replace, io).await
        })
    }
    fn edit_version(&self, path: String) -> FutureResult<'_, RemoteFileVersion> {
        Box::pin(async move { self.editor_version(&path).await })
    }
    fn edit_download(
        &self,
        path: String,
        local: std::path::PathBuf,
    ) -> FutureResult<'_, RemoteFileVersion> {
        Box::pin(async move { self.editor_download(path, local).await })
    }
    fn edit_upload(
        &self,
        path: String,
        local: std::path::PathBuf,
        expected: Option<RemoteFileVersion>,
    ) -> FutureResult<'_, RemoteFileVersion> {
        Box::pin(async move { self.editor_upload(path, local, expected).await })
    }
    fn close(&self) -> FutureResult<'_, ()> {
        Box::pin(async move {
            if let Some((sftp, _)) = self.sftp.get() {
                let _ = sftp.close_session();
            }
            self.handle
                .disconnect(Disconnect::ByApplication, "", "")
                .await
                .map_err(error)
        })
    }
}
async fn transfer_file(
    sftp: &RawSftpSession,
    can_replace: bool,
    io: TransferIo,
) -> Result<(), WorkspaceError> {
    valid_remote_path(&io.remote)?;
    let cancelled = || {
        if io.cancel.load(Ordering::Acquire) {
            Err(WorkspaceError::Cancelled)
        } else {
            Ok(())
        }
    };
    cancelled()?;
    if io.upload {
        let metadata = tokio::fs::symlink_metadata(&io.local)
            .await
            .map_err(local_error)?;
        if !metadata.is_file() {
            return Err(WorkspaceError::FilesUnsupported);
        }
        let exists = match sftp.lstat(io.remote.clone()).await {
            Ok(v) => {
                if !v.attrs.is_regular() {
                    return Err(WorkspaceError::FilesUnsupported);
                }
                true
            }
            Err(SftpError::Status(s)) if s.status_code == StatusCode::NoSuchFile => false,
            Err(e) => return Err(sftp_error(e)),
        };
        if exists && !io.overwrite {
            return Err(WorkspaceError::FilesExists);
        }
        if io.overwrite && !can_replace {
            return Err(WorkspaceError::FilesUnsupported);
        }
        let parent = io.remote.rsplit_once('/').map(|v| v.0).unwrap_or(".");
        let temporary = format!("{parent}/.vigilshell-{}.part", new_id());
        let handle = sftp
            .open(
                temporary.clone(),
                OpenFlags::WRITE | OpenFlags::CREATE | OpenFlags::EXCLUDE,
                crate::sftp::empty_attrs(),
            )
            .await
            .map_err(sftp_error)?
            .handle;
        let result = async {
            let file = tokio::fs::File::open(&io.local)
                .await
                .map_err(local_error)?;
            io.total.store(metadata.len(), Ordering::Release);
            let mut source = crate::sftp_transfer::UploadFile {
                file,
                metadata,
                read: 0,
            };
            let digest = crate::sftp_transfer::upload(
                sftp,
                &handle,
                &mut source,
                &FileExecutionControl {
                    cancel: io.cancel.clone(),
                    bytes: io.progress.clone(),
                    total: io.total.clone(),
                },
            )
            .await?;
            crate::sftp_transfer::verify(
                sftp,
                &temporary,
                digest,
                &FileExecutionControl {
                    cancel: io.cancel.clone(),
                    bytes: io.progress.clone(),
                    total: io.total.clone(),
                },
            )
            .await?;
            cancelled()?;
            Ok(())
        }
        .await;
        let closed = sftp.close(handle).await.map_err(sftp_error);
        let result = match (result, closed) {
            (Ok(()), Ok(_)) => {
                if let Err(e) = cancelled() {
                    Err(e)
                } else if io.overwrite {
                    if let Ok(target) = sftp.lstat(io.remote.clone()).await
                        && !target.attrs.is_regular()
                    {
                        let _ = sftp.remove(temporary.clone()).await;
                        return Err(WorkspaceError::FilesUnsupported);
                    }
                    let mut data = Vec::new();
                    for text in [&temporary, &io.remote] {
                        data.extend_from_slice(&(text.len() as u32).to_be_bytes());
                        data.extend_from_slice(text.as_bytes());
                    }
                    match sftp.extended("posix-rename@openssh.com", data).await {
                        Ok(Packet::Status(s)) if s.status_code == StatusCode::Ok => Ok(()),
                        Err(e) => Err(sftp_error(e)),
                        _ => Err(WorkspaceError::FilesUnsupported),
                    }
                } else {
                    sftp.rename(temporary.clone(), io.remote.clone())
                        .await
                        .map(|_| ())
                        .map_err(sftp_error)
                }
            }
            (Err(e), _) | (_, Err(e)) => Err(e),
        };
        if result.is_err() {
            let _ = sftp.remove(temporary).await;
        }
        result
    } else {
        let attrs = sftp
            .lstat(io.remote.clone())
            .await
            .map_err(sftp_error)?
            .attrs;
        if !attrs.is_regular() {
            return Err(WorkspaceError::FilesUnsupported);
        }
        if let Ok(target) = tokio::fs::symlink_metadata(&io.local).await {
            if !target.is_file() {
                return Err(WorkspaceError::FilesUnsupported);
            }
            if !io.overwrite {
                return Err(WorkspaceError::FilesExists);
            }
        }
        io.total
            .store(attrs.size.unwrap_or(u64::MAX), Ordering::Release);
        let parent = io.local.parent().ok_or(WorkspaceError::InvalidInput)?;
        let temporary = parent.join(format!(".vigilshell-{}.part", new_id()));
        let file = tokio::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .await
            .map_err(local_error)?;
        let mut file = crate::sftp_transfer::DownloadFile(file);
        let handle = match sftp
            .open(
                io.remote.clone(),
                OpenFlags::READ,
                crate::sftp::empty_attrs(),
            )
            .await
        {
            Ok(handle) => handle.handle,
            Err(e) => {
                let _ = tokio::fs::remove_file(&temporary).await;
                return Err(sftp_error(e));
            }
        };
        let result = async {
            let (size, hash) = crate::sftp_transfer::download(
                sftp,
                &handle,
                &mut file,
                &FileExecutionControl {
                    cancel: io.cancel.clone(),
                    bytes: io.progress.clone(),
                    total: io.total.clone(),
                },
            )
            .await?;
            let after = sftp.fstat(handle.clone()).await.map_err(sftp_error)?.attrs;
            if attrs.size != Some(size) || after.size != attrs.size || after.mtime != attrs.mtime {
                return Err(WorkspaceError::Conflict);
            }
            crate::sftp_transfer::verify(
                sftp,
                &io.remote,
                (size, hash),
                &FileExecutionControl {
                    cancel: io.cancel.clone(),
                    bytes: io.progress.clone(),
                    total: io.total.clone(),
                },
            )
            .await?;
            file.finish().await?;
            cancelled()?;
            if io.overwrite {
                if let Ok(target) = tokio::fs::symlink_metadata(&io.local).await
                    && !target.is_file()
                {
                    return Err(WorkspaceError::FilesUnsupported);
                }
                tokio::fs::rename(&temporary, &io.local)
                    .await
                    .map_err(local_error)?;
            } else {
                tokio::fs::hard_link(&temporary, &io.local)
                    .await
                    .map_err(local_error)?;
            }
            Ok(())
        }
        .await;
        let _ = sftp.close(handle).await;
        drop(file);
        let _ = tokio::fs::remove_file(&temporary).await;
        result
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_partial_metrics_without_fabricating_missing_values() {
        let sample = parse_sample(
            "__cpu__\ncpu 10 0 20 70 5 0 0 0 20 0\n__memory__\nMemTotal: 1024 kB\nMemAvailable: 256 kB\n__uptime__\n10.2 20\n__disks__\n/dev/root 100 30 70 30% /\ntmpfs 100 0 100 0% /run\n",
        );
        assert_eq!(sample.cpu_total, Some(105));
        assert_eq!(sample.cpu_idle, Some(75));
        assert_eq!(sample.view.memory_total, Some(1048576));
        assert_eq!(sample.view.disks.len(), 1);
        assert!(sample.view.hostname.is_none());
        assert!(parse_sample("bad input").cpu_total.is_none());
    }
}

#[cfg(test)]
mod sftp_tests {
    use super::*;
    use russh_sftp::protocol::{Attrs, Data, FileAttributes, Handle};
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicBool, AtomicU64};
    struct MemoryServer {
        files: Arc<Mutex<HashMap<String, Vec<u8>>>>,
        cancel_write: Option<Arc<AtomicBool>>,
        read_directories: std::collections::HashSet<String>,
    }
    fn ok(id: u32) -> russh_sftp::protocol::Status {
        russh_sftp::protocol::Status {
            id,
            status_code: StatusCode::Ok,
            error_message: String::new(),
            language_tag: String::new(),
        }
    }
    impl russh_sftp::server::Handler for MemoryServer {
        type Error = StatusCode;
        fn unimplemented(&self) -> Self::Error {
            StatusCode::OpUnsupported
        }
        async fn fstat(&mut self, id: u32, handle: String) -> Result<Attrs, Self::Error> {
            self.lstat(id, handle).await
        }
        async fn lstat(&mut self, id: u32, path: String) -> Result<Attrs, Self::Error> {
            if path == "/denied" {
                return Err(StatusCode::PermissionDenied);
            }
            let files = self.files.lock().unwrap();
            if path == "/" || files.contains_key(&(path.clone() + "/")) {
                return Ok(Attrs {
                    id,
                    attrs: FileAttributes {
                        permissions: Some(0o040700),
                        size: Some(0),
                        ..crate::sftp::empty_attrs()
                    },
                });
            }
            let data = files.get(&path).ok_or(StatusCode::NoSuchFile)?;
            Ok(Attrs {
                id,
                attrs: FileAttributes {
                    size: Some(data.len() as u64),
                    permissions: Some(0o100600),
                    ..Default::default()
                },
            })
        }
        async fn mkdir(
            &mut self,
            id: u32,
            path: String,
            _: FileAttributes,
        ) -> Result<russh_sftp::protocol::Status, Self::Error> {
            let mut files = self.files.lock().unwrap();
            if files.contains_key(&path) || files.contains_key(&(path.clone() + "/")) {
                return Err(StatusCode::Failure);
            }
            files.insert(path + "/", vec![]);
            Ok(ok(id))
        }
        async fn realpath(
            &mut self,
            id: u32,
            path: String,
        ) -> Result<russh_sftp::protocol::Name, Self::Error> {
            Ok(russh_sftp::protocol::Name {
                id,
                files: vec![russh_sftp::protocol::File {
                    filename: path,
                    longname: String::new(),
                    attrs: crate::sftp::empty_attrs(),
                }],
            })
        }
        async fn opendir(&mut self, id: u32, path: String) -> Result<Handle, Self::Error> {
            self.read_directories.remove(&path);
            Ok(Handle { id, handle: path })
        }
        async fn readdir(
            &mut self,
            id: u32,
            path: String,
        ) -> Result<russh_sftp::protocol::Name, Self::Error> {
            if !self.read_directories.insert(path.clone()) {
                return Err(StatusCode::Eof);
            }
            let prefix = format!("{}/", path.trim_end_matches('/'));
            let files = self.files.lock().unwrap();
            let entries = files
                .iter()
                .filter_map(|(name, data)| {
                    let name = name.strip_prefix(&prefix)?.trim_end_matches('/');
                    if name.is_empty() || name.contains('/') {
                        return None;
                    }
                    let directory = files.contains_key(&format!("{prefix}{name}/"));
                    Some(russh_sftp::protocol::File {
                        filename: name.into(),
                        longname: String::new(),
                        attrs: FileAttributes {
                            permissions: Some(if directory { 0o040700 } else { 0o100600 }),
                            size: Some(data.len() as u64),
                            ..crate::sftp::empty_attrs()
                        },
                    })
                })
                .collect();
            Ok(russh_sftp::protocol::Name { id, files: entries })
        }
        async fn open(
            &mut self,
            id: u32,
            filename: String,
            flags: OpenFlags,
            _: FileAttributes,
        ) -> Result<Handle, Self::Error> {
            let mut files = self.files.lock().unwrap();
            if flags.contains(OpenFlags::CREATE) {
                if files.contains_key(&filename) && flags.contains(OpenFlags::EXCLUDE) {
                    return Err(StatusCode::Failure);
                }
                files.insert(filename.clone(), Vec::new());
            }
            if !files.contains_key(&filename) {
                return Err(StatusCode::NoSuchFile);
            }
            Ok(Handle {
                id,
                handle: filename,
            })
        }
        async fn close(
            &mut self,
            id: u32,
            _: String,
        ) -> Result<russh_sftp::protocol::Status, Self::Error> {
            Ok(ok(id))
        }
        async fn read(
            &mut self,
            id: u32,
            handle: String,
            offset: u64,
            len: u32,
        ) -> Result<Data, Self::Error> {
            let files = self.files.lock().unwrap();
            let data = files.get(&handle).ok_or(StatusCode::NoSuchFile)?;
            if offset as usize >= data.len() {
                return Err(StatusCode::Eof);
            }
            Ok(Data {
                id,
                data: data[offset as usize..data.len().min(offset as usize + len as usize)]
                    .to_vec(),
            })
        }
        async fn write(
            &mut self,
            id: u32,
            handle: String,
            offset: u64,
            data: Vec<u8>,
        ) -> Result<russh_sftp::protocol::Status, Self::Error> {
            let mut files = self.files.lock().unwrap();
            let file = files.get_mut(&handle).ok_or(StatusCode::NoSuchFile)?;
            file.resize(file.len().max(offset as usize + data.len()), 0);
            file[offset as usize..offset as usize + data.len()].copy_from_slice(&data);
            if let Some(cancel) = &self.cancel_write {
                cancel.store(true, Ordering::Release);
            }
            Ok(ok(id))
        }
        async fn rename(
            &mut self,
            id: u32,
            from: String,
            to: String,
        ) -> Result<russh_sftp::protocol::Status, Self::Error> {
            let mut files = self.files.lock().unwrap();
            if files.contains_key(&to) {
                return Err(StatusCode::Failure);
            }
            let data = files.remove(&from).ok_or(StatusCode::NoSuchFile)?;
            files.insert(to, data);
            Ok(ok(id))
        }
        async fn remove(
            &mut self,
            id: u32,
            path: String,
        ) -> Result<russh_sftp::protocol::Status, Self::Error> {
            self.files
                .lock()
                .unwrap()
                .remove(&path)
                .ok_or(StatusCode::NoSuchFile)?;
            Ok(ok(id))
        }
        async fn extended(
            &mut self,
            id: u32,
            request: String,
            data: Vec<u8>,
        ) -> Result<Packet, Self::Error> {
            if request != "posix-rename@openssh.com" {
                return Err(StatusCode::OpUnsupported);
            }
            let mut data = data.as_slice();
            let mut read = || -> Result<String, StatusCode> {
                let len = u32::from_be_bytes(
                    data.get(..4)
                        .ok_or(StatusCode::BadMessage)?
                        .try_into()
                        .unwrap(),
                ) as usize;
                let value =
                    std::str::from_utf8(data.get(4..4 + len).ok_or(StatusCode::BadMessage)?)
                        .map_err(|_| StatusCode::BadMessage)?
                        .to_owned();
                data = &data[4 + len..];
                Ok(value)
            };
            let from = read()?;
            let to = read()?;
            let mut files = self.files.lock().unwrap();
            let bytes = files.remove(&from).ok_or(StatusCode::NoSuchFile)?;
            files.insert(to, bytes);
            Ok(Packet::Status(ok(id)))
        }
    }
    fn io(
        local: std::path::PathBuf,
        remote: &str,
        upload: bool,
        overwrite: bool,
        cancel: Arc<AtomicBool>,
    ) -> TransferIo {
        TransferIo {
            local,
            remote: remote.into(),
            upload,
            overwrite,
            cancel,
            progress: Arc::new(AtomicU64::new(0)),
            total: Arc::new(AtomicU64::new(0)),
        }
    }
    #[tokio::test]
    async fn real_sftp_packets_transfer_unicode_files_and_preserve_existing_targets() {
        let files = Arc::new(Mutex::new(HashMap::from([(
            "/existing".into(),
            b"old".to_vec(),
        )])));
        let (client, server) = tokio::io::duplex(65536);
        let task = tokio::spawn(russh_sftp::server::run(
            server,
            MemoryServer {
                files: files.clone(),
                cancel_write: None,
                read_directories: Default::default(),
            },
        ));
        let sftp = RawSftpSession::new(client);
        sftp.init().await.unwrap();
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("文件 空格.txt");
        std::fs::write(&source, b"synthetic file data").unwrap();
        let cancel = Arc::new(AtomicBool::new(false));
        assert_eq!(
            transfer_file(
                &sftp,
                true,
                io(source.clone(), "/existing", true, false, cancel.clone())
            )
            .await,
            Err(WorkspaceError::FilesExists)
        );
        assert_eq!(files.lock().unwrap()["/existing"], b"old");
        assert_eq!(
            transfer_file(
                &sftp,
                false,
                io(source.clone(), "/existing", true, true, cancel.clone())
            )
            .await,
            Err(WorkspaceError::FilesUnsupported)
        );
        transfer_file(
            &sftp,
            true,
            io(source.clone(), "/existing", true, true, cancel.clone()),
        )
        .await
        .unwrap();
        assert_eq!(files.lock().unwrap()["/existing"], b"synthetic file data");
        transfer_file(
            &sftp,
            true,
            io(source, "/目录/文件 空格.txt", true, false, cancel.clone()),
        )
        .await
        .unwrap();
        let destination = root.path().join("download.txt");
        transfer_file(
            &sftp,
            true,
            io(
                destination.clone(),
                "/目录/文件 空格.txt",
                false,
                false,
                cancel.clone(),
            ),
        )
        .await
        .unwrap();
        assert_eq!(std::fs::read(&destination).unwrap(), b"synthetic file data");
        assert_eq!(
            transfer_file(
                &sftp,
                true,
                io(
                    destination.clone(),
                    "/existing",
                    false,
                    false,
                    cancel.clone()
                )
            )
            .await,
            Err(WorkspaceError::FilesExists)
        );
        assert_eq!(
            transfer_file(&sftp, true, io(destination, "/denied", false, true, cancel)).await,
            Err(WorkspaceError::FilesPermissionDenied)
        );
        assert!(
            !files
                .lock()
                .unwrap()
                .keys()
                .any(|key| key.ends_with(".part"))
        );
        let _ = sftp.close_session();
        task.await.unwrap();
    }
    #[tokio::test]
    async fn cancelling_upload_removes_only_its_temporary_file() {
        let cancel = Arc::new(AtomicBool::new(false));
        let files = Arc::new(Mutex::new(HashMap::from([(
            "/existing".into(),
            b"old".to_vec(),
        )])));
        let (client, server) = tokio::io::duplex(65536);
        let task = tokio::spawn(russh_sftp::server::run(
            server,
            MemoryServer {
                files: files.clone(),
                cancel_write: Some(cancel.clone()),
                read_directories: Default::default(),
            },
        ));
        let sftp = RawSftpSession::new(client);
        sftp.init().await.unwrap();
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("source");
        std::fs::write(&source, vec![1; 100000]).unwrap();
        assert_eq!(
            transfer_file(&sftp, true, io(source, "/existing", true, true, cancel)).await,
            Err(WorkspaceError::Cancelled)
        );
        assert_eq!(
            *files.lock().unwrap(),
            HashMap::from([("/existing".into(), b"old".to_vec())])
        );
        let _ = sftp.close_session();
        task.await.unwrap();
    }
    #[tokio::test]
    async fn directory_upload_skips_links_preserves_empty_dirs_and_resumes_one_conflict() {
        use vigilshell_application_contracts::ports::{TransferTreeIo, TransferTreeState};
        let files = Arc::new(Mutex::new(HashMap::from([
            ("/target/".into(), vec![]),
            ("/target/file".into(), b"existing".to_vec()),
        ])));
        let (client, server) = tokio::io::duplex(65536);
        let task = tokio::spawn(russh_sftp::server::run(
            server,
            MemoryServer {
                files: files.clone(),
                cancel_write: None,
                read_directories: Default::default(),
            },
        ));
        let sftp = Arc::new(RawSftpSession::new(client));
        sftp.init().await.unwrap();
        let root = tempfile::tempdir().unwrap();
        let source = root.path().canonicalize().unwrap().join("source");
        std::fs::create_dir(&source).unwrap();
        std::fs::create_dir(source.join("empty")).unwrap();
        std::fs::write(source.join("file"), b"new").unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink("file", source.join("link")).unwrap();
        let state = Arc::new(Mutex::new(TransferTreeState::default()));
        let policy = Arc::new(Mutex::new(None));
        let run = || TransferTreeIo {
            io: io(
                source.clone(),
                "/target",
                true,
                false,
                Arc::new(AtomicBool::new(false)),
            ),
            state: state.clone(),
            batch_policy: policy.clone(),
        };
        assert_eq!(
            transfer_tree::run_session(&sftp, true, run()).await,
            Err(WorkspaceError::FilesExists)
        );
        assert_eq!(files.lock().unwrap()["/target/file"], b"existing");
        state.lock().unwrap().conflict_decision = Some(false);
        transfer_tree::run_session(&sftp, true, run())
            .await
            .unwrap();
        assert!(files.lock().unwrap().contains_key("/target/empty/"));
        assert_eq!(files.lock().unwrap()["/target/file"], b"existing");
        assert!(
            state
                .lock()
                .unwrap()
                .entries
                .iter()
                .any(|e| e.remote == "/target/file" && e.phase == "skipped")
        );
        #[cfg(unix)]
        assert!(
            state
                .lock()
                .unwrap()
                .entries
                .iter()
                .any(|e| e.remote == "/target/link" && e.phase == "skipped")
        );
        let destination = root.path().canonicalize().unwrap().join("download");
        transfer_tree::run_session(
            &sftp,
            true,
            TransferTreeIo {
                io: io(
                    destination.clone(),
                    "/target",
                    false,
                    false,
                    Arc::new(AtomicBool::new(false)),
                ),
                state: Arc::new(Mutex::new(TransferTreeState::default())),
                batch_policy: policy,
            },
        )
        .await
        .unwrap();
        assert_eq!(
            std::fs::read(destination.join("file")).unwrap(),
            b"existing"
        );
        assert!(destination.join("empty").is_dir());
        let _ = sftp.close_session();
        task.await.unwrap();
    }

    #[tokio::test]
    async fn directory_failure_continues_and_retry_does_not_replay_success() {
        use vigilshell_application_contracts::ports::{TransferTreeIo, TransferTreeState};
        let files = Arc::new(Mutex::new(HashMap::new()));
        let (client, server) = tokio::io::duplex(65536);
        let task = tokio::spawn(russh_sftp::server::run(
            server,
            MemoryServer {
                files: files.clone(),
                cancel_write: None,
                read_directories: Default::default(),
            },
        ));
        let sftp = Arc::new(RawSftpSession::new(client));
        sftp.init().await.unwrap();
        let root = tempfile::tempdir().unwrap();
        let source = root.path().canonicalize().unwrap();
        std::fs::write(source.join("good"), b"new").unwrap();
        std::fs::write(source.join("denied"), b"private").unwrap();
        let state = Arc::new(Mutex::new(TransferTreeState::default()));
        let run = || TransferTreeIo {
            io: io(
                source.clone(),
                "/",
                true,
                false,
                Arc::new(AtomicBool::new(false)),
            ),
            state: state.clone(),
            batch_policy: Arc::new(Mutex::new(None)),
        };
        assert_eq!(
            transfer_tree::run_session(&sftp, true, run()).await,
            Err(WorkspaceError::ToolsUnavailable)
        );
        assert_eq!(files.lock().unwrap()["/good"], b"new");
        assert!(
            state
                .lock()
                .unwrap()
                .entries
                .iter()
                .any(|e| e.remote == "/denied" && e.phase == "failed")
        );
        files
            .lock()
            .unwrap()
            .insert("/good".into(), b"independent update".to_vec());
        assert_eq!(
            transfer_tree::run_session(&sftp, true, run()).await,
            Err(WorkspaceError::ToolsUnavailable)
        );
        assert_eq!(files.lock().unwrap()["/good"], b"independent update");
        let cancelled = Arc::new(AtomicBool::new(true));
        let untouched = Arc::new(Mutex::new(TransferTreeState::default()));
        assert_eq!(
            transfer_tree::run_session(
                &sftp,
                true,
                TransferTreeIo {
                    io: io(source, "/never", true, false, cancelled),
                    state: untouched.clone(),
                    batch_policy: Arc::new(Mutex::new(None))
                }
            )
            .await,
            Err(WorkspaceError::Cancelled)
        );
        assert!(untouched.lock().unwrap().entries.is_empty());
        assert!(!files.lock().unwrap().contains_key("/never/"));
        let _ = sftp.close_session();
        task.await.unwrap();
    }
}

#[cfg(test)]
mod network_sample_tests {
    use super::*;
    #[test]
    fn parses_receive_and_send_columns_and_excludes_loopback() {
        let raw = parse_sample(
            "__network_time__\n100.25 20\n__network__\nInter-| Receive | Transmit\n lo: 999 0 0 0 0 0 0 0 888 0 0 0 0 0 0 0\n eth0: 1234 0 0 0 0 0 0 0 5678 0 0 0 0 0 0 0\n",
        );
        assert_eq!(raw.network_time, Some(100.25));
        assert_eq!(
            raw.network.unwrap(),
            std::collections::BTreeMap::from([("eth0".into(), (1234, 5678))])
        );
        assert!(parse_sample("__network__\n").network.is_none());
    }
}
