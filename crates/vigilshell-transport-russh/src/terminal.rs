use std::{
    collections::VecDeque,
    error::Error,
    fmt,
    net::{Shutdown, TcpStream as StandardTcpStream},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use russh::{
    Channel, ChannelMsg, Disconnect, Sig,
    client::{self, AuthResult, Handler},
    keys::{HashAlg, PublicKey},
};
use tokio::{
    net::TcpStream,
    sync::{mpsc, watch},
};
use vigilshell_core::{
    connection::{HostFingerprint, SshRoute},
    host_trust::HostKeyAlgorithm,
    human_terminal::{AuthorizedHumanTerminalOpen, TerminalSize},
};
use zeroize::Zeroizing;

use crate::{approved_preferred, map_negotiated_algorithm, negotiated_algorithm_matches_key};

const PRODUCTION_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const PRODUCTION_AUTHENTICATION_TIMEOUT: Duration = Duration::from_secs(15);
const PRODUCTION_CHANNEL_OPEN_TIMEOUT: Duration = Duration::from_secs(10);
const PRODUCTION_PTY_TIMEOUT: Duration = Duration::from_secs(10);
const PRODUCTION_SHELL_TIMEOUT: Duration = Duration::from_secs(10);
const KEEPALIVE_INTERVAL: Duration = Duration::from_secs(30);
const KEEPALIVE_MAX: usize = 3;
const CONTROL_QUEUE_CAPACITY: usize = 64;
const INITIAL_OUTPUT_CREDIT: usize = 256 * 1024;
const MAXIMUM_OUTPUT_CREDIT: usize = 1024 * 1024;
const MAXIMUM_OUTPUT_CHUNK: usize = 32 * 1024;
const MAXIMUM_INPUT: usize = 64 * 1024;
const SHUTDOWN_STEP_TIMEOUT: Duration = Duration::from_millis(250);
const HANDLE_SETTLEMENT_TIMEOUT: Duration = Duration::from_secs(1);

/// 真实 SSH terminal 建立阶段的独立 timeout。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RusshTerminalTimeouts {
    connect: Duration,
    authentication: Duration,
    channel_open: Duration,
    pty: Duration,
    shell: Duration,
}

impl RusshTerminalTimeouts {
    pub fn new(
        connect: Duration,
        authentication: Duration,
        channel_open: Duration,
        pty: Duration,
        shell: Duration,
    ) -> Result<Self, RusshTerminalTimeoutsError> {
        if connect.is_zero()
            || authentication.is_zero()
            || channel_open.is_zero()
            || pty.is_zero()
            || shell.is_zero()
        {
            return Err(RusshTerminalTimeoutsError::ZeroTimeout);
        }

        Ok(Self {
            connect,
            authentication,
            channel_open,
            pty,
            shell,
        })
    }

    pub const fn production() -> Self {
        Self {
            connect: PRODUCTION_CONNECT_TIMEOUT,
            authentication: PRODUCTION_AUTHENTICATION_TIMEOUT,
            channel_open: PRODUCTION_CHANNEL_OPEN_TIMEOUT,
            pty: PRODUCTION_PTY_TIMEOUT,
            shell: PRODUCTION_SHELL_TIMEOUT,
        }
    }

    pub const fn connect(self) -> Duration {
        self.connect
    }

    pub const fn authentication(self) -> Duration {
        self.authentication
    }

    pub const fn channel_open(self) -> Duration {
        self.channel_open
    }

    pub const fn pty(self) -> Duration {
        self.pty
    }

    pub const fn shell(self) -> Duration {
        self.shell
    }
}

impl Default for RusshTerminalTimeouts {
    fn default() -> Self {
        Self::production()
    }
}

/// Terminal timeout 配置校验失败。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RusshTerminalTimeoutsError {
    ZeroTimeout,
}

impl fmt::Display for RusshTerminalTimeoutsError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("terminal setup timeouts must not be zero")
    }
}

impl Error for RusshTerminalTimeoutsError {}

/// Password SSH 建立过程可公开的稳定失败。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RusshTerminalOpenError {
    HostKeyMismatch,
    AuthRejected,
    ChannelRejected,
    PtyRejected,
    ShellRejected,
    InvalidCredentialEncoding,
    Disconnected,
    Timeout,
    Unavailable,
}

impl fmt::Display for RusshTerminalOpenError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::HostKeyMismatch => "remote host identity does not match trusted identity",
            Self::AuthRejected => "SSH authentication was rejected",
            Self::ChannelRejected => "SSH session channel was rejected",
            Self::PtyRejected => "SSH pseudo-terminal request was rejected",
            Self::ShellRejected => "SSH shell request was rejected",
            Self::InvalidCredentialEncoding => "SSH password is not valid UTF-8",
            Self::Disconnected => "SSH connection was interrupted",
            Self::Timeout => "SSH terminal setup timed out",
            Self::Unavailable => "SSH terminal transport is unavailable",
        };
        formatter.write_str(message)
    }
}

impl Error for RusshTerminalOpenError {}

/// 使用固定 production 安全策略建立 password SSH terminal。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RusshPasswordTerminalTransport {
    timeouts: RusshTerminalTimeouts,
}

