use std::{future::Future, time::Duration};
use tokio::{sync::watch, time::Instant};

/// One deadline shared by HTTP draining and background workers.
#[derive(Clone)]
pub struct Shutdown {
    deadline: watch::Sender<Option<Instant>>,
    grace: Duration,
    release_margin: Duration,
}

impl Shutdown {
    pub fn new(grace: Duration, release_margin: Duration) -> Self {
        let (deadline, _) = watch::channel(None);
        Self {
            deadline,
            grace,
            release_margin,
        }
    }

    pub fn request(&self) {
        self.deadline.send_if_modified(|value| {
            if value.is_some() {
                return false;
            }
            *value = Some(Instant::now() + self.grace);
            true
        });
    }

    pub fn requested(&self) -> bool {
        self.deadline.borrow().is_some()
    }

    pub async fn wait(&self) -> Instant {
        let mut receiver = self.deadline.subscribe();
        loop {
            if let Some(deadline) = *receiver.borrow_and_update() {
                return deadline;
            }
            if receiver.changed().await.is_err() {
                return Instant::now();
            }
        }
    }

    /// Drain active work until the release margin before the shutdown deadline.
    /// Drop unfinished work before returning so database transactions release
    /// their locks before callers try to release a durable claim.
    pub async fn drain<T>(&self, work: impl Future<Output = T>) -> Option<T> {
        let mut work = Box::pin(work);
        tokio::select! {
            result = &mut work => Some(result),
            deadline = self.wait() => {
                tokio::time::timeout_at(deadline - self.release_margin, &mut work).await.ok()
            }
        }
    }

    /// Allow claim finalization through the shared shutdown deadline.
    pub async fn finish<T>(&self, work: impl Future<Output = T>) -> Option<T> {
        let mut work = Box::pin(work);
        tokio::select! {
            result = &mut work => Some(result),
            deadline = self.wait() => tokio::time::timeout_at(deadline, &mut work).await.ok(),
        }
    }

    pub async fn until_requested<T>(&self, work: impl Future<Output = T>) -> Option<T> {
        tokio::select! {
            result = work => Some(result),
            _ = self.wait() => None,
        }
    }

    pub async fn sleep(&self, duration: Duration) -> bool {
        self.until_requested(tokio::time::sleep(duration))
            .await
            .is_some()
    }
}

