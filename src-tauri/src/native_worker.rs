//! CPU/file work must not run synchronously in the IPC handler or on an async executor worker.
//! The operation's existing errors pass through; a failed worker is an explicit typed boundary.

pub(super) async fn run<T: Send + 'static>(
    operation: &'static str,
    work: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(work)
        .await
        .map_err(|error| format!("{operation}WorkerFailed: {error}"))?
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        future::Future,
        sync::mpsc,
        task::{Context, Poll, Waker},
        time::Duration,
    };

    #[test]
    fn blocking_native_job_yields_before_work_finishes_and_does_not_hold_async_tasks() {
        let caller = std::thread::current().id();
        let (entered_tx, entered_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let (stop_tx, stop_rx) = mpsc::channel();
        let watchdog_release = release_tx.clone();
        // A regression executing inline fails a Pending assertion rather than hanging the suite.
        let watchdog = std::thread::spawn(move || {
            if stop_rx.recv_timeout(Duration::from_secs(5)).is_err() {
                let _ = watchdog_release.send(());
            }
        });
        let future = run("ControlledNative", move || {
            entered_tx.send(std::thread::current().id()).unwrap();
            release_rx.recv().unwrap();
            Ok(17)
        });
        let mut future = std::pin::pin!(future);
        let mut context = Context::from_waker(Waker::noop());
        assert!(matches!(future.as_mut().poll(&mut context), Poll::Pending));
        assert_ne!(
            entered_rx.recv_timeout(Duration::from_secs(5)).unwrap(),
            caller
        );
        let unrelated = tauri::async_runtime::spawn(async { 23 });
        assert_eq!(tauri::async_runtime::block_on(unrelated).unwrap(), 23);
        release_tx.send(()).unwrap();
        assert_eq!(tauri::async_runtime::block_on(future).unwrap(), 17);
        stop_tx.send(()).unwrap();
        watchdog.join().unwrap();
    }

    #[test]
    fn native_worker_preserves_operation_errors_and_contains_worker_panic() {
        let operation_error = "NativeColorSampleInvalid: invalid rectangle".to_owned();
        let expected = operation_error.clone();
        assert_eq!(
            tauri::async_runtime::block_on(run::<()>("Sample", move || Err(operation_error))),
            Err(expected)
        );
        let failure = tauri::async_runtime::block_on(run::<()>("Sample", || {
            panic!("simulated worker failure")
        }))
        .unwrap_err();
        assert!(failure.starts_with("SampleWorkerFailed:"));
        assert_eq!(
            tauri::async_runtime::block_on(run("Sample", || Ok(42))).unwrap(),
            42
        );
    }
}
