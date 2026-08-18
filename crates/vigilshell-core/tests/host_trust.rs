use std::{
    cell::{Cell, RefCell},
    time::{Duration, Instant},
};

use vigilshell_core::{
    connection::{
        ConnectionDisplayName, ConnectionProfile, ConnectionProfileId, ConnectionRecordVersion,
        ConnectionSecurityRevision, ConnectionValueError, CredentialRef, HostFingerprint,
        ManagedServerId, SshRoute, StoredHostTrust,
    },
    host_trust::{
        AssessHostTrustError, ConfirmHostTrustError, ConfirmHostTrustService, HostKeyAlgorithm,
        HostTrustMutationRepository, HostTrustMutationRepositoryError, HostTrustProbeAssessment,
        HostTrustProbeId, HostTrustProbeStatus, MAXIMUM_PENDING_HOST_TRUST_DECISIONS,
        ObservedHostKey, PENDING_HOST_TRUST_TTL, PendingHostTrustDecision,
        PendingHostTrustDecisions, PendingHostTrustInsertError, PendingHostTrustLookupError,
        StoreHostTrustResult,
    },
    human_terminal::{ConnectionProfileRepository, ConnectionRepositoryError},
};

const FINGERPRINT_A: &str = "SHA256:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
const FINGERPRINT_B: &str = "SHA256:BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBQ";

fn profile(id: &str, revision: u64, enabled: bool) -> ConnectionProfile {
    ConnectionProfile::new(
        ConnectionProfileId::parse(id).expect("valid profile id"),
        ManagedServerId::parse(format!("server-{id}")).expect("valid server id"),
        ConnectionDisplayName::parse(format!("Connection {id}")).expect("valid display name"),
        ConnectionRecordVersion::INITIAL,
        ConnectionSecurityRevision::new(revision).expect("valid security revision"),
        SshRoute::new("host.example.invalid", 22, "operator").expect("valid route"),
        CredentialRef::parse(format!("credential-{id}")).expect("valid credential ref"),
        enabled,
    )
}

fn observed(value: &str) -> ObservedHostKey {
    ObservedHostKey::new(
        HostKeyAlgorithm::Ed25519,
        HostFingerprint::parse(value).expect("valid fingerprint"),
    )
}

fn probe_id(index: usize) -> HostTrustProbeId {
    HostTrustProbeId::parse(format!("00000000-0000-4000-8000-{index:012x}"))
        .expect("valid probe id")
}

fn pending_for(profile: &ConnectionProfile, index: usize) -> PendingHostTrustDecision {
    let assessment = HostTrustProbeAssessment::assess(profile, None, observed(FINGERPRINT_A))
        .expect("valid assessment");
    PendingHostTrustDecision::from_assessment(probe_id(index), assessment)
        .expect("confirmation required")
}

#[test]
fn fingerprint_requires_canonical_unpadded_sha256() {
    let fingerprint = HostFingerprint::parse(FINGERPRINT_A).expect("canonical fingerprint");
    assert_eq!(fingerprint.as_str(), FINGERPRINT_A);

    for invalid in [
        "",
        "SHA1:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
        "SHA256:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=",
        "SHA256:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAB",
        "SHA256:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
    ] {
        assert_eq!(
            HostFingerprint::parse(invalid),
            Err(ConnectionValueError::InvalidHostFingerprint)
        );
    }

    for final_character in [
        'A', 'E', 'I', 'M', 'Q', 'U', 'Y', 'c', 'g', 'k', 'o', 's', 'w', '0', '4', '8',
    ] {
        let mut value = FINGERPRINT_A.to_owned();
        value.pop();
        value.push(final_character);
        HostFingerprint::parse(value).expect("canonical SHA-256 final bits");
    }
}

#[test]
fn fingerprint_and_probe_identity_debug_are_redacted() {
    let fingerprint = HostFingerprint::parse(FINGERPRINT_A).expect("canonical fingerprint");
    assert!(!format!("{fingerprint:?}").contains(FINGERPRINT_A));

    let probe = probe_id(1);
    assert!(!format!("{probe:?}").contains(probe.as_str()));
}

