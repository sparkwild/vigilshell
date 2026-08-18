#![deny(unsafe_op_in_unsafe_fn)]

use std::{error::Error, fmt, sync::Arc};

#[cfg(any(target_os = "macos", test))]
use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{Mutex, MutexGuard},
};

type TerminationCallback = dyn Fn() -> TerminationDecision + Send + Sync + 'static;

/// 原生终止询问的同步决策。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerminationDecision {
    Cancel,
    TerminateNow,
}

/// macOS 原生生命周期适配器可公开的稳定错误。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NativeLifecycleError {
    UnsupportedPlatform,
    MainThreadRequired,
    ApplicationDelegateUnavailable,
    UnexpectedApplicationDelegateClass,
    HookClassUnavailable,
    HookClassLayoutMismatch,
    HookAlreadyInstalled,
    HookNotInstalled,
}

impl fmt::Display for NativeLifecycleError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedPlatform => {
                formatter.write_str("native lifecycle integration is unavailable on this platform")
            }
            Self::MainThreadRequired => {
                formatter.write_str("native lifecycle integration requires the main thread")
            }
            Self::ApplicationDelegateUnavailable => {
                formatter.write_str("the native application delegate is unavailable")
            }
            Self::UnexpectedApplicationDelegateClass => {
                formatter.write_str("the native application delegate is not TaoAppDelegateParent")
            }
            Self::HookClassUnavailable => {
                formatter.write_str("the native termination hook class is unavailable")
            }
            Self::HookClassLayoutMismatch => {
                formatter.write_str("the native termination hook class has an incompatible layout")
            }
            Self::HookAlreadyInstalled => {
                formatter.write_str("the native termination hook is already installed")
            }
            Self::HookNotInstalled => {
                formatter.write_str("the native termination hook is not installed")
            }
        }
    }
}

impl Error for NativeLifecycleError {}

/// 在 macOS 主线程为当前 Tao application delegate 安装一次终止询问回调。
///
/// 回调可能被 AppKit 重复调用，必须快速返回且不得阻塞主线程。回调 panic 时适配器按
/// `Cancel` 失败关闭。非 macOS 平台稳定返回 `UnsupportedPlatform`。
pub fn install_native_termination_hook<F>(callback: F) -> Result<(), NativeLifecycleError>
where
    F: Fn() -> TerminationDecision + Send + Sync + 'static,
{
    platform::install(Arc::new(callback))
}

/// 在 macOS 主线程请求 AppKit 走原生终止询问链。
///
/// 此函数不会绕过已安装的回调；尚未安装 hook 时失败关闭。非 macOS 平台稳定返回
/// `UnsupportedPlatform`。需要从 worker 或 Tao user-event 处理链触发退出时，应使用
/// [`schedule_native_termination`] 避免同步重入。
pub fn request_native_termination() -> Result<(), NativeLifecycleError> {
    platform::request_termination()
}

/// 从任意线程把原生终止请求异步调度到 macOS 主队列。
///
/// 返回成功仅表示请求已入队；真正的 `NSApplication::terminate` 会在当前 Tao user-event
/// handler 返回后的主队列轮次执行。尚未安装 hook 时失败关闭，非 macOS 平台稳定返回
/// `UnsupportedPlatform`。
pub fn schedule_native_termination() -> Result<(), NativeLifecycleError> {
    platform::schedule_termination()
}

#[cfg(any(target_os = "macos", test))]
struct CallbackSlot {
    callback: Mutex<Option<Arc<TerminationCallback>>>,
}

#[cfg(any(target_os = "macos", test))]
impl CallbackSlot {
    const fn new() -> Self {
        Self {
            callback: Mutex::new(None),
        }
    }

    fn is_installed(&self) -> bool {
        self.lock().is_some()
    }

    fn install(&self, callback: Arc<TerminationCallback>) -> Result<(), ()> {
        let mut installed = self.lock();
        if installed.is_some() {
            return Err(());
        }
        *installed = Some(callback);
        Ok(())
    }

    fn decision(&self) -> TerminationDecision {
        let callback = self.lock().clone();
        let Some(callback) = callback else {
            return TerminationDecision::Cancel;
        };
        catch_unwind(AssertUnwindSafe(|| callback())).unwrap_or(TerminationDecision::Cancel)
    }

    fn lock(&self) -> MutexGuard<'_, Option<Arc<TerminationCallback>>> {
        self.callback
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

#[cfg(target_os = "macos")]
mod platform {
    use std::{panic::AssertUnwindSafe, sync::OnceLock};

