use anyhow::{Context, Result};
use mei_proto::{SophonManifestAssetProperty, SophonManifestProto};
use reqwest::{Client, Method};
use semver::Version;
use serde_json::Value;

pub struct DispatchClient {
    client: Client,
}

impl DispatchClient {
    pub fn new() -> Self {
        Self {
            client: Client::new(),
        }
    }

    pub async fn get_dispatch_data(&self) -> Result<Value> {
        let dispatch_url =
            "https://raw.githubusercontent.com/umaichanuwu/meta/refs/heads/master/hoyodata.json";
        println!("Fetching dispatch data...");
        let response = self.client.get(dispatch_url).send().await?;
        let json: Value = response.json().await?;
        Ok(json)
    }

    pub async fn get_dispatch_versions(&self, game: &str) -> Result<Vec<String>> {
        let dispatch_json = self.get_dispatch_data().await?;
        let hashes = dispatch_json[game]["hashes"]
            .as_object()
            .context("No hashes")?;

        let mut versions: Vec<String> = hashes.keys().map(|k| k.clone()).collect();

        // Filter out problematic versions (mihoyo accident in their servers)
        if game == "hk4e" {
            versions.retain(|v| v != "3.2" && v != "3.4");
            println!("ℹ️  Note: Versions 3.2 and 3.4 excluded (server issues)");
        }
        if game == "hkrpg" {
            versions.retain(|v| v != "1.0");
            println!("ℹ️  Note: Version 1.0 excluded (no files available)");
        }

        versions.sort_by(|a, b| {
            let va = Version::parse(a).unwrap_or(Version::new(0, 0, 0));
            let vb = Version::parse(b).unwrap_or(Version::new(0, 0, 0));
            va.cmp(&vb)
        });
        versions.reverse();

        Ok(versions)
    }

    fn parse_version_with_patch(version_str: &str) -> Result<Version> {
        let parts: Vec<&str> = version_str.split('.').collect();
        match parts.len() {
            1 => Version::parse(&format!("{}.0.0", version_str)),
            2 => Version::parse(&format!("{}.0", version_str)),
            3 => Version::parse(version_str),
            _ => Version::parse(version_str),
        }
        .map_err(|e| anyhow::anyhow!("Failed to parse version '{}': {}", version_str, e))
    }

    pub async fn get_packages(&self, game: &str, version: &str) -> Result<Vec<String>> {
        let dispatch_json = self.get_dispatch_data().await?;
        let min_version_str = dispatch_json[game]["minVersion"]
            .as_str()
            .context("No minVersion")?;

        let min_version = Self::parse_version_with_patch(min_version_str)?;
        let current_version = Self::parse_version_with_patch(version)?;

        let mut packages = Vec::new();

        // Scattered Files available for versions > minVersion
        if current_version > min_version {
            packages.push("Files".to_string());
        }

        // ZIP packages
        let version_2_0 = Self::parse_version_with_patch("2.0")?;
        if !(game == "hkrpg" && current_version < version_2_0) {
            packages.push("ZIP".to_string());
        }

        // Update packages
        let version_1_0 = Self::parse_version_with_patch("1.0")?;
        let version_1_1 = Self::parse_version_with_patch("1.1")?;

        if current_version > version_1_0 {
            if game != "hk4e" || current_version > version_1_1 {
                packages.push("Update".to_string());
            }
        }

        Ok(packages)
    }

    pub async fn get_files(
        &self,
        game: &str,
        version: &str,
        mode: &str,
    ) -> Result<(SophonManifestProto, String)> {
        match mode.to_lowercase().as_str() {
            "zip" => {
                println!("Using ZIP mode");
                self.get_zip_files(game, version).await
            }
            "update" => {
                println!("Using Update mode");
                self.get_update_files(game, version).await
            }
            _ => {
                println!("Using Scattered Files mode");
                let dispatch_json = self.get_dispatch_data().await?;

                let url_base_template = dispatch_json[game]["scatterURL"]
                    .as_str()
                    .context("No scatterURL")?;
                let version_hash = dispatch_json[game]["hashes"][version]
                    .as_str()
                    .context("No hash for version")?;

                let url_base = url_base_template.replace("$0", version_hash);
                println!("URL base: {}", url_base);

                // Use filesIndexOptions to get the index file name
                let index_file = dispatch_json[game]["filesIndexOptions"]["index"]
                    .as_str()
                    .context("No index file in filesIndexOptions")?;

                let structure_url = format!("{}/{}", url_base, index_file);
                println!("Structure URL: {}", structure_url);

                let structure_raw = self.client.get(&structure_url).send().await?.text().await?;

                let mut manifest = SophonManifestProto { assets: Vec::new() };

                for line in structure_raw.lines() {
                    if line.trim().is_empty() {
                        continue;
                    }

                    let file_json: Value = serde_json::from_str(line)?;
                    let asset = SophonManifestAssetProperty {
                        asset_name: file_json["remoteName"].as_str().unwrap_or("").to_string(),
                        asset_hash_md5: file_json["md5"].as_str().unwrap_or("").to_string(),
                        asset_size: file_json["fileSize"].as_i64().unwrap_or(0),
                        asset_chunks: Vec::new(),
                        asset_type: 0,
                    };
                    manifest.assets.push(asset);
                }

                println!("Found {} files", manifest.assets.len());

                Ok((manifest, url_base))
            }
        }
    }

