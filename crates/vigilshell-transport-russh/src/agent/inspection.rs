//! 策略资源检查只读取目录元数据，不能提交命令或读取文件正文。
use super::*;
use vigilshell_domain::agent::*;

impl Connection {
    pub(super) async fn inspect(
        &self,
        query: PolicyResourceQuery,
    ) -> Result<PolicyResourcePage, WorkspaceError> {
        query.validate().map_err(|_| WorkspaceError::InvalidInput)?;
        match query {
            PolicyResourceQuery::Directory { path, after } => {
                tokio::time::timeout(Duration::from_secs(30), self.inspect_directory(path, after))
                    .await
                    .map_err(|_| WorkspaceError::Timeout)?
            }
        }
    }

    pub(super) async fn inspect_directory(
        &self,
        path: String,
        after: Option<String>,
    ) -> Result<PolicyResourcePage, WorkspaceError> {
        PolicyResourceQuery::Directory {
            path: path.clone(),
            after: after.clone(),
        }
        .validate()
        .map_err(|_| WorkspaceError::InvalidInput)?;
        let session = self
            .sftp
            .get_or_try_init(|| crate::sftp::open(&self.handle))
            .await?;
        let (canonical, mut entries) =
            crate::sftp::list(&session.0, path, &std::sync::atomic::AtomicBool::new(false)).await?;
        entries.retain(|e| {
            e.name.len() <= 255
                && !e.name.chars().any(char::is_control)
                && after.as_ref().is_none_or(|a| &e.name > a)
        });
        entries.sort_by(|a, b| a.name.cmp(&b.name));
        entries.dedup_by(|a, b| a.name == b.name);
        let more = entries.len() > 200;
        entries.truncate(200);
        let next_cursor = if more {
            entries.last().map(|e| e.name.clone())
        } else {
            None
        };
        let entries = entries
            .into_iter()
            .map(crate::sftp::agent_entry)
            .collect::<Result<_, _>>()?;
        Ok(PolicyResourcePage::Directory {
            path: canonical,
            page: AgentFilesPage {
                entries,
                next_cursor,
            },
        })
    }
}
