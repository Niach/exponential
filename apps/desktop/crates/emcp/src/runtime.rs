//! emcp's ONE tokio runtime per process. Every host of this crate is
//! blocking code (gpui executors, the CLI daemon's threads), so the async
//! SDK runs here and the hosts see blocking calls.

use std::future::Future;
use std::sync::OnceLock;

pub(crate) fn runtime() -> &'static tokio::runtime::Runtime {
    static RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    RUNTIME.get_or_init(|| {
        // reqwest 0.13 without a bundled provider builds its TLS from the
        // process default: ring, unless the host installed one already.
        let _ = rustls::crypto::ring::default_provider().install_default();
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .thread_name("emcp")
            .enable_all()
            .build()
            .expect("the emcp runtime starts")
    })
}

/// Run `future` on emcp's runtime and wait for it from blocking code. It is
/// spawned rather than `block_on`'d, so this works from ANY thread: a tool
/// provider on the blocking pool calling a client, or a host that runs its
/// own tokio runtime (`block_on` would panic inside one).
pub(crate) fn block_on<F>(future: F) -> F::Output
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    let (tx, rx) = std::sync::mpsc::sync_channel(1);
    runtime().spawn(async move {
        let _ = tx.send(future.await);
    });
    rx.recv().expect("an emcp task panicked")
}
