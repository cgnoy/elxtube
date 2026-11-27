use std::cmp::Ordering as CmpOrdering;
use std::collections::{BinaryHeap, VecDeque};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::time::{Duration, Instant};

use anyhow::Result;
use futures_channel::oneshot;

use crate::core::cancellation::CancellationToken;

#[derive(Debug)]
pub enum TaskOutcome<T> {
    Completed(T),
    Failed(String),
    Cancelled,
    TimedOut,
    Stale,
}

impl<T> TaskOutcome<T> {
    pub fn completed(self) -> Option<T> {
        match self {
            Self::Completed(value) => Some(value),
            _ => None,
        }
    }

    pub fn error_message(&self) -> Option<&str> {
        match self {
            Self::Failed(message) => Some(message),
            _ => None,
        }
    }
}

#[derive(Clone)]
pub struct TaskContext {
    cancellation: CancellationToken,
    deadline: Instant,
}

impl TaskContext {
    pub fn is_cancelled(&self) -> bool {
        self.cancellation.is_cancelled()
    }

    pub fn cancellation_token(&self) -> CancellationToken {
        self.cancellation.clone()
    }

    pub fn is_expired(&self) -> bool {
        Instant::now() >= self.deadline
    }
}

pub struct TaskHandle<T> {
    cancellation: CancellationToken,
    receiver: oneshot::Receiver<TaskOutcome<T>>,
    sender: Arc<Mutex<Option<oneshot::Sender<TaskOutcome<T>>>>>,
}

impl<T> TaskHandle<T> {
    pub fn cancel(&self) {
        self.cancellation.cancel();
        if let Some(sender) = self.sender.lock().unwrap().take() {
            let _ = sender.send(TaskOutcome::Cancelled);
        }
    }

    pub async fn completion(self) -> TaskOutcome<T> {
        self.receiver.await.unwrap_or(TaskOutcome::Cancelled)
    }
}

#[derive(Clone, Default)]
pub struct TaskRunner;

impl TaskRunner {
    pub fn new() -> Self {
        Self
    }

    pub fn scope(&self) -> TaskScope {
        TaskScope {
            generation: Arc::new(AtomicU64::new(0)),
            active: Mutex::new(None),
            pool: TaskPool::global(),
        }
    }
}

pub struct TaskScope {
    generation: Arc<AtomicU64>,
    active: Mutex<Option<CancellationToken>>,
    pool: &'static TaskPool,
}

type PoolJob = Box<dyn FnOnce() + Send + 'static>;
type TimeoutCallback = Box<dyn FnOnce() + Send + 'static>;

struct TimeoutEntry {
    deadline: Instant,
    id: u64,
    callback: TimeoutCallback,
}

impl PartialEq for TimeoutEntry {
    fn eq(&self, other: &Self) -> bool {
        self.deadline == other.deadline && self.id == other.id
    }
}

impl Eq for TimeoutEntry {}

impl PartialOrd for TimeoutEntry {
    fn partial_cmp(&self, other: &Self) -> Option<CmpOrdering> {
        Some(self.cmp(other))
    }
}

impl Ord for TimeoutEntry {
    fn cmp(&self, other: &Self) -> CmpOrdering {
        other
            .deadline
            .cmp(&self.deadline)
            .then_with(|| other.id.cmp(&self.id))
    }
}

struct TaskPool {
    queue: Arc<(Mutex<VecDeque<PoolJob>>, Condvar)>,
    timers: Arc<(Mutex<BinaryHeap<TimeoutEntry>>, Condvar)>,
    started: AtomicBool,
    next_timer: AtomicU64,
}

impl TaskPool {
    fn global() -> &'static Self {
        static POOL: OnceLock<TaskPool> = OnceLock::new();
        POOL.get_or_init(Self::new)
    }

    fn new() -> Self {
        let timers = Arc::new((
            Mutex::new(BinaryHeap::<TimeoutEntry>::new()),
            Condvar::new(),
        ));
        let timer_queue = timers.clone();
        std::thread::spawn(move || loop {
            let (lock, condvar) = &*timer_queue;
            let mut heap = lock.lock().unwrap();
            let next = heap.peek().map(|entry| entry.deadline);
            let Some(deadline) = next else {
                heap = condvar.wait(heap).unwrap();
                continue;
            };
            let now = Instant::now();
            if deadline > now {
                let (guard, _) = condvar.wait_timeout(heap, deadline - now).unwrap();
                heap = guard;
                continue;
            }
            let entry = heap.pop().unwrap();
            drop(heap);
            (entry.callback)();
        });
        Self {
            queue: Arc::new((Mutex::new(VecDeque::new()), Condvar::new())),
            timers,
            started: AtomicBool::new(false),
            next_timer: AtomicU64::new(0),
        }
    }

    fn execute(&self, job: PoolJob) {
        if !self.started.swap(true, Ordering::AcqRel) {
            for _ in 0..6 {
                let queue = self.queue.clone();
                std::thread::spawn(move || loop {
                    let job = {
                        let (lock, condvar) = &*queue;
                        let mut jobs = lock.lock().unwrap();
                        loop {
                            if let Some(job) = jobs.pop_front() {
                                break job;
                            }
                            jobs = condvar.wait(jobs).unwrap();
                        }
                    };
                    job();
                });
            }
        }
        let (lock, condvar) = &*self.queue;
        lock.lock().unwrap().push_back(job);
        condvar.notify_one();
    }

    fn schedule_timeout(&self, timeout: Duration, callback: TimeoutCallback) {
        let id = self.next_timer.fetch_add(1, Ordering::Relaxed);
        let entry = TimeoutEntry {
            deadline: Instant::now() + timeout,
            id,
            callback,
        };
        let (lock, condvar) = &*self.timers;
        lock.lock().unwrap().push(entry);
        condvar.notify_all();
    }
}