impl RusshPasswordTerminalTransport {
    pub const fn new(timeouts: RusshTerminalTimeouts) -> Self {
        Self { timeouts }
    }

    pub const fn production() -> Self {
        Self::new(RusshTerminalTimeouts::production())
    }

    pub const fn timeouts(self) -> RusshTerminalTimeouts {
        self.timeouts
    }

    pub async fn establish(
        &self,
        request: AuthorizedHumanTerminalOpen,
    ) -> Result<EstablishedRusshTerminal, RusshTerminalOpenError> {
        let route = request.profile().route().clone();
        let expected_fingerprint = request.expected_host_fingerprint().clone();
        let terminal_size = request.terminal_size();
        let password = std::str::from_utf8(request.password())
            .map_err(|_| RusshTerminalOpenError::InvalidCredentialEncoding)?;
        let mut password = Zeroizing::new(password.to_owned());
        drop(request);

        let connected = tokio::time::timeout(
            self.timeouts.connect,
            connect_client(&route, expected_fingerprint),
        )
        .await
        .map_err(|_| RusshTerminalOpenError::Timeout)??;
        let (mut handle, guard, disconnected) = connected;

        let authentication = tokio::time::timeout(
            self.timeouts.authentication,
            handle
                .authenticate_password(route.username().to_owned(), std::mem::take(&mut *password)),
        )
        .await;
        drop(password);

        match authentication {
            Err(_) => {
                abort_attempt(handle, None, guard).await;
                return Err(RusshTerminalOpenError::Timeout);
            }
            Ok(Err(_)) => {
                let error = if disconnected.load(Ordering::Acquire) || handle.is_closed() {
                    RusshTerminalOpenError::Disconnected
                } else {
                    RusshTerminalOpenError::Unavailable
                };
                abort_attempt(handle, None, guard).await;
                return Err(error);
            }
            Ok(Ok(AuthResult::Failure { .. })) => {
                let error = if disconnected.load(Ordering::Acquire) || handle.is_closed() {
                    RusshTerminalOpenError::Disconnected
                } else {
                    RusshTerminalOpenError::AuthRejected
                };
                abort_attempt(handle, None, guard).await;
                return Err(error);
            }
            Ok(Ok(AuthResult::Success)) => {}
        }

        let channel =
            match tokio::time::timeout(self.timeouts.channel_open, handle.channel_open_session())
                .await
            {
                Err(_) => {
                    abort_attempt(handle, None, guard).await;
                    return Err(RusshTerminalOpenError::Timeout);
                }
                Ok(Err(error)) => {
                    let mapped = if matches!(error, russh::Error::ChannelOpenFailure(_)) {
                        RusshTerminalOpenError::ChannelRejected
                    } else if is_disconnect_error(&error)
                        || disconnected.load(Ordering::Acquire)
                        || handle.is_closed()
                    {
                        RusshTerminalOpenError::Disconnected
                    } else {
                        RusshTerminalOpenError::Unavailable
                    };
                    abort_attempt(handle, None, guard).await;
                    return Err(mapped);
                }
                Ok(Ok(channel)) => channel,
            };

        let mut channel = channel;
        let mut setup_data = VecDeque::new();
        let mut setup_data_bytes = 0_usize;
        let pty = tokio::time::timeout(
            self.timeouts.pty,
            request_pty(
                &mut channel,
                terminal_size,
                &mut setup_data,
                &mut setup_data_bytes,
            ),
        )
        .await;
        match pty {
            Err(_) => {
                abort_attempt(handle, Some(channel), guard).await;
                return Err(RusshTerminalOpenError::Timeout);
            }
            Ok(Err(error)) => {
                abort_attempt(handle, Some(channel), guard).await;
                return Err(error);
            }
            Ok(Ok(false)) => {
                abort_attempt(handle, Some(channel), guard).await;
                return Err(RusshTerminalOpenError::PtyRejected);
            }
            Ok(Ok(true)) => {}
        }

        let shell = tokio::time::timeout(
            self.timeouts.shell,
            request_shell(&mut channel, &mut setup_data, &mut setup_data_bytes),
        )
        .await;
        match shell {
            Err(_) => {
                abort_attempt(handle, Some(channel), guard).await;
                return Err(RusshTerminalOpenError::Timeout);
            }
            Ok(Err(error)) => {
                abort_attempt(handle, Some(channel), guard).await;
                return Err(error);
            }
            Ok(Ok(false)) => {
                abort_attempt(handle, Some(channel), guard).await;
                return Err(RusshTerminalOpenError::ShellRejected);
            }
            Ok(Ok(true)) => {}
        }

        Ok(EstablishedRusshTerminal {
            inner: Some(EstablishedInner {
                handle,
                channel,
                guard,
                setup_data,
            }),
        })
    }
}

impl Default for RusshPasswordTerminalTransport {
    fn default() -> Self {
        Self::production()
    }
}

/// 已完成 Host Key、password、PTY 与 shell settlement 的 opaque transport。
pub struct EstablishedRusshTerminal {
    inner: Option<EstablishedInner>,
}