#[test]
fn assessment_distinguishes_first_seen_matches_and_changed() {
    let profile = profile("profile-a", 2, true);
    let first = HostTrustProbeAssessment::assess(&profile, None, observed(FINGERPRINT_A))
        .expect("first observation");
    assert_eq!(first.status(), HostTrustProbeStatus::FirstSeen);
    assert_eq!(first.previous_fingerprint(), None);

    let current = StoredHostTrust::new(
        profile.id().clone(),
        profile.security_revision(),
        HostFingerprint::parse(FINGERPRINT_A).expect("valid fingerprint"),
    );
    let matches =
        HostTrustProbeAssessment::assess(&profile, Some(&current), observed(FINGERPRINT_A))
            .expect("matching observation");
    assert_eq!(matches.status(), HostTrustProbeStatus::Matches);
    assert!(
        PendingHostTrustDecision::from_assessment(probe_id(1), matches).is_err(),
        "matching trust must not create a pending confirmation"
    );

    let changed =
        HostTrustProbeAssessment::assess(&profile, Some(&current), observed(FINGERPRINT_B))
            .expect("changed observation");
    assert_eq!(changed.status(), HostTrustProbeStatus::Changed);
    assert_eq!(changed.previous_fingerprint(), Some(current.fingerprint()));

    let stale_profile = ConnectionProfile::new(
        profile.id().clone(),
        profile.server_id().clone(),
        profile.display_name().clone(),
        profile.record_version(),
        ConnectionSecurityRevision::new(3).expect("valid stale-context revision"),
        profile.route().clone(),
        profile.credential_ref().clone(),
        true,
    );
    let stale_context =
        HostTrustProbeAssessment::assess(&stale_profile, Some(&current), observed(FINGERPRINT_A))
            .expect("stale trust context requires confirmation");
    assert_eq!(stale_context.status(), HostTrustProbeStatus::Changed);
    assert_eq!(
        stale_context.previous_fingerprint(),
        Some(stale_context.observed().fingerprint())
    );
}

#[test]
fn assessment_rejects_a_mismatched_stored_record() {
    let profile = profile("profile-a", 1, true);
    let unrelated = StoredHostTrust::new(
        ConnectionProfileId::parse("profile-b").expect("valid profile id"),
        profile.security_revision(),
        HostFingerprint::parse(FINGERPRINT_A).expect("valid fingerprint"),
    );
    assert_eq!(
        HostTrustProbeAssessment::assess(&profile, Some(&unrelated), observed(FINGERPRINT_A)),
        Err(AssessHostTrustError::StoredRecordMismatch)
    );
}

#[test]
fn pending_decision_is_replaced_per_profile_and_discard_is_single_use() {
    let profile = profile("profile-a", 1, true);
    let now = Instant::now();
    let mut pending = PendingHostTrustDecisions::new();
    pending
        .insert(pending_for(&profile, 1), now)
        .expect("insert first pending decision");
    pending
        .insert(pending_for(&profile, 2), now)
        .expect("replace pending decision for profile");
    assert_eq!(pending.len(), 1);
    assert_eq!(
        pending.discard(&probe_id(1), now),
        Err(PendingHostTrustLookupError::NotFound)
    );
    pending
        .discard(&probe_id(2), now)
        .expect("discard latest decision");
    assert_eq!(
        pending.discard(&probe_id(2), now),
        Err(PendingHostTrustLookupError::NotFound)
    );
}

#[test]
fn expired_pending_decision_is_consumed_and_cannot_be_reused() {
    let profile = profile("profile-a", 1, true);
    let now = Instant::now();
    let mut pending = PendingHostTrustDecisions::new();
    pending
        .insert(pending_for(&profile, 1), now)
        .expect("insert pending decision");
    let expired_at = now + PENDING_HOST_TRUST_TTL + Duration::from_nanos(1);
    assert_eq!(
        pending.discard(&probe_id(1), expired_at),
        Err(PendingHostTrustLookupError::Expired)
    );
    assert!(pending.is_empty());
    assert_eq!(
        pending.discard(&probe_id(1), expired_at),
        Err(PendingHostTrustLookupError::NotFound)
    );
}

#[test]
fn pending_registry_enforces_global_capacity_and_reclaims_expired_entries() {
    let now = Instant::now();
    let mut pending = PendingHostTrustDecisions::new();
    for index in 0..MAXIMUM_PENDING_HOST_TRUST_DECISIONS {
        pending
            .insert(
                pending_for(&profile(&format!("profile-{index}"), 1, true), index),
                now,
            )
            .expect("capacity slot available");
    }
    assert_eq!(
        pending.insert(pending_for(&profile("profile-overflow", 1, true), 100), now,),
        Err(PendingHostTrustInsertError::Capacity)
    );

    pending
        .insert(
            pending_for(&profile("profile-after-expiry", 1, true), 101),
            now + PENDING_HOST_TRUST_TTL + Duration::from_nanos(1),
        )
        .expect("expired entries should release capacity");
    assert_eq!(pending.len(), 1);
}

struct FakeProfiles {
    result: Result<Option<ConnectionProfile>, ConnectionRepositoryError>,
}

impl ConnectionProfileRepository for FakeProfiles {
    fn find(
        &self,
        _profile_id: &ConnectionProfileId,
    ) -> Result<Option<ConnectionProfile>, ConnectionRepositoryError> {
        self.result.clone()
    }
}

struct FakeTrustMutation {
    result: Result<StoreHostTrustResult, HostTrustMutationRepositoryError>,
    calls: Cell<usize>,
    captured: RefCell<Option<StoredHostTrust>>,
}

impl HostTrustMutationRepository for FakeTrustMutation {
    fn replace_if_security_revision(
        &self,
        trust: &StoredHostTrust,
        _expected_security_revision: ConnectionSecurityRevision,
    ) -> Result<StoreHostTrustResult, HostTrustMutationRepositoryError> {
        self.calls.set(self.calls.get() + 1);
        self.captured.replace(Some(trust.clone()));
        self.result
    }
}

