use std::{
    collections::HashMap,
    error::Error,
    fmt,
    time::{Duration, Instant},
};

use crate::{
    connection::{
        ConnectionProfile, ConnectionProfileId, ConnectionSecurityRevision, HostFingerprint,
        StoredHostTrust,
    },
    human_terminal::{ConnectionProfileRepository, ConnectionRepositoryError},
};

pub const MAXIMUM_PENDING_HOST_TRUST_DECISIONS: usize = 32;
pub const PENDING_HOST_TRUST_TTL: Duration = Duration::from_secs(60);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum HostKeyAlgorithm {
    Ed25519,
    EcdsaP256,
    EcdsaP384,
    EcdsaP521,
    RsaSha512,
    RsaSha256,
}

impl HostKeyAlgorithm {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Ed25519 => "ssh-ed25519",
            Self::EcdsaP256 => "ecdsa-sha2-nistp256",
            Self::EcdsaP384 => "ecdsa-sha2-nistp384",
            Self::EcdsaP521 => "ecdsa-sha2-nistp521",
            Self::RsaSha512 => "rsa-sha2-512",
            Self::RsaSha256 => "rsa-sha2-256",
        }
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct ObservedHostKey {
    algorithm: HostKeyAlgorithm,
    fingerprint: HostFingerprint,
}

impl ObservedHostKey {
    pub const fn new(algorithm: HostKeyAlgorithm, fingerprint: HostFingerprint) -> Self {
        Self {
            algorithm,
            fingerprint,
        }
    }

    pub const fn algorithm(&self) -> HostKeyAlgorithm {
        self.algorithm
    }

    pub const fn fingerprint(&self) -> &HostFingerprint {
        &self.fingerprint
    }
}

impl fmt::Debug for ObservedHostKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ObservedHostKey")
            .field("algorithm", &self.algorithm)
            .field("fingerprint", &self.fingerprint)
            .finish()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HostTrustProbeStatus {
    FirstSeen,
    Changed,
    Matches,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AssessHostTrustError {
    StoredRecordMismatch,
}

impl fmt::Display for AssessHostTrustError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("stored host trust does not match the requested connection")
    }
}

impl Error for AssessHostTrustError {}

#[derive(Clone, PartialEq, Eq)]
pub struct HostTrustProbeAssessment {
    profile_id: ConnectionProfileId,
    security_revision: ConnectionSecurityRevision,
    status: HostTrustProbeStatus,
    observed: ObservedHostKey,
    previous_fingerprint: Option<HostFingerprint>,
}

impl HostTrustProbeAssessment {
    pub fn assess(
        profile: &ConnectionProfile,
        stored: Option<&StoredHostTrust>,
        observed: ObservedHostKey,
    ) -> Result<Self, AssessHostTrustError> {
        if stored.is_some_and(|trust| trust.profile_id() != profile.id()) {
            return Err(AssessHostTrustError::StoredRecordMismatch);
        }

        let status = match stored {
            None => HostTrustProbeStatus::FirstSeen,
            Some(trust)
                if trust.matches(profile) && trust.fingerprint() == observed.fingerprint() =>
            {
                HostTrustProbeStatus::Matches
            }
            Some(_) => HostTrustProbeStatus::Changed,
        };
        let previous_fingerprint = stored.map(|trust| trust.fingerprint().clone());

        Ok(Self {
            profile_id: profile.id().clone(),
            security_revision: profile.security_revision(),
            status,
            observed,
            previous_fingerprint,
        })
    }

    pub fn profile_id(&self) -> &ConnectionProfileId {
        &self.profile_id
    }

    pub const fn security_revision(&self) -> ConnectionSecurityRevision {
        self.security_revision
    }

    pub const fn status(&self) -> HostTrustProbeStatus {
        self.status
    }

    pub const fn observed(&self) -> &ObservedHostKey {
        &self.observed
    }

    pub const fn previous_fingerprint(&self) -> Option<&HostFingerprint> {
        self.previous_fingerprint.as_ref()
    }
}

