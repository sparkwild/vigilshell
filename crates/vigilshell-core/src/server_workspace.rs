use std::{error::Error, fmt};

use crate::connection::{
    ConnectionDisplayName, ConnectionProfile, ConnectionProfileId, ConnectionRecordVersion,
    ConnectionSecurityRevision, ConnectionValueError, CredentialRef, ManagedServerId, SshRoute,
};

const MAXIMUM_DISPLAY_NAME_BYTES: usize = 128;

/// Project 与 Managed Server 值对象的校验失败。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ServerWorkspaceValueError {
    Empty(&'static str),
    TooLong {
        field: &'static str,
        maximum_bytes: usize,
    },
    InvalidRecordVersion(&'static str),
}

impl fmt::Display for ServerWorkspaceValueError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty(field) => write!(formatter, "{field} must not be empty"),
            Self::TooLong {
                field,
                maximum_bytes,
            } => write!(
                formatter,
                "{field} must not exceed {maximum_bytes} UTF-8 bytes"
            ),
            Self::InvalidRecordVersion(field) => {
                write!(formatter, "{field} must not be zero")
            }
        }
    }
}

impl Error for ServerWorkspaceValueError {}

fn normalized_display_name(
    value: impl Into<String>,
    field: &'static str,
) -> Result<String, ServerWorkspaceValueError> {
    let value = value.into();
    let value = value.trim();
    if value.is_empty() {
        return Err(ServerWorkspaceValueError::Empty(field));
    }
    if value.len() > MAXIMUM_DISPLAY_NAME_BYTES {
        return Err(ServerWorkspaceValueError::TooLong {
            field,
            maximum_bytes: MAXIMUM_DISPLAY_NAME_BYTES,
        });
    }
    Ok(value.to_owned())
}

/// Project 的可信本地身份。
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ProjectId(String);

impl ProjectId {
    pub fn parse(value: impl Into<String>) -> Result<Self, ServerWorkspaceValueError> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err(ServerWorkspaceValueError::Empty("project id"));
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Human 可见且经过规范化的 Project 名称。
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ProjectDisplayName(String);

impl ProjectDisplayName {
    pub fn parse(value: impl Into<String>) -> Result<Self, ServerWorkspaceValueError> {
        normalized_display_name(value, "project display name").map(Self)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Human 可见且经过规范化的 Managed Server 名称。
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ManagedServerDisplayName(String);

impl ManagedServerDisplayName {
    pub fn parse(value: impl Into<String>) -> Result<Self, ServerWorkspaceValueError> {
        normalized_display_name(value, "managed server display name").map(Self)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

macro_rules! record_version {
    ($name:ident, $field:literal) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub struct $name(u64);

        impl $name {
            pub const INITIAL: Self = Self(1);

            pub fn new(value: u64) -> Result<Self, ServerWorkspaceValueError> {
                if value == 0 {
                    return Err(ServerWorkspaceValueError::InvalidRecordVersion($field));
                }
                Ok(Self(value))
            }

            pub const fn get(self) -> u64 {
                self.0
            }

            pub const fn checked_next(self) -> Option<Self> {
                match self.0.checked_add(1) {
                    Some(value) => Some(Self(value)),
                    None => None,
                }
            }
        }
    };
}

record_version!(ProjectRecordVersion, "project record version");
record_version!(ManagedServerRecordVersion, "managed server record version");

/// Human 导航使用的 Project 记录。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Project {
    id: ProjectId,
    display_name: ProjectDisplayName,
    record_version: ProjectRecordVersion,
}

impl Project {
    pub const fn new(
        id: ProjectId,
        display_name: ProjectDisplayName,
        record_version: ProjectRecordVersion,
    ) -> Self {
        Self {
            id,
            display_name,
            record_version,
        }
    }

    pub const fn id(&self) -> &ProjectId {
        &self.id
    }

    pub const fn display_name(&self) -> &ProjectDisplayName {
        &self.display_name
    }

    pub const fn record_version(&self) -> ProjectRecordVersion {
        self.record_version
    }
}

/// 独立稳定的 Managed Server 记录。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ManagedServer {
    id: ManagedServerId,
    project_id: Option<ProjectId>,
    display_name: ManagedServerDisplayName,
    record_version: ManagedServerRecordVersion,
}

impl ManagedServer {
    pub const fn new(
        id: ManagedServerId,
        project_id: Option<ProjectId>,
        display_name: ManagedServerDisplayName,
        record_version: ManagedServerRecordVersion,
    ) -> Self {
        Self {
            id,
            project_id,
            display_name,
            record_version,
        }
    }

