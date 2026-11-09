//! 连接内分类型限流、应用级原子准入；排队不预占全局名额，取消自动撤销排队。
use super::*;
use std::collections::VecDeque;

const GLOBAL_LIMIT: usize = 16;
const QUEUE_LIMIT: usize = 128;

#[derive(Default)]
pub(super) struct Scheduler {
    state: Mutex<ScheduleState>,
    changed: tokio::sync::Notify,
    maintenance: Mutex<HashMap<String, std::sync::Weak<tokio::sync::Mutex<()>>>>,
}

#[derive(Default)]
struct ScheduleState {
    sequence: u64,
    limits: HashMap<String, (u64, ExecutionLimits)>,
    active: HashMap<(String, ExecutionClass), usize>,
    pending: VecDeque<Request>,
}

struct Request {
    ticket: u64,
    key: (String, ExecutionClass),
}

pub(super) struct Permit {
    scheduler: Arc<Scheduler>,
    ticket: u64,
    key: (String, ExecutionClass),
    running: bool,
}

impl Scheduler {
    pub(super) fn maintenance(&self, connection: &str) -> Arc<tokio::sync::Mutex<()>> {
        let mut locks = self.maintenance.lock().unwrap();
        locks.retain(|_, value| value.strong_count() > 0);
        if let Some(value) = locks.get(connection).and_then(std::sync::Weak::upgrade) {
            return value;
        }
        let lock = Arc::new(tokio::sync::Mutex::new(()));
        locks.insert(connection.into(), Arc::downgrade(&lock));
        lock
    }
    pub(super) fn configure(&self, connection: &str, revision: u64, limits: &ExecutionLimits) {
        let mut state = self.state.lock().unwrap();
        if state
            .limits
            .get(connection)
            .is_some_and(|(current, _)| *current > revision)
        {
            return;
        }
        state
            .limits
            .insert(connection.into(), (revision, limits.clone()));
        drop(state);
        self.changed.notify_waiters();
    }

    pub(super) async fn acquire(
        self: &Arc<Self>,
        connection: &str,
        class: ExecutionClass,
    ) -> AgentResult<Permit> {
        let key = (connection.to_owned(), class);
        let ticket = {
            let mut state = self.state.lock().unwrap();
            if state.pending.len() >= QUEUE_LIMIT {
                return Err("execution_queue_full");
            }
            state.sequence = state
                .sequence
                .checked_add(1)
                .ok_or("execution_queue_full")?;
            let ticket = state.sequence;
            state.pending.push_back(Request {
                ticket,
                key: key.clone(),
            });
            ticket
        };
        let mut permit = Permit {
            scheduler: self.clone(),
            ticket,
            key,
            running: false,
        };
        loop {
            let changed = self.changed.notified();
            tokio::pin!(changed);
            // 在读取队列前登记通知，避免最后一次释放发生在检查与等待之间。
            changed.as_mut().enable();
            let admitted = {
                let mut state = self.state.lock().unwrap();
                let next = if state.active.values().sum::<usize>() < GLOBAL_LIMIT {
                    state
                        .pending
                        .iter()
                        .find(|r| {
                            let limit = state
                                .limits
                                .get(&r.key.0)
                                .map_or(0, |(_, l)| l.concurrency(r.key.1));
                            state.active.get(&r.key).copied().unwrap_or(0) < limit
                        })
                        .map(|r| r.ticket)
                } else {
                    None
                };
                if next == Some(ticket) {
                    state.pending.retain(|r| r.ticket != ticket);
                    *state.active.entry(permit.key.clone()).or_default() += 1;
                    true
                } else {
                    false
                }
            };
            if admitted {
                permit.running = true;
                self.changed.notify_waiters();
                return Ok(permit);
            }
            changed.await;
        }
    }
}

