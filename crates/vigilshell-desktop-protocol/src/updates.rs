use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
#[derive(Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ApplicationInfo {
    pub name: String,
    pub version: String,
}
#[derive(Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AvailableRelease {
    pub version: String,
    pub tag: String,
    pub notes: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub localized_notes: Option<LocalizedReleaseNotes>,
}
#[derive(Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReleaseNoteSections {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub features: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fixes: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub upgrade_notes: Vec<String>,
}
pub type LocalizedReleaseNotes = std::collections::BTreeMap<String, ReleaseNoteSections>;
#[derive(Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReleaseNotesDocument {
    pub schema: u8,
    pub version: String,
    pub tag: String,
    pub languages: LocalizedReleaseNotes,
}
#[derive(Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum UpdatePhase {
    Idle,
    Checking,
    Current,
    Available,
    Empty,
    NetworkError,
    RateLimited,
    InvalidResponse,
    Downloading,
    Ready,
    Preparing,
    Installing,
    DownloadError,
    InstallError,
}
#[derive(Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpdateProgress {
    pub transferred: f64,
    pub total: f64,
    pub percent: f64,
}
#[derive(Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum UpdateIssue {
    InsufficientSpace,
    ReadOnlyLocation,
    UnsavedState,
}
#[derive(Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum UpdateCheckPhase {
    Idle,
    Checking,
    Current,
    Available,
    Empty,
    NetworkError,
    RateLimited,
    InvalidResponse,
}
#[derive(Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpdateSnapshot {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub check_phase: Option<UpdateCheckPhase>,
    pub phase: UpdatePhase,
    pub release: Option<AvailableRelease>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub progress: Option<UpdateProgress>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub can_download: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub issue: Option<UpdateIssue>,
}
#[derive(Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", tag = "action", deny_unknown_fields)]
pub enum ApplicationRequest {
    Info,
    State,
    Check,
    OpenRelease,
    Project,
    Issues,
    Download,
    Cancel,
    Install,
}
#[derive(JsonSchema)]
pub struct ApplicationContract {
    pub info: ApplicationInfo,
    pub state: UpdateSnapshot,
    pub request: ApplicationRequest,
    pub release_notes: ReleaseNotesDocument,
}

#[cfg(test)]
mod tests {
    use super::ReleaseNotesDocument;

    #[test]
    fn release_notes_preserve_locale_keys_without_requiring_a_specific_language() {
        let value = serde_json::json!({
            "schema": 1,
            "version": "0.2.0",
            "tag": "v0.2.0",
            "languages": {
                "fr": { "features": ["Fonction"] },
                "ja-JP": { "features": ["新機能"] }
            }
        });
        let document: ReleaseNotesDocument = serde_json::from_value(value.clone()).unwrap();
        assert!(document.languages.contains_key("fr"));
        assert_eq!(serde_json::to_value(document).unwrap(), value);
    }
}
