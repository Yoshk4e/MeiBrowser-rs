use anyhow::Result;
use mei_browser::{DispatchClient, Downloader, FileTree, SophonClient, format_size};
use mei_proto::*;
use std::fs;
use std::io::{self, Write};
use std::path::Path;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<()> {
    loop {
        println!("\nMain Menu:");
        println!("1. Sophon Mode");
        println!("2. Scattered Files Mode (legacy)");
        println!("3. Load SToken Build");
        println!("4. Exit");
        print!("\nEnter choice (1-4): ");
        io::stdout().flush()?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;

        match input.trim() {
            "1" => run_sophon_mode().await?,
            "2" => run_dispatch_mode().await?,
            "3" => run_stoken_mode().await?,
            "4" => {
                println!("\nThanks for using MeiBrowser!");
                break;
            }
            _ => println!("Invalid choice, please try again."),
        }
    }

    Ok(())
}

async fn run_stoken_mode() -> Result<()> {
    println!("\nSToken Build Mode\n");
    println!("This mode allows you to load custom build data from a JSON file.\n");

    print!("Enter path to SToken JSON file: ");

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let file_path = input.trim();

    if !Path::new(file_path).exists() {
        println!("File not found: {}", file_path);
        return Ok(());
    }

    let json_data = fs::read_to_string(file_path)?;
    let build_data: serde_json::Value = serde_json::from_str(&json_data)?;

    let version = build_data["data"]["tag"]
        .as_str()
        .unwrap_or("unknown")
        .to_string();

    println!("Loaded build data for version: {}", version);

    let manifests = build_data["data"]["manifests"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("No manifests in build data"))?;

    if manifests.is_empty() {
        println!("No packages found in build data.");
        return Ok(());
    }

    println!("\nAvailable packages:");
    for (i, manifest) in manifests.iter().enumerate() {
        let category_name = manifest["category_name"].as_str().unwrap_or("Unknown");
        let matching_field = manifest["matching_field"].as_str().unwrap_or("");

        match matching_field {
            "game" => println!("{}. Game Files - {}", i + 1, category_name),
            _ => println!("{}. {} - {}", i + 1, matching_field, category_name),
        }
    }

    print!("Select package (1-{}): ", manifests.len());

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let pkg_idx: usize = input.trim().parse().unwrap_or(1);

    if pkg_idx == 0 || pkg_idx > manifests.len() {
        println!("Invalid selection.");
        return Ok(());
    }

    let category_id = manifests[pkg_idx - 1]["category_id"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("No category_id"))?;

    let client = SophonClient::new();
    let (manifest, download_url) = client
        .get_manifest_from_build_data(&json_data, category_id)
        .await?;

    proceed_with_download(manifest, download_url, &version, None, None).await
}

