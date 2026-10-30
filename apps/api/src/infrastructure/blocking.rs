use std::{num::NonZeroUsize, sync::Arc, time::Duration};
use tokio::sync::Semaphore;

#[derive(Debug, thiserror::Error)]
pub enum BlockingError {
    #[error("Processing capacity is busy")]
    Busy,
    #[error("Processing deadline exceeded")]
    TimedOut,
    #[error("Blocking task failed")]
    Failed,
}

/// Limits submitted blocking tasks, including tasks whose callers have left.
pub struct BlockingPool {
    slots: Arc<Semaphore>,
    timeout: Duration,
}

impl BlockingPool {
    pub fn new(workers: NonZeroUsize, timeout: Duration) -> Self {
        Self {
            slots: Arc::new(Semaphore::new(workers.get())),
            timeout,
        }
    }

    /// Reject saturation before submitting work to Tokio's blocking queue.
    /// Running closures cannot be interrupted; they retain their slot until exit.
    pub async fn run<T: Send + 'static>(
        &self,
        work: impl FnOnce() -> T + Send + 'static,
    ) -> Result<T, BlockingError> {
        let permit = self
            .slots
            .clone()
            .try_acquire_owned()
            .map_err(|_| BlockingError::Busy)?;
        let mut task = tokio::task::spawn_blocking(move || {
            let _permit = permit;
            work()
        });
        match tokio::time::timeout(self.timeout, &mut task).await {
            Ok(Ok(result)) => Ok(result),
            Ok(Err(_)) => Err(BlockingError::Failed),
            Err(_) => {
                // Cancel queued work when possible. Started work still holds its slot.
                task.abort();
                Err(BlockingError::TimedOut)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::oneshot;

    async fn cancellation_keeps_capacity(abort: bool) {
        let pool = Arc::new(BlockingPool::new(
            NonZeroUsize::new(1).unwrap(),
            Duration::from_millis(30),
        ));
        let (started_tx, started_rx) = oneshot::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let caller = tokio::spawn({
            let pool = pool.clone();
            async move {
                pool.run(move || {
                    started_tx.send(()).unwrap();
                    release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
                })
                .await
            }
        });
        started_rx.await.unwrap();
        // This current-thread runtime must remain free while the closure waits.
        tokio::time::timeout(Duration::from_millis(100), tokio::task::yield_now())
            .await
            .unwrap();
        if abort {
            caller.abort();
            assert!(caller.await.unwrap_err().is_cancelled());
        } else {
            assert!(matches!(
                caller.await.unwrap(),
                Err(BlockingError::TimedOut)
            ));
        }
        assert!(matches!(
            pool.run(|| panic!("replacement must not start")).await,
            Err(BlockingError::Busy)
        ));
        release_tx.send(()).unwrap();
        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                if pool.run(|| 42).await.is_ok() {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn timed_out_work_retains_capacity_until_exit() {
        cancellation_keeps_capacity(false).await;
    }

    #[tokio::test]
    async fn aborted_caller_retains_capacity_until_exit() {
        cancellation_keeps_capacity(true).await;
    }

    #[tokio::test]
    async fn panicking_work_releases_capacity() {
        let pool = BlockingPool::new(NonZeroUsize::new(1).unwrap(), Duration::from_secs(2));
        assert!(matches!(
            pool.run(|| panic!("test panic")).await,
            Err(BlockingError::Failed)
        ));
        assert_eq!(pool.run(|| 42).await.unwrap(), 42);
    }
}