impl EstablishedRusshTerminal {
    pub fn into_owner<Output>(
        mut self,
        output: Output,
    ) -> (RusshTerminalControlHandle, RusshTerminalOwner<Output>)
    where
        Output: RusshTerminalOutputSink,
    {
        let inner = self
            .inner
            .take()
            .expect("established terminal ownership can only be transferred once");
        let (control_sender, control_receiver) = mpsc::channel(CONTROL_QUEUE_CAPACITY);
        let (close_sender, close_receiver) = watch::channel(false);
        (
            RusshTerminalControlHandle {
                sender: control_sender,
                close_sender,
            },
            RusshTerminalOwner {
                inner: Some(inner),
                output,
                control_receiver,
                close_receiver,
                credit: INITIAL_OUTPUT_CREDIT,
                buffered_data: VecDeque::new(),
            },
        )
    }
}

impl fmt::Debug for EstablishedRusshTerminal {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("EstablishedRusshTerminal([opaque])")
    }
}

struct EstablishedInner {
    handle: client::Handle<TerminalHandler>,
    channel: Channel<client::Msg>,
    guard: SocketShutdownGuard,
    setup_data: VecDeque<Vec<u8>>,
}

/// Native runtime 注入 terminal 输出的最窄同步边界。
pub trait RusshTerminalOutputSink: Send + 'static {
    fn send(&mut self, output: RusshTerminalOutput) -> Result<(), RusshTerminalOutputError>;
}

/// 输出接收方不可用。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RusshTerminalOutputError;

impl fmt::Display for RusshTerminalOutputError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("terminal output receiver is unavailable")
    }
}

impl Error for RusshTerminalOutputError {}

/// Ordered terminal 输出。
pub enum RusshTerminalOutput {
    Data(RusshTerminalData),
    Lifecycle(RusshTerminalLifecycle),
}

impl fmt::Debug for RusshTerminalOutput {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Data(data) => formatter.debug_tuple("Data").field(data).finish(),
            Self::Lifecycle(lifecycle) => {
                formatter.debug_tuple("Lifecycle").field(lifecycle).finish()
            }
        }
    }
}

/// 不以 Debug 暴露内容的 raw terminal bytes。
pub struct RusshTerminalData(Vec<u8>);

impl RusshTerminalData {
    pub fn into_bytes(self) -> Vec<u8> {
        self.0
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl fmt::Debug for RusshTerminalData {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RusshTerminalData")
            .field("length", &self.0.len())
            .finish()
    }
}

/// Renderer-safe terminal lifecycle。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RusshTerminalLifecycle {
    Ready,
    Eof,
    ExitStatus(u32),
    ExitSignal {
        signal: RusshTerminalExitSignal,
        core_dumped: bool,
    },
    Closed,
    Disconnected,
    ProtocolFailure,
}

/// 仅保留 SSH 标准 signal identity；远端自定义字符串不会透传。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RusshTerminalExitSignal {
    Abort,
    Alarm,
    FloatingPointException,
    Hangup,
    IllegalInstruction,
    Interrupt,
    Kill,
    Pipe,
    Quit,
    SegmentationViolation,
    Terminate,
    UserDefinedOne,
    Unknown,
}

/// Terminal owner 的最终 settlement。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RusshTerminalSettlement {
    LocalClosed,
    RemoteClosed,
    Disconnected,
    ProtocolFailure,
    OutputUnavailable,
}

/// Bounded terminal control 发送端。
#[derive(Clone)]
pub struct RusshTerminalControlHandle {
    sender: mpsc::Sender<TerminalControl>,
    close_sender: watch::Sender<bool>,
}

impl RusshTerminalControlHandle {
    pub fn try_input(&self, data: Vec<u8>) -> Result<(), RusshTerminalControlError> {
        if data.len() > MAXIMUM_INPUT {
            return Err(RusshTerminalControlError::InputTooLarge);
        }
        self.try_send(TerminalControl::Input(data))
    }

    pub fn try_resize(&self, size: TerminalSize) -> Result<(), RusshTerminalControlError> {
        self.try_send(TerminalControl::Resize(size))
    }

    pub fn try_grant_credit(&self, bytes: u32) -> Result<(), RusshTerminalControlError> {
        let bytes =
            usize::try_from(bytes).map_err(|_| RusshTerminalControlError::CreditOutOfRange)?;
        if bytes == 0 || bytes > MAXIMUM_OUTPUT_CREDIT {
            return Err(RusshTerminalControlError::CreditOutOfRange);
        }
        self.try_send(TerminalControl::GrantCredit(bytes))
    }

    pub fn close(&self) {
        let _ = self.close_sender.send(true);
    }

    fn try_send(&self, control: TerminalControl) -> Result<(), RusshTerminalControlError> {
        self.sender.try_send(control).map_err(|error| match error {
            mpsc::error::TrySendError::Full(_) => RusshTerminalControlError::QueueFull,
            mpsc::error::TrySendError::Closed(_) => RusshTerminalControlError::Closed,
        })
    }
}

impl fmt::Debug for RusshTerminalControlHandle {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("RusshTerminalControlHandle([opaque])")
    }
}

/// Bounded terminal control 拒绝。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RusshTerminalControlError {
    Closed,
    QueueFull,
    InputTooLarge,
    CreditOutOfRange,
}