async fn run_sophon_mode() -> Result<()> {
    let client = SophonClient::new();

    println!("\nSophon Mode (Incremental Updates)\n");
    println!("Downloads only changed files or full game archives.\n");

    let games = vec![
        ("hk4e", "Genshin Impact"),
        ("hkrpg", "Honkai: Star Rail"),
        ("nap", "Zenless Zone Zero"),
    ];

    println!("Select game:");
    for (i, (_, name)) in games.iter().enumerate() {
        println!("{}. {}", i + 1, name);
    }
    print!("Enter choice (1-{}): ", games.len());
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let game_idx: usize = input.trim().parse().unwrap_or(1);
    let game = games
        .get(game_idx.saturating_sub(1))
        .map(|(id, _)| *id)
        .unwrap_or("hk4e");

    println!("\nSelect region:");
    println!("1. Global (OS)");
    println!("2. China (CN)");
    print!("Enter choice (1-2): ");
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let region = match input.trim() {
        "2" => "CN",
        _ => "OS",
    };

    println!("\nFetching available versions...");
    let branches = client.get_game_branches(game, region).await?;

    let main_branch = &branches["data"]["game_branches"][0]["main"];
    let package_id = main_branch["package_id"].as_str().unwrap();
    let password = main_branch["password"].as_str().unwrap();

    let latest_version = if let Some(tag) = main_branch["tag"].as_str() {
        tag.split('.').take(2).collect::<Vec<&str>>().join(".")
    } else {
        "unknown".to_string()
    };

    println!("Latest version: {}", latest_version);

    let mut has_pre_download = false;
    let mut pre_download_password = String::new();
    if let Some(pre_download) = branches["data"]["game_branches"][0]["pre_download"].as_object() {
        if !pre_download.is_empty() {
            has_pre_download = true;
            if let Some(pre_tag) = pre_download["tag"].as_str() {
                let pre_version = pre_tag.split('.').take(2).collect::<Vec<&str>>().join(".");
                println!("Pre-download available: {}", pre_version);
                pre_download_password = pre_download["password"].as_str().unwrap_or("").to_string();
            }
        }
    }

    print!(
        "Enter version (or press Enter for latest {}): ",
        latest_version
    );

    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let mut version = input.trim().to_string();

    if version.is_empty() {
        version = latest_version.clone();
    }

    let mut is_pre_download = false;
    if has_pre_download {
        print!("Download pre-download version? (y/N): ");
        io::stdout().flush()?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        if input.trim().eq_ignore_ascii_case("y") {
            is_pre_download = true;
        }
    }

    println!("\nFetching available packages...");
    let use_password = if is_pre_download && !pre_download_password.is_empty() {
        &pre_download_password
    } else {
        password
    };

    let build = client
        .get_build(region, package_id, use_password, &version, is_pre_download)
        .await?;

    let manifests = build["data"]["manifests"].as_array().unwrap();

    println!("\nAvailable packages:");
    for (i, manifest) in manifests.iter().enumerate() {
        let category_name = manifest["category_name"].as_str().unwrap_or("Unknown");
        let matching_field = manifest["matching_field"].as_str().unwrap_or("");

        match matching_field {
            "game" => println!("{}. Game Files - {}", i + 1, category_name),
            _ => println!("{}. {} - {}", i + 1, matching_field, category_name),
        }
    }

    print!("Select package (1-{}): ", manifests.len());
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let pkg_idx: usize = input.trim().parse().unwrap_or(1);
    let category_id = manifests[pkg_idx.saturating_sub(1).min(manifests.len() - 1)]["category_id"]
        .as_str()
        .unwrap();

    let mut previous_version: Option<String> = None;
    println!("\nDifferential Update Mode");
    println!("Calculate what changed between versions.");
    print!("Enable diff comparison? (y/N): ");
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    if input.trim().eq_ignore_ascii_case("y") {
        print!("Enter previous version (e.g., 5.4): ");
        io::stdout().flush()?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        let prev = input.trim().to_string();
        if !prev.is_empty() {
            previous_version = Some(prev);
        }
    }

    println!("\nAnalyzing files...");
    let (manifest, download_url) = client
        .get_manifest(game, &version, region, category_id, None)
        .await?;

    proceed_with_download(
        manifest,
        download_url,
        &version,
        previous_version.as_deref(),
        Some((game, region, category_id)),
    )
    .await
}