    pub const fn id(&self) -> &ManagedServerId {
        &self.id
    }

    pub const fn project_id(&self) -> Option<&ProjectId> {
        self.project_id.as_ref()
    }

    pub const fn display_name(&self) -> &ManagedServerDisplayName {
        &self.display_name
    }

    pub const fn record_version(&self) -> ManagedServerRecordVersion {
        self.record_version
    }
}

/// Server Workspace repository 可公开的稳定失败。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ServerWorkspaceRepositoryError {
    Unavailable,
}

impl fmt::Display for ServerWorkspaceRepositoryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("server workspace repository is unavailable")
    }
}

impl Error for ServerWorkspaceRepositoryError {}

/// Project 与 Managed Server 的只读持久化端口。
pub trait ServerWorkspaceReadRepository {
    fn find_project(
        &self,
        project_id: &ProjectId,
    ) -> Result<Option<Project>, ServerWorkspaceRepositoryError>;

    fn find_server(
        &self,
        server_id: &ManagedServerId,
    ) -> Result<Option<ManagedServer>, ServerWorkspaceRepositoryError>;

    fn list_projects(&self) -> Result<Vec<Project>, ServerWorkspaceRepositoryError>;

    fn list_servers(&self) -> Result<Vec<ManagedServer>, ServerWorkspaceRepositoryError>;