impl fmt::Display for RusshTerminalControlError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::Closed => "terminal session is closed",
            Self::QueueFull => "terminal control queue is full",
            Self::InputTooLarge => "terminal input exceeds the per-control limit",
            Self::CreditOutOfRange => "terminal output credit is outside the allowed range",
        };
        formatter.write_str(message)
    }
}

impl Error for RusshTerminalControlError {}

enum TerminalControl {
    Input(Vec<u8>),
    Resize(TerminalSize),
    GrantCredit(usize),
}

impl fmt::Debug for TerminalControl {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Input(data) => formatter
                .debug_struct("Input")
                .field("length", &data.len())
                .finish(),
            Self::Resize(size) => formatter.debug_tuple("Resize").field(size).finish(),
            Self::GrantCredit(bytes) => formatter.debug_tuple("GrantCredit").field(bytes).finish(),
        }
    }
}

/// 单一 async owner，独占 russh Handle、Channel 与 socket guard。
pub struct RusshTerminalOwner<Output>
where
    Output: RusshTerminalOutputSink,
{
    inner: Option<EstablishedInner>,
    output: Output,
    control_receiver: mpsc::Receiver<TerminalControl>,
    close_receiver: watch::Receiver<bool>,
    credit: usize,
    buffered_data: VecDeque<BufferedData>,
}