/// A stopped or panicking worker stops the service; leases preserve its work.
pub async fn supervise(
    server: impl Future<Output = std::io::Result<()>>,
    shutdown: Shutdown,
    mut workers: tokio::task::JoinSet<(&'static str, anyhow::Result<()>)>,
) -> anyhow::Result<()> {
    let mut server = Box::pin(server);
    let (mut result, server_finished) = tokio::select! {
        result = &mut server => (result.map_err(anyhow::Error::from), true),
        _ = shutdown.wait() => (Ok(()), false),
        exited = workers.join_next(), if !workers.is_empty() => {
            if shutdown.requested() {
                (Ok(()), false)
            } else {
                match exited {
                    Some(Ok((name, _))) => {
                        tracing::error!(worker = name, "Background worker exited unexpectedly");
                    }
                    _ => tracing::error!("Background worker failed unexpectedly"),
                }
                (Err(anyhow::anyhow!("Background worker stopped")), false)
            }
        }
    };
    shutdown.request();
    let deadline = shutdown.wait().await;
    if !server_finished {
        let drained = match tokio::time::timeout_at(deadline, &mut server).await {
            Ok(result) => result.map_err(anyhow::Error::from),
            Err(_) => {
                tracing::warn!("HTTP drain deadline reached; unfinished work remains recoverable");
                Ok(())
            }
        };
        if result.is_ok() {
            result = drained;
        }
    }
    while !workers.is_empty() {
        match tokio::time::timeout_at(deadline, workers.join_next()).await {
            Ok(Some(Ok((_, Ok(()))))) => {}
            Ok(Some(_)) => {
                tracing::error!("Background worker failed during shutdown");
                if result.is_ok() {
                    result = Err(anyhow::anyhow!("Background worker failed during shutdown"));
                }
            }
            Ok(None) => break,
            Err(_) => {
                tracing::warn!("Worker drain deadline reached; leases remain recoverable");
                break;
            }
        }
    }
    workers.abort_all();
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    use tokio::{sync::oneshot, task::JoinSet, time::timeout};

    fn shutdown() -> Shutdown {
        Shutdown::new(Duration::from_millis(100), Duration::from_millis(30))
    }

    struct Dropped(Arc<AtomicBool>);

    impl Drop for Dropped {
        fn drop(&mut self) {
            self.0.store(true, Ordering::SeqCst);
        }
    }

    #[tokio::test]
    async fn repeated_requests_keep_the_original_deadline_and_wake_idle_workers() {
        let stop = shutdown();
        stop.request();
        let deadline = stop.wait().await;
        stop.request();
        assert_eq!(stop.wait().await, deadline);
        assert!(
            !timeout(
                Duration::from_secs(2),
                stop.sleep(Duration::from_secs(3600))
            )
            .await
            .unwrap()
        );
        assert_eq!(stop.drain(async { 42 }).await, Some(42));
    }

    #[tokio::test]
    async fn drain_drops_unfinished_work_before_releasing_its_claim() {
        let stop = shutdown();
        let worker_stop = stop.clone();
        let dropped = Arc::new(AtomicBool::new(false));
        let guard = Dropped(dropped.clone());
        let (started, ready) = oneshot::channel();
        let worker = tokio::spawn(async move {
            worker_stop
                .drain(async move {
                    let _guard = guard;
                    started.send(()).unwrap();
                    std::future::pending::<()>().await;
                })
                .await
        });
        ready.await.unwrap();
        stop.request();
        assert!(
            timeout(Duration::from_secs(2), worker)
                .await
                .unwrap()
                .unwrap()
                .is_none()
        );
        assert!(dropped.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn finalization_observes_shutdown_that_arrives_while_it_is_running() {
        let stop = shutdown();
        let worker_stop = stop.clone();
        let (started, ready) = oneshot::channel();
        let worker = tokio::spawn(async move {
            worker_stop
                .finish(async move {
                    started.send(()).unwrap();
                    std::future::pending::<()>().await;
                })
                .await
        });
        ready.await.unwrap();
        stop.request();
        assert!(
            timeout(Duration::from_secs(2), worker)
                .await
                .unwrap()
                .unwrap()
                .is_none()
        );
    }

    #[tokio::test]
    async fn normal_shutdown_drains_http_and_joins_workers_without_an_error() {
        let stop = shutdown();
        let mut workers = JoinSet::new();
        let worker_stop = stop.clone();
        let finished = Arc::new(AtomicBool::new(false));
        let worker_finished = finished.clone();
        workers.spawn(async move {
            worker_stop.wait().await;
            worker_finished.store(true, Ordering::SeqCst);
            ("fixture", Ok(()))
        });
        stop.request();
        tokio::task::yield_now().await;
        let server_stop = stop.clone();
        let drained = Arc::new(AtomicBool::new(false));
        let server_drained = drained.clone();
        supervise(
            async move {
                server_stop.wait().await;
                server_drained.store(true, Ordering::SeqCst);
                Ok(())
            },
            stop,
            workers,
        )
        .await
        .unwrap();
        assert!(finished.load(Ordering::SeqCst));
        assert!(drained.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn unexpected_worker_exit_stops_the_service_and_drains_http() {
        let stop = shutdown();
        let server_stop = stop.clone();
        let drained = Arc::new(AtomicBool::new(false));
        let server_drained = drained.clone();
        let mut workers = JoinSet::new();
        workers.spawn(async { ("fixture", Err(anyhow::anyhow!("fixture failure"))) });
        let result = supervise(
            async move {
                server_stop.wait().await;
                server_drained.store(true, Ordering::SeqCst);
                Ok(())
            },
            stop.clone(),
            workers,
        )
        .await;
        assert!(result.is_err());
        assert!(stop.requested());
        assert!(drained.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn shutdown_deadline_bounds_unresponsive_http_and_workers() {
        let stop = shutdown();
        let mut workers = JoinSet::new();
        let dropped = Arc::new(AtomicBool::new(false));
        let guard = Dropped(dropped.clone());
        let (started, ready) = oneshot::channel();
        workers.spawn(async move {
            let _guard = guard;
            started.send(()).unwrap();
            std::future::pending::<()>().await;
            ("fixture", Ok(()))
        });
        ready.await.unwrap();
        stop.request();
        timeout(
            Duration::from_secs(2),
            supervise(std::future::pending(), stop, workers),
        )
        .await
        .unwrap()
        .unwrap();
        timeout(Duration::from_secs(2), async {
            while !dropped.load(Ordering::SeqCst) {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
    }
}
