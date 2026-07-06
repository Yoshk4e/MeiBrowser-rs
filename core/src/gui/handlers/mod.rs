//! All `Backend` callbacks, categorized by page/domain. The individual modules provide
//! one `register(ui, state)` method each, which connects their portion of the callbacks;
//! `register_all` is the sole function that `main` uses.

mod config;
mod download;
mod files;
mod navigation;
mod packages;

use super::MainWindow;
use super::state::Shared;

pub(crate) fn register_all(ui: &MainWindow, state: Shared) {
    navigation::register(ui, state.clone());
    config::register(ui, state.clone());
    packages::register(ui, state.clone());
    files::register(ui, state.clone());
    download::register(ui, state);
}
