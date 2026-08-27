use std::{cell::Cell, cell::RefCell};

use vigilshell_core::{
    connection::{
        ConnectionProfile, ConnectionProfileId, ConnectionRecordVersion,
        ConnectionSecurityRevision, CredentialRef, ManagedServerId,
    },
    server_workspace::{
        AddConnectionToServerRequest, AddConnectionToServerResult, CreateProjectRequest,
        CreateProjectResult, CreateServerWithConnectionRequest, CreateServerWithConnectionResult,
        DeleteManagedServerRequest, DeleteManagedServerResult, DeleteProjectRequest,
        DeleteProjectResult, ManagedServer, ManagedServerDisplayName, ManagedServerRecordVersion,
        MutateServerWorkspaceError, Project, ProjectDisplayName, ProjectId, ProjectRecordVersion,
        ReplaceManagedServerResult, ReplaceProjectResult, ServerWorkspaceMutationRepository,
        ServerWorkspaceReadRepository, ServerWorkspaceRepositoryError, ServerWorkspaceService,
        ServerWorkspaceValueError, UpdateManagedServerRequest, UpdateProjectRequest,
    },
};

struct FakeRepository {
    project_result: RefCell<Result<Option<Project>, ServerWorkspaceRepositoryError>>,
    server_result: RefCell<Result<Option<ManagedServer>, ServerWorkspaceRepositoryError>>,
    profiles_result: RefCell<Result<Vec<ConnectionProfile>, ServerWorkspaceRepositoryError>>,
    create_project_result: Cell<Result<CreateProjectResult, ServerWorkspaceRepositoryError>>,
    replace_project_result: Cell<Result<ReplaceProjectResult, ServerWorkspaceRepositoryError>>,
    delete_project_result: Cell<Result<DeleteProjectResult, ServerWorkspaceRepositoryError>>,
    create_server_result:
        Cell<Result<CreateServerWithConnectionResult, ServerWorkspaceRepositoryError>>,
    add_connection_result:
        Cell<Result<AddConnectionToServerResult, ServerWorkspaceRepositoryError>>,
    replace_server_result: Cell<Result<ReplaceManagedServerResult, ServerWorkspaceRepositoryError>>,
    delete_server_result: Cell<Result<DeleteManagedServerResult, ServerWorkspaceRepositoryError>>,
    created_project: RefCell<Option<Project>>,
    replaced_project: RefCell<Option<Project>>,
    deleted_project: RefCell<Option<(ProjectId, ProjectRecordVersion)>>,
    created_server_and_profile: RefCell<Option<(ManagedServer, ConnectionProfile)>>,
    added_profile: RefCell<Option<ConnectionProfile>>,
    replaced_server: RefCell<Option<ManagedServer>>,
    deleted_server: RefCell<Option<(ManagedServerId, ManagedServerRecordVersion)>>,
    replace_project_calls: Cell<usize>,
    replace_server_calls: Cell<usize>,
}

impl FakeRepository {
    fn new(project: Option<Project>, server: Option<ManagedServer>) -> Self {
        Self {
            project_result: RefCell::new(Ok(project)),
            server_result: RefCell::new(Ok(server)),
            profiles_result: RefCell::new(Ok(Vec::new())),
            create_project_result: Cell::new(Ok(CreateProjectResult::Created)),
            replace_project_result: Cell::new(Ok(ReplaceProjectResult::Replaced)),
            delete_project_result: Cell::new(Ok(DeleteProjectResult::Deleted {
                unassigned_servers: 0,
            })),
            create_server_result: Cell::new(Ok(CreateServerWithConnectionResult::Created)),
            add_connection_result: Cell::new(Ok(AddConnectionToServerResult::Created)),
            replace_server_result: Cell::new(Ok(ReplaceManagedServerResult::Replaced)),
            delete_server_result: Cell::new(Ok(DeleteManagedServerResult::Deleted)),
            created_project: RefCell::new(None),
            replaced_project: RefCell::new(None),
            deleted_project: RefCell::new(None),
            created_server_and_profile: RefCell::new(None),
            added_profile: RefCell::new(None),
            replaced_server: RefCell::new(None),
            deleted_server: RefCell::new(None),
            replace_project_calls: Cell::new(0),
            replace_server_calls: Cell::new(0),
        }
    }
}

