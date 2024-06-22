use std::{
    sync::{Arc, Condvar, Mutex},
    thread::JoinHandle,
};

/// Monitors if any threads are returning error.
/// It won't catch panic.
pub struct ThreadManager {
    threads: Vec<JoinHandle<()>>,
    catcher: Arc<ErrorCatcher>,
}

struct ErrorCatcher {
    any_error: Mutex<bool>,
    error_signal: Condvar,
}

impl Default for ThreadManager {
    fn default() -> Self {
        Self::new()
    }
}

impl ThreadManager {
    pub fn new() -> Self {
        ThreadManager {
            threads: vec![],
            catcher: Arc::new(ErrorCatcher {
                any_error: Mutex::new(false),
                error_signal: Condvar::new(),
            }),
        }
    }

    pub fn spawn(
        &mut self,
        f: impl FnOnce() -> anyhow::Result<()> + Send + 'static,
    ) -> std::io::Result<()> {
        self.actual_spawn(None, f)
    }

    pub fn spawn_named(
        &mut self,
        name: &'static str,
        f: impl FnOnce() -> anyhow::Result<()> + Send + 'static,
    ) -> std::io::Result<()> {
        self.actual_spawn(Some(name), f)
    }

    fn actual_spawn(
        &mut self,
        name: Option<&'static str>,
        f: impl FnOnce() -> anyhow::Result<()> + Send + 'static,
    ) -> std::io::Result<()> {
        let builder = std::thread::Builder::new();
        let builder = if let Some(thread_name) = name {
            builder.name(thread_name.into())
        } else {
            builder
        };

        let inner_catcher = Arc::clone(&self.catcher);

        let handle = builder.spawn(move || {
            match f() {
                Ok(_) => {}
                Err(e) => {
                    let this_thread;
                    let display_name = match name {
                        Some(x) => x,
                        None => {
                            this_thread = std::thread::current();
                            this_thread.name().unwrap_or("<unnamed>")
                        }
                    };

                    log::error!("Monitored thread {display_name} terminated with an error:\n{e:?}");
                    if let Ok(mut guard) = inner_catcher.any_error.lock() {
                        *guard = true;
                    }
                }
            }

            // signal thread termination as well
            inner_catcher.error_signal.notify_all();
        })?;

        self.threads.push(handle);
        Ok(())
    }

    /// Returns true if any thread has returned error.
    /// May panic if any thread has panicked.
    pub fn join_all(self) -> bool {
        for thread in self.threads {
            thread.join().expect("thread has panicked");
        }

        // lock fails if the mutex is poisoned (thread has panicked). consider that as an error
        self.catcher.any_error.lock().map(|x| *x).unwrap_or(true)
    }

    pub fn has_error(&self) -> bool {
        self.catcher.any_error.lock().map(|x| *x).unwrap_or(true)
    }

    pub fn wait_error(&self) {
        if self.threads.is_empty() {
            return;
        }

        let mut any_error = match self.catcher.any_error.lock() {
            Ok(x) => x,
            Err(_) => return,
        };

        while !*any_error {
            any_error = match self.catcher.error_signal.wait(any_error) {
                Ok(lock) => lock,
                Err(_) => return,
            };

            // check if all threads has terminated
            let all_finished = self.threads.iter().all(|x| x.is_finished());
            if all_finished {
                return;
            }
        }
    }
}
