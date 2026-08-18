use std::cell::{Cell, RefCell};

use vigilshell_core::{
    connection::{
        ConnectionDisplayName, ConnectionProfile, ConnectionProfileId, ConnectionRecordVersion,
        ConnectionSecurityRevision, ConnectionValueError, CredentialRef, ManagedServerId, SshRoute,
    },
    connection_mutation::{
        ConnectionMutationRepositoryError, ConnectionProfileMutationRepository,
        ConnectionProfileMutationService, CreateConnectionProfileRequest,
        CreateConnectionProfileResult, DeleteConnectionProfileRequest,
        DeleteConnectionProfileResult, MutateConnectionProfileError,
        ReplaceConnectionProfileResult, UpdateConnectionProfileRequest,
    },
    human_terminal::{ConnectionProfileRepository, ConnectionRepositoryError},
};

struct FakeRepository {
    find_result: Result<Option<ConnectionProfile>, ConnectionRepositoryError>,
    create_result: Cell<Result<CreateConnectionProfileResult, ConnectionMutationRepositoryError>>,
    replace_result: Cell<Result<ReplaceConnectionProfileResult, ConnectionMutationRepositoryError>>,
    delete_result: Cell<Result<DeleteConnectionProfileResult, ConnectionMutationRepositoryError>>,
    find_calls: Cell<usize>,
    create_calls: Cell<usize>,
    replace_calls: Cell<usize>,
    delete_calls: Cell<usize>,
    created_profile: RefCell<Option<ConnectionProfile>>,
    replaced_profile: RefCell<Option<ConnectionProfile>>,
    replace_expected_version: Cell<Option<ConnectionRecordVersion>>,
    deleted_profile_id: RefCell<Option<ConnectionProfileId>>,
    delete_expected_version: Cell<Option<ConnectionRecordVersion>>,
}

impl FakeRepository {
    fn with_current(profile: ConnectionProfile) -> Self {
        Self {
            find_result: Ok(Some(profile)),
            create_result: Cell::new(Ok(CreateConnectionProfileResult::Created)),
            replace_result: Cell::new(Ok(ReplaceConnectionProfileResult::Replaced)),
            delete_result: Cell::new(Ok(DeleteConnectionProfileResult::Deleted)),
            find_calls: Cell::new(0),
            create_calls: Cell::new(0),
            replace_calls: Cell::new(0),
            delete_calls: Cell::new(0),
            created_profile: RefCell::new(None),
            replaced_profile: RefCell::new(None),
            replace_expected_version: Cell::new(None),
            deleted_profile_id: RefCell::new(None),
            delete_expected_version: Cell::new(None),
        }
    }

    fn without_current() -> Self {
        let mut repository = Self::with_current(fixture_profile(1, 1, true));
        repository.find_result = Ok(None);
        repository
    }
}

impl ConnectionProfileRepository for FakeRepository {
    fn find(
        &self,
        _profile_id: &ConnectionProfileId,
    ) -> Result<Option<ConnectionProfile>, ConnectionRepositoryError> {
        self.find_calls.set(self.find_calls.get() + 1);
        self.find_result.clone()
    }
}

impl ConnectionProfileMutationRepository for FakeRepository {
    fn create(
        &self,
        profile: &ConnectionProfile,
    ) -> Result<CreateConnectionProfileResult, ConnectionMutationRepositoryError> {
        self.create_calls.set(self.create_calls.get() + 1);
        self.created_profile.replace(Some(profile.clone()));
        self.create_result.get()
    }

    fn replace_if_version(
        &self,
        profile: &ConnectionProfile,
        expected_record_version: ConnectionRecordVersion,
    ) -> Result<ReplaceConnectionProfileResult, ConnectionMutationRepositoryError> {
        self.replace_calls.set(self.replace_calls.get() + 1);
        self.replaced_profile.replace(Some(profile.clone()));
        self.replace_expected_version
            .set(Some(expected_record_version));
        self.replace_result.get()
    }

