use super::*;

impl Default for ConnectionPolicy {
    fn default() -> Self {
        Self {
            connection_id: String::new(),
            connection_revision: 0,
            revision: 0,
            verify_connect: true,
            execution: ExecutionPolicy::default(),
        }
    }
}
impl ConnectionPolicy {
    pub fn enabled(&self) -> bool {
        self.execution.level != OperationLevel::Disabled
    }
}

pub fn beneath(root: &str, path: &str) -> bool {
    root == "/" && path.starts_with('/')
        || path == root
        || path
            .strip_prefix(root)
            .is_some_and(|suffix| suffix.starts_with('/'))
}
pub fn normalize_path(cwd: &str, path: &str) -> AgentResult<String> {
    if path.is_empty() || path.len() > 4096 || path.chars().any(char::is_control) {
        return Err("invalid_path");
    }
    let full = if path.starts_with('/') {
        path.to_owned()
    } else {
        format!("{cwd}/{path}")
    };
    let mut parts = Vec::new();
    for part in full.split('/') {
        match part {
            "" | "." => {}
            ".." => return Err("path_traversal"),
            p => parts.push(p),
        }
    }
    Ok(format!("/{}", parts.join("/")))
}
