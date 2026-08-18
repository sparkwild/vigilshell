use std::{error::Error, fmt};

use crate::{
    connection::{
        ConnectionDisplayName, ConnectionProfile, ConnectionProfileId, ConnectionRecordVersion,
        ConnectionSecurityRevision, ConnectionValueError, CredentialRef, ManagedServerId, SshRoute,
    },
    human_terminal::{ConnectionProfileRepository, ConnectionRepositoryError},
};

/// Connection 创建的原子持久化结算。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CreateConnectionProfileResult {
    Created,
    Conflict,
}

/// Connection 条件替换的原子持久化结算。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReplaceConnectionProfileResult {
    Replaced,
    Conflict,
    NotFound,
}

/// Connection 条件删除的原子持久化结算。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeleteConnectionProfileResult {
    Deleted,
    Conflict,
    NotFound,
}

/// Connection mutation 存储端口可公开的稳定失败。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConnectionMutationRepositoryError {
    Unavailable,
}

impl fmt::Display for ConnectionMutationRepositoryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("connection mutation repository is unavailable")
    }
}

impl Error for ConnectionMutationRepositoryError {}

/// 产品级 Connection mutation 持久化端口。
pub trait ConnectionProfileMutationRepository {
    fn create(
        &self,
        profile: &ConnectionProfile,
    ) -> Result<CreateConnectionProfileResult, ConnectionMutationRepositoryError>;

    fn replace_if_version(
        &self,
        profile: &ConnectionProfile,
        expected_record_version: ConnectionRecordVersion,
    ) -> Result<ReplaceConnectionProfileResult, ConnectionMutationRepositoryError>;

    fn delete_if_version(
        &self,
        profile_id: &ConnectionProfileId,
        expected_record_version: ConnectionRecordVersion,
    ) -> Result<DeleteConnectionProfileResult, ConnectionMutationRepositoryError>;
}

/// 创建 Connection 时由可信 composition root 提供的身份和 Human 输入。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CreateConnectionProfileRequest {
    profile_id: ConnectionProfileId,
    server_id: ManagedServerId,
    credential_ref: CredentialRef,
    display_name: ConnectionDisplayName,
    route: SshRoute,
    enabled: bool,
}

impl CreateConnectionProfileRequest {
    #[allow(
        clippy::too_many_arguments,
        reason = "the request keeps all trusted identities and editable fields explicit"
    )]
    pub fn new(
        profile_id: ConnectionProfileId,
        server_id: ManagedServerId,
        credential_ref: CredentialRef,
        display_name: impl Into<String>,
        host: impl Into<String>,
        port: u16,
        username: impl Into<String>,
        enabled: bool,
    ) -> Result<Self, ConnectionValueError> {
        Ok(Self {
            profile_id,
            server_id,
            credential_ref,
            display_name: ConnectionDisplayName::parse(display_name)?,
            route: SshRoute::new(host, port, username)?,
            enabled,
        })
    }
}

/// 更新 Connection 时的 Human 可编辑字段与并发令牌。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UpdateConnectionProfileRequest {
    profile_id: ConnectionProfileId,
    expected_record_version: ConnectionRecordVersion,
    display_name: ConnectionDisplayName,
    route: SshRoute,
    enabled: bool,
}

impl UpdateConnectionProfileRequest {
    pub fn new(
        profile_id: ConnectionProfileId,
        expected_record_version: ConnectionRecordVersion,
        display_name: impl Into<String>,
        host: impl Into<String>,
        port: u16,
        username: impl Into<String>,
        enabled: bool,
    ) -> Result<Self, ConnectionValueError> {
        Ok(Self {
            profile_id,
            expected_record_version,
            display_name: ConnectionDisplayName::parse(display_name)?,
            route: SshRoute::new(host, port, username)?,
            enabled,
        })
    }
}

/// 删除 Connection 时的精确记录身份与并发令牌。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeleteConnectionProfileRequest {
    profile_id: ConnectionProfileId,
    expected_record_version: ConnectionRecordVersion,
}

impl DeleteConnectionProfileRequest {
    pub const fn new(
        profile_id: ConnectionProfileId,
        expected_record_version: ConnectionRecordVersion,
    ) -> Self {
        Self {
            profile_id,
            expected_record_version,
        }
    }
}

/// Connection mutation 应用服务可公开的稳定错误。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MutateConnectionProfileError {
    RepositoryUnavailable,
    Conflict,
    NotFound,
    RecordMismatch,
    VersionExhausted,
}