    fn list_connection_profiles(
        &self,
        server_id: &ManagedServerId,
    ) -> Result<Vec<ConnectionProfile>, ServerWorkspaceRepositoryError>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CreateProjectResult {
    Created,
    Conflict,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReplaceProjectResult {
    Replaced,
    Conflict,
    NotFound,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeleteProjectResult {
    Deleted { unassigned_servers: u64 },
    Conflict,
    NotFound,
    VersionExhausted,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CreateServerWithConnectionResult {
    Created,
    ProjectNotFound,
    ServerConflict,
    ProfileConflict,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AddConnectionToServerResult {
    Created,
    ServerNotFound,
    ProfileConflict,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReplaceManagedServerResult {
    Replaced,
    ProjectNotFound,
    Conflict,
    NotFound,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeleteManagedServerResult {
    Deleted,
    NotEmpty,
    Conflict,
    NotFound,
}

/// Project、Managed Server 与 Profile 关联的原子持久化端口。
pub trait ServerWorkspaceMutationRepository {
    fn create_project(
        &self,
        project: &Project,
    ) -> Result<CreateProjectResult, ServerWorkspaceRepositoryError>;

    fn replace_project_if_version(
        &self,
        project: &Project,
        expected_record_version: ProjectRecordVersion,
    ) -> Result<ReplaceProjectResult, ServerWorkspaceRepositoryError>;

    fn delete_project_and_unassign_servers(
        &self,
        project_id: &ProjectId,
        expected_record_version: ProjectRecordVersion,
    ) -> Result<DeleteProjectResult, ServerWorkspaceRepositoryError>;

    fn create_server_with_connection(
        &self,
        server: &ManagedServer,
        profile: &ConnectionProfile,
    ) -> Result<CreateServerWithConnectionResult, ServerWorkspaceRepositoryError>;

    fn add_connection_to_server(
        &self,
        profile: &ConnectionProfile,
    ) -> Result<AddConnectionToServerResult, ServerWorkspaceRepositoryError>;

    fn replace_server_if_version(
        &self,
        server: &ManagedServer,
        expected_record_version: ManagedServerRecordVersion,
    ) -> Result<ReplaceManagedServerResult, ServerWorkspaceRepositoryError>;

    fn delete_server_if_empty(
        &self,
        server_id: &ManagedServerId,
        expected_record_version: ManagedServerRecordVersion,
    ) -> Result<DeleteManagedServerResult, ServerWorkspaceRepositoryError>;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CreateProjectRequest {
    project_id: ProjectId,
    display_name: ProjectDisplayName,
}

impl CreateProjectRequest {
    pub fn new(
        project_id: ProjectId,
        display_name: impl Into<String>,
    ) -> Result<Self, ServerWorkspaceValueError> {
        Ok(Self {
            project_id,
            display_name: ProjectDisplayName::parse(display_name)?,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UpdateProjectRequest {
    project_id: ProjectId,
    expected_record_version: ProjectRecordVersion,
    display_name: ProjectDisplayName,
}

impl UpdateProjectRequest {
    pub fn new(
        project_id: ProjectId,
        expected_record_version: ProjectRecordVersion,
        display_name: impl Into<String>,
    ) -> Result<Self, ServerWorkspaceValueError> {
        Ok(Self {
            project_id,
            expected_record_version,
            display_name: ProjectDisplayName::parse(display_name)?,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeleteProjectRequest {
    project_id: ProjectId,
    expected_record_version: ProjectRecordVersion,
}

impl DeleteProjectRequest {
    pub const fn new(project_id: ProjectId, expected_record_version: ProjectRecordVersion) -> Self {
        Self {
            project_id,
            expected_record_version,
        }
    }
}

/// 同时创建 Server 与首条 Connection 的可信请求。
#[derive(Clone, PartialEq, Eq)]
pub struct CreateServerWithConnectionRequest {
    server_id: ManagedServerId,
    project_id: Option<ProjectId>,
    server_display_name: ManagedServerDisplayName,
    profile_id: ConnectionProfileId,
    credential_ref: CredentialRef,
    connection_display_name: ConnectionDisplayName,
    route: SshRoute,
    enabled: bool,
}

impl CreateServerWithConnectionRequest {
    #[allow(
        clippy::too_many_arguments,
        reason = "the request keeps trusted identities and the complete initial profile explicit"
    )]
    pub fn new(
        server_id: ManagedServerId,
        project_id: Option<ProjectId>,
        server_display_name: impl Into<String>,
        profile_id: ConnectionProfileId,
        credential_ref: CredentialRef,
        connection_display_name: impl Into<String>,
        host: impl Into<String>,
        port: u16,
        username: impl Into<String>,
        enabled: bool,
    ) -> Result<Self, ServerWorkspaceRequestError> {
        Ok(Self {
            server_id,
            project_id,
            server_display_name: ManagedServerDisplayName::parse(server_display_name)?,
            profile_id,
            credential_ref,
            connection_display_name: ConnectionDisplayName::parse(connection_display_name)?,
            route: SshRoute::new(host, port, username)?,
            enabled,
        })
    }
}

impl fmt::Debug for CreateServerWithConnectionRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CreateServerWithConnectionRequest")
            .field("server_id", &self.server_id)
            .field("project_id", &self.project_id)
            .field("server_display_name", &self.server_display_name)
            .field("profile_id", &self.profile_id)
            .field("credential_ref", &"[opaque]")
            .field("connection_display_name", &self.connection_display_name)
            .field("route", &"[redacted]")
            .field("enabled", &self.enabled)
            .finish()
    }
}

/// 向既有 Server 添加 Connection 的可信请求。
#[derive(Clone, PartialEq, Eq)]
pub struct AddConnectionToServerRequest {
    server_id: ManagedServerId,
    profile_id: ConnectionProfileId,
    credential_ref: CredentialRef,
    display_name: ConnectionDisplayName,
    route: SshRoute,
    enabled: bool,
}

impl AddConnectionToServerRequest {
    #[allow(
        clippy::too_many_arguments,
        reason = "the request keeps the selected server and complete new profile explicit"
    )]
    pub fn new(
        server_id: ManagedServerId,
        profile_id: ConnectionProfileId,
        credential_ref: CredentialRef,
        display_name: impl Into<String>,
        host: impl Into<String>,
        port: u16,
        username: impl Into<String>,
        enabled: bool,
    ) -> Result<Self, ServerWorkspaceRequestError> {
        Ok(Self {
            server_id,
            profile_id,
            credential_ref,
            display_name: ConnectionDisplayName::parse(display_name)?,
            route: SshRoute::new(host, port, username)?,
            enabled,
        })
    }
}

impl fmt::Debug for AddConnectionToServerRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AddConnectionToServerRequest")
            .field("server_id", &self.server_id)
            .field("profile_id", &self.profile_id)
            .field("credential_ref", &"[opaque]")
            .field("display_name", &self.display_name)
            .field("route", &"[redacted]")
            .field("enabled", &self.enabled)
            .finish()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UpdateManagedServerRequest {
    server_id: ManagedServerId,
    expected_record_version: ManagedServerRecordVersion,
    project_id: Option<ProjectId>,
    display_name: ManagedServerDisplayName,
}

impl UpdateManagedServerRequest {
    pub fn new(
        server_id: ManagedServerId,
        expected_record_version: ManagedServerRecordVersion,
        project_id: Option<ProjectId>,
        display_name: impl Into<String>,
    ) -> Result<Self, ServerWorkspaceValueError> {
        Ok(Self {
            server_id,
            expected_record_version,
            project_id,
            display_name: ManagedServerDisplayName::parse(display_name)?,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeleteManagedServerRequest {
    server_id: ManagedServerId,
    expected_record_version: ManagedServerRecordVersion,
}

impl DeleteManagedServerRequest {
    pub const fn new(
        server_id: ManagedServerId,
        expected_record_version: ManagedServerRecordVersion,
    ) -> Self {
        Self {
            server_id,
            expected_record_version,
        }
    }
}

/// Server+Connection 原子创建的成功结果。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CreatedServerWithConnection {
    server: ManagedServer,
    connection: ConnectionProfile,
}

impl CreatedServerWithConnection {
    pub const fn server(&self) -> &ManagedServer {
        &self.server
    }

    pub const fn connection(&self) -> &ConnectionProfile {
        &self.connection
    }
}

/// 组合请求构造时的校验失败。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ServerWorkspaceRequestError {
    Workspace(ServerWorkspaceValueError),
    Connection(ConnectionValueError),
}

impl fmt::Display for ServerWorkspaceRequestError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Workspace(error) => error.fmt(formatter),
            Self::Connection(error) => error.fmt(formatter),
        }
    }
}

impl Error for ServerWorkspaceRequestError {}

impl From<ServerWorkspaceValueError> for ServerWorkspaceRequestError {
    fn from(error: ServerWorkspaceValueError) -> Self {
        Self::Workspace(error)
    }
}

impl From<ConnectionValueError> for ServerWorkspaceRequestError {
    fn from(error: ConnectionValueError) -> Self {
        Self::Connection(error)
    }
}

/// Server Workspace 应用服务可公开的稳定失败。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MutateServerWorkspaceError {
    RepositoryUnavailable,
    Conflict,
    ProjectNotFound,
    ServerNotFound,
    RecordMismatch,
    VersionExhausted,
    ServerNotEmpty,
}

impl fmt::Display for MutateServerWorkspaceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::RepositoryUnavailable => "server workspace repository is unavailable",
            Self::Conflict => "server workspace record conflicts with current state",
            Self::ProjectNotFound => "project was not found",
            Self::ServerNotFound => "managed server was not found",
            Self::RecordMismatch => "server workspace repository returned a mismatched record",
            Self::VersionExhausted => "server workspace record version is exhausted",
            Self::ServerNotEmpty => "managed server still has connection profiles",
        };
        formatter.write_str(message)
    }
}

impl Error for MutateServerWorkspaceError {}

/// 执行 Core-owned Project 与 Managed Server mutation 语义。
pub struct ServerWorkspaceService<'a, Repository> {
    repository: &'a Repository,
}

impl<'a, Repository> ServerWorkspaceService<'a, Repository>
where
    Repository: ServerWorkspaceReadRepository + ServerWorkspaceMutationRepository,
{
    pub const fn new(repository: &'a Repository) -> Self {
        Self { repository }
    }

    pub fn create_project(
        &self,
        request: CreateProjectRequest,
    ) -> Result<Project, MutateServerWorkspaceError> {
        let project = Project::new(
            request.project_id,
            request.display_name,
            ProjectRecordVersion::INITIAL,
        );
        match self.repository.create_project(&project) {
            Ok(CreateProjectResult::Created) => Ok(project),
            Ok(CreateProjectResult::Conflict) => Err(MutateServerWorkspaceError::Conflict),
            Err(ServerWorkspaceRepositoryError::Unavailable) => {
                Err(MutateServerWorkspaceError::RepositoryUnavailable)
            }
        }
    }

    pub fn update_project(
        &self,
        request: UpdateProjectRequest,
    ) -> Result<Project, MutateServerWorkspaceError> {
        let current = self.load_project(&request.project_id)?;
        if current.record_version() != request.expected_record_version {
            return Err(MutateServerWorkspaceError::Conflict);
        }
        if current.display_name() == &request.display_name {
            return Ok(current);
        }
        let version = current
            .record_version()
            .checked_next()
            .ok_or(MutateServerWorkspaceError::VersionExhausted)?;
        let updated = Project::new(current.id().clone(), request.display_name, version);
        match self
            .repository
            .replace_project_if_version(&updated, request.expected_record_version)
        {
            Ok(ReplaceProjectResult::Replaced) => Ok(updated),
            Ok(ReplaceProjectResult::Conflict) => Err(MutateServerWorkspaceError::Conflict),
            Ok(ReplaceProjectResult::NotFound) => Err(MutateServerWorkspaceError::ProjectNotFound),
            Err(ServerWorkspaceRepositoryError::Unavailable) => {
                Err(MutateServerWorkspaceError::RepositoryUnavailable)
            }
        }
    }

    pub fn delete_project(
        &self,
        request: DeleteProjectRequest,
    ) -> Result<u64, MutateServerWorkspaceError> {
        let current = self.load_project(&request.project_id)?;
        if current.record_version() != request.expected_record_version {
            return Err(MutateServerWorkspaceError::Conflict);
        }
        match self.repository.delete_project_and_unassign_servers(
            &request.project_id,
            request.expected_record_version,
        ) {
            Ok(DeleteProjectResult::Deleted { unassigned_servers }) => Ok(unassigned_servers),
            Ok(DeleteProjectResult::Conflict) => Err(MutateServerWorkspaceError::Conflict),
            Ok(DeleteProjectResult::NotFound) => Err(MutateServerWorkspaceError::ProjectNotFound),
            Ok(DeleteProjectResult::VersionExhausted) => {
                Err(MutateServerWorkspaceError::VersionExhausted)
            }
            Err(ServerWorkspaceRepositoryError::Unavailable) => {
                Err(MutateServerWorkspaceError::RepositoryUnavailable)
            }
        }
    }

    pub fn create_server_with_connection(
        &self,
        request: CreateServerWithConnectionRequest,
    ) -> Result<CreatedServerWithConnection, MutateServerWorkspaceError> {
        if let Some(project_id) = request.project_id.as_ref() {
            self.load_project(project_id)?;
        }
        let server = ManagedServer::new(
            request.server_id,
            request.project_id,
            request.server_display_name,
            ManagedServerRecordVersion::INITIAL,
        );
        let profile = ConnectionProfile::new(
            request.profile_id,
            server.id().clone(),
            request.connection_display_name,
            ConnectionRecordVersion::INITIAL,
            ConnectionSecurityRevision::INITIAL,
            request.route,
            request.credential_ref,
            request.enabled,
        );
        match self
            .repository
            .create_server_with_connection(&server, &profile)
        {
            Ok(CreateServerWithConnectionResult::Created) => Ok(CreatedServerWithConnection {
                server,
                connection: profile,
            }),
            Ok(CreateServerWithConnectionResult::ProjectNotFound) => {
                Err(MutateServerWorkspaceError::ProjectNotFound)
            }
            Ok(
                CreateServerWithConnectionResult::ServerConflict
                | CreateServerWithConnectionResult::ProfileConflict,
            ) => Err(MutateServerWorkspaceError::Conflict),
            Err(ServerWorkspaceRepositoryError::Unavailable) => {
                Err(MutateServerWorkspaceError::RepositoryUnavailable)
            }
        }
    }

    pub fn add_connection_to_server(
        &self,
        request: AddConnectionToServerRequest,
    ) -> Result<ConnectionProfile, MutateServerWorkspaceError> {
        let server = self.load_server(&request.server_id)?;
        let profile = ConnectionProfile::new(
            request.profile_id,
            server.id().clone(),
            request.display_name,
            ConnectionRecordVersion::INITIAL,
            ConnectionSecurityRevision::INITIAL,
            request.route,
            request.credential_ref,
            request.enabled,
        );
        match self.repository.add_connection_to_server(&profile) {
            Ok(AddConnectionToServerResult::Created) => Ok(profile),
            Ok(AddConnectionToServerResult::ServerNotFound) => {
                Err(MutateServerWorkspaceError::ServerNotFound)
            }
            Ok(AddConnectionToServerResult::ProfileConflict) => {
                Err(MutateServerWorkspaceError::Conflict)
            }
            Err(ServerWorkspaceRepositoryError::Unavailable) => {
                Err(MutateServerWorkspaceError::RepositoryUnavailable)
            }
        }
    }

    pub fn update_server(
        &self,
        request: UpdateManagedServerRequest,
    ) -> Result<ManagedServer, MutateServerWorkspaceError> {
        let current = self.load_server(&request.server_id)?;
        if current.record_version() != request.expected_record_version {
            return Err(MutateServerWorkspaceError::Conflict);
        }
        if let Some(project_id) = request.project_id.as_ref() {
            self.load_project(project_id)?;
        }
        if current.display_name() == &request.display_name
            && current.project_id() == request.project_id.as_ref()
        {
            return Ok(current);
        }
        let version = current
            .record_version()
            .checked_next()
            .ok_or(MutateServerWorkspaceError::VersionExhausted)?;
        let updated = ManagedServer::new(
            current.id().clone(),
            request.project_id,
            request.display_name,
            version,
        );
        match self
            .repository
            .replace_server_if_version(&updated, request.expected_record_version)
        {
            Ok(ReplaceManagedServerResult::Replaced) => Ok(updated),
            Ok(ReplaceManagedServerResult::ProjectNotFound) => {
                Err(MutateServerWorkspaceError::ProjectNotFound)
            }
            Ok(ReplaceManagedServerResult::Conflict) => Err(MutateServerWorkspaceError::Conflict),
            Ok(ReplaceManagedServerResult::NotFound) => {
                Err(MutateServerWorkspaceError::ServerNotFound)
            }
            Err(ServerWorkspaceRepositoryError::Unavailable) => {
                Err(MutateServerWorkspaceError::RepositoryUnavailable)
            }
        }
    }

    pub fn delete_server(
        &self,
        request: DeleteManagedServerRequest,
    ) -> Result<(), MutateServerWorkspaceError> {
        let current = self.load_server(&request.server_id)?;
        if current.record_version() != request.expected_record_version {
            return Err(MutateServerWorkspaceError::Conflict);
        }
        match self
            .repository
            .delete_server_if_empty(&request.server_id, request.expected_record_version)
        {
            Ok(DeleteManagedServerResult::Deleted) => Ok(()),
            Ok(DeleteManagedServerResult::NotEmpty) => {
                Err(MutateServerWorkspaceError::ServerNotEmpty)
            }
            Ok(DeleteManagedServerResult::Conflict) => Err(MutateServerWorkspaceError::Conflict),
            Ok(DeleteManagedServerResult::NotFound) => {
                Err(MutateServerWorkspaceError::ServerNotFound)
            }
            Err(ServerWorkspaceRepositoryError::Unavailable) => {
                Err(MutateServerWorkspaceError::RepositoryUnavailable)
            }
        }
    }

    fn load_project(&self, project_id: &ProjectId) -> Result<Project, MutateServerWorkspaceError> {
        let project = self
            .repository
            .find_project(project_id)
            .map_err(|_| MutateServerWorkspaceError::RepositoryUnavailable)?
            .ok_or(MutateServerWorkspaceError::ProjectNotFound)?;
        if project.id() != project_id {
            return Err(MutateServerWorkspaceError::RecordMismatch);
        }
        Ok(project)
    }

    fn load_server(
        &self,
        server_id: &ManagedServerId,
    ) -> Result<ManagedServer, MutateServerWorkspaceError> {
        let server = self
            .repository
            .find_server(server_id)
            .map_err(|_| MutateServerWorkspaceError::RepositoryUnavailable)?
            .ok_or(MutateServerWorkspaceError::ServerNotFound)?;
        if server.id() != server_id {
            return Err(MutateServerWorkspaceError::RecordMismatch);
        }
        Ok(server)
    }
}
