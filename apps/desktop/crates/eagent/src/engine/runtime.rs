//! #PERF-12: the engine's tokio runtime, ONE per process.
//!
//! Every thread stack is an mmap, and every mmap takes the process's mmap
//! write lock — the lock a host's main thread queues behind on its own page
//! faults. A runtime per run (2 workers + a blocking pool) and tokio's
//! default pool (512 threads, each torn down after 10 s idle) made a busy
//! host create and destroy threads all the time. So the pool here is
//! BOUNDED and its threads are KEPT: a burst of `spawn_blocking` (a file
//! read, a title poll, a report) reuses the threads the last burst left.

use std::sync::OnceLock;
use std::time::Duration;

/// The runtime's worker threads, shared by every run.
pub const WORKER_THREADS: usize = 2;

/// The blocking pool's cap. Past it a `spawn_blocking` queues instead of
/// starting a thread: far above what N hosted sessions run at once (a few
/// short reads and reports each), well below tokio's 512.
pub const MAX_BLOCKING_THREADS: usize = 64;

/// How long an idle blocking thread is kept (tokio's default is 10 s, which
/// is exactly the create/destroy churn #PERF-12 measured).
pub const BLOCKING_KEEP_ALIVE: Duration = Duration::from_secs(10 * 60);

/// The bounded pool and long keep-alive on `builder` — the steer runtime
/// (the hosts' shared one) is built with the same.
pub fn configure(builder: &mut tokio::runtime::Builder) -> &mut tokio::runtime::Builder {
    builder
        .max_blocking_threads(MAX_BLOCKING_THREADS)
        .thread_keep_alive(BLOCKING_KEEP_ALIVE)
}

/// The process's engine runtime, for a run whose caller brought none
/// ([`crate::local::RunRequest::runtime`] `None`). Built on first use and
/// never dropped. `None` only when the OS refused its threads.
pub fn shared() -> Option<&'static tokio::runtime::Runtime> {
    static RUNTIME: OnceLock<Option<tokio::runtime::Runtime>> = OnceLock::new();
    RUNTIME
        .get_or_init(|| {
            let built = configure(
                tokio::runtime::Builder::new_multi_thread()
                    .worker_threads(WORKER_THREADS)
                    .thread_name("engine-rt"),
            )
            .enable_all()
            .build();
            match built {
                Ok(runtime) => Some(runtime),
                Err(err) => {
                    log::error!("engine: the shared runtime failed to start: {err}");
                    None
                }
            }
        })
        .as_ref()
}
