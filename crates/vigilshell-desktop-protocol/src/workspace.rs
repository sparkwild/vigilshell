//! 工作区域 v9 的有限业务接口；不包含秘密或系统凭据引用。
pub const WORKSPACE_PROTOCOL_VERSION: u8 = 16;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use vigilshell_domain::workspace as domain;

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum AuthenticationMethod {
    Password,
    PrivateKey,
}
impl From<AuthenticationMethod> for domain::AuthKind {
    fn from(v: AuthenticationMethod) -> Self {
        match v {
            AuthenticationMethod::Password => Self::Password,
            AuthenticationMethod::PrivateKey => Self::PrivateKey,
        }
    }
}
impl From<domain::AuthKind> for AuthenticationMethod {
    fn from(v: domain::AuthKind) -> Self {
        match v {
            domain::AuthKind::Password => Self::Password,
            domain::AuthKind::PrivateKey => Self::PrivateKey,
        }
    }
}
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum ConnectionEnvironment {
    #[default]
    None,
    Development,
    Test,
    Staging,
    Production,
}
impl From<ConnectionEnvironment> for domain::Environment {
    fn from(v: ConnectionEnvironment) -> Self {
        match v {
            ConnectionEnvironment::None => Self::None,
            ConnectionEnvironment::Development => Self::Development,
            ConnectionEnvironment::Test => Self::Test,
            ConnectionEnvironment::Staging => Self::Staging,
            ConnectionEnvironment::Production => Self::Production,
        }
    }
}
impl From<domain::Environment> for ConnectionEnvironment {
    fn from(v: domain::Environment) -> Self {
        match v {
            domain::Environment::None => Self::None,
            domain::Environment::Development => Self::Development,
            domain::Environment::Test => Self::Test,
            domain::Environment::Staging => Self::Staging,
            domain::Environment::Production => Self::Production,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GroupBucket {
    #[serde(deserialize_with = "crate::schema::required_nullable")]
    #[schemars(with = "crate::schema::NullableRequired<String>")]
    #[schemars(extend("format" = "uuid", "pattern" = "^([0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[1-8][0-9a-fA-F]{3}-[89abAB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}|00000000-0000-0000-0000-000000000000|ffffffff-ffff-ffff-ffff-ffffffffffff)$"))]
    pub group_id: Option<String>,
    pub connection_ids: Vec<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GroupLayout {
    #[schemars(range(min = 0, max = 9_007_199_254_740_991_u64))]
    #[schemars(extend("maximum" = 9007199254740991_u64, "minimum" = 0))]
    pub revision: u64,
    pub group_ids: Vec<String>,
    pub buckets: Vec<GroupBucket>,
}
impl From<GroupLayout> for domain::GroupLayout {
    fn from(v: GroupLayout) -> Self {
        Self {
            revision: v.revision,
            group_ids: v.group_ids,
            buckets: v
                .buckets
                .into_iter()
                .map(|b| domain::GroupBucket {
                    group_id: b.group_id,
                    connection_ids: b.connection_ids,
                })
                .collect(),
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TerminalSettings {
    #[schemars(extend("enum" = ["utf-8", "gb18030", "gbk", "big5"]))]
    pub encoding: String,
    #[schemars(extend("enum" = ["del", "bs"]))]
    pub backspace: String,
    #[schemars(extend("enum" = ["vt220", "del"]))]
    pub delete: String,
}
impl Default for TerminalSettings {
    fn default() -> Self {
        domain::TerminalSettings::default().into()
    }
}
impl From<TerminalSettings> for domain::TerminalSettings {
    fn from(v: TerminalSettings) -> Self {
        Self {
            encoding: v.encoding,
            backspace: v.backspace,
            delete: v.delete,
        }
    }
}
impl From<domain::TerminalSettings> for TerminalSettings {
    fn from(v: domain::TerminalSettings) -> Self {
        Self {
            encoding: v.encoding,
            backspace: v.backspace,
            delete: v.delete,
        }
    }
}
impl From<TerminalSettings> for Box<domain::TerminalSettings> {
    fn from(v: TerminalSettings) -> Self {
        Box::new(v.into())
    }
}
impl From<Box<domain::TerminalSettings>> for TerminalSettings {
    fn from(v: Box<domain::TerminalSettings>) -> Self {
        (*v).into()
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[schemars(extend("x-vigil-rule" = "tunnel"))]
pub struct TunnelRule {
    #[schemars(extend("format" = "uuid", "pattern" = "^([0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[1-8][0-9a-fA-F]{3}-[89abAB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}|00000000-0000-0000-0000-000000000000|ffffffff-ffff-ffff-ffff-ffffffffffff)$"))]
    pub id: String,
    #[schemars(extend("minLength" = 1))]
    pub name: String,
    #[schemars(extend("enum" = ["local", "remote", "socks5"]))]
    pub kind: String,
    pub bind_host: String,
    #[schemars(extend("maximum" = 65535, "minimum" = 1))]
    pub listen_port: u16,
    #[serde(deserialize_with = "crate::schema::required_nullable")]
    #[schemars(with = "crate::schema::NullableRequired<String>")]
    pub target_host: Option<String>,
    #[serde(deserialize_with = "crate::schema::required_nullable")]
    #[schemars(with = "crate::schema::NullableRequired<u16>")]
    #[schemars(extend("maximum" = 65535, "minimum" = 1))]
    pub target_port: Option<u16>,
    pub enabled: bool,
}
impl From<TunnelRule> for domain::TunnelRule {
    fn from(v: TunnelRule) -> Self {
        Self {
            id: v.id,
            name: v.name,
            kind: v.kind,
            bind_host: v.bind_host,
            listen_port: v.listen_port,
            target_host: v.target_host,
            target_port: v.target_port,
            enabled: v.enabled,
        }
    }
}
impl From<domain::TunnelRule> for TunnelRule {
    fn from(v: domain::TunnelRule) -> Self {
        Self {
            id: v.id,
            name: v.name,
            kind: v.kind,
            bind_host: v.bind_host,
            listen_port: v.listen_port,
            target_host: v.target_host,
            target_port: v.target_port,
            enabled: v.enabled,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[schemars(extend("x-vigil-rule" = "proxy"))]
pub struct ProxyConfig {
    #[schemars(extend("minLength" = 1))]
    pub name: String,
    #[schemars(extend("enum" = ["socks5", "http"]))]
    pub kind: String,
    pub host: String,
    #[schemars(extend("maximum" = 65535, "minimum" = 1))]
    pub port: u16,
    pub username: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProxyView {
    #[schemars(extend("format" = "uuid", "pattern" = "^([0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[1-8][0-9a-fA-F]{3}-[89abAB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}|00000000-0000-0000-0000-000000000000|ffffffff-ffff-ffff-ffff-ffffffffffff)$"))]
    pub id: String,

    #[schemars(range(min = 0, max = 9_007_199_254_740_991_u64))]
    #[schemars(extend("maximum" = 9007199254740991_u64, "minimum" = 0))]
    pub version: u64,
    pub config: ProxyConfig,
    pub credential_saved: bool,
}
impl From<domain::ProxyRecord> for ProxyView {
    fn from(v: domain::ProxyRecord) -> Self {
        Self {
            id: v.id,
            version: v.version,
            credential_saved: v.credential.is_some(),
            config: ProxyConfig {
                name: v.name,
                kind: v.kind,
                host: v.host,
                port: v.port,
                username: v.username,
            },
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProxySave {
    #[serde(deserialize_with = "crate::schema::required_nullable")]
    #[schemars(with = "crate::schema::NullableRequired<String>")]
    #[schemars(extend("format" = "uuid", "pattern" = "^([0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[1-8][0-9a-fA-F]{3}-[89abAB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}|00000000-0000-0000-0000-000000000000|ffffffff-ffff-ffff-ffff-ffffffffffff)$"))]
    pub proxy_id: Option<String>,

    #[schemars(range(min = 0, max = 9_007_199_254_740_991_u64))]
    #[serde(deserialize_with = "crate::schema::required_nullable")]
    #[schemars(with = "crate::schema::NullableRequired<u64>")]
    #[schemars(extend("maximum" = 9007199254740991_u64, "minimum" = 0))]
    pub expected_version: Option<u64>,
    pub config: ProxyConfig,
    #[serde(deserialize_with = "crate::schema::required_nullable")]
    #[schemars(with = "crate::schema::NullableRequired<String>")]
    #[schemars(extend("format" = "uuid", "pattern" = "^([0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[1-8][0-9a-fA-F]{3}-[89abAB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}|00000000-0000-0000-0000-000000000000|ffffffff-ffff-ffff-ffff-ffffffffffff)$"))]
    pub draft_id: Option<String>,
    pub clear_password: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TunnelStatus {
    #[schemars(extend("format" = "uuid", "pattern" = "^([0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[1-8][0-9a-fA-F]{3}-[89abAB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}|00000000-0000-0000-0000-000000000000|ffffffff-ffff-ffff-ffff-ffffffffffff)$"))]
    pub rule_id: String,
    #[schemars(extend("enum" = ["stopped", "disabled", "running", "failed"]))]
    pub phase: String,
    #[serde(deserialize_with = "crate::schema::required_nullable")]
    #[schemars(with = "crate::schema::NullableRequired<String>")]
    pub error: Option<String>,
}
impl From<domain::TunnelStatus> for TunnelStatus {
    fn from(v: domain::TunnelStatus) -> Self {
        Self {
            rule_id: v.rule_id,
            phase: v.phase,
            error: v.error,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[schemars(extend("x-vigil-rule" = "connection"))]
pub struct ConnectionConfig {
    #[serde(default)]
    pub notes: String,
    #[serde(default)]
    pub terminal: TerminalSettings,
    #[serde(default)]
    #[schemars(extend("format" = "uuid", "pattern" = "^([0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[1-8][0-9a-fA-F]{3}-[89abAB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}|00000000-0000-0000-0000-000000000000|ffffffff-ffff-ffff-ffff-ffffffffffff)$"))]
    pub proxy_id: Option<String>,
    #[serde(default)]
    #[schemars(extend("maxItems" = 32))]
    pub tunnels: Vec<TunnelRule>,
    #[serde(default)]
    #[schemars(extend("enum" = ["none", "development", "test", "staging", "production"]))]
    pub environment: ConnectionEnvironment,
    #[schemars(extend("minLength" = 1))]
    pub name: String,
    pub host: String,
    #[schemars(extend("maximum" = 65535, "minimum" = 1))]
    pub port: u16,
    #[schemars(extend("minLength" = 1))]
    pub username: String,
    #[schemars(extend("enum" = ["password", "privateKey"]))]
    pub auth: AuthenticationMethod,
}
impl From<ConnectionConfig> for domain::ConnectionInput {
    fn from(v: ConnectionConfig) -> Self {
        Self {
            notes: v.notes,
            terminal: v.terminal.into(),
            proxy_id: v.proxy_id,
            tunnels: v.tunnels.into_iter().map(Into::into).collect(),
            name: v.name,
            host: v.host,
            port: v.port,
            username: v.username,
            auth: v.auth.into(),
            environment: v.environment.into(),
        }
    }
}
impl From<domain::ConnectionInput> for ConnectionConfig {
    fn from(v: domain::ConnectionInput) -> Self {
        Self {
            notes: v.notes,
            terminal: v.terminal.into(),
            proxy_id: v.proxy_id,
            tunnels: v.tunnels.into_iter().map(Into::into).collect(),
            name: v.name,
            host: v.host,
            port: v.port,
            username: v.username,
            auth: v.auth.into(),
            environment: v.environment.into(),
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConnectionGroupView {
    #[schemars(range(min = 0, max = 9_007_199_254_740_991_u64))]
    #[schemars(extend("maximum" = 9007199254740991_u64, "minimum" = 0))]
    pub sort_order: u64,
    #[schemars(extend("format" = "uuid", "pattern" = "^([0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[1-8][0-9a-fA-F]{3}-[89abAB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}|00000000-0000-0000-0000-000000000000|ffffffff-ffff-ffff-ffff-ffffffffffff)$"))]
    pub id: String,
    pub name: String,

    #[schemars(range(min = 0, max = 9_007_199_254_740_991_u64))]
    #[schemars(extend("maximum" = 9007199254740991_u64, "minimum" = 0))]
    pub version: u64,
}
impl From<domain::ConnectionGroup> for ConnectionGroupView {
    fn from(v: domain::ConnectionGroup) -> Self {
        Self {
            sort_order: v.sort_order,
            id: v.id,
            name: v.name,
            version: v.version,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConnectionSummary {
    pub host: String,
    #[schemars(extend("enum" = ["none", "development", "test", "staging", "production"]))]
    pub environment: ConnectionEnvironment,
    #[schemars(extend("format" = "uuid", "pattern" = "^([0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[1-8][0-9a-fA-F]{3}-[89abAB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}|00000000-0000-0000-0000-000000000000|ffffffff-ffff-ffff-ffff-ffffffffffff)$"))]
    pub id: String,
    #[serde(deserialize_with = "crate::schema::required_nullable")]
    #[schemars(with = "crate::schema::NullableRequired<String>")]
    #[schemars(extend("format" = "uuid", "pattern" = "^([0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[1-8][0-9a-fA-F]{3}-[89abAB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}|00000000-0000-0000-0000-000000000000|ffffffff-ffff-ffff-ffff-ffffffffffff)$"))]
    pub group_id: Option<String>,

    #[schemars(range(min = 0, max = 9_007_199_254_740_991_u64))]
    #[schemars(extend("maximum" = 9007199254740991_u64, "minimum" = 0))]
    pub group_sort_order: u64,
    #[serde(deserialize_with = "crate::schema::required_nullable")]
    #[schemars(with = "crate::schema::NullableRequired<String>")]
    pub created_at: Option<String>,
    pub name: String,
    #[schemars(extend("enum" = ["password", "privateKey"]))]
    pub auth: AuthenticationMethod,
    pub credential_saved: bool,
    pub credential_pending: bool,
    pub deleting: bool,

    #[schemars(range(min = 0, max = 9_007_199_254_740_991_u64))]
    #[schemars(extend("maximum" = 9007199254740991_u64, "minimum" = 0))]
    pub version: u64,

    #[schemars(range(min = 0, max = 9_007_199_254_740_991_u64))]
    #[schemars(extend("maximum" = 9007199254740991_u64, "minimum" = 0))]
    pub revision: u64,
}
impl From<domain::ConnectionRecord> for ConnectionSummary {
    fn from(v: domain::ConnectionRecord) -> Self {
        Self {
            id: v.id,
            group_id: v.group_id,
            group_sort_order: v.group_sort_order,
            created_at: v.created_at,
            name: v.input.name,
            host: v.input.host,
            environment: v.input.environment.into(),
            auth: v.input.auth.into(),
            credential_saved: v.credential.is_some(),
            credential_pending: v.credential_pending,
            deleting: v.deleting,
            version: v.version,
            revision: v.revision,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConnectionDetails {
    #[schemars(extend("format" = "uuid", "pattern" = "^([0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[1-8][0-9a-fA-F]{3}-[89abAB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}|00000000-0000-0000-0000-000000000000|ffffffff-ffff-ffff-ffff-ffffffffffff)$"))]
    pub id: String,
    #[serde(deserialize_with = "crate::schema::required_nullable")]
    #[schemars(with = "crate::schema::NullableRequired<String>")]
    #[schemars(extend("format" = "uuid", "pattern" = "^([0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[1-8][0-9a-fA-F]{3}-[89abAB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}|00000000-0000-0000-0000-000000000000|ffffffff-ffff-ffff-ffff-ffffffffffff)$"))]
    pub group_id: Option<String>,

    #[schemars(range(min = 0, max = 9_007_199_254_740_991_u64))]
    #[schemars(extend("maximum" = 9007199254740991_u64, "minimum" = 0))]
    pub group_sort_order: u64,
    #[serde(deserialize_with = "crate::schema::required_nullable")]
    #[schemars(with = "crate::schema::NullableRequired<String>")]
    pub created_at: Option<String>,
    pub config: ConnectionConfig,
    pub credential_saved: bool,
    #[serde(deserialize_with = "crate::schema::required_nullable")]
    #[schemars(with = "crate::schema::NullableRequired<String>")]
    pub credential_label: Option<String>,
    pub credential_pending: bool,
    pub deleting: bool,

    #[schemars(range(min = 0, max = 9_007_199_254_740_991_u64))]
    #[schemars(extend("maximum" = 9007199254740991_u64, "minimum" = 0))]
    pub version: u64,

    #[schemars(range(min = 0, max = 9_007_199_254_740_991_u64))]
    #[schemars(extend("maximum" = 9007199254740991_u64, "minimum" = 0))]
    pub revision: u64,
}
impl From<domain::ConnectionRecord> for ConnectionDetails {
    fn from(v: domain::ConnectionRecord) -> Self {
        Self {
            id: v.id,
            group_id: v.group_id,
            group_sort_order: v.group_sort_order,
            created_at: v.created_at,
            config: v.input.into(),
            credential_saved: v.credential.is_some(),
            credential_label: v.credential_label,
            credential_pending: v.credential_pending,
            deleting: v.deleting,
            version: v.version,
            revision: v.revision,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CataloguePage {
    #[schemars(range(min = 0, max = 9_007_199_254_740_991_u64))]
    #[schemars(extend("maximum" = 9007199254740991_u64, "minimum" = 0))]
    pub version: u64,
    #[schemars(extend("maxItems" = 50))]
    pub groups: Vec<ConnectionGroupView>,

    #[schemars(range(min = 0, max = 9_007_199_254_740_991_u64))]
    #[schemars(extend("maximum" = 9007199254740991_u64, "minimum" = 0))]
    pub layout_revision: u64,
    #[schemars(extend("maxItems" = 50))]
    pub connections: Vec<ConnectionSummary>,
    #[serde(deserialize_with = "crate::schema::required_nullable")]
    #[schemars(with = "crate::schema::NullableRequired<u32>")]
    #[schemars(extend("maximum" = 9007199254740991_u64, "minimum" = -9007199254740991_i64))]
    pub next: Option<u32>,
}
impl From<domain::Catalogue> for CataloguePage {
    fn from(v: domain::Catalogue) -> Self {
        Self {
            version: v.version,
            groups: v.groups.into_iter().map(Into::into).collect(),
            layout_revision: v.layout_revision,
            connections: v.connections.into_iter().map(Into::into).collect(),
            next: v.next,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TrustView {
    #[schemars(extend("maxLength" = 128))]
    pub fingerprint: String,
    #[schemars(extend("maxLength" = 128))]
    pub algorithm: String,
    #[schemars(extend("maxLength" = 64))]
    pub confirmed_at: String,
}
impl From<domain::HostTrust> for TrustView {
    fn from(v: domain::HostTrust) -> Self {
        Self {
            fingerprint: v.key.fingerprint,
            algorithm: v.key.algorithm,
            confirmed_at: v.confirmed_at,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum CatalogueKind {
    Groups,
    Connections,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CatalogueQuery {
    #[schemars(extend("enum" = ["groups", "connections"]))]
    pub kind: CatalogueKind,
    #[schemars(extend("maxLength" = 256))]
    pub query: String,
    #[schemars(extend("maximum" = 10000000, "minimum" = 0))]
    pub offset: u32,

    #[schemars(range(min = 0, max = 9_007_199_254_740_991_u64))]
    #[serde(deserialize_with = "crate::schema::required_nullable")]
    #[schemars(with = "crate::schema::NullableRequired<u64>")]
    #[schemars(extend("maximum" = 9007199254740991_u64, "minimum" = 0))]
    pub version: Option<u64>,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VersionedId {
    #[schemars(extend("format" = "uuid", "pattern" = "^([0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[1-8][0-9a-fA-F]{3}-[89abAB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}|00000000-0000-0000-0000-000000000000|ffffffff-ffff-ffff-ffff-ffffffffffff)$"))]
    pub id: String,

    #[schemars(range(min = 0, max = 9_007_199_254_740_991_u64))]
    #[schemars(extend("maximum" = 9007199254740991_u64, "minimum" = 0))]
    pub version: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConnectionSave {
    #[serde(deserialize_with = "crate::schema::required_nullable")]
    #[schemars(with = "crate::schema::NullableRequired<String>")]
    #[schemars(extend("format" = "uuid", "pattern" = "^([0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[1-8][0-9a-fA-F]{3}-[89abAB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}|00000000-0000-0000-0000-000000000000|ffffffff-ffff-ffff-ffff-ffffffffffff)$"))]
    pub connection_id: Option<String>,

    #[schemars(range(min = 0, max = 9_007_199_254_740_991_u64))]
    #[serde(deserialize_with = "crate::schema::required_nullable")]
    #[schemars(with = "crate::schema::NullableRequired<u64>")]
    #[schemars(extend("maximum" = 9007199254740991_u64, "minimum" = 0))]
    pub expected_version: Option<u64>,
    #[serde(deserialize_with = "crate::schema::required_nullable")]
    #[schemars(with = "crate::schema::NullableRequired<String>")]
    #[schemars(extend("format" = "uuid", "pattern" = "^([0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[1-8][0-9a-fA-F]{3}-[89abAB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}|00000000-0000-0000-0000-000000000000|ffffffff-ffff-ffff-ffff-ffffffffffff)$"))]
    pub group_id: Option<String>,
    pub config: ConnectionConfig,
    #[serde(deserialize_with = "crate::schema::required_nullable")]
    #[schemars(with = "crate::schema::NullableRequired<String>")]
    #[schemars(extend("format" = "uuid", "pattern" = "^([0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[1-8][0-9a-fA-F]{3}-[89abAB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}|00000000-0000-0000-0000-000000000000|ffffffff-ffff-ffff-ffff-ffffffffffff)$"))]
    pub draft_id: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConnectionTest {
    #[serde(deserialize_with = "crate::schema::required_nullable")]
    #[schemars(with = "crate::schema::NullableRequired<String>")]
    #[schemars(extend("format" = "uuid", "pattern" = "^([0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[1-8][0-9a-fA-F]{3}-[89abAB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}|00000000-0000-0000-0000-000000000000|ffffffff-ffff-ffff-ffff-ffffffffffff)$"))]
    pub connection_id: Option<String>,
    #[serde(deserialize_with = "crate::schema::required_nullable")]
    #[schemars(with = "crate::schema::NullableRequired<ConnectionConfig>")]
    pub config: Option<ConnectionConfig>,
    #[serde(deserialize_with = "crate::schema::required_nullable")]
    #[schemars(with = "crate::schema::NullableRequired<String>")]
    #[schemars(extend("format" = "uuid", "pattern" = "^([0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[1-8][0-9a-fA-F]{3}-[89abAB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}|00000000-0000-0000-0000-000000000000|ffffffff-ffff-ffff-ffff-ffffffffffff)$"))]
    pub draft_id: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum CredentialIntent {
    Keep,
    Replace,
    Remove,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CredentialDraftView {
    #[schemars(extend("enum" = ["keep", "replace", "remove"]))]
    pub intent: CredentialIntent,
    #[serde(deserialize_with = "crate::schema::required_nullable")]
    #[schemars(with = "crate::schema::NullableRequired<String>")]
    #[schemars(extend("format" = "uuid", "pattern" = "^([0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[1-8][0-9a-fA-F]{3}-[89abAB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}|00000000-0000-0000-0000-000000000000|ffffffff-ffff-ffff-ffff-ffffffffffff)$"))]
    pub proxy_id: Option<String>,
    #[schemars(extend("format" = "uuid", "pattern" = "^([0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[1-8][0-9a-fA-F]{3}-[89abAB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}|00000000-0000-0000-0000-000000000000|ffffffff-ffff-ffff-ffff-ffffffffffff)$"))]
    pub id: String,
    #[serde(deserialize_with = "crate::schema::required_nullable")]
    #[schemars(with = "crate::schema::NullableRequired<String>")]
    #[schemars(extend("format" = "uuid", "pattern" = "^([0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[1-8][0-9a-fA-F]{3}-[89abAB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}|00000000-0000-0000-0000-000000000000|ffffffff-ffff-ffff-ffff-ffffffffffff)$"))]
    pub connection_id: Option<String>,
    pub prepared: bool,
    pub saved: bool,
    #[serde(deserialize_with = "crate::schema::required_nullable")]
    #[schemars(with = "crate::schema::NullableRequired<AuthenticationMethod>")]
    #[schemars(extend("enum" = ["password", "privateKey", null]))]
    pub auth: Option<AuthenticationMethod>,
    #[serde(deserialize_with = "crate::schema::required_nullable")]
    #[schemars(with = "crate::schema::NullableRequired<String>")]
    pub label: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TrustChallenge {
    #[schemars(extend("format" = "uuid", "pattern" = "^([0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[1-8][0-9a-fA-F]{3}-[89abAB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}|00000000-0000-0000-0000-000000000000|ffffffff-ffff-ffff-ffff-ffffffffffff)$"))]
    pub id: String,
    pub name: String,
    pub host: String,
    #[schemars(extend("maximum" = 9007199254740991_u64, "minimum" = -9007199254740991_i64))]
    pub port: u16,
    pub fingerprint: String,
    #[serde(deserialize_with = "crate::schema::required_nullable")]
    #[schemars(with = "crate::schema::NullableRequired<String>")]
    pub previous_fingerprint: Option<String>,
    pub algorithm: String,
    #[schemars(extend("maximum" = 9007199254740991_u64, "minimum" = -9007199254740991_i64))]
    pub expires_in_seconds: u32,
    pub temporary: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum OperationPhase {
    Working,
    Trust,
    Success,
    Failed,
    Cancelled,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkspaceOperation {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_summary: Option<FileOperationSummary>,
    #[serde(deserialize_with = "crate::schema::required_nullable")]
    #[schemars(with = "crate::schema::NullableRequired<ProxyView>")]
    pub proxy: Option<ProxyView>,
    #[schemars(extend("format" = "uuid", "pattern" = "^([0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[1-8][0-9a-fA-F]{3}-[89abAB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}|00000000-0000-0000-0000-000000000000|ffffffff-ffff-ffff-ffff-ffffffffffff)$"))]
    pub id: String,
    #[schemars(extend("maxLength" = 64))]
    pub kind: String,
    #[schemars(extend("enum" = ["working", "trust", "success", "failed", "cancelled"]))]
    pub phase: OperationPhase,
    #[schemars(extend("maxLength" = 64))]
    pub stage: String,
    #[serde(deserialize_with = "crate::schema::required_nullable")]
    #[schemars(with = "crate::schema::NullableRequired<String>")]
    pub error: Option<String>,
    #[serde(deserialize_with = "crate::schema::required_nullable")]
    #[schemars(with = "crate::schema::NullableRequired<CredentialDraftView>")]
    pub draft: Option<CredentialDraftView>,
    #[serde(deserialize_with = "crate::schema::required_nullable")]
    #[schemars(with = "crate::schema::NullableRequired<ConnectionDetails>")]
    pub connection: Option<ConnectionDetails>,
    #[serde(deserialize_with = "crate::schema::required_nullable")]
    #[schemars(with = "crate::schema::NullableRequired<TrustChallenge>")]
    pub challenge: Option<TrustChallenge>,
    #[serde(deserialize_with = "crate::schema::required_nullable")]
    #[schemars(with = "crate::schema::NullableRequired<String>")]
    #[schemars(extend("format" = "uuid", "pattern" = "^([0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[1-8][0-9a-fA-F]{3}-[89abAB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}|00000000-0000-0000-0000-000000000000|ffffffff-ffff-ffff-ffff-ffffffffffff)$"))]
    pub session_id: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum TerminalPhase {
    Connecting,
    Trust,
    Authenticating,
    Opening,
    Running,
    Closed,
    Failed,
    Cancelled,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum TerminalEndKind {
    UserDisconnected,
    Cancelled,
    RemoteExit,
    RemoteClosed,
    TransportError,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TerminalEndReason {
    pub kind: TerminalEndKind,
    pub message: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TerminalSessionView {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_control: Option<vigilshell_domain::agent::WorkspaceAgentControl>,
    #[schemars(extend("format" = "uuid", "pattern" = "^([0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[1-8][0-9a-fA-F]{3}-[89abAB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}|00000000-0000-0000-0000-000000000000|ffffffff-ffff-ffff-ffff-ffffffffffff)$"))]
    pub group_id: String,
    pub terminal: TerminalSettings,
    #[schemars(extend("format" = "uuid", "pattern" = "^([0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[1-8][0-9a-fA-F]{3}-[89abAB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}|00000000-0000-0000-0000-000000000000|ffffffff-ffff-ffff-ffff-ffffffffffff)$"))]
    pub id: String,
    #[schemars(extend("format" = "uuid", "pattern" = "^([0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[1-8][0-9a-fA-F]{3}-[89abAB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}|00000000-0000-0000-0000-000000000000|ffffffff-ffff-ffff-ffff-ffffffffffff)$"))]
    pub operation_id: String,
    #[schemars(extend("format" = "uuid", "pattern" = "^([0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[1-8][0-9a-fA-F]{3}-[89abAB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}|00000000-0000-0000-0000-000000000000|ffffffff-ffff-ffff-ffff-ffffffffffff)$"))]
    pub connection_id: String,

    #[schemars(range(min = 0, max = 9_007_199_254_740_991_u64))]
    #[schemars(extend("maximum" = 9007199254740991_u64, "minimum" = 0))]
    pub connection_revision: u64,
    pub name: String,
    pub host: String,
    #[schemars(extend("maximum" = 9007199254740991_u64, "minimum" = -9007199254740991_i64))]
    pub port: u16,
    pub username: String,
    #[schemars(extend("enum" = ["connecting", "trust", "authenticating", "opening", "running", "closed", "failed", "cancelled"]))]
    pub phase: TerminalPhase,
    #[serde(deserialize_with = "crate::schema::required_nullable")]
    #[schemars(with = "crate::schema::NullableRequired<String>")]
    pub error: Option<String>,
    #[serde(deserialize_with = "crate::schema::required_nullable")]
    #[schemars(with = "crate::schema::NullableRequired<u32>")]
    #[schemars(extend("maximum" = 9007199254740991_u64, "minimum" = -9007199254740991_i64))]
    pub exit_code: Option<u32>,
    pub end_reason: Option<TerminalEndReason>,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "method", content = "params", deny_unknown_fields)]
pub enum WorkspaceCommand {
    #[serde(rename = "files.edit")]
    FileEdit(crate::file_editing::FileEditCommand),
    #[serde(rename = "agent.request")]
    Agent {
        request: vigilshell_domain::agent::AgentOperation,
    },
    #[serde(rename = "tools.subscribe")]
    ToolsSubscribe {
        #[serde(rename = "groupId")]
        #[schemars(extend("format" = "uuid", "pattern" = "^([0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[1-8][0-9a-fA-F]{3}-[89abAB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}|00000000-0000-0000-0000-000000000000|ffffffff-ffff-ffff-ffff-ffffffffffff)$"))]
        group_id: String,
        visible: bool,
    },
    #[serde(rename = "tools.refresh")]
    ToolsRefresh {
        #[serde(rename = "sessionId")]
        #[schemars(extend("format" = "uuid", "pattern" = "^([0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[1-8][0-9a-fA-F]{3}-[89abAB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}|00000000-0000-0000-0000-000000000000|ffffffff-ffff-ffff-ffff-ffffffffffff)$"))]
        session_id: String,
    },
    #[serde(rename = "files.list")]
    FilesList {
        #[serde(rename = "sessionId")]
        #[schemars(extend("format" = "uuid", "pattern" = "^([0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[1-8][0-9a-fA-F]{3}-[89abAB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}|00000000-0000-0000-0000-000000000000|ffffffff-ffff-ffff-ffff-ffffffffffff)$"))]
        session_id: String,
        #[schemars(extend("maxLength" = 16384, "minLength" = 1))]
        path: String,
        #[serde(deserialize_with = "crate::schema::required_nullable")]
        #[schemars(with = "crate::schema::NullableRequired<String>")]
        #[schemars(extend("maxLength" = 16))]
        cursor: Option<String>,
        #[serde(rename = "requestId")]
        #[schemars(extend("format" = "uuid", "pattern" = "^([0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[1-8][0-9a-fA-F]{3}-[89abAB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}|00000000-0000-0000-0000-000000000000|ffffffff-ffff-ffff-ffff-ffffffffffff)$"))]
        request_id: String,
    },
    #[serde(rename = "files.releaseList")]
    FilesReleaseList {
        #[serde(rename = "requestId")]
        #[schemars(extend("format" = "uuid", "pattern" = "^([0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[1-8][0-9a-fA-F]{3}-[89abAB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}|00000000-0000-0000-0000-000000000000|ffffffff-ffff-ffff-ffff-ffffffffffff)$"))]
        request_id: String,
    },
    #[serde(rename = "files.identities")]
    FilesIdentities {
        #[serde(rename = "sessionId")]
        #[schemars(extend("format" = "uuid", "pattern" = "^([0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[1-8][0-9a-fA-F]{3}-[89abAB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}|00000000-0000-0000-0000-000000000000|ffffffff-ffff-ffff-ffff-ffffffffffff)$"))]
        session_id: String,
        #[schemars(extend("maxItems" = 64))]
        uids: Vec<u32>,
        #[schemars(extend("maxItems" = 64))]
        gids: Vec<u32>,
    },
    #[serde(rename = "files.cancelList")]
    FilesCancelList {
        #[serde(rename = "requestId")]
        #[schemars(extend("format" = "uuid", "pattern" = "^([0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[1-8][0-9a-fA-F]{3}-[89abAB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}|00000000-0000-0000-0000-000000000000|ffffffff-ffff-ffff-ffff-ffffffffffff)$"))]
        request_id: String,
    },
    #[serde(rename = "files.stat")]
    FilesStat {
        #[serde(rename = "sessionId")]
        session_id: String,
        #[schemars(extend("maxLength" = 16384, "minLength" = 1))]
        path: String,
    },
    #[serde(rename = "files.create")]
    FilesCreate {
        #[serde(rename = "sessionId")]
        session_id: String,
        #[schemars(extend("maxLength" = 16384, "minLength" = 1))]
        path: String,
    },
    #[serde(rename = "files.startOperation")]
    FilesStartOperation {
        #[serde(rename = "sessionId")]
        session_id: String,
        #[schemars(extend("minItems" = 1, "maxItems" = 100))]
        paths: Vec<String>,
        #[schemars(extend("enum" = ["remove", "permissions"]))]
        operation: String,
        recursive: bool,
        #[serde(rename = "setMask")]
        #[schemars(range(min = 0, max = 511))]
        set_mask: u32,
        #[serde(rename = "clearMask")]
        #[schemars(range(min = 0, max = 511))]
        clear_mask: u32,
    },
    #[serde(rename = "files.operation")]
    FilesOperation {
        id: String,
        #[schemars(range(min = 0, max = 20000))]
        offset: u32,
    },
    #[serde(rename = "files.retryOperation")]
    FilesRetryOperation { id: String },
    #[serde(rename = "files.mkdir")]
    FilesMkdir {
        #[serde(rename = "sessionId")]
        #[schemars(extend("format" = "uuid", "pattern" = "^([0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[1-8][0-9a-fA-F]{3}-[89abAB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}|00000000-0000-0000-0000-000000000000|ffffffff-ffff-ffff-ffff-ffffffffffff)$"))]
        session_id: String,
        #[schemars(extend("maxLength" = 16384, "minLength" = 1))]
        path: String,
    },
    #[serde(rename = "files.rename")]
    FilesRename {
        #[serde(rename = "sessionId")]
        #[schemars(extend("format" = "uuid", "pattern" = "^([0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[1-8][0-9a-fA-F]{3}-[89abAB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}|00000000-0000-0000-0000-000000000000|ffffffff-ffff-ffff-ffff-ffffffffffff)$"))]
        session_id: String,
        #[schemars(extend("maxLength" = 16384, "minLength" = 1))]
        from: String,
        #[schemars(extend("maxLength" = 16384, "minLength" = 1))]
        to: String,
    },
    #[serde(rename = "files.remove")]
    FilesRemove {
        #[serde(rename = "sessionId")]
        #[schemars(extend("format" = "uuid", "pattern" = "^([0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[1-8][0-9a-fA-F]{3}-[89abAB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}|00000000-0000-0000-0000-000000000000|ffffffff-ffff-ffff-ffff-ffffffffffff)$"))]
        session_id: String,
        #[schemars(extend("maxLength" = 16384, "minLength" = 1))]
        path: String,
        directory: bool,
    },
    #[serde(rename = "files.pick")]
    FilesPick {
        #[schemars(extend("enum" = ["upload", "download"]))]
        direction: String,
    },
    #[serde(rename = "files.grant")]
    FilesGrant {
        direction: String,
        paths: Vec<String>,
    },
    #[serde(rename = "files.transfer")]
    FilesTransfer {
        #[serde(rename = "sessionId")]
        #[schemars(extend("format" = "uuid", "pattern" = "^([0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[1-8][0-9a-fA-F]{3}-[89abAB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}|00000000-0000-0000-0000-000000000000|ffffffff-ffff-ffff-ffff-ffffffffffff)$"))]
        session_id: String,
        #[serde(rename = "grantId")]
        #[schemars(extend("format" = "uuid", "pattern" = "^([0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[1-8][0-9a-fA-F]{3}-[89abAB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}|00000000-0000-0000-0000-000000000000|ffffffff-ffff-ffff-ffff-ffffffffffff)$"))]
        grant_id: String,
        #[schemars(extend("maxItems" = 100, "minItems" = 1))]
        #[schemars(extend("items" = {"type":"string","minLength":1,"maxLength":16384,"pattern":"^[^\\u0000]*$"}))]
        paths: Vec<String>,
        overwrite: bool,
    },
    #[serde(rename = "files.archive")]
    FilesArchive {
        #[serde(rename = "sessionId")]
        #[schemars(extend("format" = "uuid", "pattern" = "^([0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[1-8][0-9a-fA-F]{3}-[89abAB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}|00000000-0000-0000-0000-000000000000|ffffffff-ffff-ffff-ffff-ffffffffffff)$"))]
        session_id: String,
        #[serde(rename = "grantId")]
        #[schemars(extend("format" = "uuid", "pattern" = "^([0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[1-8][0-9a-fA-F]{3}-[89abAB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}|00000000-0000-0000-0000-000000000000|ffffffff-ffff-ffff-ffff-ffffffffffff)$"))]
        grant_id: String,
        #[schemars(extend("maxItems" = 100, "minItems" = 1))]
        #[schemars(extend("items" = {"type":"string","minLength":1,"maxLength":16384,"pattern":"^[^\\u0000]*$"}))]
        paths: Vec<String>,
    },
    #[serde(rename = "transfers.items")]
    TransferItems {
        #[schemars(extend("maxLength" = 64, "minLength" = 1))]
        id: String,
        #[schemars(range(min = 0, max = 20000))]
        offset: u32,
    },
    #[serde(rename = "transfers.cancel")]
    TransferCancel {
        #[schemars(extend("format" = "uuid", "pattern" = "^([0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[1-8][0-9a-fA-F]{3}-[89abAB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}|00000000-0000-0000-0000-000000000000|ffffffff-ffff-ffff-ffff-ffffffffffff)$"))]
        id: String,
    },
    #[serde(rename = "transfers.retry")]
    TransferRetry {
        #[schemars(extend("format" = "uuid", "pattern" = "^([0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[1-8][0-9a-fA-F]{3}-[89abAB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}|00000000-0000-0000-0000-000000000000|ffffffff-ffff-ffff-ffff-ffffffffffff)$"))]
        id: String,
    },
    #[serde(rename = "transfers.list")]
    Transfers {
        #[serde(rename = "requestId", default)]
        #[schemars(extend("format" = "uuid", "pattern" = "^([0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[1-8][0-9a-fA-F]{3}-[89abAB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}|00000000-0000-0000-0000-000000000000|ffffffff-ffff-ffff-ffff-ffffffffffff)$"))]
        request_id: Option<String>,
        #[serde(default)]
        #[schemars(extend("maxLength" = 16))]
        cursor: Option<String>,
    },
    #[serde(rename = "transfers.resolveConflict")]
    ResolveConflict {
        #[schemars(extend("format" = "uuid", "pattern" = "^([0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[1-8][0-9a-fA-F]{3}-[89abAB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}|00000000-0000-0000-0000-000000000000|ffffffff-ffff-ffff-ffff-ffffffffffff)$"))]
        id: String,
        #[schemars(extend("maximum" = 9007199254740991_u64, "minimum" = 0))]
        version: u32,
        #[schemars(extend("enum" = ["skip", "overwrite", "cancel"]))]
        decision: String,
        batch: bool,
    },

    #[serde(rename = "proxies.list")]
    ListProxies {
        #[schemars(extend("maximum" = 4294967295_u64, "minimum" = 0))]
        offset: u32,

        #[schemars(range(min = 0, max = 9_007_199_254_740_991_u64))]
        #[serde(deserialize_with = "crate::schema::required_nullable")]
        #[schemars(with = "crate::schema::NullableRequired<u64>")]
        #[schemars(extend("maximum" = 9007199254740991_u64, "minimum" = 0))]
        version: Option<u64>,
    },
    #[serde(rename = "proxies.references")]
    ProxyReferences {
        #[schemars(extend("format" = "uuid", "pattern" = "^([0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[1-8][0-9a-fA-F]{3}-[89abAB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}|00000000-0000-0000-0000-000000000000|ffffffff-ffff-ffff-ffff-ffffffffffff)$"))]
        id: String,
        #[schemars(extend("maximum" = 4294967295_u64, "minimum" = 0))]
        offset: u32,

        #[schemars(range(min = 0, max = 9_007_199_254_740_991_u64))]
        #[serde(deserialize_with = "crate::schema::required_nullable")]
        #[schemars(with = "crate::schema::NullableRequired<u64>")]
        #[schemars(extend("maximum" = 9007199254740991_u64, "minimum" = 0))]
        version: Option<u64>,
    },
    #[serde(rename = "proxies.save")]
    SaveProxy(ProxySave),
    #[serde(rename = "proxies.delete")]
    DeleteProxy(VersionedId),
    #[serde(rename = "proxyCredentialDrafts.begin")]
    BeginProxyDraft {
        #[serde(rename = "proxyId")]
        #[serde(deserialize_with = "crate::schema::required_nullable")]
        #[schemars(with = "crate::schema::NullableRequired<String>")]
        #[schemars(extend("format" = "uuid", "pattern" = "^([0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[1-8][0-9a-fA-F]{3}-[89abAB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}|00000000-0000-0000-0000-000000000000|ffffffff-ffff-ffff-ffff-ffffffffffff)$"))]
        proxy_id: Option<String>,
    },
    #[serde(rename = "tunnels.list")]
    TunnelStatuses {
        #[serde(rename = "connectionId")]
        #[schemars(extend("format" = "uuid", "pattern" = "^([0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[1-8][0-9a-fA-F]{3}-[89abAB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}|00000000-0000-0000-0000-000000000000|ffffffff-ffff-ffff-ffff-ffffffffffff)$"))]
        connection_id: String,
    },
    #[serde(rename = "tunnels.retry")]
    RetryTunnel {
        #[serde(rename = "connectionId")]
        #[schemars(extend("format" = "uuid", "pattern" = "^([0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[1-8][0-9a-fA-F]{3}-[89abAB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}|00000000-0000-0000-0000-000000000000|ffffffff-ffff-ffff-ffff-ffffffffffff)$"))]
        connection_id: String,
        #[serde(rename = "ruleId")]
        #[schemars(extend("format" = "uuid", "pattern" = "^([0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[1-8][0-9a-fA-F]{3}-[89abAB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}|00000000-0000-0000-0000-000000000000|ffffffff-ffff-ffff-ffff-ffffffffffff)$"))]
        rule_id: String,
    },
    #[serde(rename = "terminals.validateInput")]
    ValidateInput {
        #[serde(rename = "sessionId")]
        #[schemars(extend("format" = "uuid", "pattern" = "^([0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[1-8][0-9a-fA-F]{3}-[89abAB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}|00000000-0000-0000-0000-000000000000|ffffffff-ffff-ffff-ffff-ffffffffffff)$"))]
        session_id: String,
        #[schemars(extend("maxLength" = 8192))]
        text: String,
    },

    #[serde(rename = "groups.updateLayout")]
    UpdateLayout(GroupLayout),
    #[serde(rename = "workspace.negotiate")]
    Negotiate {
        #[schemars(extend("enum" = [WORKSPACE_PROTOCOL_VERSION]))]
        version: u8,
    },
    #[serde(rename = "workspace.context")]
    Context {
        #[schemars(extend("enum" = ["en", "zh-CN"]))]
        locale: String,
        #[schemars(extend("enum" = ["light", "dark"]))]
        theme: String,
    },
    #[serde(rename = "workspace.detach")]
    Detach {},
    #[serde(rename = "connections.unlock")]
    UnlockConnections {},
    #[serde(rename = "connections.initialize")]
    InitializeConnections {},
    #[serde(rename = "catalogue.list")]
    List(CatalogueQuery),
    #[serde(rename = "groups.create")]
    CreateGroup {
        #[schemars(extend("minLength" = 1))]
        name: String,
    },
    #[serde(rename = "groups.update")]
    UpdateGroup {
        #[schemars(extend("format" = "uuid", "pattern" = "^([0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[1-8][0-9a-fA-F]{3}-[89abAB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}|00000000-0000-0000-0000-000000000000|ffffffff-ffff-ffff-ffff-ffffffffffff)$"))]
        id: String,

        #[schemars(range(min = 0, max = 9_007_199_254_740_991_u64))]
        #[schemars(extend("maximum" = 9007199254740991_u64, "minimum" = 0))]
        version: u64,
        #[schemars(extend("minLength" = 1))]
        name: String,
    },
    #[serde(rename = "groups.delete")]
    DeleteGroup(VersionedId),
    #[serde(rename = "connections.create")]
    Create(ConnectionSave),
    #[serde(rename = "connections.update")]
    Update(ConnectionSave),
    #[serde(rename = "connections.get")]
    Get {
        #[schemars(extend("format" = "uuid", "pattern" = "^([0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[1-8][0-9a-fA-F]{3}-[89abAB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}|00000000-0000-0000-0000-000000000000|ffffffff-ffff-ffff-ffff-ffffffffffff)$"))]
        id: String,
    },
    #[serde(rename = "connections.delete")]
    Delete {
        #[schemars(extend("format" = "uuid", "pattern" = "^([0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[1-8][0-9a-fA-F]{3}-[89abAB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}|00000000-0000-0000-0000-000000000000|ffffffff-ffff-ffff-ffff-ffffffffffff)$"))]
        id: String,

        #[schemars(range(min = 0, max = 9_007_199_254_740_991_u64))]
        #[schemars(extend("maximum" = 9007199254740991_u64, "minimum" = 0))]
        version: u64,
        #[serde(rename = "disconnectSessions")]
        disconnect_sessions: bool,
        #[serde(rename = "sessionIds")]
        #[schemars(extend("maxItems" = 32))]
        session_ids: Vec<String>,
    },
    #[serde(rename = "credentials.reconcile")]
    ReconcileCredentials {
        #[schemars(extend("format" = "uuid", "pattern" = "^([0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[1-8][0-9a-fA-F]{3}-[89abAB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}|00000000-0000-0000-0000-000000000000|ffffffff-ffff-ffff-ffff-ffffffffffff)$"))]
        id: String,
    },
    #[serde(rename = "credentialDrafts.begin")]
    BeginDraft {
        #[serde(rename = "connectionId")]
        #[serde(deserialize_with = "crate::schema::required_nullable")]
        #[schemars(with = "crate::schema::NullableRequired<String>")]
        #[schemars(extend("format" = "uuid", "pattern" = "^([0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[1-8][0-9a-fA-F]{3}-[89abAB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}|00000000-0000-0000-0000-000000000000|ffffffff-ffff-ffff-ffff-ffffffffffff)$"))]
        connection_id: Option<String>,
    },
    #[serde(rename = "credentialDrafts.configure")]
    ConfigureDraft {
        #[serde(rename = "draftId")]
        #[schemars(extend("format" = "uuid", "pattern" = "^([0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[1-8][0-9a-fA-F]{3}-[89abAB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}|00000000-0000-0000-0000-000000000000|ffffffff-ffff-ffff-ffff-ffffffffffff)$"))]
        draft_id: String,
        #[schemars(extend("enum" = ["password", "privateKey"]))]
        auth: AuthenticationMethod,
    },
    #[serde(rename = "credentialDrafts.clear")]
    ClearDraft {
        #[serde(rename = "draftId")]
        #[schemars(extend("format" = "uuid", "pattern" = "^([0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[1-8][0-9a-fA-F]{3}-[89abAB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}|00000000-0000-0000-0000-000000000000|ffffffff-ffff-ffff-ffff-ffffffffffff)$"))]
        draft_id: String,
    },
    #[serde(rename = "credentialDrafts.discard")]
    DiscardDraft {
        #[serde(rename = "draftId")]
        #[schemars(extend("format" = "uuid", "pattern" = "^([0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[1-8][0-9a-fA-F]{3}-[89abAB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}|00000000-0000-0000-0000-000000000000|ffffffff-ffff-ffff-ffff-ffffffffffff)$"))]
        draft_id: String,
    },
    #[serde(rename = "connections.test")]
    Test(ConnectionTest),
    #[serde(rename = "hostTrust.confirm")]
    ConfirmTrust {
        #[serde(rename = "challengeId")]
        #[schemars(extend("format" = "uuid", "pattern" = "^([0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[1-8][0-9a-fA-F]{3}-[89abAB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}|00000000-0000-0000-0000-000000000000|ffffffff-ffff-ffff-ffff-ffffffffffff)$"))]
        challenge_id: String,
    },
    #[serde(rename = "hostTrust.reject")]
    RejectTrust {
        #[serde(rename = "challengeId")]
        #[schemars(extend("format" = "uuid", "pattern" = "^([0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[1-8][0-9a-fA-F]{3}-[89abAB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}|00000000-0000-0000-0000-000000000000|ffffffff-ffff-ffff-ffff-ffffffffffff)$"))]
        challenge_id: String,
    },
    #[serde(rename = "hostTrust.forget")]
    ForgetTrust(VersionedId),
    #[serde(rename = "terminals.open")]
    Open {
        #[serde(rename = "connectionId")]
        #[schemars(extend("format" = "uuid", "pattern" = "^([0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[1-8][0-9a-fA-F]{3}-[89abAB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}|00000000-0000-0000-0000-000000000000|ffffffff-ffff-ffff-ffff-ffffffffffff)$"))]
        connection_id: String,
        #[schemars(extend("maximum" = 65535, "minimum" = 1))]
        columns: u16,
        #[schemars(extend("maximum" = 65535, "minimum" = 1))]
        rows: u16,
    },
    #[serde(rename = "terminals.list")]
    Sessions {},
    #[serde(rename = "terminals.resize")]
    Resize {
        #[serde(rename = "sessionId")]
        #[schemars(extend("format" = "uuid", "pattern" = "^([0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[1-8][0-9a-fA-F]{3}-[89abAB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}|00000000-0000-0000-0000-000000000000|ffffffff-ffff-ffff-ffff-ffffffffffff)$"))]
        session_id: String,
        #[schemars(extend("maximum" = 65535, "minimum" = 1))]
        columns: u16,
        #[schemars(extend("maximum" = 65535, "minimum" = 1))]
        rows: u16,
    },
    #[serde(rename = "terminals.close")]
    Close {
        #[serde(rename = "sessionId")]
        #[schemars(extend("format" = "uuid", "pattern" = "^([0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[1-8][0-9a-fA-F]{3}-[89abAB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}|00000000-0000-0000-0000-000000000000|ffffffff-ffff-ffff-ffff-ffffffffffff)$"))]
        session_id: String,
    },
    #[serde(rename = "operations.get")]
    Operation {
        #[schemars(extend("format" = "uuid", "pattern" = "^([0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[1-8][0-9a-fA-F]{3}-[89abAB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}|00000000-0000-0000-0000-000000000000|ffffffff-ffff-ffff-ffff-ffffffffffff)$"))]
        id: String,
    },
    #[serde(rename = "operations.cancel")]
    Cancel {
        #[schemars(extend("format" = "uuid", "pattern" = "^([0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[1-8][0-9a-fA-F]{3}-[89abAB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}|00000000-0000-0000-0000-000000000000|ffffffff-ffff-ffff-ffff-ffffffffffff)$"))]
        id: String,
    },
    #[serde(rename = "connections.reconcile")]
    Reconcile {
        #[serde(rename = "requestId")]
        #[schemars(extend("maxLength" = 128, "minLength" = 1))]
        request_id: String,
        #[serde(rename = "runtimeId")]
        #[schemars(extend("maxLength" = 128, "minLength" = 1))]
        runtime_id: String,
    },
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct WorkspaceRequest {
    pub jsonrpc: String,
    pub id: String,
    pub owner: String,
    #[serde(flatten)]
    pub command: WorkspaceCommand,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
#[schemars(extend("x-vigil-rule" = "workspaceReply"))]
pub enum WorkspaceReply {
    ConnectionAccess {
        state: domain::ConnectionAccessState,
    },
    FileEdit {
        edit: crate::file_editing::FileEditRemote,
    },
    FileEntry {
        entry: crate::tools::RemoteEntry,
    },
    FileOperation {
        page: crate::tools::FileOperationPage,
    },
    Agent {
        result: Box<vigilshell_domain::agent::AgentReply>,
    },
    ToolSnapshot {
        snapshot: crate::tools::ToolSnapshot,
    },
    Directory {
        page: crate::tools::DirectoryPage,
    },
    FileGrant {
        grant: crate::tools::FileGrantView,
    },
    TransferItems {
        page: crate::tools::TransferItemsPage,
    },
    Transfers {
        #[schemars(extend("maxItems" = 1000))]
        tasks: Vec<crate::tools::TransferView>,
    },
    TransferPage {
        #[serde(rename = "requestId")]
        #[schemars(extend("format" = "uuid", "pattern" = "^([0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[1-8][0-9a-fA-F]{3}-[89abAB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}|00000000-0000-0000-0000-000000000000|ffffffff-ffff-ffff-ffff-ffffffffffff)$"))]
        request_id: String,
        #[schemars(extend("maxItems" = 100))]
        tasks: Vec<crate::tools::TransferView>,
        #[serde(deserialize_with = "crate::schema::required_nullable")]
        #[schemars(with = "crate::schema::NullableRequired<String>")]
        #[schemars(extend("maxLength" = 16))]
        next: Option<String>,
    },
    Identities {
        #[schemars(extend("maxItems" = 64))]
        names: Vec<crate::tools::IdentityName>,
    },
    Proxies {
        #[schemars(extend("maxItems" = 50))]
        proxies: Vec<ProxyView>,

        #[schemars(range(min = 0, max = 9_007_199_254_740_991_u64))]
        #[schemars(extend("maximum" = 9007199254740991_u64, "minimum" = 0))]
        version: u64,
        #[serde(deserialize_with = "crate::schema::required_nullable")]
        #[schemars(with = "crate::schema::NullableRequired<u32>")]
        #[schemars(extend("maximum" = 9007199254740991_u64, "minimum" = 0))]
        next: Option<u32>,
    },
    ProxyReferences {
        #[schemars(extend("maxItems" = 50))]
        connection_names: Vec<String>,

        #[schemars(range(min = 0, max = 9_007_199_254_740_991_u64))]
        #[schemars(extend("maximum" = 9007199254740991_u64, "minimum" = 0))]
        version: u64,
        #[serde(deserialize_with = "crate::schema::required_nullable")]
        #[schemars(with = "crate::schema::NullableRequired<u32>")]
        #[schemars(extend("maximum" = 9007199254740991_u64, "minimum" = 0))]
        next: Option<u32>,
    },
    Tunnels {
        #[schemars(extend("maxItems" = 32))]
        states: Vec<TunnelStatus>,
    },
    Ready {
        #[serde(rename = "connectionAccess")]
        connection_access: domain::ConnectionAccessState,
        #[schemars(extend("enum" = [WORKSPACE_PROTOCOL_VERSION]))]
        version: u8,
    },
    Catalogue {
        page: CataloguePage,
    },
    Group {
        group: ConnectionGroupView,
    },
    Layout {
        #[schemars(range(min = 0, max = 9_007_199_254_740_991_u64))]
        #[schemars(extend("maximum" = 9007199254740991_u64, "minimum" = 0))]
        revision: u64,
    },
    Connection {
        connection: Box<ConnectionDetails>,
        #[serde(deserialize_with = "crate::schema::required_nullable")]
        #[schemars(with = "crate::schema::NullableRequired<TrustView>")]
        trust: Option<TrustView>,
    },
    Draft {
        draft: CredentialDraftView,
    },
    Operation {
        operation: Box<WorkspaceOperation>,
    },
    Sessions {
        #[schemars(extend("maxItems" = 48))]
        sessions: Vec<TerminalSessionView>,
    },
    Done {},
    Reconciled {
        #[serde(deserialize_with = "crate::schema::required_nullable")]
        #[schemars(with = "crate::schema::NullableRequired<Box<WorkspaceReply>>")]
        result: Option<Box<WorkspaceReply>>,
        #[serde(deserialize_with = "crate::schema::required_nullable")]
        #[schemars(with = "crate::schema::NullableRequired<String>")]
        error: Option<String>,
    },
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum WorkspaceEvent {
    ConnectionAccess {
        state: domain::ConnectionAccessState,
    },
    ToolSnapshot {
        snapshot: crate::tools::ToolSnapshot,
    },
    Transfer {
        task: crate::tools::TransferView,
    },
    InputRejected {
        #[serde(rename = "sessionId")]
        #[schemars(extend("format" = "uuid", "pattern" = "^([0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[1-8][0-9a-fA-F]{3}-[89abAB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}|00000000-0000-0000-0000-000000000000|ffffffff-ffff-ffff-ffff-ffffffffffff)$"))]
        session_id: String,
        code: String,
    },
    Tunnels {
        #[serde(rename = "connectionId")]
        #[schemars(extend("format" = "uuid", "pattern" = "^([0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[1-8][0-9a-fA-F]{3}-[89abAB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}|00000000-0000-0000-0000-000000000000|ffffffff-ffff-ffff-ffff-ffffffffffff)$"))]
        connection_id: String,
        #[schemars(extend("maxItems" = 32))]
        states: Vec<TunnelStatus>,
    },
    Catalogue {},
    Operation {
        operation: Box<WorkspaceOperation>,
    },
    Session {
        session: Box<TerminalSessionView>,
    },
    Detached {},
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkspaceNotificationParams {
    #[schemars(extend("minLength" = 1))]
    pub runtime_id: String,
    pub owner: String,
    pub event: WorkspaceEvent,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceNotification {
    #[schemars(extend("enum" = ["2.0"]))]
    pub jsonrpc: String,
    #[schemars(extend("enum" = ["workspace.changed"]))]
    pub method: String,
    pub params: WorkspaceNotificationParams,
}
#[derive(JsonSchema)]
pub struct WorkspaceContract {
    pub connection_access_snapshot: domain::ConnectionAccessSnapshot,
    pub command: WorkspaceCommand,
    pub request: WorkspaceRequest,
    pub reply: WorkspaceReply,
    pub notification: WorkspaceNotification,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shared_workspace_examples_reject_secret_and_unknown_fields() {
        let examples: serde_json::Value = serde_json::from_str(include_str!(
            "../../../tests/fixtures/workspace-protocol-v16.json"
        ))
        .unwrap();
        for value in examples["valid"].as_array().unwrap() {
            assert!(serde_json::from_value::<WorkspaceCommand>(value.clone()).is_ok());
        }
        for value in examples["invalid"].as_array().unwrap() {
            assert!(serde_json::from_value::<WorkspaceCommand>(value.clone()).is_err());
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FileOperationSummary {
    pub connection_id: String,
    pub attempt: u32,
    pub elapsed_ms: u64,
    pub completed: u32,
    pub failed: u32,
    pub skipped: u32,
}