    use dispatch2::DispatchQueue;
    use objc2::{
        MainThreadMarker,
        runtime::{AnyClass, AnyObject, ClassBuilder, ProtocolObject, Sel},
        sel,
    };
    use objc2_app_kit::{NSApplication, NSApplicationDelegate, NSApplicationTerminateReply};

    use super::{
        Arc, CALLBACK_SLOT, NativeLifecycleError, TerminationCallback, TerminationDecision,
        catch_unwind,
    };

    const TAO_DELEGATE_CLASS_NAME: &std::ffi::CStr = c"TaoAppDelegateParent";
    const HOOK_CLASS_NAME: &std::ffi::CStr = c"VigilShellTaoTerminationDelegate";
    const TAO_AUX_STATE_IVAR_NAME: &std::ffi::CStr = c"auxState";

    static HOOK_CLASS: OnceLock<&'static AnyClass> = OnceLock::new();

    pub(super) fn install(callback: Arc<TerminationCallback>) -> Result<(), NativeLifecycleError> {
        let main_thread =
            MainThreadMarker::new().ok_or(NativeLifecycleError::MainThreadRequired)?;
        if CALLBACK_SLOT.is_installed() {
            return Err(NativeLifecycleError::HookAlreadyInstalled);
        }

        install_delegate_subclass(main_thread)?;
        CALLBACK_SLOT
            .install(callback)
            .map_err(|()| NativeLifecycleError::HookAlreadyInstalled)
    }

    pub(super) fn request_termination() -> Result<(), NativeLifecycleError> {
        let main_thread =
            MainThreadMarker::new().ok_or(NativeLifecycleError::MainThreadRequired)?;
        if !CALLBACK_SLOT.is_installed() {
            return Err(NativeLifecycleError::HookNotInstalled);
        }

        NSApplication::sharedApplication(main_thread).terminate(None);
        Ok(())
    }

    pub(super) fn schedule_termination() -> Result<(), NativeLifecycleError> {
        if !CALLBACK_SLOT.is_installed() {
            return Err(NativeLifecycleError::HookNotInstalled);
        }

        DispatchQueue::main().exec_async(|| {
            let _ = catch_unwind(AssertUnwindSafe(|| {
                let Some(main_thread) = MainThreadMarker::new() else {
                    return;
                };
                NSApplication::sharedApplication(main_thread).terminate(None);
            }));
        });
        Ok(())
    }

    fn install_delegate_subclass(
        main_thread: MainThreadMarker,
    ) -> Result<(), NativeLifecycleError> {
        let application = NSApplication::sharedApplication(main_thread);
        let delegate = application
            .delegate()
            .ok_or(NativeLifecycleError::ApplicationDelegateUnavailable)?;
        let protocol_delegate: &ProtocolObject<dyn NSApplicationDelegate> = &delegate;
        let delegate_object: &AnyObject = protocol_delegate.as_ref();

        let tao_class = AnyClass::get(TAO_DELEGATE_CLASS_NAME)
            .ok_or(NativeLifecycleError::UnexpectedApplicationDelegateClass)?;
        if delegate_object.class() != tao_class
            || tao_class
                .instance_variable(TAO_AUX_STATE_IVAR_NAME)
                .is_none()
        {
            return Err(NativeLifecycleError::UnexpectedApplicationDelegateClass);
        }

        let hook_class = termination_hook_class(tao_class)?;

        // SAFETY: 当前实例已严格验证为 hook 的直接父类；动态子类不增加 ivar，实例大小
        // 相同，且唯一覆写方法使用与 NSApplicationDelegate 一致的编码。调用发生在主线程，
        // 安装期间没有并发修改 delegate class 的合法入口。
        let previous_class = unsafe { AnyObject::set_class(delegate_object, hook_class) };
        debug_assert_eq!(previous_class, tao_class);
        Ok(())
    }

    fn termination_hook_class(
        tao_class: &'static AnyClass,
    ) -> Result<&'static AnyClass, NativeLifecycleError> {
        if let Some(hook_class) = HOOK_CLASS.get().copied() {
            verify_hook_class(hook_class, tao_class)?;
            return Ok(hook_class);
        }