impl ServerWorkspaceReadRepository for FakeRepository {
    fn find_project(
        &self,
        _project_id: &ProjectId,
    ) -> Result<Option<Project>, ServerWorkspaceRepositoryError> {
        self.project_result.borrow().clone()
    }

    fn find_server(
        &self,
        _server_id: &ManagedServerId,
    ) -> Result<Option<ManagedServer>, ServerWorkspaceRepositoryError> {
        self.server_result.borrow().clone()
    }

    fn list_projects(&self) -> Result<Vec<Project>, ServerWorkspaceRepositoryError> {
        self.project_result
            .borrow()
            .clone()
            .map(|project| project.into_iter().collect())
    }

    fn list_servers(&self) -> Result<Vec<ManagedServer>, ServerWorkspaceRepositoryError> {
        self.server_result
            .borrow()
            .clone()
            .map(|server| server.into_iter().collect())
    }

    fn list_connection_profiles(
        &self,
        _server_id: &ManagedServerId,
    ) -> Result<Vec<ConnectionProfile>, ServerWorkspaceRepositoryError> {
        self.profiles_result.borrow().clone()
    }
}

impl ServerWorkspaceMutationRepository for FakeRepository {
    fn create_project(
        &self,
        project: &Project,
    ) -> Result<CreateProjectResult, ServerWorkspaceRepositoryError> {
        self.created_project.replace(Some(project.clone()));
        self.create_project_result.get()
    }

    fn replace_project_if_version(
        &self,
        project: &Project,
        _expected_record_version: ProjectRecordVersion,
    ) -> Result<ReplaceProjectResult, ServerWorkspaceRepositoryError> {
        self.replace_project_calls
            .set(self.replace_project_calls.get() + 1);
        self.replaced_project.replace(Some(project.clone()));
        self.replace_project_result.get()
    }

    fn delete_project_and_unassign_servers(
        &self,
        project_id: &ProjectId,
        expected_record_version: ProjectRecordVersion,
    ) -> Result<DeleteProjectResult, ServerWorkspaceRepositoryError> {
        self.deleted_project
            .replace(Some((project_id.clone(), expected_record_version)));
        self.delete_project_result.get()
    }

    fn create_server_with_connection(
        &self,
        server: &ManagedServer,
        profile: &ConnectionProfile,
    ) -> Result<CreateServerWithConnectionResult, ServerWorkspaceRepositoryError> {
        self.created_server_and_profile
            .replace(Some((server.clone(), profile.clone())));
        self.create_server_result.get()
    }

    fn add_connection_to_server(
        &self,
        profile: &ConnectionProfile,
    ) -> Result<AddConnectionToServerResult, ServerWorkspaceRepositoryError> {
        self.added_profile.replace(Some(profile.clone()));
        self.add_connection_result.get()
    }

    fn replace_server_if_version(
        &self,
        server: &ManagedServer,
        _expected_record_version: ManagedServerRecordVersion,
    ) -> Result<ReplaceManagedServerResult, ServerWorkspaceRepositoryError> {
        self.replace_server_calls
            .set(self.replace_server_calls.get() + 1);
        self.replaced_server.replace(Some(server.clone()));
        self.replace_server_result.get()
    }

    fn delete_server_if_empty(
        &self,
        server_id: &ManagedServerId,
        expected_record_version: ManagedServerRecordVersion,
    ) -> Result<DeleteManagedServerResult, ServerWorkspaceRepositoryError> {
        self.deleted_server
            .replace(Some((server_id.clone(), expected_record_version)));
        self.delete_server_result.get()
    }
}