    fn delete_if_version(
        &self,
        profile_id: &ConnectionProfileId,
        expected_record_version: ConnectionRecordVersion,
    ) -> Result<DeleteConnectionProfileResult, ConnectionMutationRepositoryError> {
        self.delete_calls.set(self.delete_calls.get() + 1);
        self.deleted_profile_id.replace(Some(profile_id.clone()));
        self.delete_expected_version
            .set(Some(expected_record_version));
        self.delete_result.get()
    }
}

fn profile_id(value: &str) -> ConnectionProfileId {
    ConnectionProfileId::parse(value).expect("valid fixture profile id")
}

fn record_version(value: u64) -> ConnectionRecordVersion {
    ConnectionRecordVersion::new(value).expect("valid fixture record version")
}

fn security_revision(value: u64) -> ConnectionSecurityRevision {
    ConnectionSecurityRevision::new(value).expect("valid fixture security revision")
}

fn fixture_profile(record: u64, security: u64, enabled: bool) -> ConnectionProfile {
    ConnectionProfile::new(
        profile_id("profile-primary"),
        ManagedServerId::parse("server-primary").expect("valid fixture server id"),
        ConnectionDisplayName::parse("Primary connection").expect("valid display name"),
        record_version(record),
        security_revision(security),
        SshRoute::new("server.example.invalid", 22, "operator").expect("valid route"),
        CredentialRef::parse("credential-primary").expect("valid credential reference"),
        enabled,
    )
}

fn update_request(
    current: &ConnectionProfile,
    expected_record_version: ConnectionRecordVersion,
    display_name: &str,
    host: &str,
    port: u16,
    username: &str,
    enabled: bool,
) -> UpdateConnectionProfileRequest {
    UpdateConnectionProfileRequest::new(
        current.id().clone(),
        expected_record_version,
        display_name,
        host,
        port,
        username,
        enabled,
    )
    .expect("valid fixture update request")
}

#[test]
fn display_name_is_trimmed_and_bounded_by_utf8_bytes() {
    let name = ConnectionDisplayName::parse("  Production  ").expect("valid display name");
    assert_eq!(name.as_str(), "Production");

    let exactly_128_bytes = "界".repeat(42) + "aa";
    assert_eq!(exactly_128_bytes.len(), 128);
    assert!(ConnectionDisplayName::parse(exactly_128_bytes).is_ok());

    let too_long = "界".repeat(43);
    assert_eq!(too_long.len(), 129);
    assert!(matches!(
        ConnectionDisplayName::parse(too_long),
        Err(ConnectionValueError::TooLong {
            field: "connection display name",
            maximum_bytes: 128,
        })
    ));
    assert_eq!(
        ConnectionDisplayName::parse(" \n\t "),
        Err(ConnectionValueError::Empty("connection display name"))
    );
}

#[test]
fn ssh_route_trims_and_enforces_exact_input_boundaries() {
    let route = SshRoute::new("  host.example.invalid  ", u16::MAX, "  human operator  ")
        .expect("valid normalized route");
    assert_eq!(route.host(), "host.example.invalid");
    assert_eq!(route.port(), u16::MAX);
    assert_eq!(route.username(), "human operator");

    assert!(SshRoute::new("h".repeat(255), 22, "u".repeat(128)).is_ok());
    assert!(matches!(
        SshRoute::new("h".repeat(256), 22, "operator"),
        Err(ConnectionValueError::TooLong {
            field: "SSH host",
            maximum_bytes: 255,
        })
    ));
    assert!(matches!(
        SshRoute::new("host name", 22, "operator"),
        Err(ConnectionValueError::ContainsWhitespace("SSH host"))
    ));
    assert!(matches!(
        SshRoute::new("host\0name", 22, "operator"),
        Err(ConnectionValueError::ContainsControlCharacter("SSH host"))
    ));
    assert!(matches!(
        SshRoute::new("host", 22, "u".repeat(129)),
        Err(ConnectionValueError::TooLong {
            field: "SSH username",
            maximum_bytes: 128,
        })
    ));
    assert!(matches!(
        SshRoute::new("host", 22, "user\0name"),
        Err(ConnectionValueError::ContainsControlCharacter(
            "SSH username"
        ))
    ));
    assert_eq!(
        SshRoute::new("host", 0, "operator"),
        Err(ConnectionValueError::InvalidPort)
    );
}

