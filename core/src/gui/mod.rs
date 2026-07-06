//! the frontend

slint::include_modules!();

mod handlers;
mod state;
mod theme;
mod util;

use anyhow::Result;
use slint::{ModelRc, VecModel};
use state::{AppData, GAMES, GameDef, Shared};
use std::sync::{Arc, Mutex};

pub async fn run() -> Result<()> {
    let ui = MainWindow::new()?;
    let state: Shared = Arc::new(Mutex::new(AppData::default()));

    let game_options: Vec<GameOption> = GAMES
        .iter()
        .map(|g: &GameDef| GameOption {
            id: g.id.into(),
            name: g.name.into(),
            tag: g.tag.into(),
        })
        .collect();
    ui.global::<AppState>()
        .set_games(ModelRc::new(VecModel::from(game_options)));

    handlers::register_all(&ui, state);

    ui.run()?;
    Ok(())
}
