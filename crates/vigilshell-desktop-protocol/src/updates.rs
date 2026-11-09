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
}
#[derive(Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpdateSnapshot {
    pub phase: UpdatePhase,
    pub release: Option<AvailableRelease>,
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
}
#[derive(JsonSchema)]
pub struct ApplicationContract {
    pub info: ApplicationInfo,
    pub state: UpdateSnapshot,
    pub request: ApplicationRequest,
}
