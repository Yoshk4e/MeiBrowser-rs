//! Selecting the save path and initiating the download process: starting,
//! monitoring real-time progress by way of a 60Hz clock that’s independent
//! of network chunk timings, stopping, and “Download More” / quit functions
//! on the completion page.

use crate::gui::state::{AppData, Shared};
use crate::gui::util::{goto_page, set_ui};
use crate::gui::{AppState, Backend, MainWindow};
use crate::pause::PauseState;
use crate::{Downloader, format_size};
use mei_proto::SophonManifestAssetProperty;
use slint::{ComponentHandle, SharedString};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

pub(crate) fn register(ui: &MainWindow, state: Shared) {
    register_save_path(ui, state.clone());
    register_start_download(ui, state.clone());
    register_toggle_pause(ui, state.clone());
    register_cancel(ui, state.clone());
    register_download_more(ui, state);
    register_quit(ui);
}

fn register_save_path(ui: &MainWindow, state: Shared) {
    {
        let ui_weak = ui.as_weak();
        let state = state.clone();
        ui.global::<Backend>().on_browse_save_path(move || {
            let ui_weak = ui_weak.clone();
            let state = state.clone();
            tokio::spawn(async move {
                let picked = tokio::task::spawn_blocking(|| rfd::FileDialog::new().pick_folder())
                    .await
                    .ok()
                    .flatten();
                if let Some(path) = picked {
                    let path_str = path.display().to_string();
                    state.lock().unwrap().save_path = path_str.clone();
                    set_ui(&ui_weak, move |ui| {
                        ui.global::<AppState>().set_save_path(path_str.into());
                    });
                }
            });
        });
    }
    {
        let ui_weak = ui.as_weak();
        ui.global::<Backend>()
            .on_set_save_path(move |text: SharedString| {
                let t = text.to_string();
                state.lock().unwrap().save_path = t.clone();
                if let Some(ui) = ui_weak.upgrade() {
                    ui.global::<AppState>().set_save_path(t.into());
                }
            });
    }
}