impl<Output> RusshTerminalOwner<Output>
where
    Output: RusshTerminalOutputSink,
{
    pub async fn run(mut self) -> RusshTerminalSettlement {
        let EstablishedInner {
            mut handle,
            mut channel,
            guard,
            setup_data,
        } = self
            .inner
            .take()
            .expect("terminal owner can only be run once");
        self.buffered_data
            .extend(setup_data.into_iter().map(BufferedData::new));

        if emit_lifecycle(&mut self.output, RusshTerminalLifecycle::Ready).is_err() {
            shutdown_attempt(&mut handle, &channel, false).await;
            drop(channel);
            drop(handle);
            drop(guard);
            return RusshTerminalSettlement::OutputUnavailable;
        }

        let mut final_lifecycle_sent = false;
        let settlement = 'settled: loop {
            while self.credit > 0 && !self.buffered_data.is_empty() {
                let Some(data) = self.next_output_chunk() else {
                    break;
                };
                if self
                    .output
                    .send(RusshTerminalOutput::Data(RusshTerminalData(data)))
                    .is_err()
                {
                    break 'settled RusshTerminalSettlement::OutputUnavailable;
                }
            }

            let remote_enabled = self.credit > 0 && self.buffered_data.is_empty();
            let next = tokio::select! {
                biased;
                close = self.close_receiver.changed() => {
                    if close.is_err() || *self.close_receiver.borrow() {
                        OwnerEvent::Close
                    } else {
                        continue;
                    }
                }
                control = self.control_receiver.recv() => OwnerEvent::Control(control),
                message = channel.wait(), if remote_enabled => OwnerEvent::Remote(message),
            };

            match next {
                OwnerEvent::Close | OwnerEvent::Control(None) => {
                    final_lifecycle_sent =
                        emit_lifecycle(&mut self.output, RusshTerminalLifecycle::Closed).is_ok();
                    break RusshTerminalSettlement::LocalClosed;
                }
                OwnerEvent::Control(Some(TerminalControl::GrantCredit(bytes))) => {
                    self.credit = self.credit.saturating_add(bytes).min(MAXIMUM_OUTPUT_CREDIT);
                }
                OwnerEvent::Control(Some(TerminalControl::Resize(size))) => {
                    let resize = channel.window_change(
                        u32::from(size.columns()),
                        u32::from(size.rows()),
                        0,
                        0,
                    );
                    let result = tokio::select! {
                        _ = self.close_receiver.changed() => None,
                        result = resize => Some(result),
                    };
                    match result {
                        None => {
                            final_lifecycle_sent =
                                emit_lifecycle(&mut self.output, RusshTerminalLifecycle::Closed)
                                    .is_ok();
                            break RusshTerminalSettlement::LocalClosed;
                        }
                        Some(Ok(())) => {}
                        Some(Err(_)) => break RusshTerminalSettlement::Disconnected,
                    }
                }
                OwnerEvent::Control(Some(TerminalControl::Input(data))) => {
                    let input = channel.data_bytes(data);
                    let result = tokio::select! {
                        _ = self.close_receiver.changed() => None,
                        result = input => Some(result),
                    };
                    match result {
                        None => {
                            final_lifecycle_sent =
                                emit_lifecycle(&mut self.output, RusshTerminalLifecycle::Closed)
                                    .is_ok();
                            break RusshTerminalSettlement::LocalClosed;
                        }
                        Some(Ok(())) => {}
                        Some(Err(_)) => break RusshTerminalSettlement::Disconnected,
                    }
                }
                OwnerEvent::Remote(Some(ChannelMsg::Data { data })) => {
                    if data.len() > MAXIMUM_OUTPUT_CREDIT {
                        let settlement = if emit_lifecycle(
                            &mut self.output,
                            RusshTerminalLifecycle::ProtocolFailure,
                        )
                        .is_err()
                        {
                            RusshTerminalSettlement::OutputUnavailable
                        } else {
                            final_lifecycle_sent = true;
                            RusshTerminalSettlement::ProtocolFailure
                        };
                        break settlement;
                    }
                    self.buffered_data
                        .push_back(BufferedData::new(data.to_vec()));
                }
                OwnerEvent::Remote(Some(ChannelMsg::ExtendedData { data, ext: 1 })) => {
                    if data.len() > MAXIMUM_OUTPUT_CREDIT {
                        let settlement = if emit_lifecycle(
                            &mut self.output,
                            RusshTerminalLifecycle::ProtocolFailure,
                        )
                        .is_err()
                        {
                            RusshTerminalSettlement::OutputUnavailable
                        } else {
                            final_lifecycle_sent = true;
                            RusshTerminalSettlement::ProtocolFailure
                        };
                        break settlement;
                    }
                    self.buffered_data
                        .push_back(BufferedData::new(data.to_vec()));
                }
                OwnerEvent::Remote(Some(ChannelMsg::Eof)) => {
                    if emit_lifecycle(&mut self.output, RusshTerminalLifecycle::Eof).is_err() {
                        break RusshTerminalSettlement::OutputUnavailable;
                    }
                }
                OwnerEvent::Remote(Some(ChannelMsg::ExitStatus { exit_status })) => {
                    if emit_lifecycle(
                        &mut self.output,
                        RusshTerminalLifecycle::ExitStatus(exit_status),
                    )
                    .is_err()
                    {
                        break RusshTerminalSettlement::OutputUnavailable;
                    }
                }
                OwnerEvent::Remote(Some(ChannelMsg::ExitSignal {
                    signal_name,
                    core_dumped,
                    error_message: _,
                    lang_tag: _,
                })) => {
                    if emit_lifecycle(
                        &mut self.output,
                        RusshTerminalLifecycle::ExitSignal {
                            signal: map_exit_signal(signal_name),
                            core_dumped,
                        },
                    )
                    .is_err()
                    {
                        break RusshTerminalSettlement::OutputUnavailable;
                    }
                }
                OwnerEvent::Remote(Some(ChannelMsg::Close)) => {
                    if emit_lifecycle(&mut self.output, RusshTerminalLifecycle::Closed).is_err() {
                        break RusshTerminalSettlement::OutputUnavailable;
                    }
                    final_lifecycle_sent = true;
                    break RusshTerminalSettlement::RemoteClosed;
                }
                OwnerEvent::Remote(None) => {
                    if emit_lifecycle(&mut self.output, RusshTerminalLifecycle::Disconnected)
                        .is_err()
                    {
                        break RusshTerminalSettlement::OutputUnavailable;
                    }
                    final_lifecycle_sent = true;
                    break RusshTerminalSettlement::Disconnected;
                }
                OwnerEvent::Remote(Some(ChannelMsg::WindowAdjusted { .. }))
                | OwnerEvent::Remote(Some(ChannelMsg::XonXoff { .. })) => {}
                OwnerEvent::Remote(Some(_)) => {
                    if emit_lifecycle(&mut self.output, RusshTerminalLifecycle::ProtocolFailure)
                        .is_err()
                    {
                        break RusshTerminalSettlement::OutputUnavailable;
                    }
                    final_lifecycle_sent = true;
                    break RusshTerminalSettlement::ProtocolFailure;
                }
            }
        };

        if !final_lifecycle_sent {
            let lifecycle = match settlement {
                RusshTerminalSettlement::LocalClosed | RusshTerminalSettlement::RemoteClosed => {
                    Some(RusshTerminalLifecycle::Closed)
                }
                RusshTerminalSettlement::Disconnected => Some(RusshTerminalLifecycle::Disconnected),
                RusshTerminalSettlement::ProtocolFailure => {
                    Some(RusshTerminalLifecycle::ProtocolFailure)
                }
                RusshTerminalSettlement::OutputUnavailable => None,
            };
            if let Some(lifecycle) = lifecycle {
                let _ = emit_lifecycle(&mut self.output, lifecycle);
            }
        }

        let graceful = matches!(
            settlement,
            RusshTerminalSettlement::LocalClosed | RusshTerminalSettlement::RemoteClosed
        );
        shutdown_attempt(&mut handle, &channel, graceful).await;
        drop(channel);
        drop(handle);
        drop(guard);
        settlement
    }

    fn next_output_chunk(&mut self) -> Option<Vec<u8>> {
        let buffered = self.buffered_data.front_mut()?;
        let maximum = self.credit.min(MAXIMUM_OUTPUT_CHUNK);
        let data = buffered.take(maximum);
        self.credit -= data.len();
        if buffered.is_empty() {
            self.buffered_data.pop_front();
        }
        Some(data)
    }
}

impl<Output> fmt::Debug for RusshTerminalOwner<Output>
where
    Output: RusshTerminalOutputSink,
{
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("RusshTerminalOwner([opaque])")
    }
}

enum OwnerEvent {
    Close,
    Control(Option<TerminalControl>),
    Remote(Option<ChannelMsg>),
}

struct BufferedData {
    bytes: Vec<u8>,
    offset: usize,
}

