//! File browser: folder navigation, selection, and handing the summary off
//! to the download page.

use crate::format_size;
use crate::gui::state::Shared;
use crate::gui::util::{goto_page, refresh_file_listing};
use crate::gui::{AppState, Backend, MainWindow};
use mei_proto::SophonManifestAssetProperty;
use slint::{ComponentHandle, SharedString};

pub(crate) fn register(ui: &MainWindow, state: Shared) {
    {
        let ui_weak = ui.as_weak();
        let state = state.clone();
        ui.global::<Backend>()
            .on_navigate_into(move |path: SharedString| {
                let mut d = state.lock().unwrap();
                d.current_path = path.to_string();
                if let Some(ui) = ui_weak.upgrade() {
                    refresh_file_listing(&ui, &d);
                }
            });
    }
    {
        let ui_weak = ui.as_weak();
        let state = state.clone();
        ui.global::<Backend>().on_navigate_back_folder(move || {
            let mut d = state.lock().unwrap();
            match d.current_path.rfind('/') {
                Some(idx) => d.current_path.truncate(idx),
                None => d.current_path.clear(),
            }
            if let Some(ui) = ui_weak.upgrade() {
                refresh_file_listing(&ui, &d);
            }
        });
    }
    {
        let ui_weak = ui.as_weak();
        let state = state.clone();
        ui.global::<Backend>()
            .on_toggle_select(move |path: SharedString| {
                let path = path.to_string();
                let mut d = state.lock().unwrap();
                if let Some(tree) = &d.file_tree {
                    if let Some(node) = tree.root.find_node(&path) {
                        if node.is_file {
                            let size = node.size;
                            if d.selected_paths.remove(&path) {
                                d.selected_size -= size;
                            } else {
                                d.selected_paths.insert(path);
                                d.selected_size += size;
                            }
                        }
                    }
                }
                if let Some(ui) = ui_weak.upgrade() {
                    refresh_file_listing(&ui, &d);
                }
            });
    }
    {
        let ui_weak = ui.as_weak();
        let state = state.clone();
        ui.global::<Backend>().on_select_all_in_folder(move || {
            let mut d = state.lock().unwrap();
            let current_path = d.current_path.clone();

            let files_to_add: Vec<SophonManifestAssetProperty> = match &d.file_tree {
                Some(tree) => {
                    let node = if current_path.is_empty() {
                        &tree.root
                    } else {
                        tree.root.find_node(&current_path).unwrap_or(&tree.root)
                    };
                    node.collect_all_files()
                }
                None => Vec::new(),
            };

            for asset in files_to_add {
                if d.selected_paths.insert(asset.asset_name.clone()) {
                    d.selected_size += asset.asset_size;
                }
            }

            if let Some(ui) = ui_weak.upgrade() {
                refresh_file_listing(&ui, &d);
            }
        });
    }
    {
        let ui_weak = ui.as_weak();
        ui.global::<Backend>().on_finish_selection(move || {
            let d = state.lock().unwrap();
            let count = d.selected_paths.len();
            let size = d.selected_size;
            if let Some(ui) = ui_weak.upgrade() {
                let s = ui.global::<AppState>();
                s.set_summary_file_count(count.to_string().into());
                s.set_summary_size_text(format_size(size as u64).into());
                goto_page(&ui, 4);
            }
        });
    }
}