fn project_id(value: &str) -> ProjectId {
    ProjectId::parse(value).expect("valid project id")
}

fn server_id(value: &str) -> ManagedServerId {
    ManagedServerId::parse(value).expect("valid server id")
}

fn project_version(value: u64) -> ProjectRecordVersion {
    ProjectRecordVersion::new(value).expect("valid project version")
}

fn server_version(value: u64) -> ManagedServerRecordVersion {
    ManagedServerRecordVersion::new(value).expect("valid server version")
}

fn fixture_project(id: &str, name: &str, version: u64) -> Project {
    Project::new(
        project_id(id),
        ProjectDisplayName::parse(name).expect("valid project name"),
        project_version(version),
    )
}

fn fixture_server(id: &str, project: Option<ProjectId>, name: &str, version: u64) -> ManagedServer {
    ManagedServer::new(
        server_id(id),
        project,
        ManagedServerDisplayName::parse(name).expect("valid server name"),
        server_version(version),
    )
}

#[test]
fn workspace_names_and_versions_are_normalized_and_bounded() {
    let project_name = ProjectDisplayName::parse("  Production  ").expect("valid name");
    let server_name = ManagedServerDisplayName::parse("  API Server  ").expect("valid name");
    assert_eq!(project_name.as_str(), "Production");
    assert_eq!(server_name.as_str(), "API Server");

    let exactly_128_bytes = "界".repeat(42) + "aa";
    assert!(ProjectDisplayName::parse(&exactly_128_bytes).is_ok());
    assert!(ManagedServerDisplayName::parse(&exactly_128_bytes).is_ok());
    assert!(matches!(
        ManagedServerDisplayName::parse("界".repeat(43)),
        Err(ServerWorkspaceValueError::TooLong {
            field: "managed server display name",
            maximum_bytes: 128,
        })
    ));
    assert_eq!(
        ProjectDisplayName::parse(" \n\t "),
        Err(ServerWorkspaceValueError::Empty("project display name"))
    );
    assert_eq!(
        ManagedServerRecordVersion::new(0),
        Err(ServerWorkspaceValueError::InvalidRecordVersion(
            "managed server record version"
        ))
    );
}

#[test]
fn project_create_update_noop_and_delete_use_optimistic_versions() {
    let current = fixture_project("project-main", "Main", 1);
    let repository = FakeRepository::new(Some(current.clone()), None);
    let service = ServerWorkspaceService::new(&repository);

    let created = service
        .create_project(
            CreateProjectRequest::new(project_id("project-new"), "  New  ").expect("valid request"),
        )
        .expect("create project");
    assert_eq!(created.display_name().as_str(), "New");
    assert_eq!(created.record_version(), ProjectRecordVersion::INITIAL);

    let noop = service
        .update_project(
            UpdateProjectRequest::new(current.id().clone(), project_version(1), "Main")
                .expect("valid request"),
        )
        .expect("no-op update");
    assert_eq!(noop, current);
    assert_eq!(repository.replace_project_calls.get(), 0);

    let updated = service
        .update_project(
            UpdateProjectRequest::new(current.id().clone(), project_version(1), "Renamed")
                .expect("valid request"),
        )
        .expect("rename project");
    assert_eq!(updated.record_version(), project_version(2));
    assert_eq!(updated.display_name().as_str(), "Renamed");

    repository
        .delete_project_result
        .set(Ok(DeleteProjectResult::Deleted {
            unassigned_servers: 3,
        }));
    assert_eq!(
        service
            .delete_project(DeleteProjectRequest::new(
                current.id().clone(),
                project_version(1)
            ))
            .expect("delete project"),
        3
    );
}

