use std::cell::Cell;

use vigilshell_core::{
    connection::{
        ConnectionDisplayName, ConnectionProfile, ConnectionProfileId, ConnectionRecordVersion,
        ConnectionSecurityRevision, CredentialRef, HostFingerprint, ManagedServerId, SshRoute,
        StoredHostTrust,
    },
    human_terminal::{
        ConnectionProfileRepository, ConnectionRepositoryError, CredentialBroker,
        CredentialBrokerError, HostTrustRepository, HostTrustRepositoryError, HumanCredentialLease,
        HumanTerminalAdmissionService, HumanTerminalSessionId, OpenHumanTerminalError,
        OpenHumanTerminalRequest, TerminalSize, TerminalValueError,
    },
};

#[derive(Default)]
struct CallCounts {
    profiles: Cell<usize>,
    trust: Cell<usize>,
    credentials: Cell<usize>,
}

struct FakeProfiles<'a> {
    calls: &'a CallCounts,
    result: Result<Option<ConnectionProfile>, ConnectionRepositoryError>,
}

impl ConnectionProfileRepository for FakeProfiles<'_> {
    fn find(
        &self,
        _profile_id: &ConnectionProfileId,
    ) -> Result<Option<ConnectionProfile>, ConnectionRepositoryError> {
        self.calls.profiles.set(self.calls.profiles.get() + 1);
        self.result.clone()
    }
}

struct FakeTrust<'a> {
    calls: &'a CallCounts,
    result: Result<Option<StoredHostTrust>, HostTrustRepositoryError>,
}

impl HostTrustRepository for FakeTrust<'_> {
    fn find(
        &self,
        _profile_id: &ConnectionProfileId,
    ) -> Result<Option<StoredHostTrust>, HostTrustRepositoryError> {
        self.calls.trust.set(self.calls.trust.get() + 1);
        self.result.clone()
    }
}

#[derive(Clone, Copy)]
enum CredentialOutcome {
    Lease(&'static [u8]),
    Unavailable,
}

struct FakeCredentials<'a> {
    calls: &'a CallCounts,
    outcome: CredentialOutcome,
}

impl CredentialBroker for FakeCredentials<'_> {
    fn lease_for_human_terminal(
        &self,
        _credential_ref: &CredentialRef,
    ) -> Result<HumanCredentialLease, CredentialBrokerError> {
        self.calls.credentials.set(self.calls.credentials.get() + 1);
        match self.outcome {
            CredentialOutcome::Lease(material) => {
                Ok(HumanCredentialLease::from_password(material.to_vec())
                    .expect("valid fixture lease"))
            }
            CredentialOutcome::Unavailable => Err(CredentialBrokerError::Unavailable),
        }
    }
}

fn valid_profile(enabled: bool) -> ConnectionProfile {
    ConnectionProfile::new(
        ConnectionProfileId::parse("profile-demo").expect("valid fixture profile id"),
        ManagedServerId::parse("server-demo").expect("valid fixture server id"),
        ConnectionDisplayName::parse("Demo connection").expect("valid fixture display name"),
        ConnectionRecordVersion::INITIAL,
        ConnectionSecurityRevision::new(1).expect("valid fixture revision"),
        SshRoute::new("server.example.invalid", 22, "operator").expect("valid fixture route"),
        CredentialRef::parse("credential-demo").expect("valid fixture credential reference"),
        enabled,
    )
}

fn matching_trust(profile: &ConnectionProfile) -> StoredHostTrust {
    StoredHostTrust::new(
        profile.id().clone(),
        profile.security_revision(),
        HostFingerprint::parse("SHA256:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA")
            .expect("valid fixture fingerprint"),
    )
}

fn valid_request(profile: &ConnectionProfile) -> OpenHumanTerminalRequest {
    OpenHumanTerminalRequest::new(
        profile.id().clone(),
        TerminalSize::new(120, 40).expect("valid fixture terminal size"),
    )
}

fn assert_calls(calls: &CallCounts, profiles: usize, trust: usize, credentials: usize) {
    assert_eq!(calls.profiles.get(), profiles);
    assert_eq!(calls.trust.get(), trust);
    assert_eq!(calls.credentials.get(), credentials);
}

#[test]
fn preparation_finishes_profile_and_trust_gates_before_credential_acquisition() {
    let calls = CallCounts::default();
    let profile = valid_profile(true);
    let expected_fingerprint = matching_trust(&profile).fingerprint().clone();
    let profiles = FakeProfiles {
        calls: &calls,
        result: Ok(Some(profile.clone())),
    };
    let trust = FakeTrust {
        calls: &calls,
        result: Ok(Some(matching_trust(&profile))),
    };
    let credentials = FakeCredentials {
        calls: &calls,
        outcome: CredentialOutcome::Lease(b"human-terminal-lease"),
    };
    let admission = HumanTerminalAdmissionService::new(&profiles, &trust);

    let prepared = admission
        .prepare(valid_request(&profile))
        .expect("profile and trust gates pass");
    assert_calls(&calls, 1, 1, 0);
    assert_eq!(prepared.profile().id(), profile.id());

    let authorized = prepared
        .acquire_credential(&credentials)
        .expect("credential acquisition succeeds");
    assert_eq!(authorized.profile().id(), profile.id());
    assert_eq!(authorized.profile().server_id(), profile.server_id());
    assert_eq!(
        authorized.expected_host_fingerprint(),
        &expected_fingerprint
    );
    assert_eq!(
        authorized.terminal_size(),
        TerminalSize::new(120, 40).expect("valid fixture terminal size")
    );
    assert_eq!(authorized.password(), b"human-terminal-lease");
    assert_calls(&calls, 1, 1, 1);
}