impl fmt::Debug for HostTrustProbeAssessment {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("HostTrustProbeAssessment")
            .field("profile_id", &self.profile_id)
            .field("security_revision", &self.security_revision)
            .field("status", &self.status)
            .field("observed", &self.observed)
            .field("previous_fingerprint", &self.previous_fingerprint)
            .finish()
    }
}

#[derive(Clone, PartialEq, Eq, Hash)]
pub struct HostTrustProbeId(String);

impl HostTrustProbeId {
    pub fn parse(value: impl Into<String>) -> Result<Self, HostTrustValueError> {
        let value = value.into();
        let bytes = value.as_bytes();
        let hyphens_are_canonical = [8, 13, 18, 23]
            .into_iter()
            .all(|index| bytes.get(index) == Some(&b'-'));
        let hexadecimal_is_canonical = bytes.iter().enumerate().all(|(index, byte)| {
            [8, 13, 18, 23].contains(&index)
                || byte.is_ascii_digit()
                || (b'a'..=b'f').contains(byte)
        });
        let is_version_four = bytes.get(14) == Some(&b'4');
        let has_rfc_variant = bytes
            .get(19)
            .is_some_and(|byte| matches!(byte, b'8' | b'9' | b'a' | b'b'));

        if bytes.len() != 36
            || !hyphens_are_canonical
            || !hexadecimal_is_canonical
            || !is_version_four
            || !has_rfc_variant
        {
            return Err(HostTrustValueError::InvalidProbeId);
        }

        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for HostTrustProbeId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("HostTrustProbeId([opaque])")
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HostTrustValueError {
    InvalidProbeId,
    AlreadyTrusted,
}

impl fmt::Display for HostTrustValueError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::InvalidProbeId => "host trust probe id must be a canonical UUID v4",
            Self::AlreadyTrusted => "matching host trust does not require confirmation",
        };
        formatter.write_str(message)
    }
}

impl Error for HostTrustValueError {}

#[derive(PartialEq, Eq)]
pub struct PendingHostTrustDecision {
    probe_id: HostTrustProbeId,
    profile_id: ConnectionProfileId,
    security_revision: ConnectionSecurityRevision,
    status: HostTrustProbeStatus,
    observed: ObservedHostKey,
    previous_fingerprint: Option<HostFingerprint>,
}

impl PendingHostTrustDecision {
    pub fn from_assessment(
        probe_id: HostTrustProbeId,
        assessment: HostTrustProbeAssessment,
    ) -> Result<Self, HostTrustValueError> {
        if assessment.status == HostTrustProbeStatus::Matches {
            return Err(HostTrustValueError::AlreadyTrusted);
        }

        Ok(Self {
            probe_id,
            profile_id: assessment.profile_id,
            security_revision: assessment.security_revision,
            status: assessment.status,
            observed: assessment.observed,
            previous_fingerprint: assessment.previous_fingerprint,
        })
    }

    pub fn probe_id(&self) -> &HostTrustProbeId {
        &self.probe_id
    }

    pub fn profile_id(&self) -> &ConnectionProfileId {
        &self.profile_id
    }

    pub const fn security_revision(&self) -> ConnectionSecurityRevision {
        self.security_revision
    }

    pub const fn status(&self) -> HostTrustProbeStatus {
        self.status
    }

    pub const fn observed(&self) -> &ObservedHostKey {
        &self.observed
    }

    pub const fn previous_fingerprint(&self) -> Option<&HostFingerprint> {
        self.previous_fingerprint.as_ref()
    }
}

impl fmt::Debug for PendingHostTrustDecision {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PendingHostTrustDecision")
            .field("probe_id", &self.probe_id)
            .field("profile_id", &self.profile_id)
            .field("security_revision", &self.security_revision)
            .field("status", &self.status)
            .field("observed", &self.observed)
            .field("previous_fingerprint", &self.previous_fingerprint)
            .finish()
    }
}

pub struct ConsumedHostTrustDecision(PendingHostTrustDecision);