impl fmt::Display for MutateConnectionProfileError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::RepositoryUnavailable => "connection repository is unavailable",
            Self::Conflict => "connection record version conflicts with current state",
            Self::NotFound => "connection profile was not found",
            Self::RecordMismatch => "connection repository returned a mismatched profile",
            Self::VersionExhausted => "connection version is exhausted",
        };
        formatter.write_str(message)
    }
}

impl Error for MutateConnectionProfileError {}

/// 执行 Core-owned Connection 创建、更新与删除语义。
pub struct ConnectionProfileMutationService<'a, Repository> {
    repository: &'a Repository,
}

impl<'a, Repository> ConnectionProfileMutationService<'a, Repository>
where
    Repository: ConnectionProfileRepository + ConnectionProfileMutationRepository,
{
    pub const fn new(repository: &'a Repository) -> Self {
        Self { repository }
    }

    pub fn create(
        &self,
        request: CreateConnectionProfileRequest,
    ) -> Result<ConnectionProfile, MutateConnectionProfileError> {
        let profile = ConnectionProfile::new(
            request.profile_id,
            request.server_id,
            request.display_name,
            ConnectionRecordVersion::INITIAL,
            ConnectionSecurityRevision::INITIAL,
            request.route,
            request.credential_ref,
            request.enabled,
        );

        match self.repository.create(&profile) {
            Ok(CreateConnectionProfileResult::Created) => Ok(profile),
            Ok(CreateConnectionProfileResult::Conflict) => {
                Err(MutateConnectionProfileError::Conflict)
            }
            Err(ConnectionMutationRepositoryError::Unavailable) => {
                Err(MutateConnectionProfileError::RepositoryUnavailable)
            }
        }
    }

    pub fn update(
        &self,
        request: UpdateConnectionProfileRequest,
    ) -> Result<ConnectionProfile, MutateConnectionProfileError> {
        let current = self
            .repository
            .find(&request.profile_id)
            .map_err(map_read_error)?
            .ok_or(MutateConnectionProfileError::NotFound)?;
        if current.id() != &request.profile_id {
            return Err(MutateConnectionProfileError::RecordMismatch);
        }
        if current.record_version() != request.expected_record_version {
            return Err(MutateConnectionProfileError::Conflict);
        }

        let route_changed = current.route() != &request.route;
        let changed = route_changed
            || current.display_name() != &request.display_name
            || current.is_enabled() != request.enabled;
        if !changed {
            return Ok(current);
        }

        let record_version = current
            .record_version()
            .checked_next()
            .ok_or(MutateConnectionProfileError::VersionExhausted)?;
        let security_revision = if route_changed {
            current
                .security_revision()
                .checked_next()
                .ok_or(MutateConnectionProfileError::VersionExhausted)?
        } else {
            current.security_revision()
        };
        let updated = ConnectionProfile::new(
            current.id().clone(),
            current.server_id().clone(),
            request.display_name,
            record_version,
            security_revision,
            request.route,
            current.credential_ref().clone(),
            request.enabled,
        );

        match self
            .repository
            .replace_if_version(&updated, request.expected_record_version)
        {
            Ok(ReplaceConnectionProfileResult::Replaced) => Ok(updated),
            Ok(ReplaceConnectionProfileResult::Conflict) => {
                Err(MutateConnectionProfileError::Conflict)
            }
            Ok(ReplaceConnectionProfileResult::NotFound) => {
                Err(MutateConnectionProfileError::NotFound)
            }
            Err(ConnectionMutationRepositoryError::Unavailable) => {
                Err(MutateConnectionProfileError::RepositoryUnavailable)
            }
        }
    }

    pub fn delete(
        &self,
        request: DeleteConnectionProfileRequest,
    ) -> Result<(), MutateConnectionProfileError> {
        match self
            .repository
            .delete_if_version(&request.profile_id, request.expected_record_version)
        {
            Ok(DeleteConnectionProfileResult::Deleted) => Ok(()),
            Ok(DeleteConnectionProfileResult::Conflict) => {
                Err(MutateConnectionProfileError::Conflict)
            }
            Ok(DeleteConnectionProfileResult::NotFound) => {
                Err(MutateConnectionProfileError::NotFound)
            }
            Err(ConnectionMutationRepositoryError::Unavailable) => {
                Err(MutateConnectionProfileError::RepositoryUnavailable)
            }
        }
    }
}

fn map_read_error(_: ConnectionRepositoryError) -> MutateConnectionProfileError {
    MutateConnectionProfileError::RepositoryUnavailable
}
