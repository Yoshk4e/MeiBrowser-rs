use crate::utils::{get_game_map, get_sophon_map};
use anyhow::{Context, Result};
use mei_proto::SophonManifestProto;
use prost::Message;
use reqwest::Client;
use serde_json::Value;
use std::io::{Cursor, Read};
use zstd::stream::read::Decoder;

const BRANCH: &str = "main";

pub struct SophonClient {
    client: Client,
}

impl SophonClient {
    pub fn new() -> Self {
        Self {
            client: Client::new(),
        }
    }

    pub async fn get_game_branches(&self, game: &str, region: &str) -> Result<Value> {
        let game_map = get_game_map();
        let sophon_map = get_sophon_map();

        let game_id = game_map
            .get(&(region.to_string(), game.to_string()))
            .context("Invalid game/region combination")?;
        let sophon_data = sophon_map.get(region).context("Invalid region")?;

        let meta_url = format!(
            "{}?launcher_id={}&game_ids[]={}",
            sophon_data.get("apiBase").unwrap(),
            sophon_data.get("launcherId").unwrap(),
            game_id
        );

        println!("API URL: {}", meta_url);

        let response = self.client.get(&meta_url).send().await?;
        let json: Value = response.json().await?;

        Ok(json)
    }

    pub async fn get_build(
        &self,
        region: &str,
        package_id: &str,
        password: &str,
        version: &str,
        pre_download: bool,
    ) -> Result<Value> {
        let sophon_map = get_sophon_map();
        let sophon_data = sophon_map.get(region).context("Invalid region")?;

        let branch_name = if pre_download { "predownload" } else { BRANCH };

        let mut build_url = format!(
            "{}?branch={}&package_id={}&password={}&plat_app={}",
            sophon_data.get("sophonBase").unwrap(),
            branch_name,
            package_id,
            password,
            sophon_data.get("platApp").unwrap()
        );

        if !pre_download {
            // Ensure version has .0 suffix if needed
            let version_with_patch = if version.split('.').count() == 2 {
                format!("{}.0", version)
            } else {
                version.to_string()
            };
            build_url.push_str(&format!("&tag={}", version_with_patch));
        }

        println!("Build URL: {}", build_url);

        let response = self.client.get(&build_url).send().await?;
        let json: Value = response.json().await?;

        Ok(json)
    }

    pub async fn get_custom_build(&self, url: &str) -> Result<Value> {
        println!("Custom build URL: {}", url);
        let response = self.client.get(url).send().await?;
        let json: Value = response.json().await?;
        Ok(json)
    }

    pub async fn check_build(&self, url: &str) -> Result<String> {
        let json = self.get_custom_build(url).await?;
        let version = json["data"]["tag"]
            .as_str()
            .context("No tag field in build")?
            .to_string();
        Ok(version)
    }