async fn proceed_with_download(
    manifest: SophonManifestProto,
    download_url: String,
    version: &str,
    previous_version: Option<&str>,
    game_context: Option<(&str, &str, &str)>,
) -> Result<()> {
    let mut final_manifest = manifest.clone();

    if let Some(prev_ver) = previous_version {
        if let Some((game, region, category_id)) = game_context {
            println!("Calculating differential update from {}...", prev_ver);

            let client = SophonClient::new();
            let (prev_manifest, _) = client
                .get_manifest(game, prev_ver, region, category_id, None)
                .await?;

            let old_assets: std::collections::HashMap<_, _> = prev_manifest
                .assets
                .iter()
                .map(|a| (a.asset_name.clone(), a))
                .collect();
            let new_assets: std::collections::HashMap<_, _> = manifest
                .assets
                .iter()
                .map(|a| (a.asset_name.clone(), a))
                .collect();

            let mut deleted_files = Vec::new();
            let mut new_chunks = Vec::new();
            let mut changed_chunks = Vec::new();

            for old_asset in &prev_manifest.assets {
                if !new_assets.contains_key(&old_asset.asset_name) {
                    deleted_files.push(old_asset);
                }
            }

            for new_asset in &manifest.assets {
                if let Some(old_asset) = old_assets.get(&new_asset.asset_name) {
                    let min_chunks =
                        std::cmp::min(old_asset.asset_chunks.len(), new_asset.asset_chunks.len());

                    for i in 0..min_chunks {
                        let old_chunk = &old_asset.asset_chunks[i];
                        let new_chunk = &new_asset.asset_chunks[i];

                        if old_chunk.chunk_decompressed_hash_md5
                            != new_chunk.chunk_decompressed_hash_md5
                        {
                            changed_chunks.push((new_asset.asset_name.clone(), new_chunk.clone()));
                        }
                    }

                    if new_asset.asset_chunks.len() > old_asset.asset_chunks.len() {
                        for i in old_asset.asset_chunks.len()..new_asset.asset_chunks.len() {
                            new_chunks.push((
                                new_asset.asset_name.clone(),
                                new_asset.asset_chunks[i].clone(),
                            ));
                        }
                    }
                } else {
                    for chunk in &new_asset.asset_chunks {
                        new_chunks.push((new_asset.asset_name.clone(), chunk.clone()));
                    }
                }
            }

            let delete_size = deleted_files.iter().map(|a| a.asset_size).sum::<i64>();

            let new_size_compressed = new_chunks.iter().map(|(_, c)| c.chunk_size).sum::<i64>();
            let new_size_decompressed = new_chunks
                .iter()
                .map(|(_, c)| c.chunk_size_decompressed)
                .sum::<i64>();

            let changed_size_compressed = changed_chunks
                .iter()
                .map(|(_, c)| c.chunk_size)
                .sum::<i64>();
            let changed_size_decompressed = changed_chunks
                .iter()
                .map(|(_, c)| c.chunk_size_decompressed)
                .sum::<i64>();

            let total_download = new_size_compressed + changed_size_compressed;
            let total_decompressed = new_size_decompressed + changed_size_decompressed;

            println!("Diff Statistics:");
            println!("Version: {} -> {}", prev_ver, version);
            println!(
                "Deleted: {} files ({})",
                deleted_files.len(),
                format_size(delete_size as u64)
            );
            println!(
                "New: {} chunks ({})",
                new_chunks.len(),
                format_size(new_size_compressed as u64)
            );
            println!(
                "Changed: {} chunks ({})",
                changed_chunks.len(),
                format_size(changed_size_compressed as u64)
            );
            println!(
                "TOTAL: {} to download, {} decompressed",
                format_size(total_download as u64),
                format_size(total_decompressed as u64)
            );

            println!("\nNote: Chunk-level diff is for statistics only.");
            println!("You will still download complete files that have changes.");
            print!("Proceed to download? (y/N): ");
            io::stdout().flush()?;

            let mut input = String::new();
            io::stdin().read_line(&mut input)?;
            if !input.trim().eq_ignore_ascii_case("y") {
                println!("Download cancelled.");
                return Ok(());
            }

            let mut asset_chunks_map: std::collections::HashMap<
                String,
                Vec<SophonManifestAssetChunk>,
            > = std::collections::HashMap::new();

            for (asset_name, chunk) in new_chunks {
                asset_chunks_map.entry(asset_name).or_default().push(chunk);
            }
            for (asset_name, chunk) in changed_chunks {
                asset_chunks_map.entry(asset_name).or_default().push(chunk);
            }

            let mut final_assets = Vec::new();
            let mut processed_assets = std::collections::HashSet::new();
            let changed_asset_names: std::collections::HashSet<_> =
                asset_chunks_map.keys().cloned().collect();

            for asset_name in changed_asset_names {
                if processed_assets.insert(asset_name.clone()) {
                    if let Some(original_asset) =
                        manifest.assets.iter().find(|a| a.asset_name == asset_name)
                    {
                        final_assets.push(original_asset.clone());
                    }
                }
            }

            final_manifest = SophonManifestProto {
                assets: final_assets,
            };
        }
    }

    if final_manifest.assets.is_empty() {
        println!("\nNo files need to be downloaded. Everything is up to date!");
        return Ok(());
    }

    let total_size: i64 = final_manifest.assets.iter().map(|a| a.asset_size).sum();
    println!("\nFiles to download: {}", final_manifest.assets.len());
    println!("Total download size: {}", format_size(total_size as u64));

    println!("\nBuilding file tree...");
    let tree = FileTree::from_assets(final_manifest.assets.clone());

    println!("\nFile tree structure (top-level folders):");
    for child in &tree.root.children {
        let icon = if child.is_file { "📄" } else { "📁" };
        let size_str = format_size(child.size as u64);
        let count_str = if !child.is_file && child.file_count > 0 {
            format!(" ({} files)", child.file_count)
        } else {
            String::new()
        };
        println!("  {} {} - {}{}", icon, child.name, size_str, count_str);
    }

    println!("\nDownload options:");
    println!("1. Download all files");
    println!("2. Browse and select specific folders/files");
    print!("Enter choice (1-2): ");
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;

    let selected_assets = if input.trim() == "2" {
        select_files_interactive(&tree)?
    } else {
        final_manifest.assets.clone()
    };

    if selected_assets.is_empty() {
        println!("No files selected for download.");
        return Ok(());
    }

    let download_size: i64 = selected_assets.iter().map(|a| a.asset_size).sum();

    print!("\nEnter save path (or press Enter for current directory): ");
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let save_path = if input.trim().is_empty() {
        ".".to_string()
    } else {
        input.trim().to_string()
    };

    println!("\nDownload Summary:");
    println!("Files: {}", selected_assets.len());
    println!("Size: {}", format_size(download_size as u64));
    println!("Save to: {}", save_path);

    print!("Start download? (y/N): ");
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    if !input.trim().eq_ignore_ascii_case("y") {
        println!("Download cancelled.");
        return Ok(());
    }

    println!("\nStarting download...\n");
    let downloader = Downloader::new();

    let total = Arc::new(std::sync::Mutex::new(0u64));
    let total_size_u64 = download_size as u64;

    downloader
        .download_files(
            selected_assets,
            &download_url,
            &save_path,
            Some(Box::new(move |downloaded| {
                let mut t = total.lock().unwrap();
                *t = downloaded;
                let percent = (downloaded as f64 / total_size_u64 as f64) * 100.0;
                print!(
                    "\rProgress: [{:>5.1}%] {} / {}   ",
                    percent,
                    format_size(downloaded),
                    format_size(total_size_u64)
                );
                io::stdout().flush().ok();
            })),
        )
        .await?;

    println!("\n\nDownload complete!");
    println!("Files saved to: {}", save_path);

    Ok(())
}

