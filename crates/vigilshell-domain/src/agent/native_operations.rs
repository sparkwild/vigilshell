//! 原生适配器只接收具名业务对象，不接受程序路径、总线地址或执行身份。
use super::*;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub enum ServiceManager {
    System,
    CurrentUser,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ServiceTarget {
    pub manager: ServiceManager,
    pub name: String,
}
impl ServiceTarget {
    pub fn identifier(&self) -> AgentResult<String> {
        if self.name.starts_with("user:")
            || !valid_service_name(&self.name)
            || !self.name.ends_with(".service")
            || self.name == ".service"
        {
            return Err("invalid_service");
        }
        Ok(format!(
            "{}{}",
            if self.manager == ServiceManager::CurrentUser {
                "user:"
            } else {
                ""
            },
            self.name
        ))
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum ServiceDefinitionContent {
    Unit { text: String },
    DropIn { name: String, text: String },
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NativeServiceDefinition {
    pub target: ServiceTarget,
    pub content: ServiceDefinitionContent,
    pub expected_hash: Option<String>,
}
impl NativeServiceDefinition {
    pub fn validate(&self, update: bool) -> AgentResult<()> {
        self.target.identifier()?;
        if update != self.expected_hash.is_some()
            || self
                .expected_hash
                .as_ref()
                .is_some_and(|h| !valid_content_hash(h))
        {
            return Err("invalid_service_revision");
        }
        let text = match &self.content {
            ServiceDefinitionContent::Unit { text } => text,
            ServiceDefinitionContent::DropIn { name, text } => {
                validate_drop_in(name)?;
                text
            }
        };
        if text.trim().is_empty() || text.len() > 256 * 1024 || text.contains('\0') {
            return Err("invalid_service_definition");
        }
        Ok(())
    }
}
pub fn validate_drop_in(name: &str) -> AgentResult<()> {
    if name.len() > 128
        || !name.ends_with(".conf")
        || name == ".conf"
        || !name.as_bytes()[0].is_ascii_alphanumeric()
        || !name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
    {
        Err("invalid_service_drop_in")
    } else {
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum PackageTarget {
    Repository {
        name: String,
        version: Option<String>,
    },
    Local {
        path: String,
        sha256: String,
    },
}
impl PackageTarget {
    pub fn validate(&self, remove: bool) -> AgentResult<()> {
        match self {
            Self::Repository { name, version } => {
                if !valid_package_name(name)
                    || version.as_ref().is_some_and(|v| {
                        v.is_empty()
                            || v.len() > 256
                            || !v
                                .bytes()
                                .all(|b| b.is_ascii_alphanumeric() || b".+~:_-".contains(&b))
                    })
                    || remove && version.is_some()
                {
                    return Err("invalid_package");
                }
            }
            Self::Local { path, sha256 } => {
                if remove
                    || !path.starts_with('/')
                    || normalize_path("/", path)? != *path
                    || !valid_content_hash(sha256)
                    || !(path.ends_with(".deb") || path.ends_with(".rpm"))
                {
                    return Err("invalid_local_package");
                }
            }
        }
        Ok(())
    }
}
