//! 有界字面 Shell 子集；检查结果只在本进程内构造，不能从客户端反序列化。
use super::*;
mod parser;
mod rules;

#[derive(Clone, Debug)]
pub struct CheckedCommand {
    source: String,
    directory: String,
}
impl CheckedCommand {
    pub fn shell_source(&self) -> &str {
        &self.source
    }
    pub fn directory(&self) -> &str {
        &self.directory
    }
}

/// 单引号重建字面参数；原输入从不作为 SSH 的 exec 正文发送。
pub fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

pub fn check_command(
    request: &ExecutionRequest,
    policy: &ConnectionPolicy,
) -> AgentResult<CheckedCommand> {
    let authorization = request.authorize(policy)?;
    let RuntimeOperation::Command { command, directory } = &request.operation else {
        return Err("command_required");
    };
    let cwd = normalize_path(
        "/",
        if directory.path.is_empty() {
            "."
        } else {
            &directory.path
        },
    )?;
    let nodes = parser::parse(command)?;
    if authorization.command_check == CommandCheckRequirement::ReadOnlyAllowlist && nodes.len() != 1
    {
        return Err("read_only_command_denied");
    }
    let mut rebuilt = Vec::new();
    for node in &nodes {
        let words = node.words.clone();
        let readonly = authorization.command_check == CommandCheckRequirement::ReadOnlyAllowlist;
        let name = words[0].rsplit('/').next().ok_or("invalid_command")?;
        if readonly {
            if words[0] != name || !node.redirects.is_empty() || node.next.is_some() {
                return Err("read_only_command_denied");
            }
            rules::read_only(name, &words[1..])?;
            // 固定系统目录，不使用服务器账号自定义 PATH、别名或同名函数。
        } else {
            rules::operate(&words, &node.redirects, &cwd, &policy.execution)?;
        }
        let mut part = if readonly {
            let arguments = words[1..]
                .iter()
                .map(|w| shell_quote(w))
                .collect::<Vec<_>>()
                .join(" ");
            format!(
                "if test -x /usr/bin/{name}; then /usr/bin/{name} {arguments}; elif test -x /bin/{name}; then /bin/{name} {arguments}; else exit 127; fi"
            )
        } else {
            words
                .iter()
                .map(|w| shell_quote(w))
                .collect::<Vec<_>>()
                .join(" ")
        };
        for (operator, target) in &node.redirects {
            part.push_str(&format!(" {operator} {}", shell_quote(target)));
        }
        if let Some(operator) = &node.next {
            part.push_str(&format!(" {operator}"));
        }
        rebuilt.push(part);
    }
    Ok(CheckedCommand {
        source: rebuilt.join(" "),
        directory: cwd,
    })
}

#[cfg(test)]
mod tests;

/// 仅识别单个字面文件调用；串联、重定向和解释器表达式仍由命令检查拒绝。
pub type ScriptCommandCandidate = (String, Vec<String>, Option<ScriptInterpreter>);

pub fn script_command_candidate(
    request: &ExecutionRequest,
) -> AgentResult<Option<ScriptCommandCandidate>> {
    let RuntimeOperation::Command { command, directory } = &request.operation else {
        return Ok(None);
    };
    let nodes = parser::parse(command)?;
    if nodes.len() != 1 || !nodes[0].redirects.is_empty() || nodes[0].next.is_some() {
        return Ok(None);
    }
    let words = &nodes[0].words;
    let name = words[0].as_str();
    let interpreter = match name {
        "sh" | "/bin/sh" | "/usr/bin/sh" => Some(ScriptInterpreter::Sh),
        "bash" | "/bin/bash" | "/usr/bin/bash" => Some(ScriptInterpreter::Bash),
        "python3" | "/usr/bin/python3" => Some(ScriptInterpreter::Python3),
        "node" | "/usr/bin/node" => Some(ScriptInterpreter::Node),
        _ => None,
    };
    let (path, arguments) = if interpreter.is_some() {
        let mut index = 1;
        if words.get(index).is_some_and(|v| v == "--") {
            index += 1;
        }
        let Some(path) = words.get(index).filter(|v| !v.starts_with('-')) else {
            return Ok(None);
        };
        (path.as_str(), words[index + 1..].to_vec())
    } else if name.contains('/')
        || [".sh", ".bash", ".py", ".js", ".mjs", ".cjs"]
            .iter()
            .any(|suffix| name.ends_with(suffix))
    {
        (name, words[1..].to_vec())
    } else {
        return Ok(None);
    };
    let cwd = normalize_path(
        "/",
        if directory.path.is_empty() {
            "."
        } else {
            &directory.path
        },
    )?;
    Ok(Some((normalize_path(&cwd, path)?, arguments, interpreter)))
}
