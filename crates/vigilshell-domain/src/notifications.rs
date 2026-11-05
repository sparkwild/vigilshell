//! 系统通知总模式的共同偏好值；系统授权由桌面进程拥有。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum NotificationMode {
    Off,
    #[default]
    Unfocused,
    Always,
}
impl NotificationMode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Unfocused => "unfocused",
            Self::Always => "always",
        }
    }
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "off" => Some(Self::Off),
            "unfocused" => Some(Self::Unfocused),
            "always" => Some(Self::Always),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Preferences, PreferencesPatch, UpdatePreferencesRequest};
    #[test]
    fn notification_fields_merge_independently_and_conflicts_preserve_current() {
        let original = Preferences::default();
        assert_eq!(
            original.system_notification_mode,
            NotificationMode::Unfocused
        );
        assert!(original.attention_required_notification_enabled);
        let completion = UpdatePreferencesRequest {
            expected_revision: 0,
            patch: PreferencesPatch {
                system_notification_mode: Some(NotificationMode::Always),
                ..Default::default()
            },
            base: PreferencesPatch {
                system_notification_mode: Some(NotificationMode::Unfocused),
                ..Default::default()
            },
        };
        let current = completion.merge(&original).unwrap().preferences;
        let attention = UpdatePreferencesRequest {
            expected_revision: 0,
            patch: PreferencesPatch {
                attention_required_notification_enabled: Some(false),
                ..Default::default()
            },
            base: PreferencesPatch {
                attention_required_notification_enabled: Some(true),
                ..Default::default()
            },
        };
        let current = attention.merge(&current).unwrap().preferences;
        assert_eq!(current.revision, 2);
        assert_eq!(current.system_notification_mode, NotificationMode::Always);
        assert!(!current.attention_required_notification_enabled);
        assert!(!completion.merge(&current).unwrap().applied);
        let conflict = UpdatePreferencesRequest {
            patch: PreferencesPatch {
                system_notification_mode: Some(NotificationMode::Off),
                ..Default::default()
            },
            ..completion
        };
        assert!(conflict.merge(&current).is_err());
    }
    #[test]
    fn notification_patch_requires_matching_base_fields() {
        let request = UpdatePreferencesRequest {
            expected_revision: 0,
            patch: PreferencesPatch {
                attention_required_notification_enabled: Some(false),
                ..Default::default()
            },
            base: PreferencesPatch::default(),
        };
        assert!(request.validate().is_err());
    }
}
