use crate::utils::get_md5;
use anyhow::Result;
use mei_proto::SophonManifestAssetProperty;
use rayon::prelude::*;
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

        let download_url = download_url.to_string();
        let save_path = save_path.to_string();
        let progress_callback = Arc::new(progress_callback);

        let failed: Arc<Mutex<Vec<SophonManifestAssetProperty>>> = Arc::new(Mutex::new(Vec::new()));

        assets.par_iter().for_each(|asset| {
            let mut retries = 3;
            let mut ok = false;

            let dl = downloaded.clone();
            let fail = failed.clone();
            let cb = progress_callback.clone();

            while retries > 0 && !ok {
                let result = tokio::runtime::Runtime::new().unwrap().block_on(async {
                    self.try_download_file(asset, &download_url, &save_path, |size| {
                        let mut d = dl.lock().unwrap();
                        *d += size;
                        if let Some(ref callback) = *cb {
                            callback(*d);
                        }
                    })
                    .await
                });

                ok = result.is_ok() && {
                    let file_path = Path::new(&save_path)
                        .join(asset.asset_name.replace('/', std::path::MAIN_SEPARATOR_STR));
                    if let Ok(data) = std::fs::read(&file_path) {
                        get_md5(&data) == asset.asset_hash_md5
                    } else {
                        false
                    }
                };

                if !ok {
                    retries -= 1;
                    println!("Retry {}/3 for {}", 3 - retries, asset.asset_name);
                    std::thread::sleep(std::time::Duration::from_secs(2));
                }
            }

            if !ok {
                fail.lock().unwrap().push(asset.clone());
            }
        });

        let failed_assets = failed.lock().unwrap();
        if failed_assets.len() > 0 {
            println!("Rechecking failed files..");
            drop(failed_assets);

            let failed_vec = failed.lock().unwrap().clone();
            for asset in &failed_vec {
                let mut retries = 3;
                let mut ok = false;

                while retries > 0 && !ok {
                    let dl = downloaded.clone();
                    let cb = progress_callback.clone();

                    let result = self
                        .try_download_file(asset, &download_url, &save_path, |size| {
                            let mut d = dl.lock().unwrap();
                            *d += size;
                            if let Some(ref callback) = *cb {
                                callback(*d);
                            }
                        })
                        .await;

                    ok = result.is_ok();
                    if !ok {
                        retries -= 1;
                        println!("Final retry {}/3 for {}", 3 - retries, asset.asset_name);
                        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
                    }
                }
            }
        }

        println!("Verifying all file hashes..");
        let broken: Vec<_> = assets
            .iter()
            .filter(|a| {
                let file_path = Path::new(&save_path)
                    .join(a.asset_name.replace('/', std::path::MAIN_SEPARATOR_STR));
                if let Ok(data) = std::fs::read(&file_path) {
                    get_md5(&data) != a.asset_hash_md5
                } else {
                    true
                }
            })
            .cloned()
            .collect();

        if broken.len() > 0 {
            println!("Redownloading {} broken files..", broken.len());
            Box::pin(self.download_files(broken, &download_url, &save_path, None)).await?;
        }

        Ok(())
    }

    async fn try_download_file<F>(
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
            let bytes = response.bytes().await?;

            let mut file = File::create(&file_path)?;
            file.write_all(&bytes)?;
            on_chunk_done(bytes.len() as u64);
        } else {
            // sophon mode
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