    pub async fn get_manifest(
        &self,
        game: &str,
        version: &str,
        region: &str,
        category_id: &str,
        custom_data: Option<&str>,
    ) -> Result<(SophonManifestProto, String)> {
        let build_json: Value;
        let mut actual_category_id: Option<String> = None;

        if let Some(data) = custom_data {
            build_json = serde_json::from_str(data)?;
            let matching_field = "game"; // this is hardcoded because beta builds doesn't include audio

            let manifests = build_json["data"]["manifests"]
                .as_array()
                .context("No manifests in custom data")?;

            for manifest in manifests {
                if manifest["matching_field"].as_str() == Some(matching_field) {
                    actual_category_id = Some(
                        manifest["category_id"]
                            .as_str()
                            .context("No category_id")?
                            .to_string(),
                    );
                    break;
                }
            }
            if actual_category_id.is_none() {
                anyhow::bail!("Could not find matching manifest");
            }
        } else {
            actual_category_id = Some(category_id.to_string());

            if game != "custom" {
                let meta_json = self.get_game_branches(game, region).await?;

                let mut version_str = version.to_string();
                let mut pre_download = false;

                // Handle pre-download suffix
                if version_str.ends_with(" (pre-download).0") {
                    version_str = version_str.replace(" (pre-download).0", "");
                    pre_download = true;
                } else if version_str.ends_with(" (pre-download)") {
                    version_str = version_str.replace(" (pre-download)", "");
                    pre_download = true;
                }

                let branch_name = if pre_download { "pre_download" } else { "main" };
                let game_meta = &meta_json["data"]["game_branches"][0][branch_name];

                let package_id = game_meta["package_id"].as_str().context("No package_id")?;
                let password = game_meta["password"].as_str().context("No password")?;

                build_json = self
                    .get_build(region, package_id, password, &version_str, pre_download)
                    .await?;
            } else {
                build_json = self.get_custom_build(region).await?;
            }
        }

        let game_data = &build_json["data"];
        let manifests = game_data["manifests"]
            .as_array()
            .context("No manifests array")?;

        let manifest_info = manifests
            .iter()
            .find(|m| {
                m.get("category_id").and_then(|v| v.as_str()) == actual_category_id.as_deref()
            })
            .ok_or_else(|| anyhow::anyhow!("Category not found"))?;

        let url_prefix = manifest_info["chunk_download"]["url_prefix"]
            .as_str()
            .context("No url_prefix")?;
        let url_suffix = manifest_info["chunk_download"]["url_suffix"]
            .as_str()
            .unwrap_or("");

        let mut download_url = format!("{}/$0", url_prefix);
        if !url_suffix.is_empty() {
            download_url.push_str(&format!("?{}", url_suffix));
        }

        let manifest_url_prefix = manifest_info["manifest_download"]["url_prefix"]
            .as_str()
            .context("No manifest url_prefix")?;
        let manifest_id = manifest_info["manifest"]["id"]
            .as_str()
            .context("No manifest id")?;
        let manifest_url_suffix = manifest_info["manifest_download"]["url_suffix"]
            .as_str()
            .unwrap_or("");

        let mut manifest_url = format!("{}/{}", manifest_url_prefix, manifest_id);
        if !manifest_url_suffix.is_empty() {
            manifest_url.push_str(&format!("?{}", manifest_url_suffix));
        }

        println!("Manifest URL: {}", manifest_url);

        let compressed = self.client.get(&manifest_url).send().await?.bytes().await?;

        let mut decoder = Decoder::new(Cursor::new(compressed))?;
        let mut decompressed = Vec::new();
        decoder.read_to_end(&mut decompressed)?;

        let manifest = SophonManifestProto::decode(&decompressed[..])?;

        Ok((manifest, download_url))
    }

    // Get manifest directly from build data (for SToken mode)
    pub async fn get_manifest_from_build_data(
        &self,
        build_json_str: &str,
        category_id: &str,
    ) -> Result<(SophonManifestProto, String)> {
        let build_json: Value = serde_json::from_str(build_json_str)?;

        let game_data = &build_json["data"];
        let manifests = game_data["manifests"]
            .as_array()
            .context("No manifests array")?;

        let manifest_info = manifests
            .iter()
            .find(|m| m.get("category_id").and_then(|v| v.as_str()) == Some(category_id))
            .ok_or_else(|| anyhow::anyhow!("Category {} not found", category_id))?;

        let url_prefix = manifest_info["chunk_download"]["url_prefix"]
            .as_str()
            .context("No url_prefix")?;
        let url_suffix = manifest_info["chunk_download"]["url_suffix"]
            .as_str()
            .unwrap_or("");

        let mut download_url = format!("{}/$0", url_prefix);
        if !url_suffix.is_empty() {
            download_url.push_str(&format!("?{}", url_suffix));
        }

        let manifest_url_prefix = manifest_info["manifest_download"]["url_prefix"]
            .as_str()
            .context("No manifest url_prefix")?;
        let manifest_id = manifest_info["manifest"]["id"]
            .as_str()
            .context("No manifest id")?;
        let manifest_url_suffix = manifest_info["manifest_download"]["url_suffix"]
            .as_str()
            .unwrap_or("");

        let mut manifest_url = format!("{}/{}", manifest_url_prefix, manifest_id);
        if !manifest_url_suffix.is_empty() {
            manifest_url.push_str(&format!("?{}", manifest_url_suffix));
        }

        println!("Manifest URL: {}", manifest_url);

        let compressed = self.client.get(&manifest_url).send().await?.bytes().await?;

        let mut decoder = Decoder::new(Cursor::new(compressed))?;
        let mut decompressed = Vec::new();
        decoder.read_to_end(&mut decompressed)?;

        let manifest = SophonManifestProto::decode(&decompressed[..])?;

        Ok((manifest, download_url))
    }
}
