//! A small structured-concurrency executor.
//!
//! Zero unsafe (built on the stable `std::task::Wake` trait), zero
//! dependencies, single-threaded by design: one firing owns one
//! executor. The API deliberately echoes the parts of tokio we actually
//! use — spawn, block_on, join sets, cancellation tokens, spawn_blocking
//! — because the shape is proven; the machine is ours.
//!
//! I/O model (the honest version): there is no epoll/kqueue/IOCP reactor
//! yet. Blocking work — a model call, a sandboxed shell command — runs
//! on its own OS thread via [`spawn_blocking`], whose completion wakes
//! the awaiting task. A firing has tens of agents, not tens of
//! thousands, so a thread per in-flight call is the right bridge until
//! server mode needs a reactor; the API will not change when it lands.
//!
//! v0.2 fixes (each pinned by a test below):
//! * cancelled tasks are DROPPED (their destructors run) and reaped;
//! * dropping a [`TaskSet`] cancels whatever it still owns — no orphans;
//! * a panicking task is caught and recorded, not allowed to poison the
//!   executor and take the whole firing (and the UI) down with it;
//! * idle `block_on` sleeps until something wakes it, instead of
//!   polling every millisecond (which drained laptop batteries in tray mode).

use std::collections::{HashMap, VecDeque};
use std::future::Future;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::task::{Context, Poll, Wake, Waker};
use std::time::Duration;

type BoxedFut = Pin<Box<dyn Future<Output = ()> + Send + 'static>>;

/// Lock a mutex, ignoring poison: a panicking task is caught before it
/// can poison these, and if one ever did, the data (queues of ids) is
/// still structurally valid.
fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

/// How a task ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Running,
    Done,
    Cancelled,
    /// The task panicked; the message is kept for the Event River.
    Panicked(String),
}

struct Slot {
    fut: Option<BoxedFut>,
    outcome: Outcome,
    cancel_requested: bool,
}

struct Shared {
    ready: Mutex<VecDeque<u64>>,
    slots: Mutex<HashMap<u64, Slot>>,
    next_id: AtomicU64,
    /// Signalled whenever something becomes ready, so idle loops sleep.
    signal: Condvar,
    signal_lock: Mutex<bool>,
}

impl Shared {
    fn notify(&self) {
        let mut flag = lock(&self.signal_lock);
        *flag = true;
        self.signal.notify_all();
    }

    /// Sleep until notified (or `max` passes, as a safety net).
    fn wait(&self, max: Duration) {
        let mut flag = lock(&self.signal_lock);
        if !*flag {
            flag = self.signal.wait_timeout(flag, max).map(|(g, _)| g).unwrap_or_else(|p| p.into_inner().0);
        }
        *flag = false;
    }
}

/// A task waker: waking re-queues the task and wakes the loop.
struct TaskWaker {
    id: u64,
    shared: Arc<Shared>,
}

impl Wake for TaskWaker {
    fn wake(self: Arc<Self>) {
        Self::wake_by_ref(&self);
    }

    fn wake_by_ref(self: &Arc<Self>) {
        lock(&self.shared.ready).push_back(self.id);
        self.shared.notify();
    }
}

/// The executor. Pass it around by reference.
pub struct Executor {
    shared: Arc<Shared>,
}

impl Executor {
    pub fn new() -> Self {
        Executor {
            shared: Arc::new(Shared {
                ready: Mutex::new(VecDeque::new()),
                slots: Mutex::new(HashMap::new()),
                next_id: AtomicU64::new(1),
                signal: Condvar::new(),
                signal_lock: Mutex::new(false),
            }),
        }
    }

    /// Spawn a task. It runs on the next `tick`/`block_on` pump.
    pub fn spawn<F>(&self, fut: F) -> TaskHandle
    where
        F: Future<Output = ()> + Send + 'static,
    {
        let id = self.shared.next_id.fetch_add(1, Ordering::Relaxed);
        lock(&self.shared.slots)
            .insert(id, Slot { fut: Some(Box::pin(fut)), outcome: Outcome::Running, cancel_requested: false });
        lock(&self.shared.ready).push_back(id);
        self.shared.notify();
        TaskHandle { id, shared: Arc::clone(&self.shared) }
    }