impl Drop for Permit {
    fn drop(&mut self) {
        let mut state = self.scheduler.state.lock().unwrap();
        if self.running {
            if let Some(count) = state.active.get_mut(&self.key) {
                *count -= 1;
                if *count == 0 {
                    state.active.remove(&self.key);
                }
            }
        } else {
            state.pending.retain(|r| r.ticket != self.ticket);
        }
        drop(state);
        self.scheduler.changed.notify_waiters();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        future::{Future, poll_fn},
        pin::Pin,
        task::Poll,
    };

    async fn pending<T>(future: Pin<&mut impl Future<Output = T>>) {
        let mut future = future;
        poll_fn(|cx| {
            assert!(future.as_mut().poll(cx).is_pending());
            Poll::Ready(())
        })
        .await;
    }

    #[tokio::test]
    async fn categories_are_independent_and_waiting_connections_do_not_hoard_global_slots() {
        let scheduler = Arc::new(Scheduler::default());
        let limits = ExecutionLimits {
            concurrent_tasks: 1,
            ..ExecutionLimits::default()
        };
        scheduler.configure("a", 0, &limits);
        scheduler.configure("b", 0, &limits);
        let task = scheduler.acquire("a", ExecutionClass::Task).await.unwrap();
        let mut waiting = Box::pin(scheduler.acquire("a", ExecutionClass::Task));
        pending(waiting.as_mut()).await;
        let terminal = scheduler
            .acquire("a", ExecutionClass::Terminal)
            .await
            .unwrap();
        let upload = scheduler
            .acquire("a", ExecutionClass::Transfer)
            .await
            .unwrap();
        let other = scheduler.acquire("b", ExecutionClass::Task).await.unwrap();
        assert_eq!(
            scheduler
                .state
                .lock()
                .unwrap()
                .active
                .values()
                .sum::<usize>(),
            4
        );
        drop(task);
        let resumed = waiting.await.unwrap();
        drop((terminal, upload, other, resumed));
        assert!(scheduler.state.lock().unwrap().active.is_empty());
    }

    #[tokio::test]
    async fn lowering_limits_drains_without_killing_and_cancellation_releases_queue() {
        let scheduler = Arc::new(Scheduler::default());
        let mut limits = ExecutionLimits {
            concurrent_tasks: 2,
            ..ExecutionLimits::default()
        };
        scheduler.configure("a", 0, &limits);
        let first = scheduler.acquire("a", ExecutionClass::Task).await.unwrap();
        let second = scheduler.acquire("a", ExecutionClass::Task).await.unwrap();
        limits.concurrent_tasks = 1;
        scheduler.configure("a", 1, &limits);
        scheduler.configure("a", 0, &ExecutionLimits::default());
        let mut queued = Box::pin(scheduler.acquire("a", ExecutionClass::Task));
        pending(queued.as_mut()).await;
        drop(first);
        pending(queued.as_mut()).await;
        drop(queued);
        assert!(scheduler.state.lock().unwrap().pending.is_empty());
        drop(second);
        let next = scheduler.acquire("a", ExecutionClass::Task).await.unwrap();
        drop(next);
        assert!(scheduler.state.lock().unwrap().active.is_empty());
    }

    #[tokio::test]
    async fn global_ceiling_includes_terminals_and_transfers() {
        let scheduler = Arc::new(Scheduler::default());
        let limits = ExecutionLimits {
            concurrent_tasks: 16,
            ..ExecutionLimits::default()
        };
        scheduler.configure("a", 0, &limits);
        let mut running = Vec::new();
        for _ in 0..GLOBAL_LIMIT {
            running.push(scheduler.acquire("a", ExecutionClass::Task).await.unwrap());
        }
        let mut terminal = Box::pin(scheduler.acquire("a", ExecutionClass::Terminal));
        pending(terminal.as_mut()).await;
        drop(running.pop());
        let terminal = terminal.await.unwrap();
        drop((terminal, running));
        assert!(scheduler.state.lock().unwrap().active.is_empty());
    }
}
