//! 原生软件后端与来源业务输入；依赖和安装效果由原生管理器处理。
use super::*;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum PackageManagerKind {
    Apt,
    Dnf4,
    Dnf5,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PackageManagerSupport {
    pub kind: PackageManagerKind,
    pub version: Option<String>,
    pub available: bool,
    pub reason: Option<String>,
}

pub fn valid_package_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 256
        && name.as_bytes()[0].is_ascii_alphanumeric()
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b".+-_:".contains(&b))
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(tag = "format", rename_all = "snake_case", deny_unknown_fields)]
pub enum PackageSourceDefinition {
    Apt {
        name: String,
        url: String,
        suite: String,
        components: Vec<String>,
        public_key: String,
    },
    Rpm {
        name: String,
        url: String,
        public_key: String,
    },
}
impl PackageSourceDefinition {
    pub fn validate(&self) -> AgentResult<()> {
        let (name, url, key) = match self {
            Self::Apt {
                name,
                url,
                suite,
                components,
                public_key,
            } => {
                if !source_word(suite)
                    || components.is_empty()
                    || components.len() > 16
                    || components.iter().any(|c| !source_word(c))
                {
                    return Err("invalid_package_source");
                }
                (name, url, public_key)
            }
            Self::Rpm {
                name,
                url,
                public_key,
            } => (name, url, public_key),
        };
        if !source_word(name)
            || name.len() > 64
            || !(url.starts_with("https://") || url.starts_with("http://"))
            || url.len() > 2048
            || url
                .split_once("://")
                .and_then(|(_, v)| v.split('/').next())
                .is_none_or(str::is_empty)
            || url.chars().any(|c| {
                c.is_control() || c.is_whitespace() || matches!(c, '@' | '#' | '?' | '\\' | '$')
            })
            || !key.starts_with("-----BEGIN PGP PUBLIC KEY BLOCK-----")
            || key.contains("PRIVATE KEY")
            || key.len() > 128 * 1024
        {
            return Err("invalid_package_source");
        }
        Ok(())
    }
}
fn source_word(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"._-".contains(&c))
        && !matches!(value, "." | "..")
}

/// 省略字段保留原值；更新来源不支持修改名称或格式，也不自动延续信任。
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct PackageSourcePatch {
    pub url: Option<String>,
    pub suite: Option<String>,
    pub components: Option<Vec<String>>,
    pub public_key: Option<String>,
}
impl PackageSourcePatch {
    pub fn validate(&self) -> AgentResult<()> {
        if self.url.is_none()
            && self.suite.is_none()
            && self.components.is_none()
            && self.public_key.is_none()
        {
            return Err("invalid_package_source");
        }
        PackageSourceDefinition::Apt {
            name: "validation".into(),
            url: self
                .url
                .clone()
                .unwrap_or_else(|| "https://validation.invalid".into()),
            suite: self.suite.clone().unwrap_or_else(|| "stable".into()),
            components: self
                .components
                .clone()
                .unwrap_or_else(|| vec!["main".into()]),
            public_key: self
                .public_key
                .clone()
                .unwrap_or_else(|| "-----BEGIN PGP PUBLIC KEY BLOCK-----".into()),
        }
        .validate()
    }
}

#[cfg(test)]
mod source_tests {
    use super::*;
    #[test]
    fn source_templates_reject_injection_and_private_keys() {
        let mut source = PackageSourceDefinition::Apt {
            name: "example".into(),
            url: "https://example.test/repo".into(),
            suite: "stable".into(),
            components: vec!["main".into()],
            public_key: "-----BEGIN PGP PUBLIC KEY BLOCK-----\nfixture".into(),
        };
        assert!(source.validate().is_ok());
        let PackageSourceDefinition::Apt { url, .. } = &mut source else {
            unreachable!()
        };
        *url = "https://example.test\nTrusted: yes".into();
        assert!(source.validate().is_err());
        for url in ["https://example.test/\0", "https:///missing-host"] {
            let source = PackageSourceDefinition::Rpm {
                name: "example".into(),
                url: url.into(),
                public_key: "-----BEGIN PGP PUBLIC KEY BLOCK-----\nfixture".into(),
            };
            assert!(source.validate().is_err());
        }
        let source = PackageSourceDefinition::Rpm {
            name: "../escape".into(),
            url: "https://example.test".into(),
            public_key: "-----BEGIN PGP PUBLIC KEY BLOCK-----\nfixture".into(),
        };
        assert!(source.validate().is_err());
    }
}
