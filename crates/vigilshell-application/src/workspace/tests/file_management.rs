use super::*;
use crate::{
    ports::ServerTools,
    tools::{ToolAction as A, ToolResult as R},
};
use vigilshell_domain::tools::*;

#[derive(Default)]
struct FileTools {
    files: Mutex<HashMap<String, RemoteEntry>>,
    removed: Mutex<Vec<String>>,
    fail: AtomicBool,
}
fn entry(path: &str, kind: &str) -> RemoteEntry {
    RemoteEntry {
        name: path.rsplit('/').next().unwrap().into(),
        path: path.into(),
        kind: kind.into(),
        size: Some(1),
        modified: Some(1),
        permissions: Some(0o4755),
        uid: Some(1),
        gid: Some(1),
    }
}
impl ServerTools for FileTools {
    fn stat(&self, path: String) -> FutureResult<'_, RemoteEntry> {
        Box::pin(async move {
            self.files
                .lock()
                .unwrap()
                .get(&path)
                .cloned()
                .ok_or(WorkspaceError::NotFound)
        })
    }
    fn create_file(&self, path: String) -> FutureResult<'_, ()> {
        Box::pin(async move {
            let mut files = self.files.lock().unwrap();
            if files.contains_key(&path) {
                return Err(WorkspaceError::FilesExists);
            }
            files.insert(path.clone(), entry(&path, "file"));
            Ok(())
        })
    }
    fn permissions(
        &self,
        path: String,
        set: u32,
        clear: u32,
        _: RemoteEntry,
    ) -> FutureResult<'_, ()> {
        Box::pin(async move {
            if self.fail.load(Ordering::Acquire) && path.ends_with("blocked") {
                return Err(WorkspaceError::FilesPermissionDenied);
            }
            let mut files = self.files.lock().unwrap();
            let item = files.get_mut(&path).unwrap();
            item.permissions = item.permissions.map(|m| (m | set) & !clear);
            Ok(())
        })
    }
    fn sample(&self, _: bool) -> FutureResult<'_, RawSample> {
        Box::pin(async { Ok(Default::default()) })
    }
    fn list(
        &self,
        path: String,
        _: Arc<AtomicBool>,
    ) -> FutureResult<'_, (String, Vec<RemoteEntry>)> {
        Box::pin(async move {
            if self.fail.load(Ordering::Acquire) && path.ends_with("/scan-blocked") {
                return Err(WorkspaceError::FilesPermissionDenied);
            }
            let prefix = format!("{}/", path.trim_end_matches('/'));
            let entries = self
                .files
                .lock()
                .unwrap()
                .values()
                .filter(|e| {
                    e.path
                        .strip_prefix(&prefix)
                        .is_some_and(|s| !s.is_empty() && !s.contains('/'))
                })
                .cloned()
                .collect();
            Ok((path, entries))
        })
    }
    fn mkdir(&self, _: String) -> FutureResult<'_, ()> {
        Box::pin(async { Ok(()) })
    }
    fn rename(&self, _: String, _: String) -> FutureResult<'_, ()> {
        Box::pin(async { Ok(()) })
    }
    fn remove(&self, path: String, directory: bool) -> FutureResult<'_, ()> {
        Box::pin(async move {
            if self.fail.load(Ordering::Acquire) && path.ends_with("blocked") {
                return Err(WorkspaceError::FilesPermissionDenied);
            }
            let mut files = self.files.lock().unwrap();
            if directory && files.keys().any(|k| k.starts_with(&(path.clone() + "/"))) {
                return Err(WorkspaceError::FilesNotEmpty);
            }
            files.remove(&path).ok_or(WorkspaceError::NotFound)?;
            self.removed.lock().unwrap().push(path);
            Ok(())
        })
    }
    fn transfer(&self, _: crate::ports::TransferIo) -> FutureResult<'_, ()> {
        Box::pin(async { Ok(()) })
    }
    fn close(&self) -> FutureResult<'_, ()> {
        Box::pin(async { Ok(()) })
    }
}
struct FileTransport(Arc<FileTools>);
impl SshTransport for FileTransport {
    fn probe(&self, input: ConnectionInput) -> FutureResult<'_, HostKey> {
        Transport.probe(input)
    }
    fn authenticate(
        &self,
        input: ConnectionInput,
        key: HostKey,
        secret: SecretMaterial,
    ) -> FutureResult<'_, Box<dyn SshSession>> {
        Transport.authenticate(input, key, secret)
    }
    fn tools(
        &self,
        _: ConnectionInput,
        _: HostKey,
        _: SecretMaterial,
        _: Option<ProxyRoute>,
    ) -> FutureResult<'_, Arc<dyn ServerTools>> {
        Box::pin(async { Ok(self.0.clone() as Arc<dyn ServerTools>) })
    }
}
async fn setup() -> (Fixture, Arc<FileTools>, String) {
    let mut f = Fixture::new();
    let tools = Arc::new(FileTools::default());
    for (path, kind) in [
        ("/", "directory"),
        ("/srv", "directory"),
        ("/srv/good", "file"),
        ("/srv/blocked", "file"),
        ("/srv/link", "symlink"),
    ] {
        tools
            .files
            .lock()
            .unwrap()
            .insert(path.into(), entry(path, kind));
    }
    f.service = WorkspaceService::new(
        f.service.database.clone(),
        f.secrets.clone(),
        Arc::new(Prompt),
        Arc::new(FileTransport(tools.clone())),
        Arc::new(Sink),
    );
    let profile = f.create().await;
    let session = open_confirmed(&f, &profile.id).await.session_id.unwrap();
    (f, tools, session)
}
async fn page(f: &Fixture, id: &str) -> FileOperationPage {
    for _ in 0..200 {
        let WorkspaceResult::Tools(R::FileOperation(p)) = f
            .run(WorkspaceAction::Tools(A::FileOperation {
                id: id.into(),
                offset: 0,
            }))
            .await
            .unwrap()
        else {
            panic!()
        };
        if p.phase != "working" {
            return p;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    panic!("file operation did not settle")
}
#[tokio::test]
async fn recursive_delete_continues_and_retry_does_not_replay_successful_children() {
    let (f, tools, session) = setup().await;
    tools.fail.store(true, Ordering::Release);
    let WorkspaceResult::Operation(op) = f
        .run(WorkspaceAction::Tools(A::StartFileOperation {
            session,
            paths: vec!["/srv".into()],
            operation: "remove".into(),
            recursive: true,
            set_mask: 0,
            clear_mask: 0,
        }))
        .await
        .unwrap()
    else {
        panic!()
    };
    let p = page(&f, &op.id).await;
    assert_eq!(p.phase, "failed");
    assert_eq!(p.completed, 2);
    assert_eq!(p.failed, 2);
    let summary = f.service.state.lock().unwrap().operations[&op.id]
        .view
        .file_summary
        .clone()
        .unwrap();
    assert_eq!(summary.attempt, 0);
    assert_eq!((summary.completed, summary.failed), (2, 2));
    tools.fail.store(false, Ordering::Release);
    f.run(WorkspaceAction::Tools(A::RetryFileOperation {
        id: op.id.clone(),
    }))
    .await
    .unwrap();
    let p = page(&f, &op.id).await;
    assert_eq!(p.phase, "success");
    assert_eq!(p.completed, 4);
    let summary = f.service.state.lock().unwrap().operations[&op.id]
        .view
        .file_summary
        .clone()
        .unwrap();
    assert_eq!(summary.attempt, 1);
    assert_eq!((summary.completed, summary.failed), (4, 0));
    let calls = tools.removed.lock().unwrap();
    assert_eq!(
        calls.iter().filter(|p| p.as_str() == "/srv/good").count(),
        1
    );
    assert_eq!(calls.last().unwrap(), "/srv");
}
#[tokio::test]
async fn permissions_preserve_special_bits_skip_links_and_stat_create_are_session_owned() {
    let (f, tools, session) = setup().await;
    let WorkspaceResult::Operation(op) = f
        .run(WorkspaceAction::Tools(A::StartFileOperation {
            session: session.clone(),
            paths: vec!["/srv".into()],
            operation: "permissions".into(),
            recursive: true,
            set_mask: 0o020,
            clear_mask: 0o004,
        }))
        .await
        .unwrap()
    else {
        panic!()
    };
    let p = page(&f, &op.id).await;
    assert_eq!(p.skipped, 1);
    assert_eq!(p.completed, 3);
    assert_eq!(
        tools.files.lock().unwrap()["/srv/good"].permissions,
        Some(0o4771)
    );
    f.run(WorkspaceAction::Tools(A::CreateFile {
        session: session.clone(),
        path: "/srv/new".into(),
    }))
    .await
    .unwrap();
    assert!(matches!(
        f.run(WorkspaceAction::Tools(A::CreateFile {
            session: session.clone(),
            path: "/srv/new".into()
        }))
        .await,
        Err(WorkspaceError::FilesExists)
    ));
    assert!(matches!(
        f.service
            .handle(
                &new_id(),
                WorkspaceAction::Tools(A::Stat {
                    session,
                    path: "/srv/new".into()
                })
            )
            .await,
        Err(WorkspaceError::Disconnected)
    ));
}
#[tokio::test]
async fn recursive_limit_precedes_all_writes_and_root_deletion_is_rejected() {
    let (f, tools, session) = setup().await;
    for i in 0..20000 {
        let path = format!("/srv/{i}");
        tools
            .files
            .lock()
            .unwrap()
            .insert(path.clone(), entry(&path, "file"));
    }
    let WorkspaceResult::Operation(op) = f
        .run(WorkspaceAction::Tools(A::StartFileOperation {
            session: session.clone(),
            paths: vec!["/srv".into()],
            operation: "remove".into(),
            recursive: true,
            set_mask: 0,
            clear_mask: 0,
        }))
        .await
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(
        page(&f, &op.id).await.error,
        Some(WorkspaceError::FilesTooLarge)
    );
    assert!(tools.removed.lock().unwrap().is_empty());
    assert!(matches!(
        f.run(WorkspaceAction::Tools(A::StartFileOperation {
            session,
            paths: vec!["/".into()],
            operation: "remove".into(),
            recursive: false,
            set_mask: 0,
            clear_mask: 0
        }))
        .await,
        Err(WorkspaceError::InvalidInput)
    ));
}

#[tokio::test]
async fn permissions_retry_reaches_failed_child_of_successful_parent_without_new_children() {
    let (f, tools, session) = setup().await;
    tools.fail.store(true, Ordering::Release);
    let WorkspaceResult::Operation(op) = f
        .run(WorkspaceAction::Tools(A::StartFileOperation {
            session,
            paths: vec!["/srv".into()],
            operation: "permissions".into(),
            recursive: true,
            set_mask: 0o020,
            clear_mask: 0o004,
        }))
        .await
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(page(&f, &op.id).await.failed, 1);
    tools.files.lock().unwrap().insert(
        "/srv/new-after-scan".into(),
        entry("/srv/new-after-scan", "file"),
    );
    tools.fail.store(false, Ordering::Release);
    f.run(WorkspaceAction::Tools(A::RetryFileOperation {
        id: op.id.clone(),
    }))
    .await
    .unwrap();
    let p = page(&f, &op.id).await;
    assert_eq!(p.phase, "success");
    assert_eq!(p.completed, 3);
    assert_eq!(
        tools.files.lock().unwrap()["/srv/blocked"].permissions,
        Some(0o4771)
    );
    assert_eq!(
        tools.files.lock().unwrap()["/srv/new-after-scan"].permissions,
        Some(0o4755)
    );
}
#[tokio::test]
async fn delete_retry_never_enrolls_a_file_created_after_the_original_scan() {
    let (f, tools, session) = setup().await;
    tools.fail.store(true, Ordering::Release);
    let WorkspaceResult::Operation(op) = f
        .run(WorkspaceAction::Tools(A::StartFileOperation {
            session,
            paths: vec!["/srv".into()],
            operation: "remove".into(),
            recursive: true,
            set_mask: 0,
            clear_mask: 0,
        }))
        .await
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(page(&f, &op.id).await.failed, 2);
    tools.files.lock().unwrap().insert(
        "/srv/new-after-scan".into(),
        entry("/srv/new-after-scan", "file"),
    );
    tools.fail.store(false, Ordering::Release);
    f.run(WorkspaceAction::Tools(A::RetryFileOperation {
        id: op.id.clone(),
    }))
    .await
    .unwrap();
    let p = page(&f, &op.id).await;
    assert_eq!(p.failed, 1);
    assert_eq!(p.error, Some(WorkspaceError::FilesNotEmpty));
    assert!(
        tools
            .files
            .lock()
            .unwrap()
            .contains_key("/srv/new-after-scan")
    );
}

#[tokio::test]
async fn retry_of_deep_scan_failure_keeps_original_depth_budget() {
    let (f, tools, session) = setup().await;
    let mut path = "/srv".to_owned();
    for depth in 1..=66 {
        path.push_str(if depth == 61 {
            "/scan-blocked"
        } else {
            "/deep"
        });
        tools
            .files
            .lock()
            .unwrap()
            .insert(path.clone(), entry(&path, "directory"));
    }
    tools.fail.store(true, Ordering::Release);
    let WorkspaceResult::Operation(op) = f
        .run(WorkspaceAction::Tools(A::StartFileOperation {
            session,
            paths: vec!["/srv".into()],
            operation: "permissions".into(),
            recursive: true,
            set_mask: 0,
            clear_mask: 0,
        }))
        .await
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(page(&f, &op.id).await.phase, "failed");
    tools.fail.store(false, Ordering::Release);
    f.run(WorkspaceAction::Tools(A::RetryFileOperation {
        id: op.id.clone(),
    }))
    .await
    .unwrap();
    assert_eq!(
        page(&f, &op.id).await.error,
        Some(WorkspaceError::FilesTooLarge)
    );
}