#[test]
fn stale_project_version_fails_before_mutation() {
    let current = fixture_project("project-main", "Main", 2);
    let repository = FakeRepository::new(Some(current.clone()), None);
    let service = ServerWorkspaceService::new(&repository);
    assert_eq!(
        service.update_project(
            UpdateProjectRequest::new(current.id().clone(), project_version(1), "Changed")
                .expect("valid request")
        ),
        Err(MutateServerWorkspaceError::Conflict)
    );
    assert_eq!(repository.replace_project_calls.get(), 0);
}

#[test]
fn project_delete_maps_server_version_exhaustion() {
    let current = fixture_project("project-main", "Main", 1);
    let repository = FakeRepository::new(Some(current.clone()), None);
    repository
        .delete_project_result
        .set(Ok(DeleteProjectResult::VersionExhausted));
    let service = ServerWorkspaceService::new(&repository);
    assert_eq!(
        service.delete_project(DeleteProjectRequest::new(
            current.id().clone(),
            project_version(1)
        )),
        Err(MutateServerWorkspaceError::VersionExhausted)
    );
}

#[test]
fn server_and_first_connection_are_created_as_one_atomic_repository_call() {
    let project = fixture_project("project-main", "Main", 1);
    let repository = FakeRepository::new(Some(project.clone()), None);
    let service = ServerWorkspaceService::new(&repository);
    let request = CreateServerWithConnectionRequest::new(
        server_id("server-created"),
        Some(project.id().clone()),
        "  API Server  ",
        ConnectionProfileId::parse("profile-created").expect("profile id"),
        CredentialRef::parse("credential-created").expect("credential ref"),
        "  Primary route  ",
        "  api.example.invalid  ",
        2222,
        "  operator  ",
        true,
    )
    .expect("valid request");
    let debug = format!("{request:?}");
    assert!(!debug.contains("api.example.invalid"));
    assert!(!debug.contains("credential-created"));

    let created = service
        .create_server_with_connection(request)
        .expect("atomic create");
    assert_eq!(created.server().display_name().as_str(), "API Server");
    assert_eq!(
        created.server().record_version(),
        ManagedServerRecordVersion::INITIAL
    );
    assert_eq!(created.connection().server_id(), created.server().id());
    assert_eq!(
        created.connection().record_version(),
        ConnectionRecordVersion::INITIAL
    );
    assert_eq!(
        created.connection().security_revision(),
        ConnectionSecurityRevision::INITIAL
    );

    let captured = repository
        .created_server_and_profile
        .borrow()
        .clone()
        .expect("captured atomic write");
    assert_eq!(captured.0, *created.server());
    assert_eq!(captured.1, *created.connection());
}

#[test]
fn create_for_missing_project_fails_before_repository_mutation() {
    let repository = FakeRepository::new(None, None);
    let service = ServerWorkspaceService::new(&repository);
    let request = CreateServerWithConnectionRequest::new(
        server_id("server-created"),
        Some(project_id("missing-project")),
        "Server",
        ConnectionProfileId::parse("profile-created").expect("profile id"),
        CredentialRef::parse("credential-created").expect("credential ref"),
        "Connection",
        "host.example.invalid",
        22,
        "operator",
        true,
    )
    .expect("valid request");
    assert_eq!(
        service.create_server_with_connection(request),
        Err(MutateServerWorkspaceError::ProjectNotFound)
    );
    assert!(repository.created_server_and_profile.borrow().is_none());
}

#[test]
fn add_connection_reloads_exact_server_and_preserves_profile_scoped_state() {
    let server = fixture_server("server-main", None, "Main", 4);
    let repository = FakeRepository::new(None, Some(server.clone()));
    let service = ServerWorkspaceService::new(&repository);
    let request = AddConnectionToServerRequest::new(
        server.id().clone(),
        ConnectionProfileId::parse("profile-secondary").expect("profile id"),
        CredentialRef::parse("credential-secondary").expect("credential ref"),
        "Secondary",
        "secondary.example.invalid",
        2200,
        "secondary-user",
        false,
    )
    .expect("valid request");
    assert!(!format!("{request:?}").contains("secondary.example.invalid"));

    let profile = service
        .add_connection_to_server(request)
        .expect("add connection");
    assert_eq!(profile.server_id(), server.id());
    assert_eq!(profile.record_version(), ConnectionRecordVersion::INITIAL);
    assert_eq!(
        profile.security_revision(),
        ConnectionSecurityRevision::INITIAL
    );
    assert_eq!(profile.credential_ref().as_str(), "credential-secondary");
}