#[test]
fn record_and_security_versions_must_be_positive() {
    assert_eq!(
        ConnectionRecordVersion::new(0),
        Err(ConnectionValueError::InvalidRecordVersion)
    );
    assert_eq!(
        ConnectionSecurityRevision::new(0),
        Err(ConnectionValueError::InvalidSecurityRevision)
    );
    assert_eq!(ConnectionRecordVersion::INITIAL.get(), 1);
    assert_eq!(ConnectionSecurityRevision::INITIAL.get(), 1);
}

#[test]
fn create_uses_trusted_identities_and_forces_initial_versions() {
    let repository = FakeRepository::without_current();
    let service = ConnectionProfileMutationService::new(&repository);
    let request = CreateConnectionProfileRequest::new(
        profile_id("profile-created"),
        ManagedServerId::parse("server-created").expect("valid server id"),
        CredentialRef::parse("credential-created").expect("valid credential reference"),
        "  Created connection  ",
        "  created.example.invalid  ",
        2222,
        "  creator  ",
        false,
    )
    .expect("valid create request");

    let created = service.create(request).expect("create should succeed");

    assert_eq!(created.id().as_str(), "profile-created");
    assert_eq!(created.server_id().as_str(), "server-created");
    assert_eq!(created.display_name().as_str(), "Created connection");
    assert_eq!(created.record_version(), ConnectionRecordVersion::INITIAL);
    assert_eq!(
        created.security_revision(),
        ConnectionSecurityRevision::INITIAL
    );
    assert_eq!(created.route().host(), "created.example.invalid");
    assert_eq!(created.route().username(), "creator");
    assert_eq!(created.credential_ref().as_str(), "credential-created");
    assert!(!created.is_enabled());
    assert_eq!(repository.find_calls.get(), 0);
    assert_eq!(repository.create_calls.get(), 1);
    assert_eq!(repository.created_profile.borrow().as_ref(), Some(&created));
}

#[test]
fn create_maps_conflict_and_unavailable_without_inventing_success() {
    let repository = FakeRepository::without_current();
    repository
        .create_result
        .set(Ok(CreateConnectionProfileResult::Conflict));
    let service = ConnectionProfileMutationService::new(&repository);
    let request = CreateConnectionProfileRequest::new(
        profile_id("profile-created"),
        ManagedServerId::parse("server-created").expect("valid server id"),
        CredentialRef::parse("credential-created").expect("valid credential reference"),
        "Created connection",
        "created.example.invalid",
        22,
        "creator",
        true,
    )
    .expect("valid create request");
    assert_eq!(
        service.create(request.clone()),
        Err(MutateConnectionProfileError::Conflict)
    );

    repository
        .create_result
        .set(Err(ConnectionMutationRepositoryError::Unavailable));
    assert_eq!(
        service.create(request),
        Err(MutateConnectionProfileError::RepositoryUnavailable)
    );
}

#[test]
fn no_op_update_returns_current_without_repository_mutation() {
    let current = fixture_profile(7, 9, true);
    let repository = FakeRepository::with_current(current.clone());
    let service = ConnectionProfileMutationService::new(&repository);
    let request = update_request(
        &current,
        current.record_version(),
        "  Primary connection  ",
        "  server.example.invalid  ",
        22,
        "  operator  ",
        true,
    );

    assert_eq!(service.update(request), Ok(current));
    assert_eq!(repository.find_calls.get(), 1);
    assert_eq!(repository.replace_calls.get(), 0);
}

#[test]
fn display_and_enabled_update_only_advance_record_version() {
    let current = fixture_profile(7, 9, true);
    let credential_ref = current.credential_ref().clone();
    let repository = FakeRepository::with_current(current.clone());
    let service = ConnectionProfileMutationService::new(&repository);
    let request = update_request(
        &current,
        current.record_version(),
        "Renamed connection",
        current.route().host(),
        current.route().port(),
        current.route().username(),
        false,
    );

    let updated = service.update(request).expect("update should succeed");

    assert_eq!(updated.record_version(), record_version(8));
    assert_eq!(updated.security_revision(), security_revision(9));
    assert_eq!(updated.credential_ref(), &credential_ref);
    assert_eq!(repository.replace_calls.get(), 1);
    assert_eq!(
        repository.replace_expected_version.get(),
        Some(record_version(7))
    );
    assert_eq!(
        repository.replaced_profile.borrow().as_ref(),
        Some(&updated)
    );
}

