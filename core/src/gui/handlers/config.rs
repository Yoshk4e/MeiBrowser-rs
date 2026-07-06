use crate::gui::state::{AppData, GAMES, PackageDef, Shared};
use crate::gui::theme::{apply_theme, icon_bytes_for_tag};
use crate::gui::util::{goto_page, set_ui};
use crate::gui::{AppState, Backend, MainWindow, PackageOption};
use crate::{DEFAULT_THEME, DispatchClient, SophonClient, theme_from_icon};
use anyhow::{Context, Result};
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};

pub(crate) fn register(ui: &MainWindow, state: Shared) {
    register_mode_and_game(ui, state.clone());
    register_fetch_versions(ui, state.clone());
    register_stoken_file(ui, state);
}

fn register_mode_and_game(ui: &MainWindow, state: Shared) {
    {
        let ui_weak = ui.as_weak();
        let state = state.clone();
        ui.global::<Backend>()
            .on_select_mode(move |mode: SharedString| {
                let mode_str = mode.to_string();
                {
                    let mut d = state.lock().unwrap();
                    *d = AppData::default();
                    d.mode = mode_str.clone();
                }
                if let Some(ui) = ui_weak.upgrade() {
                    let s = ui.global::<AppState>();
                    s.set_mode(mode_str.into());
                    s.set_selected_game(-1);
                    s.set_version_input("".into());
                    s.set_latest_version("".into());
                    s.set_has_pre_download(false);
                    s.set_stoken_path("".into());
                    s.set_error_text("".into());
                    apply_theme(&ui, DEFAULT_THEME);
                    goto_page(&ui, 1);
                }
            });
    }
    {
        let ui_weak = ui.as_weak();
        let state = state.clone();
        ui.global::<Backend>().on_select_game(move |idx: i32| {
            state.lock().unwrap().selected_game = Some(idx as usize);
            state.lock().unwrap().branches_fetched = false;
            if let Some(ui) = ui_weak.upgrade() {
                ui.global::<AppState>().set_selected_game(idx);
                if let Some(game) = GAMES.get(idx as usize) {
                    let bytes = icon_bytes_for_tag(game.tag);
                    let theme = theme_from_icon(bytes);
                    apply_theme(&ui, theme);
                }
            }
        });
    }
    {
        let ui_weak = ui.as_weak();
        ui.global::<Backend>()
            .on_select_region(move |region: SharedString| {
                let region_str = region.to_string();
                {
                    let mut d = state.lock().unwrap();
                    d.region = region_str.clone();
                    d.branches_fetched = false;
                }
                if let Some(ui) = ui_weak.upgrade() {
                    ui.global::<AppState>().set_region(region_str.into());
                }
            });
    }
}