#[test]
fn confirmation_rereads_profile_and_persists_exact_revision() {
    let profile = profile("profile-a", 3, true);
    let profiles = FakeProfiles {
        result: Ok(Some(profile.clone())),
    };
    let mutation = FakeTrustMutation {
        result: Ok(StoreHostTrustResult::Stored),
        calls: Cell::new(0),
        captured: RefCell::new(None),
    };
    let service = ConfirmHostTrustService::new(&profiles, &mutation);
    let now = Instant::now();
    let mut pending = PendingHostTrustDecisions::new();
    pending
        .insert(pending_for(&profile, 1), now)
        .expect("insert pending decision");

    let consumed = pending
        .consume_for_confirmation(&probe_id(1), now)
        .expect("consume pending confirmation");
    let stored = service.confirm(consumed).expect("confirmation succeeds");
    assert_eq!(stored.profile_id(), profile.id());
    assert_eq!(stored.security_revision(), profile.security_revision());
    assert_eq!(stored.fingerprint().as_str(), FINGERPRINT_A);
    assert_eq!(mutation.calls.get(), 1);
    assert_eq!(mutation.captured.borrow().as_ref(), Some(&stored));
    assert!(matches!(
        pending.consume_for_confirmation(&probe_id(1), now),
        Err(PendingHostTrustLookupError::NotFound)
    ));
}

#[test]
fn confirmation_fails_closed_before_storage_when_profile_changed_or_disabled() {
    let observed_profile = profile("profile-a", 1, true);
    for (current, expected_error) in [
        (profile("profile-a", 2, true), ConfirmHostTrustError::Stale),
        (
            profile("profile-a", 1, false),
            ConfirmHostTrustError::ConnectionDisabled,
        ),
    ] {
        let profiles = FakeProfiles {
            result: Ok(Some(current)),
        };
        let mutation = FakeTrustMutation {
            result: Ok(StoreHostTrustResult::Stored),
            calls: Cell::new(0),
            captured: RefCell::new(None),
        };
        let service = ConfirmHostTrustService::new(&profiles, &mutation);
        let now = Instant::now();
        let mut pending = PendingHostTrustDecisions::new();
        pending
            .insert(pending_for(&observed_profile, 1), now)
            .expect("insert pending decision");
        let consumed = pending
            .consume_for_confirmation(&probe_id(1), now)
            .expect("consume pending confirmation");
        assert_eq!(service.confirm(consumed), Err(expected_error));
        assert_eq!(mutation.calls.get(), 0);
        assert!(matches!(
            pending.consume_for_confirmation(&probe_id(1), now),
            Err(PendingHostTrustLookupError::NotFound)
        ));
    }
}

#[test]
fn confirmation_maps_transaction_race_outcomes_without_accepting_trust() {
    let profile = profile("profile-a", 1, true);
    for (outcome, expected_error) in [
        (StoreHostTrustResult::Stale, ConfirmHostTrustError::Stale),
        (
            StoreHostTrustResult::NotFound,
            ConfirmHostTrustError::ConnectionNotFound,
        ),
        (
            StoreHostTrustResult::Disabled,
            ConfirmHostTrustError::ConnectionDisabled,
        ),
    ] {
        let profiles = FakeProfiles {
            result: Ok(Some(profile.clone())),
        };
        let mutation = FakeTrustMutation {
            result: Ok(outcome),
            calls: Cell::new(0),
            captured: RefCell::new(None),
        };
        let service = ConfirmHostTrustService::new(&profiles, &mutation);
        let now = Instant::now();
        let mut pending = PendingHostTrustDecisions::new();
        pending
            .insert(pending_for(&profile, 1), now)
            .expect("insert pending decision");
        let consumed = pending
            .consume_for_confirmation(&probe_id(1), now)
            .expect("consume pending confirmation");
        assert_eq!(service.confirm(consumed), Err(expected_error));
        assert_eq!(mutation.calls.get(), 1);
        assert!(matches!(
            pending.consume_for_confirmation(&probe_id(1), now),
            Err(PendingHostTrustLookupError::NotFound)
        ));
    }
}

#[test]
fn expired_confirmation_is_consumed_before_a_storage_token_is_issued() {
    let profile = profile("profile-a", 1, true);
    let inserted_at = Instant::now();
    let mut pending = PendingHostTrustDecisions::new();
    pending
        .insert(pending_for(&profile, 1), inserted_at)
        .expect("insert pending decision");

    assert!(matches!(
        pending.consume_for_confirmation(
            &probe_id(1),
            inserted_at + PENDING_HOST_TRUST_TTL + Duration::from_nanos(1),
        ),
        Err(PendingHostTrustLookupError::Expired)
    ));
    assert!(matches!(
        pending.consume_for_confirmation(&probe_id(1), inserted_at),
        Err(PendingHostTrustLookupError::NotFound)
    ));
}