fn select_files_interactive(
    tree: &FileTree,
) -> Result<Vec<mei_proto::SophonManifestAssetProperty>> {
    let mut selected_assets = Vec::new();
    let mut current_path: Vec<String> = vec!["root".to_string()];

    loop {
        let path_display = if current_path.len() == 1 {
            "/".to_string()
        } else {
            current_path[1..].join("/")
        };
        println!("\nCurrent path: {}", path_display);

        let current_node = if current_path.len() == 1 {
            &tree.root
        } else {
            let path = current_path[1..].join("/");
            tree.root.find_node(&path).unwrap()
        };

        println!("\nContents:");
        if current_path.len() > 1 {
            println!("0. Go back");
        }

        for (i, child) in current_node.children.iter().enumerate() {
            let icon = if child.is_file { "📄" } else { "📁" };
            let size_str = format_size(child.size as u64);
            let count_str = if !child.is_file && child.file_count > 0 {
                format!(" ({} files)", child.file_count)
            } else {
                String::new()
            };
            println!(
                "{}. {} {} - {}{}",
                i + 1,
                icon,
                child.name,
                size_str,
                count_str
            );
        }

        println!("\nActions:");
        println!("• Enter number to navigate/select");
        println!("• Type 'all' to select all in current folder");
        println!("• Type 'done' to finish selection");
        print!("Choice: ");
        io::stdout().flush()?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        let choice = input.trim();

        match choice {
            "done" => break,
            "all" => {
                selected_assets.extend(current_node.collect_all_files());
                println!(
                    "Selected all files in current folder ({} files)",
                    current_node.file_count
                );
            }
            "0" if current_path.len() > 1 => {
                current_path.pop();
            }
            num => {
                if let Ok(idx) = num.parse::<usize>() {
                    if idx > 0 && idx <= current_node.children.len() {
                        let selected = &current_node.children[idx - 1];
                        if selected.is_file {
                            if let Some(asset) = &selected.asset {
                                selected_assets.push(asset.clone());
                                println!(
                                    "Selected file: {} ({})",
                                    selected.name,
                                    format_size(selected.size as u64)
                                );
                            }
                        } else {
                            current_path.push(selected.name.clone());
                        }
                    }
                }
            }
        }
    }

    selected_assets.sort_by(|a, b| a.asset_name.cmp(&b.asset_name));
    selected_assets.dedup_by(|a, b| a.asset_name == b.asset_name);

    println!(
        "\nSelection complete: {} unique files selected",
        selected_assets.len()
    );

    Ok(selected_assets)
}