#[test]
fn rejects_a_missing_connection_before_other_ports() {
    let calls = CallCounts::default();
    let profile = valid_profile(true);
    let profiles = FakeProfiles {
        calls: &calls,
        result: Ok(None),
    };
    let trust = FakeTrust {
        calls: &calls,
        result: Ok(None),
    };
    let admission = HumanTerminalAdmissionService::new(&profiles, &trust);

    let error = admission
        .prepare(valid_request(&profile))
        .expect_err("missing profile must be rejected");
    assert_eq!(error, OpenHumanTerminalError::ConnectionNotFound);
    assert_calls(&calls, 1, 0, 0);
}

#[test]
fn maps_connection_repository_failure_before_other_ports() {
    let calls = CallCounts::default();
    let profile = valid_profile(true);
    let profiles = FakeProfiles {
        calls: &calls,
        result: Err(ConnectionRepositoryError::Unavailable),
    };
    let trust = FakeTrust {
        calls: &calls,
        result: Ok(None),
    };
    let admission = HumanTerminalAdmissionService::new(&profiles, &trust);

    let error = admission
        .prepare(valid_request(&profile))
        .expect_err("repository failure must be rejected");
    assert_eq!(
        error,
        OpenHumanTerminalError::ConnectionRepositoryUnavailable
    );
    assert_calls(&calls, 1, 0, 0);
}

#[test]
fn rejects_a_mismatched_connection_record_before_other_ports() {
    let calls = CallCounts::default();
    let stored_profile = valid_profile(true);
    let profiles = FakeProfiles {
        calls: &calls,
        result: Ok(Some(stored_profile.clone())),
    };
    let trust = FakeTrust {
        calls: &calls,
        result: Ok(Some(matching_trust(&stored_profile))),
    };
    let admission = HumanTerminalAdmissionService::new(&profiles, &trust);
    let request = OpenHumanTerminalRequest::new(
        ConnectionProfileId::parse("profile-requested").expect("valid fixture profile id"),
        TerminalSize::new(120, 40).expect("valid fixture terminal size"),
    );

    let error = admission
        .prepare(request)
        .expect_err("mismatched profile must be rejected");
    assert_eq!(error, OpenHumanTerminalError::ConnectionRecordMismatch);
    assert_calls(&calls, 1, 0, 0);
}

#[test]
fn rejects_a_disabled_connection_before_trust_and_credentials() {
    let calls = CallCounts::default();
    let profile = valid_profile(false);
    let profiles = FakeProfiles {
        calls: &calls,
        result: Ok(Some(profile.clone())),
    };
    let trust = FakeTrust {
        calls: &calls,
        result: Ok(Some(matching_trust(&profile))),
    };
    let admission = HumanTerminalAdmissionService::new(&profiles, &trust);

    let error = admission
        .prepare(valid_request(&profile))
        .expect_err("disabled profile must be rejected");
    assert_eq!(error, OpenHumanTerminalError::ConnectionDisabled);
    assert_calls(&calls, 1, 0, 0);
}

#[test]
fn rejects_missing_host_trust_before_credentials() {
    let calls = CallCounts::default();
    let profile = valid_profile(true);
    let profiles = FakeProfiles {
        calls: &calls,
        result: Ok(Some(profile.clone())),
    };
    let trust = FakeTrust {
        calls: &calls,
        result: Ok(None),
    };
    let admission = HumanTerminalAdmissionService::new(&profiles, &trust);

    let error = admission
        .prepare(valid_request(&profile))
        .expect_err("missing Host Trust must be rejected");
    assert_eq!(error, OpenHumanTerminalError::HostTrustRequired);
    assert_calls(&calls, 1, 1, 0);
}

#[test]
fn maps_host_trust_repository_failure_before_credentials() {
    let calls = CallCounts::default();
    let profile = valid_profile(true);
    let profiles = FakeProfiles {
        calls: &calls,
        result: Ok(Some(profile.clone())),
    };
    let trust = FakeTrust {
        calls: &calls,
        result: Err(HostTrustRepositoryError::Unavailable),
    };
    let admission = HumanTerminalAdmissionService::new(&profiles, &trust);

    let error = admission
        .prepare(valid_request(&profile))
        .expect_err("Host Trust repository failure must be rejected");
    assert_eq!(
        error,
        OpenHumanTerminalError::HostTrustRepositoryUnavailable
    );
    assert_calls(&calls, 1, 1, 0);
}