    /// Run a future to completion on this executor, pumping spawned
    /// tasks between polls. This is the entry point of a firing.
    pub fn block_on<F: Future>(&self, fut: F) -> F::Output {
        let mut fut = Box::pin(fut);
        // The main future gets id 0; its waker just signals the loop.
        let waker = Waker::from(Arc::new(TaskWaker { id: 0, shared: Arc::clone(&self.shared) }));
        let mut cx = Context::from_waker(&waker);
        loop {
            if let Poll::Ready(v) = fut.as_mut().poll(&mut cx) {
                return v;
            }
            if !self.tick() {
                // Nothing ready: sleep until a waker fires. The timeout
                // is only a safety net against a lost wake-up.
                self.shared.wait(Duration::from_millis(250));
            }
        }
    }

    /// Run every currently-ready task once. Returns true if any ran.
    pub fn tick(&self) -> bool {
        let mut ran = false;
        loop {
            let next = lock(&self.shared.ready).pop_front();
            match next {
                Some(0) => ran = true, // main future: block_on re-polls it
                Some(id) => {
                    ran = true;
                    self.run_task(id);
                }
                None => return ran,
            }
        }
    }

    fn run_task(&self, id: u64) {
        // Take the future OUT of the slot before polling, so a task that
        // spawns or wakes others never deadlocks on the slots mutex.
        let taken = {
            let mut slots = lock(&self.shared.slots);
            match slots.get_mut(&id) {
                None => None,
                Some(s) if s.outcome != Outcome::Running => None,
                Some(s) if s.cancel_requested => {
                    // Drop the future NOW so its destructors run (files
                    // closed, child processes killed), then reap.
                    let fut = s.fut.take();
                    s.outcome = Outcome::Cancelled;
                    drop(slots);
                    drop(fut);
                    None
                }
                Some(s) => s.fut.take(),
            }
        };
        let Some(mut fut) = taken else { return };

        let waker = Waker::from(Arc::new(TaskWaker { id, shared: Arc::clone(&self.shared) }));
        let mut cx = Context::from_waker(&waker);
        let polled = catch_unwind(AssertUnwindSafe(|| fut.as_mut().poll(&mut cx)));
        let mut slots = lock(&self.shared.slots);
        match polled {
            Ok(Poll::Ready(())) => {
                if let Some(s) = slots.get_mut(&id) {
                    s.outcome = Outcome::Done;
                }
            }
            Ok(Poll::Pending) => {
                if let Some(s) = slots.get_mut(&id) {
                    s.fut = Some(fut);
                }
            }
            Err(payload) => {
                let msg = payload
                    .downcast_ref::<&str>()
                    .map(|s| s.to_string())
                    .or_else(|| payload.downcast_ref::<String>().cloned())
                    .unwrap_or_else(|| "task panicked".into());
                if let Some(s) = slots.get_mut(&id) {
                    s.outcome = Outcome::Panicked(msg);
                }
            }
        }
    }

    /// Number of tasks whose slots are still held (for leak tests).
    pub fn live_slots(&self) -> usize {
        lock(&self.shared.slots).values().filter(|s| s.fut.is_some()).count()
    }
}

impl Default for Executor {
    fn default() -> Self {
        Self::new()
    }
}

/// A handle to a spawned task: cancel it, or ask how it ended.
pub struct TaskHandle {
    id: u64,
    shared: Arc<Shared>,
}

impl TaskHandle {
    /// Request cancellation. The task's future is dropped at the next
    /// pump; anything awaiting a [`CancellationToken`] should be woken
    /// by cancelling that token too.
    pub fn cancel(&self) {
        if let Some(s) = lock(&self.shared.slots).get_mut(&self.id) {
            s.cancel_requested = true;
        }
        lock(&self.shared.ready).push_back(self.id);
        self.shared.notify();
    }

    pub fn outcome(&self) -> Outcome {
        lock(&self.shared.slots).get(&self.id).map(|s| s.outcome.clone()).unwrap_or(Outcome::Done)
    }

    pub fn is_done(&self) -> bool {
        self.outcome() != Outcome::Running
    }
}