impl BufferedData {
    fn new(bytes: Vec<u8>) -> Self {
        Self { bytes, offset: 0 }
    }

    fn take(&mut self, maximum: usize) -> Vec<u8> {
        let end = self.offset.saturating_add(maximum).min(self.bytes.len());
        let data = self.bytes[self.offset..end].to_vec();
        self.offset = end;
        data
    }

    fn is_empty(&self) -> bool {
        self.offset == self.bytes.len()
    }
}

async fn connect_client(
    route: &SshRoute,
    expected_fingerprint: HostFingerprint,
) -> Result<
    (
        client::Handle<TerminalHandler>,
        SocketShutdownGuard,
        Arc<AtomicBool>,
    ),
    RusshTerminalOpenError,
> {
    let socket = TcpStream::connect((route.host(), route.port()))
        .await
        .map_err(|_| RusshTerminalOpenError::Unavailable)?;
    socket
        .set_nodelay(true)
        .map_err(|_| RusshTerminalOpenError::Unavailable)?;
    let standard_socket = socket
        .into_std()
        .map_err(|_| RusshTerminalOpenError::Unavailable)?;
    let shutdown_socket = standard_socket
        .try_clone()
        .map_err(|_| RusshTerminalOpenError::Unavailable)?;
    let socket =
        TcpStream::from_std(standard_socket).map_err(|_| RusshTerminalOpenError::Unavailable)?;
    let guard = SocketShutdownGuard(shutdown_socket);

    let verification = Arc::new(Mutex::new(TerminalHostKeyVerification::default()));
    let disconnected = Arc::new(AtomicBool::new(false));
    let handler = TerminalHandler {
        expected_fingerprint: Some(expected_fingerprint),
        verification: Arc::clone(&verification),
        disconnected: Arc::clone(&disconnected),
    };
    let config = Arc::new(client::Config {
        preferred: approved_preferred(),
        inactivity_timeout: None,
        keepalive_interval: Some(KEEPALIVE_INTERVAL),
        keepalive_max: KEEPALIVE_MAX,
        anonymous: false,
        nodelay: true,
        maximum_packet_size: MAXIMUM_OUTPUT_CHUNK as u32,
        ..client::Config::default()
    });

    match client::connect_stream(config, socket, handler).await {
        Ok(handle) => {
            let matched = verification
                .lock()
                .map_err(|_| RusshTerminalOpenError::Unavailable)?
                .verdict
                == HostKeyVerdict::Matched;
            if matched {
                Ok((handle, guard, disconnected))
            } else {
                let _ = handle
                    .disconnect(Disconnect::HostKeyNotVerifiable, "", "")
                    .await;
                drop(handle);
                drop(guard);
                Err(RusshTerminalOpenError::Unavailable)
            }
        }
        Err(error) => {
            let mapped = map_connect_error(&error, &verification);
            drop(guard);
            Err(mapped)
        }
    }
}

async fn request_pty(
    channel: &mut Channel<client::Msg>,
    terminal_size: TerminalSize,
    setup_data: &mut VecDeque<Vec<u8>>,
    setup_data_bytes: &mut usize,
) -> Result<bool, RusshTerminalOpenError> {
    channel
        .request_pty(
            true,
            "xterm-256color",
            u32::from(terminal_size.columns()),
            u32::from(terminal_size.rows()),
            0,
            0,
            &[],
        )
        .await
        .map_err(map_setup_transport_error)?;
    wait_request_reply(channel, setup_data, setup_data_bytes).await
}

async fn request_shell(
    channel: &mut Channel<client::Msg>,
    setup_data: &mut VecDeque<Vec<u8>>,
    setup_data_bytes: &mut usize,
) -> Result<bool, RusshTerminalOpenError> {
    channel
        .request_shell(true)
        .await
        .map_err(map_setup_transport_error)?;
    wait_request_reply(channel, setup_data, setup_data_bytes).await
}

async fn wait_request_reply(
    channel: &mut Channel<client::Msg>,
    setup_data: &mut VecDeque<Vec<u8>>,
    setup_data_bytes: &mut usize,
) -> Result<bool, RusshTerminalOpenError> {
    loop {
        match channel.wait().await {
            Some(ChannelMsg::Success) => return Ok(true),
            Some(ChannelMsg::Failure) => return Ok(false),
            Some(ChannelMsg::Data { data }) => {
                buffer_setup_data(setup_data, setup_data_bytes, data.to_vec())?;
            }
            Some(ChannelMsg::ExtendedData { data, ext: 1 }) => {
                buffer_setup_data(setup_data, setup_data_bytes, data.to_vec())?;
            }
            Some(ChannelMsg::WindowAdjusted { .. }) | Some(ChannelMsg::XonXoff { .. }) => {}
            Some(ChannelMsg::Eof | ChannelMsg::Close) | None => {
                return Err(RusshTerminalOpenError::Disconnected);
            }
            Some(_) => return Err(RusshTerminalOpenError::Unavailable),
        }
    }
}

