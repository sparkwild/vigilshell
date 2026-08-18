use std::{
    collections::VecDeque,
    ffi::OsString,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, SyncSender, TrySendError},
    },
    time::Duration,
};

use vigilshell_core::{
    connection::{ConnectionProfile, ConnectionProfileId, ConnectionValueError, CredentialRef},
    human_terminal::{PreparedHumanTerminalOpen, TerminalSize},
};

use crate::human_terminal_runtime::TerminalControlSendError;

pub(crate) const DEBUG_TERMINAL_FIXTURE_ENV: &str = "VIGILSHELL_DEBUG_TERMINAL_FIXTURE";
pub(crate) const DEBUG_FIXTURE_BANNER: &[u8] =
    "\r\nVigilShell debug fixture · no network\r\nType ordinary text to echo. Press Ctrl+D to simulate a disconnect.\r\n"
        .as_bytes();

const READY_PROFILE_ID: &str = "fixture-ready";
const READY_SERVER_ID: &str = "server-ready";
const READY_HOST: &str = "ready.vigilshell.invalid";
const READY_USERNAME: &str = "operator-ready";
const READY_CREDENTIAL_REF: &str = "credential-ref-ready";
const READY_FINGERPRINT: &str = "SHA256:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
const STALE_PROFILE_ID: &str = "fixture-stale";
const STALE_SERVER_ID: &str = "server-stale";
const STALE_HOST: &str = "stale.vigilshell.invalid";
const STALE_USERNAME: &str = "operator-stale";
const STALE_CREDENTIAL_REF: &str = "credential-ref-stale";
const DISABLED_PROFILE_ID: &str = "fixture-disabled";
const DISABLED_SERVER_ID: &str = "server-disabled";
const DISABLED_HOST: &str = "disabled.vigilshell.invalid";
const DISABLED_USERNAME: &str = "operator-disabled";
const DISABLED_CREDENTIAL_REF: &str = "credential-ref-disabled";
const FIXTURE_PORT: u16 = 22;
const CONTROL_QUEUE_CAPACITY: usize = 64;
const INITIAL_OUTPUT_CREDIT: usize = 256 * 1024;
const MAXIMUM_OUTPUT_CREDIT: usize = 1024 * 1024;
const MAXIMUM_BUFFERED_OUTPUT: usize = 1024 * 1024;
const MAXIMUM_OUTPUT_CHUNK: usize = 32 * 1024;
const CLOSE_POLL_INTERVAL: Duration = Duration::from_millis(10);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FixtureProfileKind {
    Ready,
    Stale,
    Disabled,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DebugCredentialSelection {
    Fixture,
    Production,
    ReservedMismatch,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DebugTerminalSelection {
    Fixture,
    Production,
    ReservedMismatch,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DebugTerminalLifecycle {
    Opened,
    Closed,
    Disconnected,
    ProtocolFailure,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct DebugTerminalOutputError;

pub(crate) trait DebugTerminalOutputSink: Send + 'static {
    fn send_data(&mut self, data: Vec<u8>) -> Result<(), DebugTerminalOutputError>;

    fn send_lifecycle(
        &mut self,
        lifecycle: DebugTerminalLifecycle,
    ) -> Result<(), DebugTerminalOutputError>;
}

pub(crate) fn fixture_enabled(
    requested: Option<OsString>,
    database_override: Option<&OsString>,
) -> Result<bool, &'static str> {
    let Some(requested) = requested else {
        return Ok(false);
    };
    if requested != "1" {
        return Err("debug terminal fixture switch must be exactly 1");
    }
    if database_override.is_none() {
        return Err("debug terminal fixture requires an isolated database override");
    }
    Ok(true)
}

pub(crate) fn is_reserved_profile_id(profile_id: &ConnectionProfileId) -> bool {
    matches!(
        profile_id.as_str(),
        READY_PROFILE_ID | STALE_PROFILE_ID | DISABLED_PROFILE_ID
    )
}

pub(crate) fn reserved_credential_refs() -> Result<[CredentialRef; 3], ConnectionValueError> {
    Ok([
        CredentialRef::parse(READY_CREDENTIAL_REF)?,
        CredentialRef::parse(STALE_CREDENTIAL_REF)?,
        CredentialRef::parse(DISABLED_CREDENTIAL_REF)?,
    ])
}

pub(crate) fn classify_credential(
    profile: &ConnectionProfile,
    fixture_enabled: bool,
) -> DebugCredentialSelection {
    match classify_profile(profile) {
        Some(_) if fixture_enabled => DebugCredentialSelection::Fixture,
        Some(_) => DebugCredentialSelection::ReservedMismatch,
        None if is_reserved_profile_id(profile.id()) => DebugCredentialSelection::ReservedMismatch,
        None => DebugCredentialSelection::Production,
    }
}

pub(crate) fn classify_terminal(
    prepared: &PreparedHumanTerminalOpen,
    fixture_enabled: bool,
) -> DebugTerminalSelection {
    classify_terminal_parts(
        prepared.profile(),
        prepared.expected_host_fingerprint().as_str(),
        fixture_enabled,
    )
}

fn classify_terminal_parts(
    profile: &ConnectionProfile,
    expected_fingerprint: &str,
    fixture_enabled: bool,
) -> DebugTerminalSelection {
    match classify_credential(profile, fixture_enabled) {
        DebugCredentialSelection::Fixture
            if classify_profile(profile) == Some(FixtureProfileKind::Ready)
                && expected_fingerprint == READY_FINGERPRINT =>
        {
            DebugTerminalSelection::Fixture
        }
        DebugCredentialSelection::Production => DebugTerminalSelection::Production,
        DebugCredentialSelection::Fixture | DebugCredentialSelection::ReservedMismatch => {
            DebugTerminalSelection::ReservedMismatch
        }
    }
}

fn classify_profile(profile: &ConnectionProfile) -> Option<FixtureProfileKind> {
    let (kind, server_id, host, username, credential_ref, security_revision, enabled) =
        match profile.id().as_str() {
            READY_PROFILE_ID => (
                FixtureProfileKind::Ready,
                READY_SERVER_ID,
                READY_HOST,
                READY_USERNAME,
                READY_CREDENTIAL_REF,
                1,
                true,
            ),
            STALE_PROFILE_ID => (
                FixtureProfileKind::Stale,
                STALE_SERVER_ID,
                STALE_HOST,
                STALE_USERNAME,
                STALE_CREDENTIAL_REF,
                2,
                true,
            ),
            DISABLED_PROFILE_ID => (
                FixtureProfileKind::Disabled,
                DISABLED_SERVER_ID,
                DISABLED_HOST,
                DISABLED_USERNAME,
                DISABLED_CREDENTIAL_REF,
                1,
                false,
            ),
            _ => return None,
        };

    let route = profile.route();
    (profile.server_id().as_str() == server_id
        && route.host() == host
        && route.port() == FIXTURE_PORT
        && route.username() == username
        && profile.credential_ref().as_str() == credential_ref
        && profile.security_revision().get() == security_revision
        && profile.is_enabled() == enabled)
        .then_some(kind)
}

pub(crate) struct EstablishedDebugTerminal;

impl EstablishedDebugTerminal {
    pub(crate) fn new() -> Self {
        Self
    }

    pub(crate) fn into_owner<Output>(
        self,
        output: Output,
    ) -> (DebugTerminalControlHandle, DebugTerminalOwner<Output>)
    where
        Output: DebugTerminalOutputSink,
    {
        let (sender, receiver) = mpsc::sync_channel(CONTROL_QUEUE_CAPACITY);
        let closed = Arc::new(AtomicBool::new(false));
        (
            DebugTerminalControlHandle {
                sender,
                closed: Arc::clone(&closed),
            },
            DebugTerminalOwner {
                output,
                receiver,
                closed,
                credit: INITIAL_OUTPUT_CREDIT,
                buffered: VecDeque::new(),
                buffered_bytes: 0,
            },
        )
    }
}

#[derive(Clone)]
pub(crate) struct DebugTerminalControlHandle {
    sender: SyncSender<DebugTerminalControl>,
    closed: Arc<AtomicBool>,
}

impl DebugTerminalControlHandle {
    pub(crate) fn try_input(&self, input: Vec<u8>) -> Result<(), TerminalControlSendError> {
        self.try_send(DebugTerminalControl::Input(input))
    }

    pub(crate) fn try_resize(&self, size: TerminalSize) -> Result<(), TerminalControlSendError> {
        self.try_send(DebugTerminalControl::Resize(size))
    }

    pub(crate) fn try_grant_credit(&self, bytes: u32) -> Result<(), TerminalControlSendError> {
        self.try_send(DebugTerminalControl::GrantCredit(bytes as usize))
    }

    pub(crate) fn close(&self) {
        self.closed.store(true, Ordering::Release);
        let _ = self.sender.try_send(DebugTerminalControl::Wake);
    }

    fn try_send(&self, control: DebugTerminalControl) -> Result<(), TerminalControlSendError> {
        if self.closed.load(Ordering::Acquire) {
            return Err(TerminalControlSendError::Closed);
        }
        self.sender.try_send(control).map_err(|error| match error {
            TrySendError::Full(_) => TerminalControlSendError::QueueFull,
            TrySendError::Disconnected(_) => TerminalControlSendError::Closed,
        })
    }
}

enum DebugTerminalControl {
    Input(Vec<u8>),
    Resize(TerminalSize),
    GrantCredit(usize),
    Wake,
}

pub(crate) struct DebugTerminalOwner<Output>
where
    Output: DebugTerminalOutputSink,
{
    output: Output,
    receiver: Receiver<DebugTerminalControl>,
    closed: Arc<AtomicBool>,
    credit: usize,
    buffered: VecDeque<Vec<u8>>,
    buffered_bytes: usize,
}

impl<Output> DebugTerminalOwner<Output>
where
    Output: DebugTerminalOutputSink,
{
    pub(crate) fn run(mut self) {
        if self
            .output
            .send_lifecycle(DebugTerminalLifecycle::Opened)
            .is_err()
        {
            return;
        }
        if !self.enqueue(DEBUG_FIXTURE_BANNER.to_vec()) || !self.flush_output() {
            return;
        }

        loop {
            if self.closed.load(Ordering::Acquire) {
                let _ = self.output.send_lifecycle(DebugTerminalLifecycle::Closed);
                return;
            }
            match self.receiver.recv_timeout(CLOSE_POLL_INTERVAL) {
                Ok(DebugTerminalControl::Input(input)) if input.as_slice() == [0x04] => {
                    let _ = self
                        .output
                        .send_lifecycle(DebugTerminalLifecycle::Disconnected);
                    return;
                }
                Ok(DebugTerminalControl::Input(input)) => {
                    if !self.enqueue(input) || !self.flush_output() {
                        return;
                    }
                }
                Ok(DebugTerminalControl::Resize(size)) => {
                    let notice = format!(
                        "\r\n[debug fixture resized to {}x{}]\r\n",
                        size.columns(),
                        size.rows()
                    )
                    .into_bytes();
                    if !self.enqueue(notice) || !self.flush_output() {
                        return;
                    }
                }
                Ok(DebugTerminalControl::GrantCredit(bytes)) => {
                    self.credit = self.credit.saturating_add(bytes).min(MAXIMUM_OUTPUT_CREDIT);
                    if !self.flush_output() {
                        return;
                    }
                }
                Ok(DebugTerminalControl::Wake) | Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    let _ = self.output.send_lifecycle(DebugTerminalLifecycle::Closed);
                    return;
                }
            }
        }
    }

    fn enqueue(&mut self, data: Vec<u8>) -> bool {
        if data.is_empty() {
            return true;
        }
        if self.buffered_bytes.saturating_add(data.len()) > MAXIMUM_BUFFERED_OUTPUT {
            let _ = self
                .output
                .send_lifecycle(DebugTerminalLifecycle::ProtocolFailure);
            return false;
        }
        self.buffered_bytes += data.len();
        self.buffered.push_back(data);
        true
    }

    fn flush_output(&mut self) -> bool {
        while self.credit > 0 {
            let Some(mut data) = self.buffered.pop_front() else {
                return true;
            };
            let chunk_len = data.len().min(self.credit).min(MAXIMUM_OUTPUT_CHUNK);
            let remainder = data.split_off(chunk_len);
            self.credit -= data.len();
            self.buffered_bytes -= data.len();
            if !remainder.is_empty() {
                self.buffered.push_front(remainder);
            }
            if self.output.send_data(data).is_err() {
                return false;
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;

    use vigilshell_core::connection::{
        ConnectionDisplayName, ConnectionProfileId, ConnectionRecordVersion,
        ConnectionSecurityRevision, CredentialRef, ManagedServerId, SshRoute,
    };

    use super::*;

    enum ObservedOutput {
        Data(Vec<u8>),
        Lifecycle(DebugTerminalLifecycle),
    }

    struct TestSink(mpsc::Sender<ObservedOutput>);

    impl DebugTerminalOutputSink for TestSink {
        fn send_data(&mut self, data: Vec<u8>) -> Result<(), DebugTerminalOutputError> {
            self.0
                .send(ObservedOutput::Data(data))
                .map_err(|_| DebugTerminalOutputError)
        }

        fn send_lifecycle(
            &mut self,
            lifecycle: DebugTerminalLifecycle,
        ) -> Result<(), DebugTerminalOutputError> {
            self.0
                .send(ObservedOutput::Lifecycle(lifecycle))
                .map_err(|_| DebugTerminalOutputError)
        }
    }

    struct FailOpenedSink;

    impl DebugTerminalOutputSink for FailOpenedSink {
        fn send_data(&mut self, _data: Vec<u8>) -> Result<(), DebugTerminalOutputError> {
            Err(DebugTerminalOutputError)
        }

        fn send_lifecycle(
            &mut self,
            _lifecycle: DebugTerminalLifecycle,
        ) -> Result<(), DebugTerminalOutputError> {
            Err(DebugTerminalOutputError)
        }
    }

    fn profile(
        profile_id: &str,
        server_id: &str,
        host: &str,
        username: &str,
        credential_ref: &str,
        revision: u64,
        enabled: bool,
    ) -> ConnectionProfile {
        ConnectionProfile::new(
            ConnectionProfileId::parse(profile_id).expect("profile id"),
            ManagedServerId::parse(server_id).expect("server id"),
            ConnectionDisplayName::parse(server_id).expect("display name"),
            ConnectionRecordVersion::INITIAL,
            ConnectionSecurityRevision::new(revision).expect("revision"),
            SshRoute::new(host, 22, username).expect("route"),
            CredentialRef::parse(credential_ref).expect("credential ref"),
            enabled,
        )
    }

    #[test]
    fn fixture_opt_in_requires_exact_switch_and_database_override() {
        let path = OsString::from("/tmp/fixture.sqlite3");
        assert_eq!(fixture_enabled(None, Some(&path)), Ok(false));
        assert_eq!(
            fixture_enabled(Some(OsString::from("1")), Some(&path)),
            Ok(true)
        );
        assert!(fixture_enabled(Some(OsString::from("true")), Some(&path)).is_err());
        assert!(fixture_enabled(Some(OsString::from("1")), None).is_err());
    }

    #[test]
    fn credential_classification_is_exact_and_reserved_mismatches_fail_closed() {
        let ready = profile(
            READY_PROFILE_ID,
            READY_SERVER_ID,
            READY_HOST,
            READY_USERNAME,
            READY_CREDENTIAL_REF,
            1,
            true,
        );
        assert_eq!(
            classify_credential(&ready, true),
            DebugCredentialSelection::Fixture
        );
        assert_eq!(
            classify_credential(&ready, false),
            DebugCredentialSelection::ReservedMismatch
        );

        let changed = profile(
            READY_PROFILE_ID,
            READY_SERVER_ID,
            "changed.vigilshell.invalid",
            READY_USERNAME,
            READY_CREDENTIAL_REF,
            2,
            true,
        );
        assert_eq!(
            classify_credential(&changed, true),
            DebugCredentialSelection::ReservedMismatch
        );

        let ordinary = profile(
            "ordinary-profile",
            READY_SERVER_ID,
            READY_HOST,
            READY_USERNAME,
            READY_CREDENTIAL_REF,
            1,
            true,
        );
        assert_eq!(
            classify_credential(&ordinary, true),
            DebugCredentialSelection::Production
        );
    }

    #[test]
    fn actor_emits_opened_banner_echo_resize_disconnect_in_order() {
        let (sender, receiver) = mpsc::channel();
        let (control, owner) = EstablishedDebugTerminal::new().into_owner(TestSink(sender));
        let thread = std::thread::spawn(move || owner.run());

        assert!(matches!(
            receiver.recv_timeout(Duration::from_secs(1)),
            Ok(ObservedOutput::Lifecycle(DebugTerminalLifecycle::Opened))
        ));
        assert!(matches!(
            receiver.recv_timeout(Duration::from_secs(1)),
            Ok(ObservedOutput::Data(data)) if data == DEBUG_FIXTURE_BANNER
        ));

        control.try_input(vec![0xff, 0x00]).expect("echo input");
        assert!(matches!(
            receiver.recv_timeout(Duration::from_secs(1)),
            Ok(ObservedOutput::Data(data)) if data == [0xff, 0x00]
        ));

        control
            .try_resize(TerminalSize::new(120, 40).expect("terminal size"))
            .expect("resize");
        assert!(matches!(
            receiver.recv_timeout(Duration::from_secs(1)),
            Ok(ObservedOutput::Data(data)) if data == b"\r\n[debug fixture resized to 120x40]\r\n"
        ));

        control.try_input(vec![0x04]).expect("disconnect");
        assert!(matches!(
            receiver.recv_timeout(Duration::from_secs(1)),
            Ok(ObservedOutput::Lifecycle(
                DebugTerminalLifecycle::Disconnected
            ))
        ));
        thread.join().expect("owner thread");
    }

    #[test]
    fn actor_close_is_reachable_and_emits_closed() {
        let (sender, receiver) = mpsc::channel();
        let (control, owner) = EstablishedDebugTerminal::new().into_owner(TestSink(sender));
        let thread = std::thread::spawn(move || owner.run());
        let _ = receiver.recv_timeout(Duration::from_secs(1));
        let _ = receiver.recv_timeout(Duration::from_secs(1));
        control.close();
        assert!(matches!(
            receiver.recv_timeout(Duration::from_secs(1)),
            Ok(ObservedOutput::Lifecycle(DebugTerminalLifecycle::Closed))
        ));
        thread.join().expect("owner thread");
    }

    #[test]
    fn control_queue_is_bounded_before_the_owner_starts() {
        let (sender, _receiver) = mpsc::channel();
        let (control, _owner) = EstablishedDebugTerminal::new().into_owner(TestSink(sender));
        for _ in 0..CONTROL_QUEUE_CAPACITY {
            control.try_input(vec![1]).expect("queue within capacity");
        }
        assert_eq!(
            control.try_input(vec![1]),
            Err(TerminalControlSendError::QueueFull)
        );
    }

    #[test]
    fn output_credit_exhaustion_pauses_and_grant_resumes_exact_bytes() {
        let (sender, receiver) = mpsc::channel();
        let (control, owner) = EstablishedDebugTerminal::new().into_owner(TestSink(sender));
        let thread = std::thread::spawn(move || owner.run());
        assert!(matches!(
            receiver.recv_timeout(Duration::from_secs(1)),
            Ok(ObservedOutput::Lifecycle(DebugTerminalLifecycle::Opened))
        ));
        assert!(matches!(
            receiver.recv_timeout(Duration::from_secs(1)),
            Ok(ObservedOutput::Data(data)) if data == DEBUG_FIXTURE_BANNER
        ));

        control
            .try_input(vec![7; INITIAL_OUTPUT_CREDIT])
            .expect("buffer large echo");
        let expected_before_grant = INITIAL_OUTPUT_CREDIT - DEBUG_FIXTURE_BANNER.len();
        let mut observed_before_grant = 0;
        while observed_before_grant < expected_before_grant {
            match receiver
                .recv_timeout(Duration::from_secs(1))
                .expect("credited data")
            {
                ObservedOutput::Data(data) => observed_before_grant += data.len(),
                ObservedOutput::Lifecycle(_) => panic!("unexpected lifecycle"),
            }
        }
        assert_eq!(observed_before_grant, expected_before_grant);
        assert!(receiver.recv_timeout(Duration::from_millis(30)).is_err());

        control
            .try_grant_credit(DEBUG_FIXTURE_BANNER.len() as u32)
            .expect("grant exact remainder");
        assert!(matches!(
            receiver.recv_timeout(Duration::from_secs(1)),
            Ok(ObservedOutput::Data(data)) if data == vec![7; DEBUG_FIXTURE_BANNER.len()]
        ));
        control.close();
        thread.join().expect("owner thread");
    }

    #[test]
    fn opened_delivery_failure_ends_the_owner_and_closes_controls() {
        let (control, owner) = EstablishedDebugTerminal::new().into_owner(FailOpenedSink);
        owner.run();
        assert_eq!(
            control.try_input(vec![1]),
            Err(TerminalControlSendError::Closed)
        );
    }

    #[test]
    fn terminal_classification_requires_the_expected_fingerprint() {
        let ready = profile(
            READY_PROFILE_ID,
            READY_SERVER_ID,
            READY_HOST,
            READY_USERNAME,
            READY_CREDENTIAL_REF,
            1,
            true,
        );
        assert_eq!(
            classify_terminal_parts(&ready, READY_FINGERPRINT, true),
            DebugTerminalSelection::Fixture
        );
        assert_eq!(
            classify_terminal_parts(
                &ready,
                "SHA256:BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBQ",
                true,
            ),
            DebugTerminalSelection::ReservedMismatch
        );
    }
}
