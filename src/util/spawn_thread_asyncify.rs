use std::future::Future;
use tokio::sync::oneshot;

/// Spawns a standard OS thread (using `std::thread`), and returns
/// a future that can be awaited on. Future panics if inner function panics.
pub fn spawn_thread_asyncify<F, T>(f: F) -> impl Future<Output = T> + Send
where
    F: FnOnce() -> T,
    F: Send + 'static,
    T: Send + 'static,
{
    let (tx, rx) = oneshot::channel();

    let handle = std::thread::spawn(move || {
        // Don't care even if future is dropped earlier
        drop(tx.send(f()));
    });

    async move {
        let output = rx.await;

        // handle panic first (I think that would be the case when output is Err)
        handle.join().expect("worker thread has panicked");

        output.expect("worker thread has dropped before producing output")
    }
}