    pub async fn get_zip_files(
        &self,
        game: &str,
        version: &str,
    ) -> Result<(SophonManifestProto, String)> {
        let dispatch_json = self.get_dispatch_data().await?;
        let ver = Self::parse_version_with_patch(version)?;

        let full_urls = dispatch_json[game]["urls"]["full"]
            .as_object()
            .context("No full URLs")?;

        let mut max_ver = Self::parse_version_with_patch("0.0.0")?;
        let mut url_template = String::new();

        for (k, v) in full_urls {
            let key_ver = Self::parse_version_with_patch(k)?;
            if key_ver <= ver && key_ver > max_ver {
                max_ver = key_ver;
                url_template = v.as_str().unwrap_or("").to_string();
            }
        }

        let version_hash = dispatch_json[game]["hashes"][version]
            .as_str()
            .context("No hash")?;

        let mut url_base = url_template
            .replace("$0", version_hash)
            .replace("$1", &format!("{}.0", version));

        println!("ZIP URL: {}", url_base);

        let mut manifest = SophonManifestProto { assets: Vec::new() };
        let is_multipart = url_base.ends_with("$4");

        if is_multipart {
            println!("Is multipart zip: true");
        }

        for i in 1..=15 {
            let mut url = url_base.clone();

            if is_multipart {
                url = url_base.replace("$4", &format!("{:03}", i));
            }

            let request = self.client.request(Method::HEAD, &url).send().await?;

            if !request.status().is_success() {
                break;
            }

            let size = request.content_length().unwrap_or(0);

            // HSR can return ">.<" with status 200 which means not found
            if size == 3 {
                break;
            }

            println!("Found part {} ({})", i, crate::utils::format_size(size));

            let filename = url.split('/').last().unwrap_or("").to_string();
            let asset = SophonManifestAssetProperty {
                asset_name: filename,
                asset_hash_md5: String::new(),
                asset_size: size as i64,
                asset_chunks: Vec::new(),
                asset_type: 0,
            };
            manifest.assets.push(asset);

            if !is_multipart {
                break;
            }
        }

        if url_base.ends_with("$4") {
            url_base = url_base[..url_base.rfind('/').unwrap_or(url_base.len())].to_string();
        }

        Ok((manifest, url_base))
    }

    pub async fn get_update_files(
        &self,
        game: &str,
        version: &str,
    ) -> Result<(SophonManifestProto, String)> {
        let dispatch_json = self.get_dispatch_data().await?;

        let updates_hashes = dispatch_json[game]["updatesHashes"]
            .as_object()
            .context("No updatesHashes")?;

        let mut props: Vec<String> = updates_hashes.keys().map(|k| k.clone()).collect();
        props.sort_by(|a, b| {
            let va = Self::parse_version_with_patch(a)
                .unwrap_or_else(|_| Self::parse_version_with_patch("0.0.0").unwrap());
            let vb = Self::parse_version_with_patch(b)
                .unwrap_or_else(|_| Self::parse_version_with_patch("0.0.0").unwrap());
            va.cmp(&vb)
        });

        let idx = props
            .iter()
            .position(|x| x == version)
            .context("Version not found in updatesHashes")?;

        let mut previous_version = if idx > 0 {
            props[idx - 1].clone()
        } else {
            return Err(anyhow::anyhow!("No previous version available for update"));
        };

        if let Some(special) = dispatch_json[game]["updatesHashesSpecial"][version].as_str() {
            previous_version = special.to_string();
        }

        // Add .0 if version has only two parts
        if previous_version.split('.').count() == 2 {
            previous_version.push_str(".0");
        }

        println!("Previous version: {}", previous_version);

        // Get update URL
        let ver = Self::parse_version_with_patch(version)?;
        let update_urls = dispatch_json[game]["urls"]["update"]
            .as_object()
            .context("No update URLs")?;

        let mut max_ver = Self::parse_version_with_patch("0.0.0")?;
        let mut url_template = String::new();

        for (k, v) in update_urls {
            let key_ver = Self::parse_version_with_patch(k)?;
            if key_ver <= ver && key_ver > max_ver {
                max_ver = key_ver;
                url_template = v.as_str().unwrap_or("").to_string();
            }
        }

        let update_hash = dispatch_json[game]["updatesHashes"][version]
            .as_str()
            .context("No update hash for version")?;

        let url_base = url_template
            .replace("$0", update_hash)
            .replace("$2", &previous_version)
            .replace("$3", &format!("{}.0", version));

        println!("Update URL: {}", url_base);

        let request = self.client.request(Method::HEAD, &url_base).send().await?;

        let mut manifest = SophonManifestProto { assets: Vec::new() };

        if request.status().is_success() {
            let size = request.content_length().unwrap_or(0);

            if size > 3 {
                let filename = url_base.split('/').last().unwrap_or("").to_string();
                let asset = SophonManifestAssetProperty {
                    asset_name: filename,
                    asset_hash_md5: String::new(),
                    asset_size: size as i64,
                    asset_chunks: Vec::new(),
                    asset_type: 0,
                };
                manifest.assets.push(asset);
                println!("Update file found ({})", crate::utils::format_size(size));
            } else {
                println!("Update file not found or empty (size: {} bytes)", size);
            }
        } else {
            println!("Update file not available (HTTP {})", request.status());
        }

        Ok((manifest, url_base))
    }
}
