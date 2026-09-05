use std::fs;
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::time::Duration;

use chrono::Utc;
use flate2::read::GzDecoder;
use futures_util::StreamExt;
use minisign_verify::{PublicKey, Signature};
use reqwest::header::CACHE_CONTROL;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Manager};
use tokio::io::AsyncWriteExt;

const MANIFEST_URL: &str =
    "https://github.com/MingoZacwu/DataNexa-JRE/releases/download/jre-latest/jre-manifest.json";
const MANIFEST_SIGNATURE_URL: &str = "https://github.com/MingoZacwu/DataNexa-JRE/releases/download/jre-latest/jre-manifest.json.minisig";
const MAX_ARCHIVE_BYTES: u64 = 256 * 1024 * 1024;
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(10 * 60);
const MANIFEST_PUBLIC_KEY: &str = include_str!("../jre-manifest.pub");

#[derive(Debug, Clone, Deserialize)]
pub struct JreManifest {
    pub schema_version: u16,
    pub java_major: u16,
    pub java_version: String,
    pub release_tag: String,
    pub artifacts: std::collections::HashMap<String, JreArtifact>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct JreArtifact {
    pub url: String,
    pub sha256: String,
    pub size: u64,
    pub archive: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledRuntime {
    pub version: String,
    pub release_tag: String,
    pub target: String,
    pub archive_sha256: String,
    pub installed_at: String,
    pub runtime_dir: String,
}

#[derive(Debug, Clone)]
pub struct RuntimeUpdate {
    pub available_version: String,
}

pub fn target() -> &'static str {
    if cfg!(target_os = "macos") {
        if cfg!(target_arch = "aarch64") {
            "macos-aarch64"
        } else {
            "macos-x86_64"
        }
    } else if cfg!(target_os = "windows") {
        "windows-x86_64"
    } else {
        "unsupported"
    }
}

pub fn java_path(app: &AppHandle) -> anyhow::Result<Option<(PathBuf, InstalledRuntime)>> {
    let target = target();
    if target == "unsupported" {
        return Ok(None);
    }
    let base = runtime_base(app)?;
    let target_dir = base.join(target);
    let metadata_path = target_dir.join("current.json");
    let metadata = match fs::read_to_string(&metadata_path) {
        Ok(value) => serde_json::from_str::<InstalledRuntime>(&value)?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    if metadata.target != target || !valid_runtime_dir(&metadata.runtime_dir) {
        return Err(anyhow::anyhow!("Managed Java Runtime metadata is invalid"));
    }
    let java_name = if cfg!(target_os = "windows") {
        "java.exe"
    } else {
        "java"
    };
    let java = target_dir
        .join(&metadata.runtime_dir)
        .join("bin")
        .join(java_name);
    if !java.is_file() {
        return Err(anyhow::anyhow!("Managed Java Runtime is incomplete"));
    }
    Ok(Some((java, metadata)))
}

pub fn installed(app: &AppHandle) -> anyhow::Result<Option<InstalledRuntime>> {
    Ok(java_path(app)?.map(|(_, metadata)| metadata))
}

pub async fn check_update(app: &AppHandle) -> anyhow::Result<Option<RuntimeUpdate>> {
    let current = installed(app)?.map(|value| value.version);
    if current.is_none() {
        return Ok(None);
    }
    let manifest = fetch_manifest().await?;
    if current.as_deref() == Some(manifest.java_version.as_str()) {
        return Ok(None);
    }
    Ok(Some(RuntimeUpdate {
        available_version: manifest.java_version,
    }))
}

pub async fn install(app: &AppHandle) -> anyhow::Result<InstalledRuntime> {
    let manifest = fetch_manifest().await?;
    let target = target();
    let artifact = manifest
        .artifacts
        .get(target)
        .ok_or_else(|| anyhow::anyhow!("DataNexa JRE is not available for {target}"))?;
    validate_artifact(artifact)?;

    let base = runtime_base(app)?;
    let target_dir = base.join(target);
    fs::create_dir_all(&target_dir)?;
    let temporary = target_dir.join(format!(".{}.part", uuid::Uuid::new_v4()));
    download_archive(&artifact.url, &temporary, artifact.size).await?;
    let actual_hash = sha256_file(&temporary)?;
    if !actual_hash.eq_ignore_ascii_case(&artifact.sha256) {
        let _ = fs::remove_file(&temporary);
        return Err(anyhow::anyhow!(
            "Downloaded DataNexa JRE failed SHA-256 verification"
        ));
    }

    let runtime_dir = format!("runtime-{}", sanitize_component(&manifest.release_tag));
    let staging = target_dir.join(format!(".install-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&staging)?;
    let extract_result = extract_archive(&temporary, &staging);
    let _ = fs::remove_file(&temporary);
    extract_result?;

    let final_dir = target_dir.join(&runtime_dir);
    if final_dir.exists() {
        fs::remove_dir_all(&final_dir)?;
    }
    fs::rename(&staging, &final_dir)?;
    let java_name = if cfg!(target_os = "windows") {
        "java.exe"
    } else {
        "java"
    };
    if !final_dir.join("bin").join(java_name).is_file() {
        let _ = fs::remove_dir_all(&final_dir);
        return Err(anyhow::anyhow!("Downloaded DataNexa JRE is incomplete"));
    }

    let metadata = InstalledRuntime {
        version: manifest.java_version,
        release_tag: manifest.release_tag,
        target: target.to_string(),
        archive_sha256: artifact.sha256.clone(),
        installed_at: Utc::now().to_rfc3339(),
        runtime_dir,
    };
    let metadata_text = format!("{}\n", serde_json::to_string_pretty(&metadata)?);
    let metadata_tmp = target_dir.join(format!(".current-{}.tmp", uuid::Uuid::new_v4()));
    fs::write(&metadata_tmp, metadata_text)?;
    fs::rename(metadata_tmp, target_dir.join("current.json"))?;
    Ok(metadata)
}

async fn fetch_manifest() -> anyhow::Result<JreManifest> {
    if MANIFEST_PUBLIC_KEY.trim_start().starts_with('#') || MANIFEST_PUBLIC_KEY.lines().count() < 2
    {
        return Err(anyhow::anyhow!("DataNexa JRE public key is not configured"));
    }
    let client = reqwest::Client::builder()
        .timeout(DOWNLOAD_TIMEOUT)
        .user_agent("DataNexa-JRE-Manager")
        .build()?;
    let manifest_response = client
        .get(MANIFEST_URL)
        .header(CACHE_CONTROL, "no-cache")
        .send()
        .await?
        .error_for_status()?;
    let manifest_bytes = manifest_response.bytes().await?.to_vec();
    let signature = client
        .get(MANIFEST_SIGNATURE_URL)
        .header(CACHE_CONTROL, "no-cache")
        .send()
        .await?
        .error_for_status()?
        .text()
        .await?;
    verify_manifest(&manifest_bytes, &signature)?;
    let manifest = serde_json::from_slice::<JreManifest>(&manifest_bytes)?;
    if manifest.schema_version != 1 || manifest.java_major != 21 {
        return Err(anyhow::anyhow!("Unsupported DataNexa JRE manifest"));
    }
    Ok(manifest)
}

fn verify_manifest(bytes: &[u8], signature_text: &str) -> anyhow::Result<()> {
    let public_key = PublicKey::decode(MANIFEST_PUBLIC_KEY.trim())
        .map_err(|error| anyhow::anyhow!("Invalid DataNexa JRE public key: {error}"))?;
    let signature = Signature::decode(signature_text.trim())
        .map_err(|error| anyhow::anyhow!("Invalid DataNexa JRE manifest signature: {error}"))?;
    public_key.verify(bytes, &signature, true).map_err(|error| {
        anyhow::anyhow!("DataNexa JRE manifest signature verification failed: {error}")
    })
}

fn validate_artifact(artifact: &JreArtifact) -> anyhow::Result<()> {
    if artifact.size == 0 || artifact.size > MAX_ARCHIVE_BYTES {
        return Err(anyhow::anyhow!("DataNexa JRE archive size is invalid"));
    }
    if artifact.sha256.len() != 64
        || !artifact
            .sha256
            .chars()
            .all(|value| value.is_ascii_hexdigit())
    {
        return Err(anyhow::anyhow!("DataNexa JRE archive SHA-256 is invalid"));
    }
    if !artifact
        .url
        .starts_with("https://github.com/MingoZacwu/DataNexa-JRE/releases/download/")
    {
        return Err(anyhow::anyhow!("DataNexa JRE archive URL is not trusted"));
    }
    if !artifact.archive.ends_with(".tar.gz") {
        return Err(anyhow::anyhow!(
            "DataNexa JRE archive format is unsupported"
        ));
    }
    Ok(())
}

async fn download_archive(url: &str, path: &Path, expected_size: u64) -> anyhow::Result<()> {
    let client = reqwest::Client::builder()
        .timeout(DOWNLOAD_TIMEOUT)
        .user_agent("DataNexa-JRE-Manager")
        .build()?;
    let response = client.get(url).send().await?.error_for_status()?;
    if response
        .content_length()
        .is_some_and(|size| size > MAX_ARCHIVE_BYTES)
    {
        return Err(anyhow::anyhow!("DataNexa JRE archive is too large"));
    }
    let mut file = tokio::fs::File::create(path).await?;
    let mut total = 0u64;
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        total = total.saturating_add(chunk.len() as u64);
        if total > MAX_ARCHIVE_BYTES || total > expected_size.saturating_add(1024) {
            drop(file);
            let _ = fs::remove_file(path);
            return Err(anyhow::anyhow!(
                "DataNexa JRE archive exceeded its declared size"
            ));
        }
        file.write_all(&chunk).await?;
    }
    file.flush().await?;
    if total != expected_size {
        let _ = fs::remove_file(path);
        return Err(anyhow::anyhow!(
            "DataNexa JRE archive size does not match its manifest"
        ));
    }
    Ok(())
}

fn extract_archive(archive_path: &Path, destination: &Path) -> anyhow::Result<()> {
    let file = fs::File::open(archive_path)?;
    let decoder = GzDecoder::new(file);
    let mut archive = tar::Archive::new(decoder);
    for entry in archive.entries()? {
        let entry = entry?;
        let path = entry.path()?.to_path_buf();
        if path.is_absolute()
            || path
                .components()
                .any(|component| matches!(component, Component::ParentDir))
        {
            return Err(anyhow::anyhow!(
                "DataNexa JRE archive contains an unsafe path"
            ));
        }
        if !entry.header().entry_type().is_file() && !entry.header().entry_type().is_dir() {
            return Err(anyhow::anyhow!(
                "DataNexa JRE archive contains an unsupported entry"
            ));
        }
    }

    let file = fs::File::open(archive_path)?;
    let decoder = GzDecoder::new(file);
    let mut archive = tar::Archive::new(decoder);
    archive.unpack(destination)?;
    Ok(())
}

fn runtime_base(app: &AppHandle) -> anyhow::Result<PathBuf> {
    Ok(app.path().app_data_dir()?.join("jdbc-runtime"))
}

fn valid_runtime_dir(value: &str) -> bool {
    !value.is_empty()
        && value != "."
        && value != ".."
        && !value.contains('/')
        && !value.contains('\\')
}

fn sanitize_component(value: &str) -> String {
    value
        .chars()
        .map(|value| {
            if value.is_ascii_alphanumeric() || matches!(value, '.' | '-' | '_' | '+') {
                value
            } else {
                '_'
            }
        })
        .collect()
}

fn sha256_file(path: &Path) -> anyhow::Result<String> {
    let mut file = fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 1024 * 1024];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_directory_names_are_restricted() {
        assert!(valid_runtime_dir("runtime-jre-21.0.8+9"));
        assert!(!valid_runtime_dir("../runtime"));
        assert!(!valid_runtime_dir("runtime/other"));
    }
}
