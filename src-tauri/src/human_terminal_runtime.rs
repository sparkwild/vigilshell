use std::{
    collections::HashMap,
    sync::{Arc, Condvar, Mutex, MutexGuard},
    time::{Duration, Instant},
};

use vigilshell_core::{
    connection::ConnectionProfileId,
    human_terminal::{HumanTerminalSessionId, TerminalSize},
};

pub(crate) const MAX_LIVE_TERMINALS: usize = 32;
pub(crate) const MAX_LIVE_TERMINALS_PER_PROFILE: usize = 4;
pub(crate) const MAX_TERMINAL_INPUT_BYTES: usize = 64 * 1024;
pub(crate) const MAX_OUTPUT_CREDIT_GRANT_BYTES: u32 = 1024 * 1024;
pub(crate) const TERMINAL_CLOSE_SETTLEMENT_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct DisplayContextId {
    window_label: String,
    webview_label: String,
}

impl DisplayContextId {
    pub(crate) fn new(window_label: impl Into<String>, webview_label: impl Into<String>) -> Self {
        Self {
            window_label: window_label.into(),
            webview_label: webview_label.into(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TerminalRegistryError {
    StateUnavailable,
    ShuttingDown,
    GlobalCapacityReached,
    ProfileCapacityReached,
    ProfileDeleting,
    ProfileInUse,
    DeleteAlreadyReserved,
    ReservationUnavailable,
    SessionAlreadyRegistered,
    SessionNotFound,
    DisplayContextMismatch,
    InputTooLarge,
    CreditGrantOutOfRange,
    ControlQueueFull,
    SessionClosed,
    CloseSettlementTimeout,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TerminalControlSendError {
    QueueFull,
    Closed,
}

pub(crate) trait LiveTerminalControl: Send + Sync {
    fn try_input(&self, input: Vec<u8>) -> Result<(), TerminalControlSendError>;

    fn try_resize(&self, size: TerminalSize) -> Result<(), TerminalControlSendError>;

    fn try_grant_output_credit(&self, bytes: u32) -> Result<(), TerminalControlSendError>;

    fn close(&self);
}

#[derive(Clone)]
pub(crate) struct HumanTerminalRuntime {
    inner: Arc<RegistryInner>,
}

impl HumanTerminalRuntime {
    pub(crate) fn new() -> Self {
        Self {
            inner: Arc::new(RegistryInner {
                state: Mutex::new(RegistryState::new()),
                changed: Condvar::new(),
            }),
        }
    }

    pub(crate) fn reserve_open(
        &self,
        profile_id: ConnectionProfileId,
    ) -> Result<OpenTerminalReservation, TerminalRegistryError> {
        let mut state = self.lock()?;
        state.require_running()?;
        if state.deleting.contains_key(&profile_id) {
            return Err(TerminalRegistryError::ProfileDeleting);
        }

        let reserved_count = state.opening.len() + state.live.len();
        if reserved_count >= MAX_LIVE_TERMINALS {
            return Err(TerminalRegistryError::GlobalCapacityReached);
        }

        let profile_count = state
            .opening
            .values()
            .filter(|candidate| *candidate == &profile_id)
            .count()
            + state
                .live
                .values()
                .filter(|session| session.profile_id == profile_id)
                .count();
        if profile_count >= MAX_LIVE_TERMINALS_PER_PROFILE {
            return Err(TerminalRegistryError::ProfileCapacityReached);
        }

        let reservation_id = state.next_reservation_id()?;
        state.opening.insert(reservation_id, profile_id.clone());
        drop(state);

        Ok(OpenTerminalReservation {
            inner: Arc::clone(&self.inner),
            reservation_id,
            profile_id,
            active: true,
        })
    }

    pub(crate) fn reserve_delete(
        &self,
        profile_id: ConnectionProfileId,
    ) -> Result<DeleteTerminalReservation, TerminalRegistryError> {
        let mut state = self.lock()?;
        state.require_running()?;
        if state.deleting.contains_key(&profile_id) {
            return Err(TerminalRegistryError::DeleteAlreadyReserved);
        }
        if state
            .opening
            .values()
            .any(|candidate| candidate == &profile_id)
            || state
                .live
                .values()
                .any(|session| session.profile_id == profile_id)
        {
            return Err(TerminalRegistryError::ProfileInUse);
        }

        let reservation_id = state.next_reservation_id()?;
        state.deleting.insert(profile_id.clone(), reservation_id);
        drop(state);

        Ok(DeleteTerminalReservation {
            inner: Arc::clone(&self.inner),
            reservation_id,
            profile_id,
            active: true,
        })
    }

    pub(crate) fn try_input(
        &self,
        session_id: &HumanTerminalSessionId,
        display_context: &DisplayContextId,
        input: Vec<u8>,
    ) -> Result<(), TerminalRegistryError> {
        let control = self.control_for(session_id, display_context)?;
        if input.len() > MAX_TERMINAL_INPUT_BYTES {
            return Err(TerminalRegistryError::InputTooLarge);
        }
        control.try_input(input).map_err(map_control_send_error)
    }

    pub(crate) fn try_resize(
        &self,
        session_id: &HumanTerminalSessionId,
        display_context: &DisplayContextId,
        size: TerminalSize,
    ) -> Result<(), TerminalRegistryError> {
        self.control_for(session_id, display_context)?
            .try_resize(size)
            .map_err(map_control_send_error)
    }

    pub(crate) fn try_grant_output_credit(
        &self,
        session_id: &HumanTerminalSessionId,
        display_context: &DisplayContextId,
        bytes: u32,
    ) -> Result<(), TerminalRegistryError> {
        let control = self.control_for(session_id, display_context)?;
        if bytes == 0 || bytes > MAX_OUTPUT_CREDIT_GRANT_BYTES {
            return Err(TerminalRegistryError::CreditGrantOutOfRange);
        }
        control
            .try_grant_output_credit(bytes)
            .map_err(map_control_send_error)
    }

    pub(crate) fn begin_quit(&self) -> Result<TerminalQuitWaiter, TerminalRegistryError> {
        let controls = {
            let mut state = self.lock()?;
            if state.phase == RegistryPhase::Running {
                state.phase = RegistryPhase::Quitting;
                state
                    .live
                    .values()
                    .map(|session| Arc::clone(&session.control))
                    .collect::<Vec<_>>()
            } else {
                Vec::new()
            }
        };

        for control in controls {
            control.close();
        }

        Ok(TerminalQuitWaiter {
            inner: Arc::clone(&self.inner),
        })
    }

    pub(crate) fn close(
        &self,
        session_id: &HumanTerminalSessionId,
        display_context: &DisplayContextId,
    ) -> Result<TerminalCloseWaiter, TerminalRegistryError> {
        let (control, settlement) = self.session_for(session_id, display_context)?;
        control.close();
        Ok(TerminalCloseWaiter { settlement })
    }

    pub(crate) fn close_all(&self, display_context: &DisplayContextId) {
        let controls = match self.lock() {
            Ok(state) => state
                .live
                .values()
                .filter(|session| &session.display_context == display_context)
                .map(|session| Arc::clone(&session.control))
                .collect::<Vec<_>>(),
            Err(_) => Vec::new(),
        };
        for control in controls {
            control.close();
        }
    }

    pub(crate) fn close_all_for_window(&self, window_label: &str) {
        let controls = cleanup_state(&self.inner)
            .live
            .values()
            .filter(|session| session.display_context.window_label == window_label)
            .map(|session| Arc::clone(&session.control))
            .collect::<Vec<_>>();
        for control in controls {
            control.close();
        }
    }

    #[cfg(test)]
    pub(crate) fn has_live_for_window(&self, window_label: &str) -> bool {
        cleanup_state(&self.inner)
            .live
            .values()
            .any(|session| session.display_context.window_label == window_label)
    }

    fn control_for(
        &self,
        session_id: &HumanTerminalSessionId,
        display_context: &DisplayContextId,
    ) -> Result<Arc<dyn LiveTerminalControl>, TerminalRegistryError> {
        let state = self.lock()?;
        state.require_running()?;
        let session = state
            .live
            .get(session_id)
            .ok_or(TerminalRegistryError::SessionNotFound)?;
        if &session.display_context != display_context {
            return Err(TerminalRegistryError::DisplayContextMismatch);
        }
        Ok(Arc::clone(&session.control))
    }

    fn session_for(
        &self,
        session_id: &HumanTerminalSessionId,
        display_context: &DisplayContextId,
    ) -> Result<(Arc<dyn LiveTerminalControl>, Arc<SessionSettlement>), TerminalRegistryError> {
        let state = self.lock()?;
        let session = state
            .live
            .get(session_id)
            .ok_or(TerminalRegistryError::SessionNotFound)?;
        if &session.display_context != display_context {
            return Err(TerminalRegistryError::DisplayContextMismatch);
        }
        Ok((
            Arc::clone(&session.control),
            Arc::clone(&session.settlement),
        ))
    }

    fn lock(&self) -> Result<MutexGuard<'_, RegistryState>, TerminalRegistryError> {
        self.inner
            .state
            .lock()
            .map_err(|_| TerminalRegistryError::StateUnavailable)
    }
}

impl Default for HumanTerminalRuntime {
    fn default() -> Self {
        Self::new()
    }
}

pub(crate) struct OpenTerminalReservation {
    inner: Arc<RegistryInner>,
    reservation_id: u64,
    profile_id: ConnectionProfileId,
    active: bool,
}

impl OpenTerminalReservation {
    pub(crate) fn promote(
        mut self,
        session_id: HumanTerminalSessionId,
        display_context: DisplayContextId,
        control: Arc<dyn LiveTerminalControl>,
    ) -> Result<LiveTerminalRegistration, TerminalRegistryError> {
        let mut state = self
            .inner
            .state
            .lock()
            .map_err(|_| TerminalRegistryError::StateUnavailable)?;
        state.require_running()?;
        if state.opening.get(&self.reservation_id) != Some(&self.profile_id) {
            return Err(TerminalRegistryError::ReservationUnavailable);
        }
        if state.live.contains_key(&session_id) {
            return Err(TerminalRegistryError::SessionAlreadyRegistered);
        }

        let settlement = Arc::new(SessionSettlement::new());
        state.opening.remove(&self.reservation_id);
        state.live.insert(
            session_id.clone(),
            LiveTerminalSession {
                profile_id: self.profile_id.clone(),
                display_context,
                control,
                settlement: Arc::clone(&settlement),
            },
        );
        drop(state);
        self.active = false;

        Ok(LiveTerminalRegistration {
            inner: Arc::clone(&self.inner),
            session_id,
            settlement,
            active: true,
        })
    }
}

impl Drop for OpenTerminalReservation {
    fn drop(&mut self) {
        if self.active {
            {
                cleanup_state(&self.inner)
                    .opening
                    .remove(&self.reservation_id);
            }
            self.inner.changed.notify_all();
            self.active = false;
        }
    }
}

pub(crate) struct DeleteTerminalReservation {
    inner: Arc<RegistryInner>,
    reservation_id: u64,
    profile_id: ConnectionProfileId,
    active: bool,
}

impl Drop for DeleteTerminalReservation {
    fn drop(&mut self) {
        if !self.active {
            return;
        }
        let mut state = cleanup_state(&self.inner);
        if state.deleting.get(&self.profile_id) == Some(&self.reservation_id) {
            state.deleting.remove(&self.profile_id);
        }
        drop(state);
        self.inner.changed.notify_all();
        self.active = false;
    }
}

pub(crate) struct LiveTerminalRegistration {
    inner: Arc<RegistryInner>,
    session_id: HumanTerminalSessionId,
    settlement: Arc<SessionSettlement>,
    active: bool,
}

impl LiveTerminalRegistration {
    pub(crate) fn settlement_waiter(&self) -> TerminalCloseWaiter {
        TerminalCloseWaiter {
            settlement: Arc::clone(&self.settlement),
        }
    }
}

impl Drop for LiveTerminalRegistration {
    fn drop(&mut self) {
        if self.active {
            {
                cleanup_state(&self.inner).live.remove(&self.session_id);
            }
            self.settlement.mark_settled();
            self.inner.changed.notify_all();
            self.active = false;
        }
    }
}

pub(crate) struct TerminalCloseWaiter {
    settlement: Arc<SessionSettlement>,
}

impl TerminalCloseWaiter {
    pub(crate) fn wait(self, timeout: Duration) -> Result<(), TerminalRegistryError> {
        let settled = self
            .settlement
            .settled
            .lock()
            .map_err(|_| TerminalRegistryError::StateUnavailable)?;
        let (settled, timeout_result) = self
            .settlement
            .changed
            .wait_timeout_while(settled, timeout, |settled| !*settled)
            .map_err(|_| TerminalRegistryError::StateUnavailable)?;
        if timeout_result.timed_out() && !*settled {
            return Err(TerminalRegistryError::CloseSettlementTimeout);
        }
        Ok(())
    }
}

#[derive(Clone)]
pub(crate) struct TerminalQuitWaiter {
    inner: Arc<RegistryInner>,
}

impl TerminalQuitWaiter {
    pub(crate) fn wait(self, timeout: Duration) -> Result<(), TerminalRegistryError> {
        let started_at = Instant::now();
        let mut remaining = timeout;
        let mut state = self
            .inner
            .state
            .lock()
            .map_err(|_| TerminalRegistryError::StateUnavailable)?;

        loop {
            if state.is_settled() {
                return Ok(());
            }

            let (next_state, timeout_result) = self
                .inner
                .changed
                .wait_timeout(state, remaining)
                .map_err(|_| TerminalRegistryError::StateUnavailable)?;
            state = next_state;

            if state.is_settled() {
                return Ok(());
            }

            let elapsed = started_at.elapsed();
            if timeout_result.timed_out() || elapsed >= timeout {
                return Err(TerminalRegistryError::CloseSettlementTimeout);
            }
            remaining = timeout.saturating_sub(elapsed);
        }
    }
}

struct RegistryInner {
    state: Mutex<RegistryState>,
    changed: Condvar,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RegistryPhase {
    Running,
    Quitting,
}

struct RegistryState {
    phase: RegistryPhase,
    next_reservation_id: u64,
    opening: HashMap<u64, ConnectionProfileId>,
    deleting: HashMap<ConnectionProfileId, u64>,
    live: HashMap<HumanTerminalSessionId, LiveTerminalSession>,
}

impl RegistryState {
    fn new() -> Self {
        Self {
            phase: RegistryPhase::Running,
            next_reservation_id: 1,
            opening: HashMap::new(),
            deleting: HashMap::new(),
            live: HashMap::new(),
        }
    }

    fn next_reservation_id(&mut self) -> Result<u64, TerminalRegistryError> {
        let reservation_id = self.next_reservation_id;
        self.next_reservation_id = self
            .next_reservation_id
            .checked_add(1)
            .ok_or(TerminalRegistryError::StateUnavailable)?;
        Ok(reservation_id)
    }

    fn require_running(&self) -> Result<(), TerminalRegistryError> {
        match self.phase {
            RegistryPhase::Running => Ok(()),
            RegistryPhase::Quitting => Err(TerminalRegistryError::ShuttingDown),
        }
    }

    fn is_settled(&self) -> bool {
        self.opening.is_empty() && self.deleting.is_empty() && self.live.is_empty()
    }
}

struct LiveTerminalSession {
    profile_id: ConnectionProfileId,
    display_context: DisplayContextId,
    control: Arc<dyn LiveTerminalControl>,
    settlement: Arc<SessionSettlement>,
}

struct SessionSettlement {
    settled: Mutex<bool>,
    changed: Condvar,
}

impl SessionSettlement {
    fn new() -> Self {
        Self {
            settled: Mutex::new(false),
            changed: Condvar::new(),
        }
    }

    fn mark_settled(&self) {
        let mut settled = self
            .settled
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *settled = true;
        self.changed.notify_all();
    }
}

fn cleanup_state(inner: &RegistryInner) -> MutexGuard<'_, RegistryState> {
    inner
        .state
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

const fn map_control_send_error(error: TerminalControlSendError) -> TerminalRegistryError {
    match error {
        TerminalControlSendError::QueueFull => TerminalRegistryError::ControlQueueFull,
        TerminalControlSendError::Closed => TerminalRegistryError::SessionClosed,
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{
        Arc, Barrier, Mutex,
        atomic::{AtomicUsize, Ordering},
    };

    use super::*;

    #[derive(Clone, Copy)]
    enum FakeFailure {
        None,
        Full,
        Closed,
    }

    struct FakeControl {
        inputs: Mutex<Vec<Vec<u8>>>,
        resizes: Mutex<Vec<TerminalSize>>,
        credits: Mutex<Vec<u32>>,
        closes: AtomicUsize,
        failure: FakeFailure,
    }

    impl FakeControl {
        fn new(failure: FakeFailure) -> Self {
            Self {
                inputs: Mutex::new(Vec::new()),
                resizes: Mutex::new(Vec::new()),
                credits: Mutex::new(Vec::new()),
                closes: AtomicUsize::new(0),
                failure,
            }
        }

        fn result(&self) -> Result<(), TerminalControlSendError> {
            match self.failure {
                FakeFailure::None => Ok(()),
                FakeFailure::Full => Err(TerminalControlSendError::QueueFull),
                FakeFailure::Closed => Err(TerminalControlSendError::Closed),
            }
        }
    }

    impl LiveTerminalControl for FakeControl {
        fn try_input(&self, input: Vec<u8>) -> Result<(), TerminalControlSendError> {
            self.result()?;
            self.inputs.lock().expect("input lock").push(input);
            Ok(())
        }

        fn try_resize(&self, size: TerminalSize) -> Result<(), TerminalControlSendError> {
            self.result()?;
            self.resizes.lock().expect("resize lock").push(size);
            Ok(())
        }

        fn try_grant_output_credit(&self, bytes: u32) -> Result<(), TerminalControlSendError> {
            self.result()?;
            self.credits.lock().expect("credit lock").push(bytes);
            Ok(())
        }

        fn close(&self) {
            self.closes.fetch_add(1, Ordering::Relaxed);
        }
    }

    #[test]
    fn open_reservations_enforce_profile_and_global_capacity() {
        let runtime = HumanTerminalRuntime::new();
        let profile = profile_id("profile-limited");
        let mut profile_reservations = Vec::new();
        for _ in 0..MAX_LIVE_TERMINALS_PER_PROFILE {
            profile_reservations.push(
                runtime
                    .reserve_open(profile.clone())
                    .expect("profile capacity should remain"),
            );
        }
        assert!(matches!(
            runtime.reserve_open(profile),
            Err(TerminalRegistryError::ProfileCapacityReached)
        ));
        drop(profile_reservations);

        let mut global_reservations = Vec::new();
        for index in 0..MAX_LIVE_TERMINALS {
            global_reservations.push(
                runtime
                    .reserve_open(profile_id(&format!("profile-{index}")))
                    .expect("global capacity should remain"),
            );
        }
        assert!(matches!(
            runtime.reserve_open(profile_id("profile-overflow")),
            Err(TerminalRegistryError::GlobalCapacityReached)
        ));
    }

    #[test]
    fn deleting_blocks_open_and_opening_or_live_blocks_delete() {
        let runtime = HumanTerminalRuntime::new();
        let profile = profile_id("profile-mutual-exclusion");
        let deleting = runtime
            .reserve_delete(profile.clone())
            .expect("delete reservation should succeed");
        assert!(matches!(
            runtime.reserve_open(profile.clone()),
            Err(TerminalRegistryError::ProfileDeleting)
        ));
        assert!(matches!(
            runtime.reserve_delete(profile.clone()),
            Err(TerminalRegistryError::DeleteAlreadyReserved)
        ));
        drop(deleting);

        let opening = runtime
            .reserve_open(profile.clone())
            .expect("open reservation should succeed");
        assert!(matches!(
            runtime.reserve_delete(profile.clone()),
            Err(TerminalRegistryError::ProfileInUse)
        ));
        let live = opening
            .promote(
                session_id(1),
                display_context("main", "main"),
                Arc::new(FakeControl::new(FakeFailure::None)),
            )
            .expect("open reservation should promote");
        assert!(matches!(
            runtime.reserve_delete(profile.clone()),
            Err(TerminalRegistryError::ProfileInUse)
        ));
        drop(live);
        runtime
            .reserve_delete(profile)
            .expect("dropped live registration should unblock delete");
    }

    #[test]
    fn dropping_reservations_releases_their_state() {
        let runtime = HumanTerminalRuntime::new();
        let profile = profile_id("profile-drop");
        let opening = runtime
            .reserve_open(profile.clone())
            .expect("open reservation should succeed");
        drop(opening);
        let deleting = runtime
            .reserve_delete(profile.clone())
            .expect("dropped opening should unblock delete");
        drop(deleting);
        runtime
            .reserve_open(profile)
            .expect("dropped delete reservation should unblock open");
    }

    #[test]
    fn delete_reservation_survives_outer_blocking_job_handle_drop() {
        let runtime = HumanTerminalRuntime::new();
        let profile = profile_id("profile-blocking-delete");
        let deleting = runtime
            .reserve_delete(profile.clone())
            .expect("delete reservation should succeed");
        let entered = Arc::new(Barrier::new(2));
        let release = Arc::new(Barrier::new(2));
        let finished = Arc::new(Barrier::new(2));
        let worker_entered = Arc::clone(&entered);
        let worker_release = Arc::clone(&release);
        let worker_finished = Arc::clone(&finished);
        let worker = std::thread::spawn(move || {
            let reservation = deleting;
            worker_entered.wait();
            worker_release.wait();
            drop(reservation);
            worker_finished.wait();
        });
        entered.wait();
        drop(worker);

        assert!(matches!(
            runtime.reserve_open(profile.clone()),
            Err(TerminalRegistryError::ProfileDeleting)
        ));
        release.wait();
        finished.wait();
        runtime
            .reserve_open(profile)
            .expect("finished blocking delete should release reservation");
    }

    #[test]
    fn controls_are_display_bound_and_enforce_public_bounds() {
        let runtime = HumanTerminalRuntime::new();
        let profile = profile_id("profile-controls");
        let session = session_id(2);
        let display = display_context("main", "terminal");
        let other_display = display_context("other", "terminal");
        let control = Arc::new(FakeControl::new(FakeFailure::None));
        let registration = runtime
            .reserve_open(profile)
            .expect("open reservation should succeed")
            .promote(session.clone(), display.clone(), control.clone())
            .expect("open reservation should promote");

        assert_eq!(
            runtime.try_input(&session, &other_display, vec![1]),
            Err(TerminalRegistryError::DisplayContextMismatch)
        );
        assert_eq!(
            runtime.try_input(&session, &display, vec![0; MAX_TERMINAL_INPUT_BYTES + 1]),
            Err(TerminalRegistryError::InputTooLarge)
        );
        assert_eq!(
            runtime.try_grant_output_credit(&session, &display, MAX_OUTPUT_CREDIT_GRANT_BYTES + 1),
            Err(TerminalRegistryError::CreditGrantOutOfRange)
        );
        assert_eq!(
            runtime.try_grant_output_credit(&session, &display, 0),
            Err(TerminalRegistryError::CreditGrantOutOfRange)
        );

        let size = TerminalSize::new(120, 40).expect("terminal size should be valid");
        runtime
            .try_input(&session, &display, vec![0, 0xff])
            .expect("bounded input should be sent");
        runtime
            .try_resize(&session, &display, size)
            .expect("valid resize should be sent");
        runtime
            .try_grant_output_credit(&session, &display, 4096)
            .expect("bounded credit should be sent");
        runtime
            .close(&session, &display)
            .expect("local close should remain reachable");

        assert_eq!(
            control.inputs.lock().expect("input lock").as_slice(),
            &[vec![0, 0xff]]
        );
        assert_eq!(
            control.resizes.lock().expect("resize lock").as_slice(),
            &[size]
        );
        assert_eq!(
            control.credits.lock().expect("credit lock").as_slice(),
            &[4096]
        );
        assert_eq!(control.closes.load(Ordering::Relaxed), 1);
        drop(registration);
    }

    #[test]
    fn explicit_close_waits_for_registration_settlement() {
        let runtime = HumanTerminalRuntime::new();
        let session = session_id(6);
        let display = display_context("main", "terminal");
        let control = Arc::new(FakeControl::new(FakeFailure::None));
        let registration = register(
            &runtime,
            "profile-close-settlement",
            session.clone(),
            display.clone(),
            control.clone(),
        );
        let waiter = runtime
            .close(&session, &display)
            .expect("close should produce a settlement waiter");
        let worker = std::thread::spawn(move || drop(registration));

        waiter
            .wait(Duration::from_secs(1))
            .expect("registration drop should settle close");
        worker.join().expect("registration worker should finish");
        assert_eq!(control.closes.load(Ordering::Relaxed), 1);
        assert!(matches!(
            runtime.close(&session, &display),
            Err(TerminalRegistryError::SessionNotFound)
        ));
    }

    #[test]
    fn window_lifecycle_queries_and_close_are_scoped() {
        let runtime = HumanTerminalRuntime::new();
        let first = Arc::new(FakeControl::new(FakeFailure::None));
        let second = Arc::new(FakeControl::new(FakeFailure::None));
        let registrations = [
            register(
                &runtime,
                "profile-window-first",
                session_id(7),
                display_context("main", "first"),
                first.clone(),
            ),
            register(
                &runtime,
                "profile-window-second",
                session_id(8),
                display_context("other", "second"),
                second.clone(),
            ),
        ];

        assert!(runtime.has_live_for_window("main"));
        assert!(!runtime.has_live_for_window("missing"));
        runtime.close_all_for_window("main");
        assert_eq!(first.closes.load(Ordering::Relaxed), 1);
        assert_eq!(second.closes.load(Ordering::Relaxed), 0);
        drop(registrations);
    }

    #[test]
    fn control_failures_are_stable_and_close_all_is_scoped() {
        let runtime = HumanTerminalRuntime::new();
        let display = display_context("main", "terminal");
        let other_display = display_context("main", "other");
        let full = Arc::new(FakeControl::new(FakeFailure::Full));
        let closed = Arc::new(FakeControl::new(FakeFailure::Closed));
        let other = Arc::new(FakeControl::new(FakeFailure::None));
        let full_session = session_id(3);
        let closed_session = session_id(4);
        let registrations = [
            register(
                &runtime,
                "profile-full",
                full_session.clone(),
                display.clone(),
                full.clone(),
            ),
            register(
                &runtime,
                "profile-closed",
                closed_session.clone(),
                display.clone(),
                closed.clone(),
            ),
            register(
                &runtime,
                "profile-other",
                session_id(5),
                other_display,
                other.clone(),
            ),
        ];

        assert_eq!(
            runtime.try_input(&full_session, &display, vec![1]),
            Err(TerminalRegistryError::ControlQueueFull)
        );
        assert_eq!(
            runtime.try_input(&closed_session, &display, vec![1]),
            Err(TerminalRegistryError::SessionClosed)
        );
        runtime.close_all(&display);
        assert_eq!(full.closes.load(Ordering::Relaxed), 1);
        assert_eq!(closed.closes.load(Ordering::Relaxed), 1);
        assert_eq!(other.closes.load(Ordering::Relaxed), 0);
        drop(registrations);
    }

    #[test]
    fn begin_quit_is_idempotent_and_closes_each_live_session_once() {
        let runtime = HumanTerminalRuntime::new();
        let first = Arc::new(FakeControl::new(FakeFailure::None));
        let second = Arc::new(FakeControl::new(FakeFailure::None));
        let registrations = [
            register(
                &runtime,
                "profile-quit-first",
                session_id(9),
                display_context("main", "first"),
                first.clone(),
            ),
            register(
                &runtime,
                "profile-quit-second",
                session_id(10),
                display_context("main", "second"),
                second.clone(),
            ),
        ];

        let first_waiter = runtime.begin_quit().expect("quit should begin");
        let repeated_waiter = runtime.begin_quit().expect("quit should be idempotent");

        assert_eq!(first.closes.load(Ordering::Relaxed), 1);
        assert_eq!(second.closes.load(Ordering::Relaxed), 1);
        drop(registrations);
        first_waiter
            .wait(Duration::from_secs(1))
            .expect("first quit waiter should settle");
        repeated_waiter
            .wait(Duration::ZERO)
            .expect("repeated quit waiter should share settlement");
    }

    #[test]
    fn quitting_rejects_admission_promotion_and_live_controls() {
        let runtime = HumanTerminalRuntime::new();
        let opening = runtime
            .reserve_open(profile_id("profile-opening-during-quit"))
            .expect("open reservation should succeed");
        let session = session_id(11);
        let display = display_context("main", "terminal");
        let control = Arc::new(FakeControl::new(FakeFailure::None));
        let registration = register(
            &runtime,
            "profile-controls-during-quit",
            session.clone(),
            display.clone(),
            control.clone(),
        );

        let waiter = runtime.begin_quit().expect("quit should begin");

        assert!(matches!(
            runtime.reserve_open(profile_id("profile-new-open")),
            Err(TerminalRegistryError::ShuttingDown)
        ));
        assert!(matches!(
            runtime.reserve_delete(profile_id("profile-new-delete")),
            Err(TerminalRegistryError::ShuttingDown)
        ));
        assert!(matches!(
            opening.promote(
                session_id(12),
                display.clone(),
                Arc::new(FakeControl::new(FakeFailure::None)),
            ),
            Err(TerminalRegistryError::ShuttingDown)
        ));
        assert_eq!(
            runtime.try_input(&session, &display, vec![1]),
            Err(TerminalRegistryError::ShuttingDown)
        );
        assert_eq!(
            runtime.try_resize(
                &session,
                &display,
                TerminalSize::new(120, 40).expect("terminal size should be valid"),
            ),
            Err(TerminalRegistryError::ShuttingDown)
        );
        assert_eq!(
            runtime.try_grant_output_credit(&session, &display, 4096),
            Err(TerminalRegistryError::ShuttingDown)
        );
        assert!(control.inputs.lock().expect("input lock").is_empty());
        assert!(control.resizes.lock().expect("resize lock").is_empty());
        assert!(control.credits.lock().expect("credit lock").is_empty());

        let close_waiter = runtime
            .close(&session, &display)
            .expect("close must remain reachable during quit");
        assert_eq!(control.closes.load(Ordering::Relaxed), 2);
        drop(registration);
        close_waiter
            .wait(Duration::from_secs(1))
            .expect("close should settle during quit");
        waiter
            .wait(Duration::from_secs(1))
            .expect("quit should settle after reservations and live sessions drop");
    }

    #[test]
    fn quit_waiter_observes_open_delete_and_live_drop_notifications() {
        let runtime = HumanTerminalRuntime::new();
        let opening = runtime
            .reserve_open(profile_id("profile-opening-drop"))
            .expect("open reservation should succeed");
        let deleting = runtime
            .reserve_delete(profile_id("profile-deleting-drop"))
            .expect("delete reservation should succeed");
        let registration = register(
            &runtime,
            "profile-live-drop",
            session_id(13),
            display_context("main", "terminal"),
            Arc::new(FakeControl::new(FakeFailure::None)),
        );
        let waiter = runtime.begin_quit().expect("quit should begin");

        let worker = std::thread::spawn(move || {
            drop(opening);
            drop(deleting);
            drop(registration);
        });

        waiter
            .wait(Duration::from_secs(1))
            .expect("all registry ownership should settle");
        worker.join().expect("drop worker should finish");
    }

    #[test]
    fn quit_waiter_uses_one_total_deadline_across_notifications() {
        let runtime = HumanTerminalRuntime::new();
        let opening = runtime
            .reserve_open(profile_id("profile-quit-deadline"))
            .expect("open reservation should succeed");
        let waiter = runtime.begin_quit().expect("quit should begin");
        let notifier_inner = Arc::clone(&runtime.inner);
        let notifier = std::thread::spawn(move || {
            for _ in 0..3 {
                std::thread::sleep(Duration::from_millis(50));
                notifier_inner.changed.notify_all();
            }
        });
        let timeout = Duration::from_millis(200);
        let started_at = Instant::now();

        assert_eq!(
            waiter.wait(timeout),
            Err(TerminalRegistryError::CloseSettlementTimeout)
        );
        let elapsed = started_at.elapsed();
        assert!(
            elapsed >= timeout,
            "wait returned before its deadline: {elapsed:?}"
        );
        assert!(
            elapsed < Duration::from_millis(300),
            "notifications reset the total deadline: {elapsed:?}"
        );

        notifier.join().expect("notifier should finish");
        drop(opening);
    }

    fn register(
        runtime: &HumanTerminalRuntime,
        profile: &str,
        session: HumanTerminalSessionId,
        display: DisplayContextId,
        control: Arc<FakeControl>,
    ) -> LiveTerminalRegistration {
        runtime
            .reserve_open(profile_id(profile))
            .expect("open reservation should succeed")
            .promote(session, display, control)
            .expect("open reservation should promote")
    }

    fn profile_id(value: &str) -> ConnectionProfileId {
        ConnectionProfileId::parse(value).expect("profile id should be valid")
    }

    fn session_id(suffix: u64) -> HumanTerminalSessionId {
        HumanTerminalSessionId::parse(format!("00000000-0000-4000-8000-{suffix:012x}"))
            .expect("session id should be a canonical UUIDv4")
    }

    fn display_context(window: &str, webview: &str) -> DisplayContextId {
        DisplayContextId::new(window, webview)
    }
}