/// A join set: spawn many, wait for all. Dropping the set cancels any
/// task still running — structured concurrency means no orphans.
pub struct TaskSet {
    handles: Vec<TaskHandle>,
}

impl TaskSet {
    pub fn new() -> Self {
        TaskSet { handles: Vec::new() }
    }

    pub fn spawn<F>(&mut self, ex: &Executor, fut: F)
    where
        F: Future<Output = ()> + Send + 'static,
    {
        self.handles.push(ex.spawn(fut));
    }

    /// Drive the executor until every task in the set has ended.
    /// Returns each task's outcome, in spawn order.
    pub fn block_until_all_done(&self, ex: &Executor) -> Vec<Outcome> {
        loop {
            if self.handles.iter().all(|h| h.is_done()) {
                return self.handles.iter().map(|h| h.outcome()).collect();
            }
            if !ex.tick() {
                ex.shared.wait(Duration::from_millis(250));
            }
        }
    }

    pub fn cancel_all(&self) {
        for h in &self.handles {
            h.cancel();
        }
    }

    pub fn len(&self) -> usize {
        self.handles.len()
    }

    pub fn is_empty(&self) -> bool {
        self.handles.is_empty()
    }
}

impl Default for TaskSet {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for TaskSet {
    fn drop(&mut self) {
        for h in &self.handles {
            if !h.is_done() {
                h.cancel();
            }
        }
    }
}

// ── blocking bridge ──────────────────────────────────────────────

struct BlockingState<T> {
    result: Option<std::thread::Result<T>>,
    waker: Option<Waker>,
}

/// Run blocking work on its own OS thread; await its result.
///
/// The closure's panics are caught and surface as `Err` with the panic
/// message, so a crashing tool never crashes the firing.
pub fn spawn_blocking<T, F>(f: F) -> impl Future<Output = Result<T, String>>
where
    T: Send + 'static,
    F: FnOnce() -> T + Send + 'static,
{
    let state = Arc::new(Mutex::new(BlockingState::<T> { result: None, waker: None }));
    let s2 = Arc::clone(&state);
    let spawned = std::thread::Builder::new().name("wright-blocking".into()).spawn(move || {
        let r = catch_unwind(AssertUnwindSafe(f));
        let waker = {
            let mut st = lock(&s2);
            st.result = Some(r);
            st.waker.take()
        };
        if let Some(w) = waker {
            w.wake();
        }
    });
    let spawn_error = spawned.err().map(|e| format!("could not start a worker thread: {e}"));
    std::future::poll_fn(move |cx| {
        if let Some(e) = &spawn_error {
            return Poll::Ready(Err(e.clone()));
        }
        let mut st = lock(&state);
        match st.result.take() {
            Some(Ok(v)) => Poll::Ready(Ok(v)),
            Some(Err(payload)) => {
                let msg = payload
                    .downcast_ref::<&str>()
                    .map(|s| s.to_string())
                    .or_else(|| payload.downcast_ref::<String>().cloned())
                    .unwrap_or_else(|| "blocking task panicked".into());
                Poll::Ready(Err(msg))
            }
            None => {
                st.waker = Some(cx.waker().clone());
                Poll::Pending
            }
        }
    })
}

/// Sleep without blocking the executor (a timer thread wakes us).
pub fn sleep(d: Duration) -> impl Future<Output = ()> {
    let fut = spawn_blocking(move || std::thread::sleep(d));
    async move {
        let _ = fut.await;
    }
}

// ── cancellation ─────────────────────────────────────────────────

#[derive(Default)]
struct TokenInner {
    flag: AtomicBool,
    wakers: Mutex<Vec<Waker>>,
}

/// A cancellation token in the CancellationToken spirit: cooperative,
/// cloneable, wakes every waiter on cancel.
#[derive(Clone, Default)]
pub struct CancellationToken {
    inner: Arc<TokenInner>,
}

impl CancellationToken {
    pub fn new() -> Self {
        CancellationToken::default()
    }

    pub fn cancel(&self) {
        self.inner.flag.store(true, Ordering::SeqCst);
        let waiters: Vec<Waker> = lock(&self.inner.wakers).drain(..).collect();
        for w in waiters {
            w.wake();
        }
    }

