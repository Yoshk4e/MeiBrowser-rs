use super::state::AppData;
use super::{AppState, FileEntry, MainWindow};
use crate::format_size;
use slint::{ComponentHandle, ModelRc, SharedString, VecModel, Weak};

pub(crate) fn set_ui(ui_weak: &Weak<MainWindow>, f: impl FnOnce(&MainWindow) + Send + 'static) {
    let ui_weak = ui_weak.clone();
    slint::invoke_from_event_loop(move || {
        if let Some(ui) = ui_weak.upgrade() {
            f(&ui);
        }
    })
    .ok();
}

/// Change page while recording the previous page, so the UI can animate the
/// transition in the right direction.
pub(crate) fn goto_page(ui: &MainWindow, new_page: i32) {
    let cur = ui.global::<AppState>().get_page();
    ui.global::<AppState>().set_previous_page(cur);
    ui.global::<AppState>().set_page(new_page);
}

pub(crate) fn refresh_file_listing(ui: &MainWindow, data: &AppData) {
    let Some(tree) = &data.file_tree else { return };

    let node = if data.current_path.is_empty() {
        &tree.root
    } else {
        tree.root
            .find_node(&data.current_path)
            .unwrap_or(&tree.root)
    };

    let entries: Vec<FileEntry> = node
        .children
        .iter()
        .map(|child| FileEntry {
            name: child.name.clone().into(),
            path: child.full_path.clone().into(),
            is_file: child.is_file,
            size_text: format_size(child.size as u64).into(),
            count_text: if !child.is_file && child.file_count > 0 {
                format!("{} files", child.file_count).into()
            } else {
                SharedString::from("")
            },
            selected: child.is_file && data.selected_paths.contains(&child.full_path),
        })
        .collect();

    let state = ui.global::<AppState>();
    state.set_file_entries(ModelRc::new(VecModel::from(entries)));
    state.set_current_path_display(if data.current_path.is_empty() {
        "/".into()
    } else {
        format!("/{}", data.current_path).into()
    });
    state.set_can_go_back(!data.current_path.is_empty());
    state.set_selected_file_count(data.selected_paths.len() as i32);
    state.set_selected_size_text(format_size(data.selected_size as u64).into());
}