        let hook_class = catch_unwind(AssertUnwindSafe(|| {
            let mut builder = ClassBuilder::new(HOOK_CLASS_NAME, tao_class)?;

            // SAFETY: selector 参数和返回值使用 objc2-app-kit 生成的精确 AppKit 类型；实现
            // 捕获所有 Rust unwind，且不持有 Objective-C 对象或增加实例变量。
            unsafe {
                builder.add_method(
                    sel!(applicationShouldTerminate:),
                    application_should_terminate as extern "C" fn(_, _, _) -> _,
                );
            }
            Some(builder.register())
        }))
        .ok()
        .flatten()
        .ok_or(NativeLifecycleError::HookClassUnavailable)?;

        verify_hook_class(hook_class, tao_class)?;
        HOOK_CLASS
            .set(hook_class)
            .map_err(|_| NativeLifecycleError::HookClassUnavailable)?;
        Ok(hook_class)
    }

    fn verify_hook_class(
        hook_class: &AnyClass,
        tao_class: &AnyClass,
    ) -> Result<(), NativeLifecycleError> {
        if hook_class.superclass() != Some(tao_class)
            || hook_class.instance_size() != tao_class.instance_size()
            || !hook_class.instance_variables().is_empty()
        {
            return Err(NativeLifecycleError::HookClassLayoutMismatch);
        }
        Ok(())
    }

    extern "C" fn application_should_terminate(
        _delegate: &AnyObject,
        _selector: Sel,
        _application: &NSApplication,
    ) -> NSApplicationTerminateReply {
        let decision = catch_unwind(AssertUnwindSafe(|| CALLBACK_SLOT.decision()))
            .unwrap_or(TerminationDecision::Cancel);
        match decision {
            TerminationDecision::Cancel => NSApplicationTerminateReply::TerminateCancel,
            TerminationDecision::TerminateNow => NSApplicationTerminateReply::TerminateNow,
        }
    }
}

#[cfg(target_os = "macos")]
static CALLBACK_SLOT: CallbackSlot = CallbackSlot::new();

#[cfg(not(target_os = "macos"))]
mod platform {
    use super::{Arc, NativeLifecycleError, TerminationCallback};

    pub(super) fn install(_callback: Arc<TerminationCallback>) -> Result<(), NativeLifecycleError> {
        Err(NativeLifecycleError::UnsupportedPlatform)
    }

    pub(super) fn request_termination() -> Result<(), NativeLifecycleError> {
        Err(NativeLifecycleError::UnsupportedPlatform)
    }

    pub(super) fn schedule_termination() -> Result<(), NativeLifecycleError> {
        Err(NativeLifecycleError::UnsupportedPlatform)
    }
}

#[cfg(test)]
mod tests {
    use std::{sync::Arc, thread};

    use super::{
        CallbackSlot, NativeLifecycleError, TerminationDecision, schedule_native_termination,
    };

    #[test]
    fn callback_slot_fails_closed_before_installation() {
        assert_eq!(CallbackSlot::new().decision(), TerminationDecision::Cancel);
    }

    #[test]
    fn callback_slot_keeps_the_first_callback() {
        let slot = CallbackSlot::new();
        assert!(
            slot.install(Arc::new(|| TerminationDecision::TerminateNow))
                .is_ok()
        );
        assert!(
            slot.install(Arc::new(|| TerminationDecision::Cancel))
                .is_err()
        );
        assert_eq!(slot.decision(), TerminationDecision::TerminateNow);
    }

    #[test]
    fn callback_panic_fails_closed() {
        let slot = CallbackSlot::new();
        slot.install(Arc::new(|| panic!("synthetic callback panic")))
            .expect("first callback should install");

        assert_eq!(slot.decision(), TerminationDecision::Cancel);
    }

    #[test]
    fn callback_state_is_thread_safe() {
        fn assert_send_sync<T: Send + Sync>() {}

        assert_send_sync::<CallbackSlot>();
        let slot = Arc::new(CallbackSlot::new());
        slot.install(Arc::new(|| TerminationDecision::TerminateNow))
            .expect("first callback should install");
        let worker_slot = Arc::clone(&slot);

        assert_eq!(
            thread::spawn(move || worker_slot.decision())
                .join()
                .expect("callback worker should finish"),
            TerminationDecision::TerminateNow
        );
    }

    #[test]
    fn scheduled_termination_is_worker_callable_and_fails_closed_without_a_hook() {
        let result = thread::spawn(schedule_native_termination)
            .join()
            .expect("schedule worker should finish");

        #[cfg(target_os = "macos")]
        assert_eq!(result, Err(NativeLifecycleError::HookNotInstalled));
        #[cfg(not(target_os = "macos"))]
        assert_eq!(result, Err(NativeLifecycleError::UnsupportedPlatform));
    }
}
