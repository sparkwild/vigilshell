//! 系统维护只授权确定的效果；独立能力不能覆盖用户明确设置的路径保护。
use super::*;

impl SystemSetting {
    pub fn program(self) -> &'static str {
        match self {
            Self::Hostname => "hostnamectl",
            Self::Timezone => "timedatectl",
            Self::Locale => "localectl",
        }
    }
    pub fn targets(self) -> &'static [&'static str] {
        match self {
            Self::Hostname => &["/etc/hostname", "/etc/machine-info"],
            Self::Timezone => &["/etc/localtime", "/etc/timezone"],
            Self::Locale => &["/etc/locale.conf", "/etc/default/locale"],
        }
    }
    pub fn validate_value(self, value: &str) -> AgentResult<()> {
        let valid = match self {
            Self::Hostname => {
                !value.is_empty()
                    && value.len() <= 64
                    && value.split('.').all(|label| {
                        !label.is_empty()
                            && label.len() <= 63
                            && label.as_bytes()[0].is_ascii_alphanumeric()
                            && label
                                .as_bytes()
                                .last()
                                .is_some_and(u8::is_ascii_alphanumeric)
                            && label
                                .bytes()
                                .all(|c| c.is_ascii_alphanumeric() || c == b'-')
                    })
            }
            Self::Timezone => {
                !value.is_empty()
                    && value.len() <= 128
                    && value.as_bytes()[0].is_ascii_alphanumeric()
                    && value.split('/').all(|part| {
                        !part.is_empty()
                            && part != "."
                            && part != ".."
                            && part
                                .bytes()
                                .all(|c| c.is_ascii_alphanumeric() || b"_+-".contains(&c))
                    })
            }
            Self::Locale => {
                !value.is_empty()
                    && value.len() <= 64
                    && value.as_bytes()[0].is_ascii_alphabetic()
                    && value
                        .bytes()
                        .all(|c| c.is_ascii_alphanumeric() || b"_.@-".contains(&c))
            }
        };
        if valid {
            Ok(())
        } else {
            Err("invalid_system_setting")
        }
    }
    pub fn arguments(self, value: &str) -> AgentResult<Vec<String>> {
        self.validate_value(value)?;
        Ok(match self {
            Self::Hostname => vec![
                "--no-ask-password".into(),
                "set-hostname".into(),
                value.into(),
            ],
            Self::Timezone => vec![
                "--no-ask-password".into(),
                "set-timezone".into(),
                value.into(),
            ],
            Self::Locale => vec![
                "--no-ask-password".into(),
                "set-locale".into(),
                format!("LANG={value}"),
            ],
        })
    }
}