#[test]
fn each_route_field_change_advances_record_and_security_versions() {
    let current = fixture_profile(7, 9, true);
    let variants = [
        ("other.example.invalid", 22, "operator"),
        ("server.example.invalid", 2222, "operator"),
        ("server.example.invalid", 22, "other-user"),
    ];

    for (host, port, username) in variants {
        let repository = FakeRepository::with_current(current.clone());
        let service = ConnectionProfileMutationService::new(&repository);
        let request = update_request(
            &current,
            current.record_version(),
            current.display_name().as_str(),
            host,
            port,
            username,
            current.is_enabled(),
        );

        let updated = service.update(request).expect("update should succeed");
        assert_eq!(updated.record_version(), record_version(8));
        assert_eq!(updated.security_revision(), security_revision(10));
        assert_eq!(updated.credential_ref(), current.credential_ref());
        assert_eq!(repository.replace_calls.get(), 1);
    }
}

#[test]
fn update_rejects_stale_version_and_mismatched_or_missing_records() {
    let current = fixture_profile(8, 9, true);
    let repository = FakeRepository::with_current(current.clone());
    let service = ConnectionProfileMutationService::new(&repository);
    let stale = update_request(
        &current,
        record_version(7),
        "Renamed",
        current.route().host(),
        current.route().port(),
        current.route().username(),
        current.is_enabled(),
    );
    assert_eq!(
        service.update(stale),
        Err(MutateConnectionProfileError::Conflict)
    );
    assert_eq!(repository.replace_calls.get(), 0);

    let returned = fixture_profile(1, 1, true);
    let requested_id = profile_id("profile-requested");
    let mismatch_repository = FakeRepository::with_current(returned);
    let mismatch_service = ConnectionProfileMutationService::new(&mismatch_repository);
    let mismatch_request = UpdateConnectionProfileRequest::new(
        requested_id,
        ConnectionRecordVersion::INITIAL,
        "Requested",
        "requested.example.invalid",
        22,
        "operator",
        true,
    )
    .expect("valid request");
    assert_eq!(
        mismatch_service.update(mismatch_request),
        Err(MutateConnectionProfileError::RecordMismatch)
    );
    assert_eq!(mismatch_repository.replace_calls.get(), 0);

    let missing_repository = FakeRepository::without_current();
    let missing_service = ConnectionProfileMutationService::new(&missing_repository);
    let missing_request = UpdateConnectionProfileRequest::new(
        profile_id("profile-missing"),
        ConnectionRecordVersion::INITIAL,
        "Missing",
        "missing.example.invalid",
        22,
        "operator",
        true,
    )
    .expect("valid request");
    assert_eq!(
        missing_service.update(missing_request),
        Err(MutateConnectionProfileError::NotFound)
    );
    assert_eq!(missing_repository.replace_calls.get(), 0);
}

#[test]
fn update_maps_read_and_conditional_replace_failures() {
    let current = fixture_profile(1, 1, true);
    let mut read_unavailable = FakeRepository::with_current(current.clone());
    read_unavailable.find_result = Err(ConnectionRepositoryError::Unavailable);
    let read_service = ConnectionProfileMutationService::new(&read_unavailable);
    let request = update_request(
        &current,
        current.record_version(),
        "Renamed",
        current.route().host(),
        current.route().port(),
        current.route().username(),
        true,
    );
    assert_eq!(
        read_service.update(request.clone()),
        Err(MutateConnectionProfileError::RepositoryUnavailable)
    );

    let outcomes = [
        (
            Ok(ReplaceConnectionProfileResult::Conflict),
            MutateConnectionProfileError::Conflict,
        ),
        (
            Ok(ReplaceConnectionProfileResult::NotFound),
            MutateConnectionProfileError::NotFound,
        ),
        (
            Err(ConnectionMutationRepositoryError::Unavailable),
            MutateConnectionProfileError::RepositoryUnavailable,
        ),
    ];
    for (outcome, expected_error) in outcomes {
        let repository = FakeRepository::with_current(current.clone());
        repository.replace_result.set(outcome);
        let service = ConnectionProfileMutationService::new(&repository);
        assert_eq!(service.update(request.clone()), Err(expected_error));
        assert_eq!(repository.replace_calls.get(), 1);
    }
}

