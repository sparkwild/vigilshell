//! 策略资源浏览独立于 Agent 会话；空闲、窗口释放和应用退出都归还只读连接租约。
use super::*;

pub(super) struct InspectorPool {
    sessions: Mutex<HashMap<String, Arc<Inspector>>>,
    capacity: Arc<tokio::sync::Semaphore>,
}

impl Default for InspectorPool {
    fn default() -> Self {
        Self {
            sessions: Mutex::new(HashMap::new()),
            capacity: Arc::new(tokio::sync::Semaphore::new(4)),
        }
    }
}

struct Inspector {
    connection_id: String,
    connection_revision: u64,
    connection: Arc<dyn AgentConnection>,
    cancelled: CancellationToken,
    touched: Mutex<Instant>,
    read: tokio::sync::Mutex<()>,
    _permit: tokio::sync::OwnedSemaphorePermit,
}

impl AgentService {
    pub(super) async fn open_policy_browser(
        self: &Arc<Self>,
        connection_id: String,
        connection_revision: u64,
    ) -> AgentResult<AgentReply> {
        self.check_browser_connection(&connection_id, connection_revision)
            .await?;
        let permit = self
            .inspectors
            .capacity
            .clone()
            .try_acquire_owned()
            .map_err(|_| "resource_browser_busy")?;
        let connection = tokio::select! { biased;
            _=self.stop.cancelled()=>return Err("service_stopped"),
            result=tokio::time::timeout(Duration::from_secs(45),self.connector.connect(connection_id.clone(),connection_revision))=>{
                result.map_err(|_|"resource_browser_timeout")?.map_err(WorkspaceError::code)?
            }
        };
        if let Err(error) = self
            .check_browser_connection(&connection_id, connection_revision)
            .await
        {
            let _ = connection.close().await;
            return Err(error);
        }
        let browser = Arc::new(Inspector {
            connection_id,
            connection_revision,
            connection,
            cancelled: CancellationToken::new(),
            touched: Mutex::new(Instant::now()),
            read: tokio::sync::Mutex::new(()),
            _permit: permit,
        });
        let id = new_id();
        self.inspectors
            .sessions
            .lock()
            .unwrap()
            .insert(id.clone(), browser.clone());
        let service = self.clone();
        let key = id.clone();
        self.jobs.spawn(async move {
            loop {
                let remaining = Duration::from_secs(120)
                    .saturating_sub(browser.touched.lock().unwrap().elapsed());
                tokio::select! { biased;
                    _=service.stop.cancelled()=>break,
                    _=browser.cancelled.cancelled()=>break,
                    _=tokio::time::sleep(remaining)=>{}
                }
                if browser.touched.lock().unwrap().elapsed() >= Duration::from_secs(120) {
                    service.close_policy_browser(&key).await;
                    break;
                }
            }
        });
        Ok(AgentReply::PolicyBrowser { id })
    }

    async fn check_browser_connection(&self, id: &str, revision: u64) -> AgentResult<()> {
        let result = self
            .database
            .execute(DbCommand::Get(id.into()))
            .await
            .map_err(WorkspaceError::code)?;
        if !matches!(result,DbReply::Connection(c) if c.revision==revision && !c.deleting && !c.credential_pending)
        {
            return Err("connection_changed");
        }
        Ok(())
    }

    pub(super) async fn read_policy_resources(
        &self,
        id: &str,
        query: PolicyResourceQuery,
    ) -> AgentResult<AgentReply> {
        query.validate()?;
        let browser = self
            .inspectors
            .sessions
            .lock()
            .unwrap()
            .get(id)
            .cloned()
            .ok_or("resource_browser_expired")?;
        self.check_browser_connection(&browser.connection_id, browser.connection_revision)
            .await?;
        let _read = tokio::select! { biased;
            _=browser.cancelled.cancelled()=>return Err("resource_browser_expired"),
            _=self.stop.cancelled()=>return Err("service_stopped"),
            read=browser.read.lock()=>read,
        };
        *browser.touched.lock().unwrap() = Instant::now();
        let page = tokio::select! { biased;
            _=self.stop.cancelled()=>return Err("service_stopped"),
            _=browser.cancelled.cancelled()=>return Err("resource_browser_expired"),
            result=tokio::time::timeout(Duration::from_secs(45),browser.connection.inspect_resources(query))=>{
                result.map_err(|_|"resource_browser_timeout")?.map_err(WorkspaceError::code)?
            }
        };
        self.check_browser_connection(&browser.connection_id, browser.connection_revision)
            .await?;
        *browser.touched.lock().unwrap() = Instant::now();
        Ok(AgentReply::PolicyResources { page })
    }

    pub(super) async fn close_policy_browser(&self, id: &str) {
        let browser = self.inspectors.sessions.lock().unwrap().remove(id);
        if let Some(browser) = browser {
            browser.cancelled.cancel();
            let _ = tokio::time::timeout(Duration::from_secs(5), browser.connection.close()).await;
        }
    }

    pub(super) async fn close_policy_browsers(&self) {
        let ids = self
            .inspectors
            .sessions
            .lock()
            .unwrap()
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        for id in ids {
            self.close_policy_browser(&id).await;
        }
    }
}