#[test]
fn server_update_supports_move_unassign_noop_and_version_progression() {
    let project = fixture_project("project-main", "Main", 1);
    let server = fixture_server("server-main", None, "Server", 7);
    let repository = FakeRepository::new(Some(project.clone()), Some(server.clone()));
    let service = ServerWorkspaceService::new(&repository);

    let noop = service
        .update_server(
            UpdateManagedServerRequest::new(server.id().clone(), server_version(7), None, "Server")
                .expect("valid request"),
        )
        .expect("no-op update");
    assert_eq!(noop, server);
    assert_eq!(repository.replace_server_calls.get(), 0);

    let moved = service
        .update_server(
            UpdateManagedServerRequest::new(
                server.id().clone(),
                server_version(7),
                Some(project.id().clone()),
                "Renamed",
            )
            .expect("valid request"),
        )
        .expect("move server");
    assert_eq!(moved.project_id(), Some(project.id()));
    assert_eq!(moved.display_name().as_str(), "Renamed");
    assert_eq!(moved.record_version(), server_version(8));
}

#[test]
fn non_empty_server_delete_fails_closed() {
    let server = fixture_server("server-main", None, "Server", 1);
    let repository = FakeRepository::new(None, Some(server.clone()));
    repository
        .delete_server_result
        .set(Ok(DeleteManagedServerResult::NotEmpty));
    let service = ServerWorkspaceService::new(&repository);
    assert_eq!(
        service.delete_server(DeleteManagedServerRequest::new(
            server.id().clone(),
            server_version(1)
        )),
        Err(MutateServerWorkspaceError::ServerNotEmpty)
    );
}

#[test]
fn mismatched_repository_records_and_unavailable_storage_fail_closed() {
    let requested = server_id("server-requested");
    let mismatched = fixture_server("server-other", None, "Other", 1);
    let repository = FakeRepository::new(None, Some(mismatched));
    let service = ServerWorkspaceService::new(&repository);
    let request = AddConnectionToServerRequest::new(
        requested,
        ConnectionProfileId::parse("profile-new").expect("profile id"),
        CredentialRef::parse("credential-new").expect("credential ref"),
        "Connection",
        "host.example.invalid",
        22,
        "operator",
        true,
    )
    .expect("valid request");
    assert_eq!(
        service.add_connection_to_server(request),
        Err(MutateServerWorkspaceError::RecordMismatch)
    );

    let _ = repository
        .server_result
        .replace(Err(ServerWorkspaceRepositoryError::Unavailable));
    assert_eq!(
        service.delete_server(DeleteManagedServerRequest::new(
            server_id("server-requested"),
            server_version(1)
        )),
        Err(MutateServerWorkspaceError::RepositoryUnavailable)
    );
}

#[test]
fn record_version_exhaustion_prevents_replace() {
    let server = fixture_server("server-main", None, "Server", u64::MAX);
    let repository = FakeRepository::new(None, Some(server.clone()));
    let service = ServerWorkspaceService::new(&repository);
    assert_eq!(
        service.update_server(
            UpdateManagedServerRequest::new(
                server.id().clone(),
                server_version(u64::MAX),
                None,
                "Changed",
            )
            .expect("valid request")
        ),
        Err(MutateServerWorkspaceError::VersionExhausted)
    );
    assert_eq!(repository.replace_server_calls.get(), 0);
}
