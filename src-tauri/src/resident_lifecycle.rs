use std::{
    sync::{Arc, Condvar, Mutex, MutexGuard},
    time::{Duration, Instant},
};

use tauri::{
    AppHandle, CloseRequestApi, Manager, Runtime, WebviewWindow, WebviewWindowBuilder, Window,
    menu::{Menu, MenuBuilder, MenuEvent},
    tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent},
};

#[cfg(target_os = "macos")]
use tauri::menu::{AboutMetadata, MenuItem, SubmenuBuilder};

pub(crate) const QUIT_DRAIN_TIMEOUT: Duration = Duration::from_secs(5);
pub(crate) const MAIN_WINDOW_LABEL: &str = "main";
pub(crate) const RESIDENT_TRAY_ID: &str = "resident";
pub(crate) const OPEN_VIGILSHELL_MENU_ID: &str = "resident.open_vigilshell";
pub(crate) const APPLICATION_QUIT_VIGILSHELL_MENU_ID: &str = "application.quit_vigilshell";
pub(crate) const TRAY_QUIT_VIGILSHELL_MENU_ID: &str = "resident.quit_vigilshell";
const RESIDENT_TRAY_ICON: tauri::image::Image<'static> =
    tauri::include_image!("./icons/tray-icon.png");

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ResidentLifecyclePhase {
    Running,
    Quitting,
    ExitReady,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OperationAdmissionError {
    ShuttingDown,
    CapacityExhausted,
}

#[derive(Clone)]
pub(crate) struct ResidentLifecycleRuntime {
    inner: Arc<LifecycleInner>,
}

struct LifecycleInner {
    state: Mutex<LifecycleState>,
    changed: Condvar,
    drain_timeout: Duration,
}

struct LifecycleState {
    phase: ResidentLifecyclePhase,
    active_operations: usize,
    quit_deadline: Option<Instant>,
    quit_outcome: Option<QuitWaitOutcome>,
}

impl Default for ResidentLifecycleRuntime {
    fn default() -> Self {
        Self::new()
    }
}

impl ResidentLifecycleRuntime {
    pub(crate) fn new() -> Self {
        Self::with_drain_timeout(QUIT_DRAIN_TIMEOUT)
    }

    fn with_drain_timeout(drain_timeout: Duration) -> Self {
        Self {
            inner: Arc::new(LifecycleInner {
                state: Mutex::new(LifecycleState {
                    phase: ResidentLifecyclePhase::Running,
                    active_operations: 0,
                    quit_deadline: None,
                    quit_outcome: None,
                }),
                changed: Condvar::new(),
                drain_timeout,
            }),
        }
    }

    pub(crate) fn phase(&self) -> ResidentLifecyclePhase {
        lock_state(&self.inner).phase
    }

    pub(crate) fn try_admit_operation(
        &self,
    ) -> Result<ResidentOperationGuard, OperationAdmissionError> {
        let mut state = lock_state(&self.inner);
        if state.phase != ResidentLifecyclePhase::Running {
            return Err(OperationAdmissionError::ShuttingDown);
        }
        state.active_operations = state
            .active_operations
            .checked_add(1)
            .ok_or(OperationAdmissionError::CapacityExhausted)?;
        drop(state);
        Ok(ResidentOperationGuard {
            inner: Some(Arc::clone(&self.inner)),
        })
    }

    pub(crate) fn begin_quit(&self) -> BeginQuit {
        let mut state = lock_state(&self.inner);
        match state.phase {
            ResidentLifecyclePhase::Running => {
                let deadline = Instant::now() + self.inner.drain_timeout;
                state.phase = ResidentLifecyclePhase::Quitting;
                state.quit_deadline = Some(deadline);
                let waiter = QuitWaiter {
                    inner: Arc::clone(&self.inner),
                    deadline,
                };
                BeginQuit::Leader(QuitLeader {
                    waiter,
                    settled: false,
                })
            }
            ResidentLifecyclePhase::Quitting => BeginQuit::Follower(QuitWaiter {
                inner: Arc::clone(&self.inner),
                deadline: state
                    .quit_deadline
                    .expect("quitting lifecycle must have a deadline"),
            }),
            ResidentLifecyclePhase::ExitReady => BeginQuit::ExitReady(
                state
                    .quit_outcome
                    .expect("exit-ready lifecycle must have a settlement"),
            ),
        }
    }
}

pub(crate) struct ResidentOperationGuard {
    inner: Option<Arc<LifecycleInner>>,
}

impl Drop for ResidentOperationGuard {
    fn drop(&mut self) {
        let Some(inner) = self.inner.take() else {
            return;
        };
        let mut state = lock_state(&inner);
        state.active_operations = state.active_operations.saturating_sub(1);
        if state.active_operations == 0 {
            inner.changed.notify_all();
        }
    }
}

pub(crate) enum BeginQuit {
    Leader(QuitLeader),
    Follower(QuitWaiter),
    ExitReady(QuitWaitOutcome),
}

pub(crate) struct QuitLeader {
    waiter: QuitWaiter,
    settled: bool,
}

impl QuitLeader {
    #[cfg(test)]
    pub(crate) fn deadline(&self) -> Instant {
        self.waiter.deadline
    }

    pub(crate) fn remaining(&self) -> Duration {
        self.waiter.remaining()
    }

    #[cfg(test)]
    pub(crate) fn waiter(&self) -> QuitWaiter {
        self.waiter.clone()
    }

    pub(crate) fn wait_for_operations(&self) -> ResidentDrainOutcome {
        let mut state = lock_state(&self.waiter.inner);
        loop {
            if state.active_operations == 0 {
                return ResidentDrainOutcome::Drained;
            }
            let remaining = self.remaining();
            if remaining.is_zero() {
                return ResidentDrainOutcome::DeadlineElapsed {
                    active_operations: state.active_operations,
                };
            }
            state = wait_for_change(&self.waiter.inner, state, remaining);
        }
    }

    pub(crate) fn mark_exit_ready(mut self) -> QuitWaitOutcome {
        let outcome = settle_exit_ready(&self.waiter.inner, self.waiter.deadline);
        self.settled = true;
        outcome
    }
}

impl Drop for QuitLeader {
    fn drop(&mut self) {
        if !self.settled {
            settle_exit_ready(&self.waiter.inner, self.waiter.deadline);
        }
    }
}

#[derive(Clone)]
pub(crate) struct QuitWaiter {
    inner: Arc<LifecycleInner>,
    deadline: Instant,
}

impl QuitWaiter {
    pub(crate) fn remaining(&self) -> Duration {
        self.deadline.saturating_duration_since(Instant::now())
    }

    #[cfg(test)]
    pub(crate) fn wait(&self) -> QuitWaitOutcome {
        let mut state = lock_state(&self.inner);
        loop {
            if let Some(outcome) = state.quit_outcome {
                return outcome;
            }
            let remaining = self.remaining();
            if remaining.is_zero() {
                return settle_exit_ready_locked(&self.inner, &mut state, self.deadline);
            }
            state = wait_for_change(&self.inner, state, remaining);
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum QuitWaitOutcome {
    Settled,
    DeadlineElapsed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ResidentDrainOutcome {
    Drained,
    DeadlineElapsed { active_operations: usize },
}

fn settle_exit_ready(inner: &LifecycleInner, deadline: Instant) -> QuitWaitOutcome {
    let mut state = lock_state(inner);
    settle_exit_ready_locked(inner, &mut state, deadline)
}

fn settle_exit_ready_locked(
    inner: &LifecycleInner,
    state: &mut LifecycleState,
    deadline: Instant,
) -> QuitWaitOutcome {
    if let Some(outcome) = state.quit_outcome {
        return outcome;
    }
    let outcome = if Instant::now() >= deadline {
        QuitWaitOutcome::DeadlineElapsed
    } else {
        QuitWaitOutcome::Settled
    };
    state.phase = ResidentLifecyclePhase::ExitReady;
    state.quit_outcome = Some(outcome);
    inner.changed.notify_all();
    outcome
}

fn lock_state(inner: &LifecycleInner) -> MutexGuard<'_, LifecycleState> {
    inner
        .state
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn wait_for_change<'a>(
    inner: &'a LifecycleInner,
    state: MutexGuard<'a, LifecycleState>,
    timeout: Duration,
) -> MutexGuard<'a, LifecycleState> {
    match inner.changed.wait_timeout(state, timeout) {
        Ok((state, _)) => state,
        Err(poisoned) => poisoned.into_inner().0,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ResidentMenuAction {
    Ignored,
    OpenVigilShell,
    QuitVigilShell,
}

pub(crate) fn build_resident_application_menu<R: Runtime>(
    app: &AppHandle<R>,
) -> tauri::Result<Menu<R>> {
    #[cfg(target_os = "macos")]
    {
        build_macos_application_menu(app)
    }
    #[cfg(not(target_os = "macos"))]
    {
        Menu::default(app)
    }
}

#[cfg(target_os = "macos")]
fn build_macos_application_menu<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<Menu<R>> {
    let package = app.package_info();
    let about = AboutMetadata {
        name: Some(package.name.clone()),
        version: Some(package.version.to_string()),
        copyright: app.config().bundle.copyright.clone(),
        authors: app
            .config()
            .bundle
            .publisher
            .clone()
            .map(|author| vec![author]),
        ..Default::default()
    };
    let quit = MenuItem::with_id(
        app,
        APPLICATION_QUIT_VIGILSHELL_MENU_ID,
        "Quit VigilShell",
        true,
        Some("CmdOrCtrl+Q"),
    )?;
    let app_menu = SubmenuBuilder::new(app, package.name.clone())
        .about(Some(about))
        .separator()
        .services()
        .separator()
        .hide()
        .hide_others()
        .show_all()
        .separator()
        .item(&quit)
        .build()?;
    let file_menu = SubmenuBuilder::new(app, "File").close_window().build()?;
    let edit_menu = SubmenuBuilder::new(app, "Edit")
        .undo()
        .redo()
        .separator()
        .cut()
        .copy()
        .paste()
        .select_all()
        .build()?;
    let view_menu = SubmenuBuilder::new(app, "View").fullscreen().build()?;
    let window_menu = SubmenuBuilder::new(app, "Window")
        .minimize()
        .maximize()
        .separator()
        .close_window()
        .build()?;
    let help_menu = SubmenuBuilder::new(app, "Help").build()?;
    MenuBuilder::new(app)
        .items(&[
            &app_menu,
            &file_menu,
            &edit_menu,
            &view_menu,
            &window_menu,
            &help_menu,
        ])
        .build()
}

pub(crate) fn install_resident_tray<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<TrayIcon<R>> {
    let tray_menu = MenuBuilder::new(app)
        .text(OPEN_VIGILSHELL_MENU_ID, "Open VigilShell")
        .separator()
        .text(TRAY_QUIT_VIGILSHELL_MENU_ID, "Quit VigilShell")
        .build()?;
    TrayIconBuilder::with_id(RESIDENT_TRAY_ID)
        .menu(&tray_menu)
        .icon(RESIDENT_TRAY_ICON)
        .icon_as_template(true)
        .tooltip("VigilShell")
        .show_menu_on_left_click(false)
        .build(app)
}

pub(crate) fn handle_resident_menu_event<R: Runtime>(
    app: &AppHandle<R>,
    event: &MenuEvent,
) -> tauri::Result<ResidentMenuAction> {
    let action = resident_menu_action(event.id().as_ref());
    if action == ResidentMenuAction::OpenVigilShell {
        show_and_focus_main_window(app)?;
    }
    Ok(action)
}

pub(crate) fn handle_resident_tray_event<R: Runtime>(
    app: &AppHandle<R>,
    event: &TrayIconEvent,
) -> tauri::Result<bool> {
    if !is_resident_tray_activation(event) {
        return Ok(false);
    }
    show_and_focus_main_window(app)?;
    Ok(true)
}

pub(crate) fn show_and_focus_main_window<R: Runtime>(
    app: &AppHandle<R>,
) -> tauri::Result<WebviewWindow<R>> {
    let window = match app.get_webview_window(MAIN_WINDOW_LABEL) {
        Some(window) => window,
        None => {
            let config = app
                .config()
                .app
                .windows
                .iter()
                .find(|config| config.label == MAIN_WINDOW_LABEL)
                .ok_or(tauri::Error::WindowNotFound)?;
            WebviewWindowBuilder::from_config(app, config)?.build()?
        }
    };
    #[cfg(target_os = "macos")]
    app.show()?;
    window.show()?;
    if window.is_minimized()? {
        window.unminimize()?;
    }
    window.set_focus()?;
    Ok(window)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SingleInstanceActivationOutcome {
    Opened,
    Ignored,
}

pub(crate) fn handle_single_instance_activation<R: Runtime>(
    app: &AppHandle<R>,
) -> tauri::Result<SingleInstanceActivationOutcome> {
    let Some(lifecycle) = app.try_state::<ResidentLifecycleRuntime>() else {
        return Ok(SingleInstanceActivationOutcome::Ignored);
    };
    let Ok(_activation) = lifecycle.try_admit_operation() else {
        return Ok(SingleInstanceActivationOutcome::Ignored);
    };

    show_and_focus_main_window(app)?;
    Ok(SingleInstanceActivationOutcome::Opened)
}

#[cfg(test)]
fn single_instance_activation_outcome(
    lifecycle_phase: Option<ResidentLifecyclePhase>,
) -> SingleInstanceActivationOutcome {
    match lifecycle_phase {
        Some(ResidentLifecyclePhase::Running) => SingleInstanceActivationOutcome::Opened,
        None | Some(ResidentLifecyclePhase::Quitting | ResidentLifecyclePhase::ExitReady) => {
            SingleInstanceActivationOutcome::Ignored
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MainWindowCloseOutcome {
    Ignored,
    Hidden,
}

pub(crate) fn apply_main_window_close_policy<R: Runtime>(
    window: &Window<R>,
    api: &CloseRequestApi,
) -> tauri::Result<MainWindowCloseOutcome> {
    if window.label() != MAIN_WINDOW_LABEL {
        return Ok(MainWindowCloseOutcome::Ignored);
    }
    api.prevent_close();
    window.hide()?;
    Ok(MainWindowCloseOutcome::Hidden)
}

fn resident_menu_action(id: &str) -> ResidentMenuAction {
    match id {
        OPEN_VIGILSHELL_MENU_ID => ResidentMenuAction::OpenVigilShell,
        APPLICATION_QUIT_VIGILSHELL_MENU_ID | TRAY_QUIT_VIGILSHELL_MENU_ID => {
            ResidentMenuAction::QuitVigilShell
        }
        _ => ResidentMenuAction::Ignored,
    }
}

fn is_resident_tray_activation(event: &TrayIconEvent) -> bool {
    match event {
        TrayIconEvent::Click {
            id,
            button,
            button_state,
            ..
        } => is_left_up_activation(id.as_ref(), *button, *button_state),
        _ => false,
    }
}

fn is_left_up_activation(
    tray_id: &str,
    button: MouseButton,
    button_state: MouseButtonState,
) -> bool {
    tray_id == RESIDENT_TRAY_ID
        && button == MouseButton::Left
        && button_state == MouseButtonState::Up
}

#[cfg(test)]
mod tests {
    use std::{
        sync::{Arc, Barrier},
        thread,
        time::Duration,
    };

    use super::{
        BeginQuit, OperationAdmissionError, QUIT_DRAIN_TIMEOUT, QuitWaitOutcome,
        ResidentDrainOutcome, ResidentLifecyclePhase, ResidentLifecycleRuntime, ResidentMenuAction,
        SingleInstanceActivationOutcome, is_left_up_activation, resident_menu_action,
        single_instance_activation_outcome,
    };
    use tauri::tray::{MouseButton, MouseButtonState};

    #[test]
    fn lifecycle_types_are_thread_safe_and_use_one_five_second_budget() {
        fn assert_send_sync<T: Send + Sync>() {}

        assert_send_sync::<ResidentLifecycleRuntime>();
        assert_eq!(QUIT_DRAIN_TIMEOUT, Duration::from_secs(5));
        let runtime = ResidentLifecycleRuntime::new();
        let BeginQuit::Leader(leader) = runtime.begin_quit() else {
            panic!("first quit must own the drain");
        };
        assert!(leader.deadline() > std::time::Instant::now());
        assert!(leader.remaining() <= QUIT_DRAIN_TIMEOUT);
        leader.mark_exit_ready();
    }

    #[test]
    fn operation_admission_and_quit_transition_share_one_atomic_state() {
        let runtime = ResidentLifecycleRuntime::new();
        let operation = runtime
            .try_admit_operation()
            .expect("running lifecycle should admit work");
        let BeginQuit::Leader(leader) = runtime.begin_quit() else {
            panic!("first quit must own the drain");
        };

        assert_eq!(runtime.phase(), ResidentLifecyclePhase::Quitting);
        assert_eq!(
            runtime.try_admit_operation().err(),
            Some(OperationAdmissionError::ShuttingDown)
        );
        drop(operation);
        assert_eq!(leader.wait_for_operations(), ResidentDrainOutcome::Drained);
        assert_eq!(leader.mark_exit_ready(), QuitWaitOutcome::Settled);
        assert_eq!(runtime.phase(), ResidentLifecyclePhase::ExitReady);
    }

    #[test]
    fn repeated_quit_requests_share_one_settlement() {
        let runtime = ResidentLifecycleRuntime::new();
        let BeginQuit::Leader(leader) = runtime.begin_quit() else {
            panic!("first quit must own the drain");
        };
        let BeginQuit::Follower(follower) = runtime.begin_quit() else {
            panic!("repeated quit must follow the existing drain");
        };
        let follower_thread = thread::spawn(move || follower.wait());

        assert_eq!(leader.mark_exit_ready(), QuitWaitOutcome::Settled);
        assert_eq!(
            follower_thread.join().expect("follower should finish"),
            QuitWaitOutcome::Settled
        );
        assert!(matches!(
            runtime.begin_quit(),
            BeginQuit::ExitReady(QuitWaitOutcome::Settled)
        ));
    }

    #[test]
    fn concurrent_guards_drain_without_lost_notifications() {
        let runtime = ResidentLifecycleRuntime::new();
        let barrier = Arc::new(Barrier::new(5));
        let mut workers = Vec::new();
        for _ in 0..4 {
            let runtime = runtime.clone();
            let barrier = Arc::clone(&barrier);
            workers.push(thread::spawn(move || {
                let operation = runtime
                    .try_admit_operation()
                    .expect("operation should be admitted before quit");
                barrier.wait();
                drop(operation);
            }));
        }
        barrier.wait();
        let BeginQuit::Leader(leader) = runtime.begin_quit() else {
            panic!("first quit must own the drain");
        };

        assert_eq!(leader.wait_for_operations(), ResidentDrainOutcome::Drained);
        leader.mark_exit_ready();
        for worker in workers {
            worker.join().expect("operation worker should finish");
        }
    }

    #[test]
    fn one_deadline_forces_exit_ready_when_work_does_not_settle() {
        let runtime = ResidentLifecycleRuntime::with_drain_timeout(Duration::ZERO);
        let operation = runtime
            .try_admit_operation()
            .expect("running lifecycle should admit work");
        let BeginQuit::Leader(leader) = runtime.begin_quit() else {
            panic!("first quit must own the drain");
        };
        let waiter = leader.waiter();

        assert_eq!(
            leader.wait_for_operations(),
            ResidentDrainOutcome::DeadlineElapsed {
                active_operations: 1
            }
        );
        assert_eq!(waiter.wait(), QuitWaitOutcome::DeadlineElapsed);
        assert_eq!(runtime.phase(), ResidentLifecyclePhase::ExitReady);
        assert_eq!(leader.mark_exit_ready(), QuitWaitOutcome::DeadlineElapsed);
        drop(operation);
    }

    #[test]
    fn dropping_the_unique_quit_leader_cannot_strand_followers() {
        let runtime = ResidentLifecycleRuntime::new();
        let BeginQuit::Leader(leader) = runtime.begin_quit() else {
            panic!("first quit must own the drain");
        };
        let BeginQuit::Follower(follower) = runtime.begin_quit() else {
            panic!("repeated quit must follow the existing drain");
        };

        drop(leader);
        assert_eq!(follower.wait(), QuitWaitOutcome::Settled);
        assert_eq!(runtime.phase(), ResidentLifecyclePhase::ExitReady);
    }

    #[test]
    fn resident_menu_ids_map_only_to_owned_actions() {
        assert_ne!(
            super::APPLICATION_QUIT_VIGILSHELL_MENU_ID,
            super::TRAY_QUIT_VIGILSHELL_MENU_ID
        );
        assert_eq!(
            resident_menu_action(super::OPEN_VIGILSHELL_MENU_ID),
            ResidentMenuAction::OpenVigilShell
        );
        assert_eq!(
            resident_menu_action(super::APPLICATION_QUIT_VIGILSHELL_MENU_ID),
            ResidentMenuAction::QuitVigilShell
        );
        assert_eq!(
            resident_menu_action(super::TRAY_QUIT_VIGILSHELL_MENU_ID),
            ResidentMenuAction::QuitVigilShell
        );
        assert_eq!(
            resident_menu_action("predefined-quit"),
            ResidentMenuAction::Ignored
        );
    }

    #[test]
    fn resident_tray_activation_is_only_left_button_up_on_the_owned_icon() {
        assert!(is_left_up_activation(
            super::RESIDENT_TRAY_ID,
            MouseButton::Left,
            MouseButtonState::Up
        ));
        assert!(!is_left_up_activation(
            super::RESIDENT_TRAY_ID,
            MouseButton::Left,
            MouseButtonState::Down
        ));
        assert!(!is_left_up_activation(
            "another-tray",
            MouseButton::Left,
            MouseButtonState::Up
        ));
        assert!(!is_left_up_activation(
            super::RESIDENT_TRAY_ID,
            MouseButton::Right,
            MouseButtonState::Up
        ));
    }

    #[test]
    fn single_instance_activation_opens_only_while_running() {
        assert_eq!(
            single_instance_activation_outcome(None),
            SingleInstanceActivationOutcome::Ignored
        );
        assert_eq!(
            single_instance_activation_outcome(Some(ResidentLifecyclePhase::Running)),
            SingleInstanceActivationOutcome::Opened
        );
        assert_eq!(
            single_instance_activation_outcome(Some(ResidentLifecyclePhase::Quitting)),
            SingleInstanceActivationOutcome::Ignored
        );
        assert_eq!(
            single_instance_activation_outcome(Some(ResidentLifecyclePhase::ExitReady)),
            SingleInstanceActivationOutcome::Ignored
        );
    }
}