    pub fn is_cancelled(&self) -> bool {
        self.inner.flag.load(Ordering::SeqCst)
    }

    /// Resolves when the token is cancelled.
    pub async fn cancelled(&self) {
        CancelledFut { token: self.clone() }.await
    }
}

struct CancelledFut {
    token: CancellationToken,
}

impl Future for CancelledFut {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        if self.token.is_cancelled() {
            return Poll::Ready(());
        }
        {
            let mut wakers = lock(&self.token.inner.wakers);
            // Re-polls must not pile up copies of the same waker.
            if !wakers.iter().any(|w| w.will_wake(cx.waker())) {
                wakers.push(cx.waker().clone());
            }
        }
        // Re-check AFTER registering: a cancel that landed between the
        // first check and the push drained the list before our waker was
        // in it, and would otherwise never wake us. (Race found in review.)
        if self.token.is_cancelled() {
            return Poll::Ready(());
        }
        Poll::Pending
    }
}

// CancellationToken is Unpin (Arc inside), so Pin<&mut Self> derefs
// to &mut Self safely without unsafe — asserted by the compiler below
// via the plain field access.
impl Unpin for CancelledFut {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;

    /// Cooperative one-shot yield for tests: wake self once, return
    /// Pending; on the NEXT poll return Ready (yield_now semantics).
    fn yield_once() -> impl Future<Output = ()> {
        let mut yielded = false;
        std::future::poll_fn(move |cx: &mut Context<'_>| {
            if yielded {
                Poll::Ready(())
            } else {
                yielded = true;
                cx.waker().wake_by_ref();
                Poll::Pending
            }
        })
    }

    #[test]
    fn block_on_returns_the_value() {
        let ex = Executor::new();
        assert_eq!(ex.block_on(async { 41 + 1 }), 42);
    }

    #[test]
    fn spawned_tasks_all_run() {
        let ex = Executor::new();
        let hits = Arc::new(AtomicUsize::new(0));
        let mut set = TaskSet::new();
        for _ in 0..3 {
            let hits = Arc::clone(&hits);
            set.spawn(&ex, async move {
                hits.fetch_add(1, Ordering::SeqCst);
            });
        }
        set.block_until_all_done(&ex);
        assert_eq!(hits.load(Ordering::SeqCst), 3);
    }

    #[test]
    fn yield_based_cooperation_works() {
        let ex = Executor::new();
        let order = Arc::new(Mutex::new(Vec::<u8>::new()));
        let mut set = TaskSet::new();

        let o = Arc::clone(&order);
        set.spawn(&ex, async move {
            o.lock().unwrap().push(1);
            yield_once().await;
            o.lock().unwrap().push(3);
        });
        let o = Arc::clone(&order);
        set.spawn(&ex, async move {
            o.lock().unwrap().push(2);
            yield_once().await;
            o.lock().unwrap().push(4);
        });
        set.block_until_all_done(&ex);
        assert_eq!(*order.lock().unwrap(), vec![1, 2, 3, 4]);
    }

    #[test]
    fn cancellation_token_wakes_waiters() {
        let ex = Executor::new();
        let token = CancellationToken::new();
        let done = Arc::new(AtomicBool::new(false));

        // Waiter: parks on the token.
        let (t, d) = (token.clone(), Arc::clone(&done));
        let mut set = TaskSet::new();
        set.spawn(&ex, async move {
            t.cancelled().await;
            d.store(true, Ordering::SeqCst);
        });

        // Canceller: yields a few times, then pulls the trigger.
        let t = token.clone();
        set.spawn(&ex, async move {
            for _ in 0..3 {
                yield_once().await;
            }
            t.cancel();
        });

        set.block_until_all_done(&ex);
        assert!(done.load(Ordering::SeqCst), "waiter must observe cancellation");
    }

    #[test]
    fn repeated_polls_do_not_duplicate_wakers() {
        let token = CancellationToken::new();
        let mut fut = Box::pin(token.cancelled());
        let waker = Waker::from(Arc::new(TaskWaker { id: 0, shared: Arc::new(Shared {
            ready: Mutex::new(VecDeque::new()),
            slots: Mutex::new(HashMap::new()),
            next_id: AtomicU64::new(1),
            signal: Condvar::new(),
            signal_lock: Mutex::new(false),
        }) }));
        let mut cx = Context::from_waker(&waker);
        for _ in 0..5 {
            assert!(fut.as_mut().poll(&mut cx).is_pending());
        }
        assert_eq!(lock(&token.inner.wakers).len(), 1);
        token.cancel();
        assert!(fut.as_mut().poll(&mut cx).is_ready());
    }

    #[test]
    fn task_handle_cancel_marks_done() {
        let ex = Executor::new();
        let token = CancellationToken::new();
        let handle = ex.spawn({
            let t = token.clone();
            async move {
                // Loop bounded: a cancelled task ends even without the
                // token (test cannot hang), but the token ends it early.
                for _ in 0..100_000 {
                    if t.is_cancelled() {
                        break;
                    }
                    yield_once().await;
                }
            }
        });
        handle.cancel();
        let set = TaskSet::new();
        set.block_until_all_done(&ex);
        ex.tick();
        assert!(handle.is_done());
        assert_eq!(handle.outcome(), Outcome::Cancelled);
    }

    /// Increments a counter when dropped — proves destructors ran.
    struct DropFlag(Arc<AtomicUsize>);
    impl Drop for DropFlag {
        fn drop(&mut self) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }

    #[test]
    fn cancelled_futures_are_dropped_and_reaped() {
        let ex = Executor::new();
        let drops = Arc::new(AtomicUsize::new(0));
        let flag = DropFlag(Arc::clone(&drops));
        let handle = ex.spawn(async move {
            let _keep = flag;
            std::future::pending::<()>().await;
        });
        ex.tick(); // first poll: parks forever
        assert_eq!(ex.live_slots(), 1);
        handle.cancel();
        ex.tick();
        assert_eq!(drops.load(Ordering::SeqCst), 1, "destructor ran");
        assert_eq!(ex.live_slots(), 0, "slot reaped");
    }

    #[test]
    fn dropping_a_task_set_cancels_its_tasks() {
        let ex = Executor::new();
        let drops = Arc::new(AtomicUsize::new(0));
        {
            let mut set = TaskSet::new();
            let flag = DropFlag(Arc::clone(&drops));
            set.spawn(&ex, async move {
                let _keep = flag;
                std::future::pending::<()>().await;
            });
            ex.tick();
        } // set dropped here
        ex.tick();
        assert_eq!(drops.load(Ordering::SeqCst), 1, "no orphans");
    }

    #[test]
    fn a_panicking_task_does_not_take_the_executor_down() {
        let ex = Executor::new();
        let mut set = TaskSet::new();
        set.spawn(&ex, async { panic!("agent exploded") });
        let ok = Arc::new(AtomicBool::new(false));
        let o = Arc::clone(&ok);
        set.spawn(&ex, async move { o.store(true, Ordering::SeqCst) });
        let outcomes = set.block_until_all_done(&ex);
        assert_eq!(outcomes[0], Outcome::Panicked("agent exploded".into()));
        assert_eq!(outcomes[1], Outcome::Done);
        assert!(ok.load(Ordering::SeqCst));
    }

    #[test]
    fn spawn_blocking_wakes_the_awaiting_task() {
        let ex = Executor::new();
        let v = ex.block_on(async {
            spawn_blocking(|| {
                std::thread::sleep(Duration::from_millis(20));
                7
            })
            .await
        });
        assert_eq!(v, Ok(7));
        let e = ex.block_on(async { spawn_blocking(|| -> u8 { panic!("tool crashed") }).await });
        assert_eq!(e, Err("tool crashed".into()));
    }

    #[test]
    fn parallel_blocking_work_overlaps() {
        let ex = Executor::new();
        let started = std::time::Instant::now();
        let mut set = TaskSet::new();
        for _ in 0..4 {
            set.spawn(&ex, async {
                let _ = spawn_blocking(|| std::thread::sleep(Duration::from_millis(100))).await;
            });
        }
        set.block_until_all_done(&ex);
        assert!(started.elapsed() < Duration::from_millis(350), "four 100ms jobs ran concurrently");
    }
}
