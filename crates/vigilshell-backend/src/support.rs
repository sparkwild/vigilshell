//! 进程级诊断、受监督的阻塞工作与有期限收尾。
use std::{future::Future, io, panic::AssertUnwindSafe, time::Duration};
use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;

pub const QUEUE_CAPACITY: usize = 64;
pub const SHUTDOWN_BUDGET: Duration = Duration::from_secs(2);

pub fn diagnostics(target: &'static str, application: &'static str) {
    let authentication =
        if std::env::var_os("VIGILSHELL_AUTH_DIAGNOSTICS").is_some_and(|v| v == "1") {
            ",vigilshell_auth_diagnostics=info"
        } else {
            ""
        };
    tracing_subscriber::fmt()
        .json()
        .with_ansi(false)
        .with_target(false)
        .with_writer(io::stderr)
        .with_env_filter(tracing_subscriber::EnvFilter::new(format!(
            "{target}=info,vigilshell_credentials=info{authentication}"
        )))
        .init();
    if !authentication.is_empty() {
        tracing::info!(target: "vigilshell_auth_diagnostics", stage="auth_diagnostics", outcome="enabled");
    }
    std::panic::set_hook(Box::new(move |_| {
        tracing::error!(
            application,
            pid = std::process::id(),
            stage = "panic",
            outcome = "failed"
        );
    }));
}

pub fn thread<T: Send + 'static>(
    name: &str,
    work: impl FnOnce() -> io::Result<T> + Send + 'static,
) -> io::Result<oneshot::Receiver<io::Result<T>>> {
    let (sender, receiver) = oneshot::channel();
    std::thread::Builder::new()
        .name(name.into())
        .spawn(move || {
            let result = std::panic::catch_unwind(AssertUnwindSafe(work))
                .unwrap_or_else(|_| Err(io::Error::other("worker_panicked")));
            let _ = sender.send(result);
        })?;
    Ok(receiver)
}

pub async fn completion<T>(receiver: oneshot::Receiver<io::Result<T>>) -> io::Result<T> {
    receiver
        .await
        .map_err(|_| io::Error::other("worker_lost"))?
}

/// 专用 stdin 线程可停在 OS read；它不参加会阻塞进程退出的 join。
/// 数据库和输出工作则必须在预算内完成，否则不能确认成功退出。
pub async fn supervise(
    work: impl Future<Output = io::Result<()>>,
    shutdown: CancellationToken,
) -> io::Result<()> {
    tokio::pin!(work);
    tokio::select! {
        result = &mut work => result,
        () = shutdown.cancelled() => {
            tokio::time::timeout(SHUTDOWN_BUDGET, &mut work).await
                .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "shutdown_timeout"))?
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test(start_paused = true)]
    async fn shutdown_bounds_a_stalled_worker() {
        let shutdown = CancellationToken::new();
        shutdown.cancel();
        let start = tokio::time::Instant::now();
        let result = supervise(std::future::pending(), shutdown).await;
        assert_eq!(result.unwrap_err().kind(), io::ErrorKind::TimedOut);
        assert_eq!(start.elapsed(), SHUTDOWN_BUDGET);
    }
    #[tokio::test]
    async fn thread_panics_are_observed() {
        let completion = thread::<()>("panic-test", || panic!("test worker panic")).unwrap();
        assert!(super::completion(completion).await.is_err());
    }
}
