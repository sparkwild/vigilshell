use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "action", rename_all = "camelCase", deny_unknown_fields)]
pub enum FileEditCommand {
    #[serde(rename_all = "camelCase")]
    Open {
        id: String,
        lease_owner: String,
        session_id: String,
        path: String,
        local: String,
    },
    Inspect {
        id: String,
    },
    Upload {
        id: String,
        local: String,
        expected: RemoteFileVersion,
        destination: Option<String>,
    },
    Reload {
        id: String,
        local: String,
    },
    Close {
        id: String,
    },
}
impl From<FileEditCommand> for vigilshell_application_contracts::file_editing::FileEditCommand {
    fn from(value: FileEditCommand) -> Self {
        match value {
            FileEditCommand::Open {
                id,
                lease_owner,
                session_id,
                path,
                local,
            } => Self::Open {
                id,
                lease_owner,
                session: session_id,
                path,
                local: local.into(),
            },
            FileEditCommand::Inspect { id } => Self::Inspect { id },
            FileEditCommand::Upload {
                id,
                local,
                expected,
                destination,
            } => Self::Upload {
                id,
                local: local.into(),
                expected: expected.into(),
                destination,
            },
            FileEditCommand::Reload { id, local } => Self::Reload {
                id,
                local: local.into(),
            },
            FileEditCommand::Close { id } => Self::Close { id },
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FileEditRemote {
    pub id: String,
    pub connection_id: String,
    pub group_id: String,
    pub path: String,
    pub version: RemoteFileVersion,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RemoteFileVersion {
    pub hash: String,
    pub size: u64,
    pub modified: u64,
    pub permissions: u32,
    pub uid: u32,
    pub gid: u32,
}
impl From<vigilshell_domain::tools::RemoteFileVersion> for RemoteFileVersion {
    fn from(v: vigilshell_domain::tools::RemoteFileVersion) -> Self {
        Self {
            hash: v.hash,
            size: v.size,
            modified: v.modified,
            permissions: v.permissions,
            uid: v.uid,
            gid: v.gid,
        }
    }
}
impl From<vigilshell_domain::tools::FileEditRemote> for FileEditRemote {
    fn from(v: vigilshell_domain::tools::FileEditRemote) -> Self {
        Self {
            id: v.id,
            connection_id: v.connection_id,
            group_id: v.group_id,
            path: v.path,
            version: v.version.into(),
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "action", rename_all = "camelCase", deny_unknown_fields)]
pub enum FileEditorRequest {
    List {},
    Show {},
    ChooseEditor {},
    #[serde(rename_all = "camelCase")]
    Open {
        session_id: String,
        path: String,
        alternative: bool,
    },
    Reopen {
        id: String,
    },
    Retry {
        id: String,
    },
    #[serde(rename_all = "camelCase")]
    Recover {
        id: String,
        session_id: String,
    },
    Resolve {
        id: String,
        choice: String,
        destination: Option<String>,
    },
    Export {
        id: String,
    },
    End {
        id: String,
        discard: bool,
    },
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FileEditorView {
    pub id: String,
    pub connection_id: String,
    pub path: String,
    pub application: String,
    pub phase: String,
    pub error: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FileEditorReply {
    pub ok: bool,
    pub sessions: Vec<FileEditorView>,
    pub error: Option<String>,
}
#[derive(JsonSchema)]
pub struct FileEditorContract {
    pub request: FileEditorRequest,
    pub reply: FileEditorReply,
}

impl From<RemoteFileVersion> for vigilshell_domain::tools::RemoteFileVersion {
    fn from(v: RemoteFileVersion) -> Self {
        Self {
            hash: v.hash,
            size: v.size,
            modified: v.modified,
            permissions: v.permissions,
            uid: v.uid,
            gid: v.gid,
        }
    }
}
