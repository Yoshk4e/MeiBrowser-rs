use crate::FileTree;
use crate::pause::PauseState;
use mei_proto::SophonManifestAssetProperty;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

pub(crate) struct GameDef {
    pub(crate) id: &'static str,
    pub(crate) name: &'static str,
    pub(crate) tag: &'static str,
}

pub(crate) const GAMES: &[GameDef] = &[
    GameDef {
        id: "hk4e",
        name: "Genshin Impact",
        tag: "GI",
    },
    GameDef {
        id: "hkrpg",
        name: "Honkai: Star Rail",
        tag: "HSR",
    },
    GameDef {
        id: "nap",
        name: "Zenless Zone Zero",
        tag: "ZZZ",
    },
];

#[derive(Clone)]
pub(crate) struct PackageDef {
    pub(crate) category_id: String,
    pub(crate) label: String,
    pub(crate) description: String,
}

pub(crate) struct AppData {
    pub(crate) mode: String, // "sophon" | "legacy" | "stoken"
    pub(crate) selected_game: Option<usize>,
    pub(crate) region: String,

    // sophon branch/build bookkeeping
    pub(crate) branches_fetched: bool,
    pub(crate) version_input: String, // resolved version used for manifest calls
    pub(crate) latest_version: String,
    pub(crate) package_id: String,
    pub(crate) password: String,
    pub(crate) has_pre_download: bool,
    pub(crate) pre_download_password: String,

    // legacy mode
    pub(crate) dispatch_version: Option<String>,

    // stoken mode
    pub(crate) stoken_json: Option<String>,

    // package selection
    pub(crate) packages: Vec<PackageDef>,
    pub(crate) selected_package: Option<usize>,
    pub(crate) diff_enabled: bool,
    pub(crate) previous_version: String,

    // file browser
    pub(crate) manifest_assets: Vec<SophonManifestAssetProperty>,
    pub(crate) asset_lookup: HashMap<String, SophonManifestAssetProperty>,
    pub(crate) download_url: String,
    pub(crate) file_tree: Option<FileTree>,
    pub(crate) current_path: String, // "" == root
    pub(crate) selected_paths: HashSet<String>,
    pub(crate) selected_size: i64,

    // summary / download
    pub(crate) save_path: String,
    pub(crate) download_handle: Option<tokio::task::AbortHandle>,
    pub(crate) pause_state: Option<PauseState>,
}

impl Default for AppData {
    fn default() -> Self {
        Self {
            mode: "sophon".to_string(),
            selected_game: None,
            region: "OS".to_string(),
            branches_fetched: false,
            version_input: String::new(),
            latest_version: String::new(),
            package_id: String::new(),
            password: String::new(),
            has_pre_download: false,
            pre_download_password: String::new(),
            dispatch_version: None,
            stoken_json: None,
            packages: Vec::new(),
            selected_package: None,
            diff_enabled: false,
            previous_version: String::new(),
            manifest_assets: Vec::new(),
            asset_lookup: HashMap::new(),
            download_url: String::new(),
            file_tree: None,
            current_path: String::new(),
            selected_paths: HashSet::new(),
            selected_size: 0,
            save_path: "./download".to_string(),
            download_handle: None,
            pause_state: None,
        }
    }
}

pub(crate) type Shared = Arc<Mutex<AppData>>;
