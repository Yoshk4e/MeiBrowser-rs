//! Pause/resume/cancel control for in-progress downloads.
//!
//! `PauseState` is a small, cheaply-cloneable handle that can be shared
//! between the task actually performing a download (CLI or GUI) and
//! whatever is driving user input (a keypress, a button click, ...).
//!
//! Pausing does **not** stop the underlying task or throw away any
//! progress: the downloader simply parks itself at the next safe
//! checkpoint (between files, and between chunks within a file for
//! Sophon's chunked assets) until `resume()` is called. Because
//! `try_download_file` already skips chunks/files that are present on
//! disk with a matching hash, a paused-then-resumed (or even a fully
//! restarted) download picks up right where it left off instead of
//! starting over.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::Notify;

#[derive(Clone)]
pub struct PauseState {
    paused: Arc<AtomicBool>,
    cancelled: Arc<AtomicBool>,
    notify: Arc<Notify>,
}

impl PauseState {
    pub fn new() -> Self {
        Self {
            paused: Arc::new(AtomicBool::new(false)),
            cancelled: Arc::new(AtomicBool::new(false)),
            notify: Arc::new(Notify::new()),
        }
    }

    /// Pause at the next checkpoint. Already-downloaded bytes are kept.
    pub fn pause(&self) {
        self.paused.store(true, Ordering::SeqCst);
    }

    /// Resume a paused download, waking any checkpoint currently parked.
    pub fn resume(&self) {
        self.paused.store(false, Ordering::SeqCst);
        self.notify.notify_waiters();
    }

    /// Flip paused <-> resumed. Returns the new paused state.
    pub fn toggle(&self) -> bool {
        if self.paused.load(Ordering::SeqCst) {
            self.resume();
            false
        } else {
            self.pause();
            true
        }
    }

    pub fn is_paused(&self) -> bool {
        self.paused.load(Ordering::SeqCst)
    }

    /// Request cancellation. Any checkpoint (including one currently
    /// parked waiting for a resume) will bail out with an error.
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
        self.notify.notify_waiters();
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }

    /// Park here for as long as the state is paused. Returns
    /// immediately (no cost beyond an atomic load) when not paused.
    pub async fn wait_if_paused(&self) {
        loop {
            if self.cancelled.load(Ordering::SeqCst) || !self.paused.load(Ordering::SeqCst) {
                return;
            }

            // Register interest *before* re-checking the flag so a
            // resume()/cancel() that races with us can't be missed
            // between the check above and the await below.
            let notified = self.notify.notified();
            if self.cancelled.load(Ordering::SeqCst) || !self.paused.load(Ordering::SeqCst) {
                return;
            }
            notified.await;
        }
    }
}

impl Default for PauseState {
    fn default() -> Self {
        Self::new()
    }
}
