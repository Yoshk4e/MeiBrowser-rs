//! Package selection (which category to download, diff toggle) and
//! `confirm_package`, which fetches the manifest and builds the file tree.

use crate::gui::state::{GAMES, Shared};
use crate::gui::util::{goto_page, refresh_file_listing, set_ui};
use crate::gui::{AppState, Backend, MainWindow};
use crate::{DispatchClient, FileTree, SophonClient};
use anyhow::{Context, Result};
use mei_proto::SophonManifestAssetProperty;
use slint::{ComponentHandle, SharedString};
use std::collections::HashMap;

pub(crate) fn register(ui: &MainWindow, state: Shared) {
    register_selection(ui, state.clone());
    register_confirm(ui, state);
}

fn register_selection(ui: &MainWindow, state: Shared) {
    {
        let ui_weak = ui.as_weak();
        let state = state.clone();
        ui.global::<Backend>().on_select_package(move |idx: i32| {
            state.lock().unwrap().selected_package = Some(idx as usize);
            if let Some(ui) = ui_weak.upgrade() {
                ui.global::<AppState>().set_selected_package(idx);
            }
        });
    }
    {
        let ui_weak = ui.as_weak();
        let state = state.clone();
        ui.global::<Backend>().on_toggle_diff(move |v: bool| {
            state.lock().unwrap().diff_enabled = v;
            if let Some(ui) = ui_weak.upgrade() {
                ui.global::<AppState>().set_diff_enabled(v);
            }
        });
    }
    {
        let ui_weak = ui.as_weak();
        ui.global::<Backend>()
            .on_set_previous_version(move |text: SharedString| {
                let t = text.to_string();
                state.lock().unwrap().previous_version = t.clone();
                if let Some(ui) = ui_weak.upgrade() {
                    ui.global::<AppState>().set_previous_version(t.into());
                }
            });
    }
}

// confirm_package fetches the manifest (with optional differential
// filtering for sophon mode) and builds the file tree for browsing.
fn register_confirm(ui: &MainWindow, state: Shared) {
    let ui_weak = ui.as_weak();
    ui.global::<Backend>().on_confirm_package(move || {
        let ui_weak = ui_weak.clone();
        let state = state.clone();

        tokio::spawn(async move {
            set_ui(&ui_weak, |ui| {
                let s = ui.global::<AppState>();
                s.set_is_loading(true);
                s.set_loading_text("Fetching file manifest...".into());
                s.set_error_text("".into());
            });

            let (
                mode,
                game,
                region,
                version,
                selected_package,
                category_ids,
                diff_enabled,
                previous_version,
                stoken_json,
                dispatch_version,
            ) = {
                let d = state.lock().unwrap();
                (
                    d.mode.clone(),
                    d.selected_game.map(|i| GAMES[i].id.to_string()),
                    d.region.clone(),
                    d.version_input.clone(),
                    d.selected_package,
                    d.packages
                        .iter()
                        .map(|p| p.category_id.clone())
                        .collect::<Vec<_>>(),
                    d.diff_enabled,
                    d.previous_version.clone(),
                    d.stoken_json.clone(),
                    d.dispatch_version.clone(),
                )
            };

            let result: Result<(Vec<SophonManifestAssetProperty>, String)> = async {
                let pkg_idx = selected_package.context("No package selected")?;
                let category_id = category_ids
                    .get(pkg_idx)
                    .cloned()
                    .context("Invalid package")?;

                match mode.as_str() {
                    "sophon" => {
                        let game = game.context("No game selected")?;
                        let client = SophonClient::new();
                        let (manifest, url) = client
                            .get_manifest(&game, &version, &region, &category_id, None)
                            .await?;

                        let mut assets = manifest.assets.clone();

                        if diff_enabled && !previous_version.trim().is_empty() {
                            let (prev_manifest, _) = client
                                .get_manifest(
                                    &game,
                                    previous_version.trim(),
                                    &region,
                                    &category_id,
                                    None,
                                )
                                .await?;
                            let old_map: HashMap<&str, &SophonManifestAssetProperty> =
                                prev_manifest
                                    .assets
                                    .iter()
                                    .map(|a| (a.asset_name.as_str(), a))
                                    .collect();
                            assets = manifest
                                .assets
                                .into_iter()
                                .filter(|a| match old_map.get(a.asset_name.as_str()) {
                                    Some(old) => old.asset_hash_md5 != a.asset_hash_md5,
                                    None => true,
                                })
                                .collect();
                        }

                        Ok((assets, url))
                    }
                    "legacy" => {
                        let game = game.context("No game selected")?;
                        let version = dispatch_version.unwrap_or(version);
                        let client = DispatchClient::new();
                        let (manifest, url) = client
                            .get_files(&game, &version, &category_id.to_lowercase())
                            .await?;
                        Ok((manifest.assets, url))
                    }
                    "stoken" => {
                        let json = stoken_json.context("No build data loaded")?;
                        let client = SophonClient::new();
                        let (manifest, url) = client
                            .get_manifest_from_build_data(&json, &category_id)
                            .await?;
                        Ok((manifest.assets, url))
                    }
                    other => anyhow::bail!("Unknown mode: {other}"),
                }
            }
            .await;

            match result {
                Ok((assets, url)) if assets.is_empty() => {
                    let _ = url;
                    set_ui(&ui_weak, |ui| {
                        let s = ui.global::<AppState>();
                        s.set_is_loading(false);
                        s.set_error_text(
                            "No files need to be downloaded, everything is up to date!".into(),
                        );
                    });
                }
                Ok((assets, url)) => {
                    let tree = FileTree::from_assets(assets.clone());
                    let lookup: HashMap<String, SophonManifestAssetProperty> = assets
                        .iter()
                        .map(|a| (a.asset_name.clone(), a.clone()))
                        .collect();

                    {
                        let mut d = state.lock().unwrap();
                        d.manifest_assets = assets;
                        d.download_url = url;
                        d.file_tree = Some(tree);
                        d.asset_lookup = lookup;
                        d.current_path.clear();
                        d.selected_paths.clear();
                        d.selected_size = 0;
                    }

                    let state2 = state.clone();
                    set_ui(&ui_weak, move |ui| {
                        ui.global::<AppState>().set_is_loading(false);
                        let d = state2.lock().unwrap();
                        refresh_file_listing(ui, &d);
                        goto_page(ui, 3);
                    });
                }
                Err(e) => {
                    let msg = e.to_string();
                    set_ui(&ui_weak, move |ui| {
                        let s = ui.global::<AppState>();
                        s.set_is_loading(false);
                        s.set_error_text(msg.into());
                    });
                }
            }
        });
    });
}
