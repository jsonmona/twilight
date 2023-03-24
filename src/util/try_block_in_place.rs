use tokio::runtime::{Handle, RuntimeFlavor};
use tokio::task::{block_in_place, spawn_blocking, JoinError};

pub async fn try_block_in_place<F, R>(f: F) -> Result<R, JoinError>
where
    F: FnOnce() -> R + Send + 'static,
    R: Send + 'static,
{
    match Handle::try_current() {
        Ok(handle) => match handle.runtime_flavor() {
            RuntimeFlavor::MultiThread => Ok(block_in_place(f)),
            _ => spawn_blocking(f).await,
        },
        Err(_) => Ok(f()),
    }
}