fn register_fetch_versions(ui: &MainWindow, state: Shared) {
    let ui_weak = ui.as_weak();
    ui.global::<Backend>().on_fetch_versions(move || {
        let ui_weak = ui_weak.clone();
        let state = state.clone();

        // Fetch the UI-bound fields in the UI thread before switching over to tokio
        // (pre-download-selected and version-input both have their values set by
        // .slint, hence AppData does not have the authoritative value).
        let (version_input, pre_download_selected) = ui_weak
            .upgrade()
            .map(|ui| {
                let s = ui.global::<AppState>();
                (
                    s.get_version_input().to_string(),
                    s.get_pre_download_selected(),
                )
            })
            .unwrap_or_default();

        tokio::spawn(async move {
            set_ui(&ui_weak, |ui| {
                let s = ui.global::<AppState>();
                s.set_is_loading(true);
                s.set_loading_text("Fetching...".into());
                s.set_error_text("".into());
                s.set_status_text("".into());
            });

            let (mode, game, region, branches_fetched) = {
                let d = state.lock().unwrap();
                (
                    d.mode.clone(),
                    d.selected_game.map(|i| GAMES[i].id.to_string()),
                    d.region.clone(),
                    d.branches_fetched,
                )
            };

            let outcome: Result<Option<Vec<PackageDef>>> = async {
                if mode == "legacy" {
                    let game = game.context("Select a game first")?;
                    if version_input.trim().is_empty() {
                        anyhow::bail!("Enter a version, e.g. 5.2");
                    }
                    let client = DispatchClient::new();
                    let packages = client.get_packages(&game, version_input.trim()).await?;
                    if packages.is_empty() {
                        anyhow::bail!("No packages available for this version.");
                    }
                    let pkgs: Vec<PackageDef> = packages
                        .iter()
                        .map(|p| {
                            let description = match p.as_str() {
                                "Files" => "Individual game files (scattered)",
                                "ZIP" => "Full game ZIP package",
                                "Update" => "Update ZIP (differential)",
                                _ => p.as_str(),
                            }
                            .to_string();
                            PackageDef {
                                category_id: p.clone(),
                                label: p.clone(),
                                description,
                            }
                        })
                        .collect();
                    {
                        let mut d = state.lock().unwrap();
                        d.dispatch_version = Some(version_input.trim().to_string());
                        d.version_input = version_input.trim().to_string();
                        d.packages = pkgs.clone();
                    }
                    Ok(Some(pkgs))
                } else {
                    // sophon
                    let game = game.context("Select a game first")?;
                    let client = SophonClient::new();
                    let user_wants_specific_version = !version_input.trim().is_empty();

                    if !branches_fetched {
                        let branches = client.get_game_branches(&game, &region).await?;
                        let main_branch = &branches["data"]["game_branches"][0]["main"];
                        let package_id = main_branch["package_id"]
                            .as_str()
                            .context("No package_id in branch data")?
                            .to_string();
                        let password = main_branch["password"]
                            .as_str()
                            .context("No password in branch data")?
                            .to_string();
                        let latest_version = main_branch["tag"]
                            .as_str()
                            .map(|t| t.split('.').take(2).collect::<Vec<_>>().join("."))
                            .unwrap_or_else(|| "unknown".to_string());

                        let mut has_pre = false;
                        let mut pre_password = String::new();
                        if let Some(pd) =
                            branches["data"]["game_branches"][0]["pre_download"].as_object()
                        {
                            if !pd.is_empty() {
                                has_pre = true;
                                pre_password = pd
                                    .get("password")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("")
                                    .to_string();
                            }
                        }

                        {
                            let mut d = state.lock().unwrap();
                            d.package_id = package_id;
                            d.password = password;
                            d.latest_version = latest_version.clone();
                            d.has_pre_download = has_pre;
                            d.pre_download_password = pre_password;
                            d.branches_fetched = true;
                        }

                        let latest_for_ui = latest_version.clone();
                        set_ui(&ui_weak, move |ui| {
                            let s = ui.global::<AppState>();
                            s.set_latest_version(latest_for_ui.into());
                            s.set_has_pre_download(has_pre);
                            if !user_wants_specific_version {
                                s.set_status_text(
                                    "Latest version fetched, press \"Fetch Packages\" again to load packages, or edit the version first.".into(),
                                );
                            }
                        });

                        // Only stop-and-wait when the user hasn't picked a
                        // specific version yet. Otherwise fall through and
                        // resolve the build in the same click.
                        if !user_wants_specific_version {
                            return Ok(None);
                        }
                    }

                    let (package_id, password, pre_password, latest_version) = {
                        let d = state.lock().unwrap();
                        (
                            d.package_id.clone(),
                            d.password.clone(),
                            d.pre_download_password.clone(),
                            d.latest_version.clone(),
                        )
                    };

                    let version = if version_input.trim().is_empty() {
                        latest_version
                    } else {
                        version_input.trim().to_string()
                    };
                    let use_password = if pre_download_selected && !pre_password.is_empty() {
                        &pre_password
                    } else {
                        &password
                    };

                    eprintln!(
                        "[gui] sophon getBuild: game={} region={} version={} pre_download={}",
                        game, region, version, pre_download_selected
                    );

                    let build = client
                        .get_build(
                            &region,
                            &package_id,
                            use_password,
                            &version,
                            pre_download_selected,
                        )
                        .await?;
                    let manifests = build["data"]["manifests"]
                        .as_array()
                        .context("No manifests in build data")?;
                    if manifests.is_empty() {
                        anyhow::bail!("No packages found for this build.");
                    }

                    let pkgs: Vec<PackageDef> = manifests
                        .iter()
                        .map(|m| {
                            let category_name =
                                m["category_name"].as_str().unwrap_or("Unknown").to_string();
                            let matching_field =
                                m["matching_field"].as_str().unwrap_or("").to_string();
                            let category_id = m["category_id"].as_str().unwrap_or("").to_string();
                            let label = if matching_field == "game" {
                                "Game Files".to_string()
                            } else {
                                matching_field
                            };
                            PackageDef {
                                category_id,
                                label,
                                description: category_name,
                            }
                        })
                        .collect();

                    {
                        let mut d = state.lock().unwrap();
                        d.packages = pkgs.clone();
                        d.version_input = version;
                    }
                    Ok(Some(pkgs))
                }
            }
            .await;

            match outcome {
                Ok(Some(pkgs)) => {
                    let package_options: Vec<PackageOption> = pkgs
                        .iter()
                        .map(|p| PackageOption {
                            category_id: p.category_id.clone().into(),
                            label: p.label.clone().into(),
                            description: p.description.clone().into(),
                        })
                        .collect();
                    set_ui(&ui_weak, move |ui| {
                        let s = ui.global::<AppState>();
                        s.set_is_loading(false);
                        s.set_packages(ModelRc::new(VecModel::from(package_options)));
                        s.set_selected_package(-1);
                        goto_page(ui, 2);
                    });
                }
                Ok(None) => {
                    set_ui(&ui_weak, |ui| ui.global::<AppState>().set_is_loading(false));
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

fn register_stoken_file(ui: &MainWindow, state: Shared) {
    {
        let ui_weak = ui.as_weak();
        ui.global::<Backend>().on_browse_stoken_file(move || {
            let ui_weak = ui_weak.clone();
            tokio::spawn(async move {
                let picked = tokio::task::spawn_blocking(|| {
                    rfd::FileDialog::new()
                        .add_filter("JSON", &["json"])
                        .pick_file()
                })
                .await
                .ok()
                .flatten();
                if let Some(path) = picked {
                    let path_str = path.display().to_string();
                    set_ui(&ui_weak, move |ui| {
                        ui.global::<AppState>().set_stoken_path(path_str.into());
                    });
                }
            });
        });
    }
    {
        let ui_weak = ui.as_weak();
        ui.global::<Backend>()
            .on_load_stoken_file(move |path: SharedString| {
                let path = path.to_string();
                let ui_weak = ui_weak.clone();
                let state = state.clone();
                tokio::spawn(async move {
                    set_ui(&ui_weak, |ui| {
                        let s = ui.global::<AppState>();
                        s.set_is_loading(true);
                        s.set_loading_text("Loading build data...".into());
                        s.set_error_text("".into());
                    });

                    let result: Result<Vec<PackageDef>> = async {
                        if !std::path::Path::new(&path).exists() {
                            anyhow::bail!("File not found: {}", path);
                        }
                        let json_data = tokio::fs::read_to_string(&path).await?;
                        let build_data: serde_json::Value = serde_json::from_str(&json_data)?;
                        let version = build_data["data"]["tag"]
                            .as_str()
                            .unwrap_or("unknown")
                            .to_string();
                        let manifests = build_data["data"]["manifests"]
                            .as_array()
                            .context("No manifests in build data")?;
                        if manifests.is_empty() {
                            anyhow::bail!("No packages found in build data.");
                        }

                        let pkgs: Vec<PackageDef> = manifests
                            .iter()
                            .map(|m| {
                                let category_name =
                                    m["category_name"].as_str().unwrap_or("Unknown").to_string();
                                let matching_field =
                                    m["matching_field"].as_str().unwrap_or("").to_string();
                                let category_id =
                                    m["category_id"].as_str().unwrap_or("").to_string();
                                let label = if matching_field == "game" {
                                    "Game Files".to_string()
                                } else {
                                    matching_field
                                };
                                PackageDef {
                                    category_id,
                                    label,
                                    description: category_name,
                                }
                            })
                            .collect();

                        {
                            let mut d = state.lock().unwrap();
                            d.stoken_json = Some(json_data);
                            d.packages = pkgs.clone();
                            d.version_input = version;
                        }

                        Ok(pkgs)
                    }
                    .await;

                    match result {
                        Ok(pkgs) => {
                            let package_options: Vec<PackageOption> = pkgs
                                .iter()
                                .map(|p| PackageOption {
                                    category_id: p.category_id.clone().into(),
                                    label: p.label.clone().into(),
                                    description: p.description.clone().into(),
                                })
                                .collect();
                            set_ui(&ui_weak, move |ui| {
                                let s = ui.global::<AppState>();
                                s.set_is_loading(false);
                                s.set_packages(ModelRc::new(VecModel::from(package_options)));
                                s.set_selected_package(-1);
                                goto_page(ui, 2);
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
}
