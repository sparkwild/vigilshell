//! 用户编辑策略时的只读资源发现；不属于 Agent 工具或 Agent 会话授权。
use super::*;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum PolicyResourceQuery {
    Directory { path: String, after: Option<String> },
}

impl PolicyResourceQuery {
    pub fn validate(&self) -> AgentResult<()> {
        let Self::Directory { path, after } = self;
        if path.len() > 4096
            || !path.starts_with('/')
            || normalize_path("/", path)? != *path
            || after
                .as_ref()
                .is_some_and(|v| v.len() > 255 || v.chars().any(|c| c.is_control() || c == '/'))
        {
            return Err("invalid_resource_query");
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum PolicyResourcePage {
    Directory { path: String, page: AgentFilesPage },
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn retired_selection_queries_are_rejected_without_removing_directory_inspection() {
        for kind in ["services", "packages", "packageSources"] {
            assert!(
                serde_json::from_value::<PolicyResourceQuery>(serde_json::json!({"kind": kind}))
                    .is_err()
            );
            assert!(
                serde_json::from_value::<PolicyResourcePage>(
                    serde_json::json!({"kind": kind, "entries": []})
                )
                .is_err()
            );
        }
        let directory: PolicyResourceQuery = serde_json::from_value(
            serde_json::json!({"kind": "directory", "path": "/srv", "after": null}),
        )
        .unwrap();
        directory.validate().unwrap();
    }
}
