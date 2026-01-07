use std::collections::HashMap;

pub fn get_game_map() -> HashMap<(String, String), String> {
    let mut map = HashMap::new();
    map.insert(
        ("OS".to_string(), "nap".to_string()),
        "U5hbdsT9W7".to_string(),
    );
    map.insert(
        ("CN".to_string(), "nap".to_string()),
        "x6znKlJ0xK".to_string(),
    );
    map.insert(
        ("OS".to_string(), "hkrpg".to_string()),
        "4ziysqXOQ8".to_string(),
    );
    map.insert(
        ("CN".to_string(), "hkrpg".to_string()),
        "64kMb5iAWu".to_string(),
    );
    map.insert(
        ("OS".to_string(), "hk4e".to_string()),
        "gopR6Cufr3".to_string(),
    );
    map.insert(
        ("CN".to_string(), "hk4e".to_string()),
        "1Z8W5NHUQb".to_string(),
    );
    map.insert(
        ("OS".to_string(), "bh3".to_string()),
        "5TIVvvcwtM".to_string(),
    );
    map.insert(
        ("CN".to_string(), "bh3".to_string()),
        "osvnlOc0S8".to_string(),
    );
    map
}

pub fn get_sophon_map() -> HashMap<String, HashMap<String, String>> {
    let mut map = HashMap::new();

    let mut os = HashMap::new();
    os.insert(
        "apiBase".to_string(),
        "https://sg-hyp-api.hoyoverse.com/hyp/hyp-connect/api/getGameBranches".to_string(),
    );
    os.insert(
        "sophonBase".to_string(),
        "https://sg-public-api.hoyoverse.com/downloader/sophon_chunk/api/getBuild".to_string(),
    );
    os.insert("launcherId".to_string(), "VYTpXlbWo8".to_string());
    os.insert("platApp".to_string(), "ddxf6vlr1reo".to_string());
    map.insert("OS".to_string(), os);

    let mut cn = HashMap::new();
    cn.insert(
        "apiBase".to_string(),
        "https://hyp-api.mihoyo.com/hyp/hyp-connect/api/getGameBranches".to_string(),
    );
    cn.insert(
        "sophonBase".to_string(),
        "https://api-takumi.mihoyo.com/downloader/sophon_chunk/api/getBuild".to_string(),
    );
    cn.insert("launcherId".to_string(), "jGHBHlcOq1".to_string());
    cn.insert("platApp".to_string(), "ddxf5qt290cg".to_string());
    map.insert("CN".to_string(), cn);

    map
}

pub fn format_size(bytes: u64) -> String {
    if bytes >= 1 << 30 {
        format!("{:.2} GB", bytes as f64 / (1 << 30) as f64)
    } else if bytes >= 1 << 20 {
        format!("{:.2} MB", bytes as f64 / (1 << 20) as f64)
    } else if bytes >= 1 << 10 {
        format!("{:.2} KB", bytes as f64 / (1 << 10) as f64)
    } else {
        format!("{} B", bytes)
    }
}

pub fn get_md5(data: &[u8]) -> String {
    let digest = md5::compute(data);
    format!("{:x}", digest)
}