impl TaskScope {
    pub fn cancel(&self) {
        if let Some(cancellation) = self.active.lock().unwrap().take() {
            cancellation.cancel();
        }
    }

    pub fn spawn<T, F>(&self, timeout: Duration, work: F) -> TaskHandle<T>
    where
        T: Send + 'static,
        F: FnOnce(&TaskContext) -> Result<T> + Send + 'static,
    {
        self.cancel();
        let generation = self.generation.fetch_add(1, Ordering::AcqRel) + 1;
        let cancellation = CancellationToken::default();
        *self.active.lock().unwrap() = Some(cancellation.clone());
        let context = TaskContext {
            cancellation: cancellation.clone(),
            deadline: Instant::now() + timeout,
        };
        let (sender, receiver) = oneshot::channel();
        let sender = Arc::new(Mutex::new(Some(sender)));
        let scope_generation = self.generation.clone();
        let worker_sender = sender.clone();
        self.pool.execute(Box::new(move || {
            let outcome = if context.is_cancelled() {
                TaskOutcome::Cancelled
            } else {
                match catch_unwind(AssertUnwindSafe(|| work(&context))) {
                    Ok(Ok(value)) => {
                        if scope_generation.load(Ordering::Acquire) != generation {
                            TaskOutcome::Stale
                        } else if context.is_cancelled() {
                            TaskOutcome::Cancelled
                        } else if context.is_expired() {
                            TaskOutcome::TimedOut
                        } else {
                            TaskOutcome::Completed(value)
                        }
                    }
                    Ok(Err(error)) => {
                        if scope_generation.load(Ordering::Acquire) != generation {
                            TaskOutcome::Stale
                        } else if context.is_cancelled() {
                            TaskOutcome::Cancelled
                        } else {
                            TaskOutcome::Failed(error.to_string())
                        }
                    }
                    Err(_) => {
                        if scope_generation.load(Ordering::Acquire) != generation {
                            TaskOutcome::Stale
                        } else if context.is_cancelled() {
                            TaskOutcome::Cancelled
                        } else {
                            TaskOutcome::Failed("task panicked".to_string())
                        }
                    }
                }
            };
            if let Some(sender) = worker_sender.lock().unwrap().take() {
                let _ = sender.send(outcome);
            }
        }));
        let timeout_sender = sender.clone();
        let timeout_cancellation = cancellation.clone();
        self.pool.schedule_timeout(
            timeout,
            Box::new(move || {
                if let Some(sender) = timeout_sender.lock().unwrap().take() {
                    let _ = sender.send(TaskOutcome::TimedOut);
                    timeout_cancellation.cancel();
                }
            }),
        );
        TaskHandle {
            cancellation,
            receiver,
            sender,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn outcome<T>(handle: &mut TaskHandle<T>) -> TaskOutcome<T> {
        for _ in 0..200 {
            match handle.receiver.try_recv() {
                Ok(Some(outcome)) => return outcome,
                Ok(None) => std::thread::sleep(Duration::from_millis(5)),
                Err(_) => panic!("task sender dropped"),
            }
        }
        panic!("task did not complete")
    }

    #[test]
    fn completes_successful_work() {
        let scope = TaskRunner::new().scope();
        let mut handle = scope.spawn(Duration::from_secs(1), |_| Ok(42));
        assert!(matches!(outcome(&mut handle), TaskOutcome::Completed(42)));
    }

    #[test]
    fn reports_failed_work() {
        let scope = TaskRunner::new().scope();
        let mut handle = scope.spawn(Duration::from_secs(1), |_| {
            Err::<i32, _>(anyhow::anyhow!("failed"))
        });
        assert!(matches!(outcome(&mut handle), TaskOutcome::Failed(_)));
    }

    #[test]
    fn cancels_active_work() {
        let scope = TaskRunner::new().scope();
        let mut handle = scope.spawn(Duration::from_secs(1), |_| {
            std::thread::sleep(Duration::from_millis(10));
            Ok(42)
        });
        handle.cancel();
        assert!(matches!(outcome(&mut handle), TaskOutcome::Cancelled));
    }

    #[test]
    fn reports_timeout_without_waiting_for_blocking_work() {
        let scope = TaskRunner::new().scope();
        let started = Instant::now();
        let mut handle = scope.spawn(Duration::from_millis(1), |_| {
            std::thread::sleep(Duration::from_millis(100));
            Ok(42)
        });
        assert!(matches!(outcome(&mut handle), TaskOutcome::TimedOut));
        assert!(started.elapsed() < Duration::from_millis(50));
    }

    #[test]
    fn superseded_errors_are_reported_as_stale() {
        let scope = TaskRunner::new().scope();
        let (started_tx, started_rx) = std::sync::mpsc::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let mut first = scope.spawn(Duration::from_secs(1), move |_| {
            started_tx.send(()).ok();
            release_rx.recv().ok();
            Err::<i32, _>(anyhow::anyhow!("cancelled"))
        });
        started_rx.recv().ok();
        let mut second = scope.spawn(Duration::from_secs(1), |_| Ok(2));
        release_tx.send(()).ok();
        assert!(matches!(outcome(&mut first), TaskOutcome::Stale));
        assert!(matches!(outcome(&mut second), TaskOutcome::Completed(2)));
    }

    #[test]
    fn supersedes_previous_work() {
        let scope = TaskRunner::new().scope();
        let first = scope.spawn(Duration::from_secs(1), |_| Ok(1));
        let mut second = scope.spawn(Duration::from_secs(1), |_| Ok(2));
        assert!(matches!(outcome(&mut second), TaskOutcome::Completed(2)));
        drop(first);
    }
}