impl fmt::Debug for ConsumedHostTrustDecision {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("ConsumedHostTrustDecision")
            .field(&self.0)
            .finish()
    }
}

struct PendingEntry {
    decision: PendingHostTrustDecision,
    expires_at: Instant,
}

pub struct PendingHostTrustDecisions {
    by_probe: HashMap<HostTrustProbeId, PendingEntry>,
    by_profile: HashMap<ConnectionProfileId, HostTrustProbeId>,
}

impl PendingHostTrustDecisions {
    pub fn new() -> Self {
        Self {
            by_probe: HashMap::new(),
            by_profile: HashMap::new(),
        }
    }

    pub fn insert(
        &mut self,
        decision: PendingHostTrustDecision,
        now: Instant,
    ) -> Result<(), PendingHostTrustInsertError> {
        self.purge_expired(now);
        if self.by_probe.contains_key(decision.probe_id()) {
            return Err(PendingHostTrustInsertError::DuplicateProbeId);
        }

        let replaces_existing_profile = self.by_profile.contains_key(decision.profile_id());
        if !replaces_existing_profile && self.by_probe.len() >= MAXIMUM_PENDING_HOST_TRUST_DECISIONS
        {
            return Err(PendingHostTrustInsertError::Capacity);
        }

        let expires_at = now
            .checked_add(PENDING_HOST_TRUST_TTL)
            .ok_or(PendingHostTrustInsertError::ClockOutOfRange)?;
        if let Some(previous_probe_id) = self.by_profile.remove(decision.profile_id()) {
            self.by_probe.remove(&previous_probe_id);
        }

        self.by_profile
            .insert(decision.profile_id().clone(), decision.probe_id().clone());
        self.by_probe.insert(
            decision.probe_id().clone(),
            PendingEntry {
                decision,
                expires_at,
            },
        );
        Ok(())
    }

    pub fn discard(
        &mut self,
        probe_id: &HostTrustProbeId,
        now: Instant,
    ) -> Result<(), PendingHostTrustLookupError> {
        self.remove(probe_id, now).map(|_| ())
    }

    pub fn consume_for_confirmation(
        &mut self,
        probe_id: &HostTrustProbeId,
        now: Instant,
    ) -> Result<ConsumedHostTrustDecision, PendingHostTrustLookupError> {
        self.remove(probe_id, now).map(ConsumedHostTrustDecision)
    }

    pub fn len(&self) -> usize {
        self.by_probe.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_probe.is_empty()
    }

    fn remove(
        &mut self,
        probe_id: &HostTrustProbeId,
        now: Instant,
    ) -> Result<PendingHostTrustDecision, PendingHostTrustLookupError> {
        let entry = self
            .by_probe
            .remove(probe_id)
            .ok_or(PendingHostTrustLookupError::NotFound)?;
        self.by_profile.remove(entry.decision.profile_id());
        if now >= entry.expires_at {
            return Err(PendingHostTrustLookupError::Expired);
        }
        Ok(entry.decision)
    }

    fn purge_expired(&mut self, now: Instant) {
        let expired: Vec<_> = self
            .by_probe
            .iter()
            .filter_map(|(probe_id, entry)| (now >= entry.expires_at).then_some(probe_id.clone()))
            .collect();
        for probe_id in expired {
            if let Some(entry) = self.by_probe.remove(&probe_id) {
                self.by_profile.remove(entry.decision.profile_id());
            }
        }
    }
}

impl Default for PendingHostTrustDecisions {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PendingHostTrustInsertError {
    Capacity,
    DuplicateProbeId,
    ClockOutOfRange,
}

impl fmt::Display for PendingHostTrustInsertError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::Capacity => "pending host trust capacity is exhausted",
            Self::DuplicateProbeId => "host trust probe id is already pending",
            Self::ClockOutOfRange => "pending host trust expiry cannot be represented",
        };
        formatter.write_str(message)
    }
}

impl Error for PendingHostTrustInsertError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PendingHostTrustLookupError {
    NotFound,
    Expired,
}