#[test]
fn rejects_stale_host_trust_before_credentials() {
    let calls = CallCounts::default();
    let profile = valid_profile(true);
    let stale_trust = StoredHostTrust::new(
        profile.id().clone(),
        ConnectionSecurityRevision::new(2).expect("valid fixture revision"),
        HostFingerprint::parse("SHA256:BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBQ")
            .expect("valid fixture fingerprint"),
    );
    let profiles = FakeProfiles {
        calls: &calls,
        result: Ok(Some(profile.clone())),
    };
    let trust = FakeTrust {
        calls: &calls,
        result: Ok(Some(stale_trust)),
    };
    let admission = HumanTerminalAdmissionService::new(&profiles, &trust);

    let error = admission
        .prepare(valid_request(&profile))
        .expect_err("stale Host Trust must be rejected");
    assert_eq!(error, OpenHumanTerminalError::HostTrustStale);
    assert_calls(&calls, 1, 1, 0);
}

#[test]
fn rejects_host_trust_from_another_connection_before_credentials() {
    let calls = CallCounts::default();
    let profile = valid_profile(true);
    let unrelated_trust = StoredHostTrust::new(
        ConnectionProfileId::parse("profile-other").expect("valid fixture profile id"),
        profile.security_revision(),
        HostFingerprint::parse("SHA256:CCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCg")
            .expect("valid fixture fingerprint"),
    );
    let profiles = FakeProfiles {
        calls: &calls,
        result: Ok(Some(profile.clone())),
    };
    let trust = FakeTrust {
        calls: &calls,
        result: Ok(Some(unrelated_trust)),
    };
    let admission = HumanTerminalAdmissionService::new(&profiles, &trust);

    let error = admission
        .prepare(valid_request(&profile))
        .expect_err("Host Trust from another profile must be rejected");
    assert_eq!(error, OpenHumanTerminalError::HostTrustStale);
    assert_calls(&calls, 1, 1, 0);
}

#[test]
fn credential_failure_returns_no_authorized_open() {
    let calls = CallCounts::default();
    let profile = valid_profile(true);
    let profiles = FakeProfiles {
        calls: &calls,
        result: Ok(Some(profile.clone())),
    };
    let trust = FakeTrust {
        calls: &calls,
        result: Ok(Some(matching_trust(&profile))),
    };
    let credentials = FakeCredentials {
        calls: &calls,
        outcome: CredentialOutcome::Unavailable,
    };
    let prepared = HumanTerminalAdmissionService::new(&profiles, &trust)
        .prepare(valid_request(&profile))
        .expect("profile and trust gates pass");

    assert!(matches!(
        prepared.acquire_credential(&credentials),
        Err(OpenHumanTerminalError::CredentialUnavailable)
    ));
    assert_calls(&calls, 1, 1, 1);
}

#[test]
fn invalid_terminal_dimensions_are_rejected_before_request_construction() {
    assert_eq!(
        TerminalSize::new(0, 40),
        Err(TerminalValueError::ZeroColumns)
    );
    assert_eq!(TerminalSize::new(120, 0), Err(TerminalValueError::ZeroRows));
}

#[test]
fn terminal_session_identity_requires_a_canonical_uuid_v4() {
    let canonical = "00000000-0000-4000-8000-000000000001";
    assert_eq!(
        HumanTerminalSessionId::parse(canonical)
            .expect("canonical UUID v4")
            .as_str(),
        canonical
    );
    for invalid in [
        "",
        "human-terminal-session",
        "00000000-0000-3000-8000-000000000001",
        "00000000-0000-4000-7000-000000000001",
        "00000000-0000-4000-8000-00000000000A",
    ] {
        assert_eq!(
            HumanTerminalSessionId::parse(invalid),
            Err(TerminalValueError::InvalidSessionId)
        );
    }
}

#[test]
fn sensitive_connection_and_lease_values_are_redacted_from_debug_output() {
    let profile_debug = format!("{:?}", valid_profile(true));
    assert!(!profile_debug.contains("server.example.invalid"));
    assert!(!profile_debug.contains("operator"));
    assert!(!profile_debug.contains("credential-demo"));

    let lease = HumanCredentialLease::from_password(b"secret-lease-handle".to_vec())
        .expect("valid fixture credential lease");
    assert!(!format!("{lease:?}").contains("secret-lease-handle"));
}

#[test]
fn human_credential_lease_rejects_empty_material() {
    assert!(matches!(
        HumanCredentialLease::from_password(Vec::new()),
        Err(TerminalValueError::Empty("credential lease id"))
    ));
}

#[test]
fn human_credential_lease_allows_space_only_material() {
    assert!(HumanCredentialLease::from_password(b"   ".to_vec()).is_ok());
}