async fn run_dispatch_mode() -> Result<()> {
    let client = DispatchClient::new();

    println!("\nScattered Files Mode (Legacy)\n");

    let games = vec![
        ("hk4e", "Genshin Impact"),
        ("hkrpg", "Honkai: Star Rail"),
        ("nap", "Zenless Zone Zero"),
    ];

    println!("Select game:");
    for (i, (_, name)) in games.iter().enumerate() {
        println!("{}. {}", i + 1, name);
    }
    print!("\nEnter choice (1-{}): ", games.len());
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let game_idx: usize = input.trim().parse().unwrap_or(1);
    let game = games
        .get(game_idx.saturating_sub(1))
        .map(|(id, _)| *id)
        .unwrap_or("hk4e");

    println!("\nFetching available versions...");
    let versions = client.get_dispatch_versions(game).await?;

    if versions.is_empty() {
        println!("No versions available for {}", game);
        return Ok(());
    }

    println!("\nAvailable versions ({} total):", versions.len());
    for (i, v) in versions.iter().take(10).enumerate() {
        println!("{}. {}", i + 1, v);
    }
    if versions.len() > 10 {
        println!("... and {} more", versions.len() - 10);
    }

    print!(
        "Enter version number (1-{}) or version string: ",
        versions.len()
    );
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let version = if let Ok(idx) = input.trim().parse::<usize>() {
        versions
            .get(idx.saturating_sub(1))
            .cloned()
            .unwrap_or_else(|| versions[0].clone())
    } else {
        input.trim().to_string()
    };

    println!("Selected version: {}", version);

    println!("\nFetching available packages...");
    let packages = client.get_packages(game, &version).await?;

    if packages.is_empty() {
        println!("No packages available for version {}", version);
        return Ok(());
    }

    println!("\nAvailable download options:");
    for (i, pkg) in packages.iter().enumerate() {
        let description = match pkg.as_str() {
            "Files" => "Individual game files (scattered)",
            "ZIP" => "Full game ZIP package",
            "Update" => "Update ZIP (differential)",
            _ => pkg,
        };
        println!("{}. {}", i + 1, description);
    }

    print!("Select package (1-{}): ", packages.len());
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let pkg_idx: usize = input.trim().parse().unwrap_or(1);
    let package = packages[pkg_idx.saturating_sub(1).min(packages.len() - 1)].to_lowercase();

    println!("Selected: {}", package);

    println!("\nFetching file information...");
    let (manifest, download_url) = client.get_files(game, &version, &package).await?;

    if manifest.assets.is_empty() {
        println!("\nNo files found for this package.");
        return Ok(());
    }

    proceed_with_download(manifest, download_url, &version, None, None).await
}
