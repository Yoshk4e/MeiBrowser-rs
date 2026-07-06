//! Home / back navigation.

use crate::DEFAULT_THEME;
use crate::gui::state::{AppData, Shared};
use crate::gui::theme::apply_theme;
use crate::gui::util::goto_page;
use crate::gui::{AppState, Backend, MainWindow};
use slint::ComponentHandle;

pub(crate) fn register(ui: &MainWindow, state: Shared) {
    {
        let ui_weak = ui.as_weak();
        let state = state.clone();
        ui.global::<Backend>().on_go_home(move || {
            if let Some(ui) = ui_weak.upgrade() {
                *state.lock().unwrap() = AppData::default();
                ui.global::<AppState>().set_error_text("".into());
                ui.global::<AppState>().set_status_text("".into());
                apply_theme(&ui, DEFAULT_THEME);
                goto_page(&ui, 0);
            }
        });
    }
    {
        let ui_weak = ui.as_weak();
        ui.global::<Backend>().on_go_back(move || {
            if let Some(ui) = ui_weak.upgrade() {
                let cur = ui.global::<AppState>().get_page();
                let new_page = (cur - 1).max(0);
                if new_page == 0 {
                    *state.lock().unwrap() = AppData::default();
                    ui.global::<AppState>().set_error_text("".into());
                    ui.global::<AppState>().set_status_text("".into());
                    apply_theme(&ui, DEFAULT_THEME);
                }
                goto_page(&ui, new_page);
            }
        });
    }
}