impl fmt::Display for PendingHostTrustLookupError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::NotFound => "pending host trust decision was not found",
            Self::Expired => "pending host trust decision has expired",
        };
        formatter.write_str(message)
    }
}

impl Error for PendingHostTrustLookupError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StoreHostTrustResult {
    Stored,
    Stale,
    NotFound,
    Disabled,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HostTrustMutationRepositoryError {
    Unavailable,
}

impl fmt::Display for HostTrustMutationRepositoryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("host trust mutation repository is unavailable")
    }
}

impl Error for HostTrustMutationRepositoryError {}

pub trait HostTrustMutationRepository {
    fn replace_if_security_revision(
        &self,
        trust: &StoredHostTrust,
        expected_security_revision: ConnectionSecurityRevision,
    ) -> Result<StoreHostTrustResult, HostTrustMutationRepositoryError>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConfirmHostTrustError {
    DecisionNotFound,
    DecisionExpired,
    ConnectionRepositoryUnavailable,
    ConnectionNotFound,
    ConnectionRecordMismatch,
    ConnectionDisabled,
    Stale,
    StorageUnavailable,
}

impl fmt::Display for ConfirmHostTrustError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::DecisionNotFound => "pending host trust decision was not found",
            Self::DecisionExpired => "pending host trust decision has expired",
            Self::ConnectionRepositoryUnavailable => "connection repository is unavailable",
            Self::ConnectionNotFound => "connection profile was not found",
            Self::ConnectionRecordMismatch => "connection repository returned a mismatched profile",
            Self::ConnectionDisabled => "connection profile is disabled",
            Self::Stale => "host trust probe is stale",
            Self::StorageUnavailable => "host trust storage is unavailable",
        };
        formatter.write_str(message)
    }
}

impl Error for ConfirmHostTrustError {}

pub struct ConfirmHostTrustService<'a, Profiles, Trust> {
    profiles: &'a Profiles,
    trust: &'a Trust,
}

impl<'a, Profiles, Trust> ConfirmHostTrustService<'a, Profiles, Trust>
where
    Profiles: ConnectionProfileRepository,
    Trust: HostTrustMutationRepository,
{
    pub const fn new(profiles: &'a Profiles, trust: &'a Trust) -> Self {
        Self { profiles, trust }
    }

    pub fn confirm(
        &self,
        consumed: ConsumedHostTrustDecision,
    ) -> Result<StoredHostTrust, ConfirmHostTrustError> {
        let pending = consumed.0;
        let profile = self
            .profiles
            .find(pending.profile_id())
            .map_err(map_connection_repository_error)?
            .ok_or(ConfirmHostTrustError::ConnectionNotFound)?;
        if profile.id() != pending.profile_id() {
            return Err(ConfirmHostTrustError::ConnectionRecordMismatch);
        }
        if !profile.is_enabled() {
            return Err(ConfirmHostTrustError::ConnectionDisabled);
        }
        if profile.security_revision() != pending.security_revision() {
            return Err(ConfirmHostTrustError::Stale);
        }

        let stored = StoredHostTrust::new(
            pending.profile_id,
            pending.security_revision,
            pending.observed.fingerprint,
        );
        match self
            .trust
            .replace_if_security_revision(&stored, stored.security_revision())
        {
            Ok(StoreHostTrustResult::Stored) => Ok(stored),
            Ok(StoreHostTrustResult::Stale) => Err(ConfirmHostTrustError::Stale),
            Ok(StoreHostTrustResult::NotFound) => Err(ConfirmHostTrustError::ConnectionNotFound),
            Ok(StoreHostTrustResult::Disabled) => Err(ConfirmHostTrustError::ConnectionDisabled),
            Err(HostTrustMutationRepositoryError::Unavailable) => {
                Err(ConfirmHostTrustError::StorageUnavailable)
            }
        }
    }
}

fn map_connection_repository_error(_: ConnectionRepositoryError) -> ConfirmHostTrustError {
    ConfirmHostTrustError::ConnectionRepositoryUnavailable
}
