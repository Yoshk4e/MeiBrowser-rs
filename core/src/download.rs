use crate::utils::get_md5;
use anyhow::Result;
use futures_util::stream::StreamExt;
use mei_proto::SophonManifestAssetProperty;
use reqwest::Client;
use std::fs::{self, File};
use std::io::{Cursor, Read, Seek, SeekFrom, Write};
use std::path::Path;
use std::sync::{Arc, Mutex};
use zstd::stream::read::Decoder;

pub struct Downloader {
    client: Client,
}

impl Downloader {
    pub fn new() -> Self {
        Self {
            client: Client::new(),
        }
    }

    pub async fn download_files(
        &self,
        assets: Vec<SophonManifestAssetProperty>,
        download_url: &str,
        save_path: &str,
        progress_callback: Option<Box<dyn Fn(u64) + Send + Sync>>,
    ) -> Result<()> {
        println!("Start download..");

        let _total_size: i64 = assets.iter().map(|a| a.asset_size).sum();
        let downloaded = Arc::new(Mutex::new(0u64));
        let progress_callback = Arc::new(progress_callback);
        let failed: Arc<Mutex<Vec<SophonManifestAssetProperty>>> = Arc::new(Mutex::new(Vec::new()));

        for asset in &assets {
            let mut retries = 3;
            let mut ok = false;

            while retries > 0 && !ok {
                let dl = downloaded.clone();
                let cb = progress_callback.clone();

                match self
                    .try_download_file(asset, download_url, save_path, |size| {
                        let mut d = dl.lock().unwrap();
                        *d += size;
                        if let Some(ref callback) = *cb {
                            callback(*d);
                        }
                    })
                    .await
                {
                    Ok(_) => {
                        // Verify hash
                        let file_path = Path::new(save_path)
                            .join(asset.asset_name.replace('/', std::path::MAIN_SEPARATOR_STR));
                        if let Ok(data) = std::fs::read(&file_path) {
                            if get_md5(&data) == asset.asset_hash_md5 {
                                ok = true;
                            } else {
                                println!("MD5 mismatch for {}, retrying...", asset.asset_name);
                            }
                        } else {
                            println!("Failed to read file for verification: {}", asset.asset_name);
                        }
                    }
                    Err(e) => {
                        println!(
                            "Download failed for {}: {}, retrying...",
                            asset.asset_name, e
                        );
                    }
                }

                if !ok {
                    retries -= 1;
                    if retries > 0 {
                        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
                    }
                }
            }

            if !ok {
                failed.lock().unwrap().push(asset.clone());
            }
        }

        // Recheck failed files
        let failed_assets = failed.lock().unwrap().clone();
        if !failed_assets.is_empty() {
            println!("Rechecking {} failed files..", failed_assets.len());

            for asset in &failed_assets {
                let mut retries = 3;
                let mut ok = false;

                while retries > 0 && !ok {
                    let dl = downloaded.clone();
                    let cb = progress_callback.clone();

                    match self
                        .try_download_file(asset, download_url, save_path, |size| {
                            let mut d = dl.lock().unwrap();
                            *d += size;
                            if let Some(ref callback) = *cb {
                                callback(*d);
                            }
                        })
                        .await
                    {
                        Ok(_) => {
                            let file_path = Path::new(save_path)
                                .join(asset.asset_name.replace('/', std::path::MAIN_SEPARATOR_STR));
                            if let Ok(data) = std::fs::read(&file_path) {
                                if get_md5(&data) == asset.asset_hash_md5 {
                                    ok = true;
                                }
                            }
                        }
                        Err(_) => {
                            retries -= 1;
                        }
                    }
                }
            }
        }

        println!("Verifying all file hashes..");
        let broken: Vec<_> = assets
            .iter()
            .filter(|a| {
                let file_path = Path::new(save_path)
                    .join(a.asset_name.replace('/', std::path::MAIN_SEPARATOR_STR));
                if let Ok(data) = std::fs::read(&file_path) {
                    get_md5(&data) != a.asset_hash_md5
                } else {
                    true
                }
            })
            .cloned()
            .collect();

        if !broken.is_empty() {
            println!("Redownloading {} broken files..", broken.len());
            Box::pin(self.download_files(broken, download_url, save_path, None)).await?;
        }

        println!("Download complete!");
        Ok(())
    }

