use super::*;

#[derive(Default)]
enum Access {
    #[default]
    Unread,
    Reading,
    Ready(SecretMaterial),
    Blocked(WorkspaceError),
    Released,
}

/// Owns the read independently of any waiter. Notifications never contain secrets.
#[derive(Default)]
pub(super) struct CredentialAccess {
    state: Mutex<Access>,
    changed: tokio::sync::Notify,
}
impl CredentialAccess {
    pub fn release(&self) {
        *self.state.lock().unwrap() = Access::Released;
        self.changed.notify_waiters();
    }
    pub fn retry(&self) {
        let mut state = self.state.lock().unwrap();
        if matches!(*state, Access::Blocked(_)) {
            *state = Access::Unread;
        }
    }
    pub async fn read(
        self: &Arc<Self>,
        store: Arc<dyn SecretStore>,
        namespace: String,
        reference: Option<String>,
        context: CredentialContext,
    ) -> Result<SecretMaterial, WorkspaceError> {
        loop {
            let changed = self.changed.notified();
            tokio::pin!(changed);
            changed.as_mut().enable();
            {
                let mut state = self.state.lock().unwrap();
                match &*state {
                    Access::Ready(secret) => {
                        if let Some(trace) = &secret.diagnostic {
                            trace.check("runtime_credential", &secret.bytes);
                        }
                        return Ok(secret.duplicate());
                    }
                    Access::Blocked(error) => return Err(*error),
                    Access::Released => return Err(WorkspaceError::Cancelled),
                    Access::Reading => {}
                    Access::Unread => {
                        *state = Access::Reading;
                        let slot = self.clone();
                        let store = store.clone();
                        let namespace = namespace.clone();
                        let reference = reference.clone();
                        let context = context.clone();
                        // The shared read outlives any individual connection waiter.
                        tokio::spawn(async move {
                            if matches!(*slot.state.lock().unwrap(), Access::Released) {
                                return;
                            }
                            let result = match reference {
                                Some(reference) => {
                                    store.read(&namespace, &reference, context).await
                                }
                                None => Err(WorkspaceError::CredentialMissing),
                            };
                            slot.finish(result);
                        });
                    }
                }
            }
            changed.await;
        }
    }
    fn finish(&self, result: Result<SecretMaterial, WorkspaceError>) {
        let mut state = self.state.lock().unwrap();
        // Released is terminal: late results are dropped and zeroized, never cached.
        if matches!(*state, Access::Reading) {
            *state = match result {
                Ok(secret) => Access::Ready(secret),
                Err(error) => Access::Blocked(error),
            };
        }
        drop(state);
        self.changed.notify_waiters();
    }
}
impl groups::RuntimeGroup {
    pub fn retry_credentials(&self) {
        self.login_access.retry();
        self.proxy_access.retry();
    }
    pub fn release_credentials(&self) {
        self.login_access.release();
        self.proxy_access.release();
    }
}
impl WorkspaceService {
    pub(super) async fn group_secret(
        &self,
        group: &groups::RuntimeGroup,
    ) -> Result<SecretMaterial, WorkspaceError> {
        group
            .login_access
            .read(
                self.secrets.clone(),
                group.namespace.clone(),
                group.profile.credential.clone(),
                CredentialContext::login(&group.profile.id, group.profile.input.auth),
            )
            .await
    }
    pub(super) async fn group_route(
        &self,
        group: &groups::RuntimeGroup,
    ) -> Result<Option<ProxyRoute>, WorkspaceError> {
        let Some(proxy) = &group.proxy else {
            return Ok(None);
        };
        let secret = if proxy.credential.is_some() {
            Some(
                group
                    .proxy_access
                    .read(
                        self.secrets.clone(),
                        group.namespace.clone(),
                        proxy.credential.clone(),
                        CredentialContext::proxy(&proxy.id),
                    )
                    .await?,
            )
        } else {
            None
        };
        Ok(Some(ProxyRoute {
            proxy: proxy.clone(),
            secret,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;

    struct Store {
        reads: AtomicUsize,
        gate: (AtomicBool, tokio::sync::Notify),
        error: Mutex<Option<WorkspaceError>>,
    }
    impl Store {
        fn new(block: bool) -> Arc<Self> {
            Arc::new(Self {
                reads: AtomicUsize::new(0),
                gate: (AtomicBool::new(block), tokio::sync::Notify::new()),
                error: Mutex::new(None),
            })
        }
        fn unblock(&self) {
            self.gate.0.store(false, Ordering::SeqCst);
            self.gate.1.notify_waiters();
        }
    }
    impl SecretStore for Store {
        fn read<'a>(
            &'a self,
            namespace: &'a str,
            reference: &'a str,
            context: CredentialContext,
        ) -> FutureResult<'a, SecretMaterial> {
            Box::pin(async move {
                self.reads.fetch_add(1, Ordering::SeqCst);
                loop {
                    let notified = self.gate.1.notified();
                    tokio::pin!(notified);
                    notified.as_mut().enable();
                    if !self.gate.0.load(Ordering::SeqCst) {
                        break;
                    }
                    notified.await;
                }
                if let Some(error) = *self.error.lock().unwrap() {
                    return Err(error);
                }
                Ok(SecretMaterial {
                    diagnostic: None,
                    kind: context.purpose.kind(),
                    bytes: format!("{namespace}/{reference}").into_bytes().into(),
                    label: String::new(),
                })
            })
        }
        fn write<'a>(
            &'a self,
            _: &'a str,
            _: &'a str,
            _: &'a SecretMaterial,
            _: CredentialContext,
        ) -> FutureResult<'a, ()> {
            unreachable!()
        }
        fn remove<'a>(&'a self, _: &'a str, _: &'a str) -> FutureResult<'a, ()> {
            unreachable!()
        }
    }
    async fn read(
        slot: &Arc<CredentialAccess>,
        store: &Arc<Store>,
    ) -> Result<SecretMaterial, WorkspaceError> {
        slot.read(
            store.clone(),
            "dev".into(),
            Some("login".into()),
            CredentialContext::login("owner", AuthKind::Password),
        )
        .await
    }
    async fn started(store: &Store) {
        tokio::time::timeout(Duration::from_secs(2), async {
            while store.reads.load(Ordering::SeqCst) == 0 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
    }
    #[tokio::test]
    async fn concurrent_waiters_survive_cancellation_and_reuse_one_read() {
        let slot = Arc::new(CredentialAccess::default());
        let store = Store::new(true);
        let a = slot.clone();
        let b = store.clone();
        let first = tokio::spawn(async move { read(&a, &b).await });
        started(&store).await;
        first.abort();
        let a = slot.clone();
        let b = store.clone();
        let second = tokio::spawn(async move { read(&a, &b).await });
        tokio::task::yield_now().await;
        store.unblock();
        assert_eq!(&*second.await.unwrap().unwrap().bytes, b"dev/login");
        for _ in 0..10 {
            read(&slot, &store).await.unwrap();
        }
        assert_eq!(store.reads.load(Ordering::SeqCst), 1);
    }
    #[tokio::test]
    async fn failures_latch_until_explicit_retry() {
        for error in [
            WorkspaceError::CredentialDenied,
            WorkspaceError::CredentialUnavailable,
            WorkspaceError::CredentialMissing,
        ] {
            let slot = Arc::new(CredentialAccess::default());
            let store = Store::new(false);
            *store.error.lock().unwrap() = Some(error);
            for _ in 0..5 {
                assert_eq!(read(&slot, &store).await.unwrap_err(), error);
            }
            assert_eq!(store.reads.load(Ordering::SeqCst), 1);
            *store.error.lock().unwrap() = None;
            slot.retry();
            slot.retry();
            let (a, b) = tokio::join!(read(&slot, &store), read(&slot, &store));
            a.unwrap();
            b.unwrap();
            slot.retry();
            read(&slot, &store).await.unwrap();
            assert_eq!(store.reads.load(Ordering::SeqCst), 2);
        }
    }
    #[tokio::test]
    async fn release_rejects_late_results_and_cannot_be_retried() {
        let slot = Arc::new(CredentialAccess::default());
        let store = Store::new(true);
        let a = slot.clone();
        let b = store.clone();
        let waiter = tokio::spawn(async move { read(&a, &b).await });
        started(&store).await;
        slot.release();
        assert_eq!(
            waiter.await.unwrap().unwrap_err(),
            WorkspaceError::Cancelled
        );
        store.unblock();
        slot.finish(Ok(SecretMaterial {
            diagnostic: None,
            kind: AuthKind::Password,
            bytes: vec![1].into(),
            label: String::new(),
        }));
        slot.retry();
        assert_eq!(
            read(&slot, &store).await.unwrap_err(),
            WorkspaceError::Cancelled
        );
        assert!(matches!(*slot.state.lock().unwrap(), Access::Released));
        assert_eq!(store.reads.load(Ordering::SeqCst), 1);
    }
    #[tokio::test]
    async fn independent_groups_keep_distinct_frozen_values() {
        let store = Store::new(false);
        let first = Arc::new(CredentialAccess::default());
        let second = Arc::new(CredentialAccess::default());
        let a = read(&first, &store).await.unwrap();
        let b = second
            .read(
                store.clone(),
                "other".into(),
                Some("replacement".into()),
                CredentialContext::login("owner", AuthKind::Password),
            )
            .await
            .unwrap();
        assert_ne!(*a.bytes, *b.bytes);
        first.release();
        second.retry();
        assert!(matches!(*second.state.lock().unwrap(), Access::Ready(_)));
        assert_eq!(store.reads.load(Ordering::SeqCst), 2);
    }
}