fn buffer_setup_data(
    setup_data: &mut VecDeque<Vec<u8>>,
    setup_data_bytes: &mut usize,
    data: Vec<u8>,
) -> Result<(), RusshTerminalOpenError> {
    let total = setup_data_bytes
        .checked_add(data.len())
        .ok_or(RusshTerminalOpenError::Unavailable)?;
    if total > INITIAL_OUTPUT_CREDIT {
        return Err(RusshTerminalOpenError::Unavailable);
    }
    *setup_data_bytes = total;
    setup_data.push_back(data);
    Ok(())
}

async fn abort_attempt(
    mut handle: client::Handle<TerminalHandler>,
    channel: Option<Channel<client::Msg>>,
    guard: SocketShutdownGuard,
) {
    if let Some(channel) = channel.as_ref() {
        let _ = tokio::time::timeout(SHUTDOWN_STEP_TIMEOUT, channel.close()).await;
    }
    let _ = tokio::time::timeout(
        SHUTDOWN_STEP_TIMEOUT,
        handle.disconnect(Disconnect::ByApplication, "", ""),
    )
    .await;
    drop(channel);
    drop(guard);
    let _ = tokio::time::timeout(HANDLE_SETTLEMENT_TIMEOUT, &mut handle).await;
    drop(handle);
}

async fn shutdown_attempt(
    handle: &mut client::Handle<TerminalHandler>,
    channel: &Channel<client::Msg>,
    graceful: bool,
) {
    if graceful {
        let _ = tokio::time::timeout(SHUTDOWN_STEP_TIMEOUT, channel.eof()).await;
    }
    let _ = tokio::time::timeout(SHUTDOWN_STEP_TIMEOUT, channel.close()).await;
    let _ = tokio::time::timeout(
        SHUTDOWN_STEP_TIMEOUT,
        handle.disconnect(Disconnect::ByApplication, "", ""),
    )
    .await;
    let _ = tokio::time::timeout(HANDLE_SETTLEMENT_TIMEOUT, &mut *handle).await;
}

fn map_setup_transport_error(error: russh::Error) -> RusshTerminalOpenError {
    if is_disconnect_error(&error) {
        RusshTerminalOpenError::Disconnected
    } else {
        RusshTerminalOpenError::Unavailable
    }
}

fn is_disconnect_error(error: &russh::Error) -> bool {
    matches!(
        error,
        russh::Error::Disconnect
            | russh::Error::HUP
            | russh::Error::ConnectionTimeout
            | russh::Error::KeepaliveTimeout
            | russh::Error::RecvError
            | russh::Error::SendError
    )
}

fn emit_lifecycle<Output>(
    output: &mut Output,
    lifecycle: RusshTerminalLifecycle,
) -> Result<(), RusshTerminalOutputError>
where
    Output: RusshTerminalOutputSink,
{
    output.send(RusshTerminalOutput::Lifecycle(lifecycle))
}

fn map_exit_signal(signal: Sig) -> RusshTerminalExitSignal {
    match signal {
        Sig::ABRT => RusshTerminalExitSignal::Abort,
        Sig::ALRM => RusshTerminalExitSignal::Alarm,
        Sig::FPE => RusshTerminalExitSignal::FloatingPointException,
        Sig::HUP => RusshTerminalExitSignal::Hangup,
        Sig::ILL => RusshTerminalExitSignal::IllegalInstruction,
        Sig::INT => RusshTerminalExitSignal::Interrupt,
        Sig::KILL => RusshTerminalExitSignal::Kill,
        Sig::PIPE => RusshTerminalExitSignal::Pipe,
        Sig::QUIT => RusshTerminalExitSignal::Quit,
        Sig::SEGV => RusshTerminalExitSignal::SegmentationViolation,
        Sig::TERM => RusshTerminalExitSignal::Terminate,
        Sig::USR1 => RusshTerminalExitSignal::UserDefinedOne,
        Sig::Custom(_) => RusshTerminalExitSignal::Unknown,
    }
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum HostKeyVerdict {
    #[default]
    Pending,
    Matched,
    Mismatched,
    Unsupported,
}

#[derive(Default)]
struct TerminalHostKeyVerification {
    negotiated_algorithm: Option<HostKeyAlgorithm>,
    verdict: HostKeyVerdict,
}

struct TerminalHandler {
    expected_fingerprint: Option<HostFingerprint>,
    verification: Arc<Mutex<TerminalHostKeyVerification>>,
    disconnected: Arc<AtomicBool>,
}

impl Handler for TerminalHandler {
    type Error = TerminalHandlerError;

    async fn kex_done(
        &mut self,
        _shared_secret: Option<&[u8]>,
        names: &russh::Names,
        _session: &mut client::Session,
    ) -> Result<(), Self::Error> {
        let mut verification = self
            .verification
            .lock()
            .map_err(|_| TerminalHandlerError(russh::Error::Inconsistent))?;
        verification.negotiated_algorithm = map_negotiated_algorithm(&names.key);
        if verification.negotiated_algorithm.is_none() {
            verification.verdict = HostKeyVerdict::Unsupported;
        }
        Ok(())
    }

    async fn check_server_key(
        &mut self,
        server_public_key: &PublicKey,
    ) -> Result<bool, Self::Error> {
        let mut verification = self
            .verification
            .lock()
            .map_err(|_| TerminalHandlerError(russh::Error::Inconsistent))?;
        let Some(negotiated) = verification.negotiated_algorithm else {
            verification.verdict = HostKeyVerdict::Unsupported;
            return Ok(false);
        };
        if !negotiated_algorithm_matches_key(negotiated, &server_public_key.algorithm()) {
            verification.verdict = HostKeyVerdict::Unsupported;
            return Ok(false);
        }

        let fingerprint = server_public_key.fingerprint(HashAlg::Sha256).to_string();
        let Ok(fingerprint) = HostFingerprint::parse(fingerprint) else {
            verification.verdict = HostKeyVerdict::Unsupported;
            return Ok(false);
        };
        let expected_fingerprint = self.expected_fingerprint.take();
        let matched = expected_fingerprint.as_ref() == Some(&fingerprint);
        verification.verdict = if matched {
            HostKeyVerdict::Matched
        } else {
            HostKeyVerdict::Mismatched
        };
        Ok(matched)
    }

    async fn disconnected(
        &mut self,
        _reason: client::DisconnectReason<Self::Error>,
    ) -> Result<(), Self::Error> {
        self.disconnected.store(true, Ordering::Release);
        Ok(())
    }
}

struct TerminalHandlerError(russh::Error);

impl From<russh::Error> for TerminalHandlerError {
    fn from(error: russh::Error) -> Self {
        Self(error)
    }
}

impl fmt::Debug for TerminalHandlerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("TerminalHandlerError([redacted])")
    }
}