    pub async fn try_download_file<F>(
        &self,
        asset: &SophonManifestAssetProperty,
        download_url: &str,
        save_path: &str,
        mut on_chunk_done: F,
    ) -> Result<()>
    where
        F: FnMut(u64),
    {
        println!("Download file {}", asset.asset_name);

        let normalized = asset.asset_name.replace('/', std::path::MAIN_SEPARATOR_STR);
        let file_path = Path::new(save_path).join(&normalized);

        if let Some(parent) = file_path.parent() {
            fs::create_dir_all(parent)?;
        }

        if file_path.exists() {
            if !asset.asset_hash_md5.is_empty() {
                let data = fs::read(&file_path)?;
                let md5 = get_md5(&data);
                if md5 == asset.asset_hash_md5 {
                    on_chunk_done(asset.asset_size as u64);
                    return Ok(());
                }
            } else {
                println!("No MD5 provided, redownloading file");
            }
        }

        if asset.asset_chunks.is_empty() {
            let mut url = download_url.to_string();
            if !url.ends_with(&asset.asset_name) {
                url = format!("{}/{}", download_url, asset.asset_name);
            }

            println!("Downloading from {}", url);

            let response = self.client.get(&url).send().await?;
            let total_size = response.content_length().unwrap_or(0);

            let mut stream = response.bytes_stream();
            let mut file = File::create(&file_path)?;
            let mut downloaded = 0u64;

            while let Some(chunk) = stream.next().await {
                let chunk = chunk?;
                file.write_all(&chunk)?;
                downloaded += chunk.len() as u64;
                on_chunk_done(chunk.len() as u64);
            }

            if total_size > 0 && downloaded != total_size {
                return Err(anyhow::anyhow!(
                    "Download incomplete: {}/{} bytes",
                    downloaded,
                    total_size
                ));
            }
        } else {
            let mut file = File::options()
                .read(true)
                .write(true)
                .create(true)
                .open(&file_path)?;

            if file.metadata()?.len() < asset.asset_size as u64 {
                file.set_len(asset.asset_size as u64)?;
            }

            for chunk in &asset.asset_chunks {
                file.seek(SeekFrom::Start(chunk.chunk_on_file_offset as u64))?;

                let mut existing = vec![0u8; chunk.chunk_size_decompressed as usize];
                file.read_exact(&mut existing)?;

                if get_md5(&existing) == chunk.chunk_decompressed_hash_md5 {
                    on_chunk_done(existing.len() as u64);
                    continue;
                }

                let chunk_url = download_url.replace("$0", &chunk.chunk_name);

                let response = match self.client.get(&chunk_url).send().await {
                    Ok(r) => r,
                    Err(e) => {
                        println!("Failed to download chunk: {}", e);
                        continue;
                    }
                };

                let compressed = response.bytes().await?;

                let decompressed = match Decoder::new(Cursor::new(&compressed[..])) {
                    Ok(mut decoder) => {
                        let mut buf = Vec::new();
                        decoder.read_to_end(&mut buf)?;
                        buf
                    }
                    Err(_) => vec![0u8; chunk.chunk_size_decompressed as usize],
                };

                let final_data = if get_md5(&decompressed) != chunk.chunk_decompressed_hash_md5 {
                    vec![0u8; chunk.chunk_size_decompressed as usize]
                } else {
                    decompressed
                };

                file.seek(SeekFrom::Start(chunk.chunk_on_file_offset as u64))?;
                file.write_all(&final_data)?;
                file.flush()?;
                on_chunk_done(final_data.len() as u64);
            }

            file.flush()?;
        }

        let data = fs::read(&file_path)?;
        if get_md5(&data) != asset.asset_hash_md5 {
            println!("Final file MD5 mismatch");
        }

        Ok(())
    }
}