fn register_start_download(ui: &MainWindow, state: Shared) {
    let ui_weak = ui.as_weak();
    ui.global::<Backend>().on_start_download(move || {
        let ui_weak = ui_weak.clone();
        let state = state.clone();

        let ui_save_path = ui_weak
            .upgrade()
            .map(|ui| ui.global::<AppState>().get_save_path().to_string());

        tokio::spawn(async move {
            let (assets, download_url, save_path) = {
                let mut d = state.lock().unwrap();
                if let Some(p) = ui_save_path {
                    if !p.is_empty() {
                        d.save_path = p;
                    }
                }
                let assets: Vec<SophonManifestAssetProperty> = d
                    .selected_paths
                    .iter()
                    .filter_map(|p| d.asset_lookup.get(p).cloned())
                    .collect();
                (assets, d.download_url.clone(), d.save_path.clone())
            };

            if assets.is_empty() {
                return;
            }

            let total_size: i64 = assets.iter().map(|a| a.asset_size).sum();

            set_ui(&ui_weak, move |ui| {
                let s = ui.global::<AppState>();
                s.set_is_downloading(true);
                s.set_is_paused(false);
                s.set_progress(0.0);
                s.set_downloaded_text("0 B".into());
                s.set_total_text(format_size(total_size as u64).into());
                s.set_speed_text("".into());
                goto_page(ui, 5);
            });

            let downloader = Downloader::new();
            let start_time = Instant::now();
            let downloaded_counter = Arc::new(AtomicU64::new(0));
            let pause_state = PauseState::new();
            state.lock().unwrap().pause_state = Some(pause_state.clone());

            let cb_counter = downloaded_counter.clone();
            let dl_pause = pause_state.clone();
            let handle = tokio::spawn(async move {
                downloader
                    .download_files(
                        assets,
                        &download_url,
                        &save_path,
                        Some(Box::new(move |downloaded: u64| {
                            cb_counter.store(downloaded, Ordering::Relaxed);
                        })),
                        dl_pause,
                    )
                    .await
            });

            let ticker_counter = downloaded_counter.clone();
            let ticker_ui = ui_weak.clone();
            let ticker_handle = tokio::spawn(async move {
                let mut interval = tokio::time::interval(std::time::Duration::from_millis(16));
                loop {
                    interval.tick().await;

                    let downloaded = ticker_counter.load(Ordering::Relaxed);
                    let elapsed = start_time.elapsed().as_secs_f64().max(0.001);
                    let speed = downloaded as f64 / elapsed;
                    let progress = if total_size > 0 {
                        (downloaded as f64 / total_size as f64).min(1.0)
                    } else {
                        0.0
                    };
                    let downloaded_text = format_size(downloaded);
                    let speed_text = format!("{}/s", format_size(speed as u64));

                    set_ui(&ticker_ui, move |ui| {
                        let s = ui.global::<AppState>();
                        s.set_progress(progress as f32);
                        s.set_downloaded_text(downloaded_text.into());
                        s.set_speed_text(speed_text.into());
                    });
                }
            });

            state.lock().unwrap().download_handle = Some(handle.abort_handle());

            let result = handle.await;
            ticker_handle.abort();

            match result {
                Ok(Ok(())) => {
                    set_ui(&ui_weak, |ui| {
                        let s = ui.global::<AppState>();
                        s.set_progress(1.0);
                        s.set_is_downloading(false);
                        s.set_is_paused(false);
                        s.set_download_complete(true);
                        goto_page(ui, 6);
                    });
                }
                Ok(Err(e)) => {
                    let cancelled = pause_state.is_cancelled();
                    let msg = if cancelled {
                        "Download cancelled. Files downloaded so far were kept — start the same download again to resume.".to_string()
                    } else {
                        e.to_string()
                    };
                    set_ui(&ui_weak, move |ui| {
                        let s = ui.global::<AppState>();
                        s.set_is_downloading(false);
                        s.set_is_paused(false);
                        s.set_error_text(msg.into());
                        goto_page(ui, 4);
                    });
                }
                Err(_) => {
                    // aborted via cancel_download
                    set_ui(&ui_weak, |ui| {
                        let s = ui.global::<AppState>();
                        s.set_is_downloading(false);
                        s.set_is_paused(false);
                        s.set_status_text("Download cancelled.".into());
                        goto_page(ui, 4);
                    });
                }
            }

            let mut d = state.lock().unwrap();
            d.download_handle = None;
            d.pause_state = None;
        });
    });
}

fn register_toggle_pause(ui: &MainWindow, state: Shared) {
    let ui_weak = ui.as_weak();
    ui.global::<Backend>().on_toggle_pause_download(move || {
        let now_paused = {
            let d = state.lock().unwrap();
            d.pause_state.as_ref().map(|p| p.toggle())
        };
        if let Some(now_paused) = now_paused {
            set_ui(&ui_weak, move |ui| {
                ui.global::<AppState>().set_is_paused(now_paused);
            });
        }
    });
}

fn register_cancel(ui: &MainWindow, state: Shared) {
    ui.global::<Backend>().on_cancel_download(move || {
        let mut d = state.lock().unwrap();
        if let Some(p) = d.pause_state.as_ref() {
            // Wakes the task if it's currently parked on a pause so the
            // abort below doesn't need to wait on anything.
            p.cancel();
        }
        if let Some(h) = d.download_handle.take() {
            h.abort();
        }
    });
}

fn register_download_more(ui: &MainWindow, state: Shared) {
    let ui_weak = ui.as_weak();
    ui.global::<Backend>().on_download_more(move || {
        if let Some(ui) = ui_weak.upgrade() {
            *state.lock().unwrap() = AppData::default();
            let s = ui.global::<AppState>();
            s.set_error_text("".into());
            s.set_status_text("".into());
            s.set_download_complete(false);
            goto_page(&ui, 0);
        }
    });
}

fn register_quit(ui: &MainWindow) {
    ui.global::<Backend>().on_quit(move || {
        std::process::exit(0);
    });
}