impl fmt::Display for TerminalHandlerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SSH terminal handler failed")
    }
}

impl Error for TerminalHandlerError {}

fn map_connect_error(
    error: &TerminalHandlerError,
    verification: &Mutex<TerminalHostKeyVerification>,
) -> RusshTerminalOpenError {
    if let Ok(verification) = verification.lock()
        && verification.verdict == HostKeyVerdict::Mismatched
    {
        return RusshTerminalOpenError::HostKeyMismatch;
    }
    if is_disconnect_error(&error.0) {
        RusshTerminalOpenError::Disconnected
    } else {
        RusshTerminalOpenError::Unavailable
    }
}

struct SocketShutdownGuard(StandardTcpStream);

impl Drop for SocketShutdownGuard {
    fn drop(&mut self) {
        let _ = self.0.shutdown(Shutdown::Both);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn production_timeouts_and_keepalive_are_explicit() {
        let timeouts = RusshTerminalTimeouts::production();
        assert_eq!(timeouts.connect(), Duration::from_secs(10));
        assert_eq!(timeouts.authentication(), Duration::from_secs(15));
        assert_eq!(timeouts.channel_open(), Duration::from_secs(10));
        assert_eq!(timeouts.pty(), Duration::from_secs(10));
        assert_eq!(timeouts.shell(), Duration::from_secs(10));
        assert_eq!(KEEPALIVE_INTERVAL, Duration::from_secs(30));
        assert_eq!(KEEPALIVE_MAX, 3);
    }

    #[test]
    fn timeout_configuration_rejects_zero() {
        assert_eq!(
            RusshTerminalTimeouts::new(
                Duration::ZERO,
                Duration::from_secs(1),
                Duration::from_secs(1),
                Duration::from_secs(1),
                Duration::from_secs(1),
            ),
            Err(RusshTerminalTimeoutsError::ZeroTimeout)
        );
    }

    #[test]
    fn public_errors_and_raw_data_debug_are_redacted() {
        let data = RusshTerminalData(b"sensitive-terminal-payload".to_vec());
        let rendered = format!("{data:?}");
        assert!(!rendered.contains("sensitive-terminal-payload"));
        assert!(rendered.contains("26"));

        let errors = [
            RusshTerminalOpenError::HostKeyMismatch,
            RusshTerminalOpenError::AuthRejected,
            RusshTerminalOpenError::ChannelRejected,
            RusshTerminalOpenError::PtyRejected,
            RusshTerminalOpenError::ShellRejected,
            RusshTerminalOpenError::InvalidCredentialEncoding,
            RusshTerminalOpenError::Disconnected,
            RusshTerminalOpenError::Timeout,
            RusshTerminalOpenError::Unavailable,
        ];
        for error in errors {
            let rendered = format!("{error:?} {error}");
            assert!(!rendered.contains("sensitive-endpoint"));
            assert!(!rendered.contains("SHA256:"));
            assert!(!rendered.contains("sensitive-password"));
        }
    }

    #[test]
    fn control_limits_fail_before_enqueue() {
        let (sender, _receiver) = mpsc::channel(CONTROL_QUEUE_CAPACITY);
        let (close_sender, _close_receiver) = watch::channel(false);
        let handle = RusshTerminalControlHandle {
            sender,
            close_sender,
        };
        assert_eq!(
            handle.try_input(vec![0_u8; MAXIMUM_INPUT + 1]),
            Err(RusshTerminalControlError::InputTooLarge)
        );
        assert_eq!(
            handle.try_grant_credit(0),
            Err(RusshTerminalControlError::CreditOutOfRange)
        );
        assert_eq!(
            handle.try_grant_credit((MAXIMUM_OUTPUT_CREDIT + 1) as u32),
            Err(RusshTerminalControlError::CreditOutOfRange)
        );
    }
}