#[test]
fn update_fails_closed_when_either_version_is_exhausted() {
    let record_exhausted = fixture_profile(u64::MAX, 1, true);
    let record_repository = FakeRepository::with_current(record_exhausted.clone());
    let record_service = ConnectionProfileMutationService::new(&record_repository);
    let record_request = update_request(
        &record_exhausted,
        record_exhausted.record_version(),
        "Renamed",
        record_exhausted.route().host(),
        record_exhausted.route().port(),
        record_exhausted.route().username(),
        true,
    );
    assert_eq!(
        record_service.update(record_request),
        Err(MutateConnectionProfileError::VersionExhausted)
    );
    assert_eq!(record_repository.replace_calls.get(), 0);

    let security_exhausted = fixture_profile(1, u64::MAX, true);
    let security_repository = FakeRepository::with_current(security_exhausted.clone());
    let security_service = ConnectionProfileMutationService::new(&security_repository);
    let security_request = update_request(
        &security_exhausted,
        security_exhausted.record_version(),
        security_exhausted.display_name().as_str(),
        "new-route.example.invalid",
        security_exhausted.route().port(),
        security_exhausted.route().username(),
        true,
    );
    assert_eq!(
        security_service.update(security_request),
        Err(MutateConnectionProfileError::VersionExhausted)
    );
    assert_eq!(security_repository.replace_calls.get(), 0);
}

#[test]
fn delete_is_conditional_and_maps_all_repository_settlements() {
    let expected_version = record_version(17);
    let outcomes = [
        (Ok(DeleteConnectionProfileResult::Deleted), Ok(())),
        (
            Ok(DeleteConnectionProfileResult::Conflict),
            Err(MutateConnectionProfileError::Conflict),
        ),
        (
            Ok(DeleteConnectionProfileResult::NotFound),
            Err(MutateConnectionProfileError::NotFound),
        ),
        (
            Err(ConnectionMutationRepositoryError::Unavailable),
            Err(MutateConnectionProfileError::RepositoryUnavailable),
        ),
    ];

    for (outcome, expected) in outcomes {
        let repository = FakeRepository::without_current();
        repository.delete_result.set(outcome);
        let service = ConnectionProfileMutationService::new(&repository);
        let request =
            DeleteConnectionProfileRequest::new(profile_id("profile-delete"), expected_version);

        assert_eq!(service.delete(request), expected);
        assert_eq!(repository.delete_calls.get(), 1);
        assert_eq!(
            repository
                .deleted_profile_id
                .borrow()
                .as_ref()
                .map(ConnectionProfileId::as_str),
            Some("profile-delete")
        );
        assert_eq!(
            repository.delete_expected_version.get(),
            Some(expected_version)
        );
    }
}

#[test]
fn mutation_debug_output_redacts_route_and_credential_values() {
    let request = CreateConnectionProfileRequest::new(
        profile_id("profile-debug"),
        ManagedServerId::parse("server-debug").expect("valid server id"),
        CredentialRef::parse("credential-do-not-leak").expect("valid credential reference"),
        "Debug connection",
        "sensitive-host.example.invalid",
        22,
        "sensitive-user",
        true,
    )
    .expect("valid request");
    let request_debug = format!("{request:?}");
    assert!(!request_debug.contains("credential-do-not-leak"));
    assert!(!request_debug.contains("sensitive-host.example.invalid"));
    assert!(!request_debug.contains("sensitive-user"));

    let profile_debug = format!("{:?}", fixture_profile(1, 1, true));
    assert!(!profile_debug.contains("server.example.invalid"));
    assert!(!profile_debug.contains("operator"));
    assert!(!profile_debug.contains("credential-primary"));
}
