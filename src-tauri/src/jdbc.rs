use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::Utc;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, RefreshKind, System};
use tauri::{AppHandle, Emitter, Manager};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::sync::Mutex;
use tokio::time::timeout;
use uuid::Uuid;
use zeroize::{Zeroize, Zeroizing};

use crate::config::{ConfigStore, ConnectionConfig, DbKind};
use crate::db::{ColumnInfo, QueryResult, TableInfo};
use crate::vault::CredentialVault;

const PROTOCOL_VERSION: u16 = 1;
const MAVEN_CENTRAL: &str = "https://repo.maven.apache.org/maven2/";
const MAX_SIDECAR_RESPONSE_BYTES: usize = 10 * 1024 * 1024;
const SIDECAR_START_TIMEOUT: Duration = Duration::from_secs(15);
const MAVEN_INSTALL_TIMEOUT: Duration = Duration::from_secs(5 * 60);
const JAVA_PROBE_TIMEOUT: Duration = Duration::from_secs(10);
const MAX_LOCAL_DRIVER_FILES: usize = 256;
const MAX_LOCAL_DRIVER_BYTES: u64 = 1024 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JdbcDriverFile {
    pub name: String,
    pub size: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JdbcDriverBundle {
    pub schema_version: u16,
    pub bundle_id: String,
    pub display_name: String,
    pub maven_coordinate: String,
    pub repository_url: String,
    pub installed_at: String,
    pub driver_classes: Vec<String>,
    pub files: Vec<JdbcDriverFile>,
    pub total_size: u64,
    #[serde(default = "default_driver_source")]
    pub source: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct JdbcRuntimeStatus {
    pub available: bool,
    pub source: String,
    pub java_version: Option<String>,
    pub sidecar_available: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct JdbcStatus {
    pub runtime: JdbcRuntimeStatus,
    pub drivers: Vec<JdbcDriverBundle>,
}

pub const JDBC_INSTALL_PROGRESS_EVENT: &str = "jdbc://driver-install-progress";

#[derive(Debug, Clone, Serialize)]
pub struct JdbcInstallProgress {
    pub operation: String,
    pub phase: String,
    pub progress: Option<u8>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct InstallJdbcDriverInput {
    pub display_name: String,
    pub maven_coordinate: String,
    #[serde(default)]
    pub repository_url: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ImportJdbcDriverInput {
    pub display_name: String,
    pub paths: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct JdbcStorageItem {
    pub id: String,
    pub label: String,
    pub path: String,
    pub bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct JdbcDriverRuntimeInfo {
    pub bundle_id: String,
    pub display_name: String,
    pub status: String,
    pub health: String,
    pub process_count: usize,
    pub memory_bytes: u64,
    pub cpu_percent: f32,
}

#[derive(Debug, Clone, Serialize)]
pub struct JdbcStorageStatus {
    pub storage_root: String,
    pub total_bytes: u64,
    pub items: Vec<JdbcStorageItem>,
    pub runtimes: Vec<JdbcDriverRuntimeInfo>,
}

#[derive(Debug, Serialize)]
struct SidecarRequest<'a> {
    protocol_version: u16,
    request_id: String,
    action: &'a str,
    bundle_path: &'a str,
    jdbc_url: &'a str,
    username: &'a str,
    password: &'a str,
    driver_class: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    sql: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    schema: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    table: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_rows: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_result_bytes: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    query_timeout_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    maven_coordinate: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    maven_repository: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    maven_local_repository: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    maven_output_directory: Option<&'a str>,
}

#[derive(Debug, Default)]
struct SidecarQuery<'a> {
    sql: Option<&'a str>,
    schema: Option<&'a str>,
    table: Option<&'a str>,
    max_rows: Option<u32>,
    max_result_bytes: Option<usize>,
    query_timeout_ms: Option<u64>,
    maven_coordinate: Option<&'a str>,
    maven_repository: Option<&'a str>,
    maven_local_repository: Option<&'a str>,
    maven_output_directory: Option<&'a str>,
}

#[derive(Debug, Deserialize)]
struct SidecarResponse {
    protocol_version: u16,
    request_id: String,
    ok: bool,
    #[serde(default)]
    driver_classes: Vec<String>,
    #[serde(default)]
    database_product: String,
    #[serde(default)]
    database_version: String,
    #[serde(default)]
    driver_name: String,
    #[serde(default)]
    driver_version: String,
    #[serde(default)]
    tables: Vec<TableInfo>,
    #[serde(default)]
    columns: Vec<ColumnInfo>,
    #[serde(default)]
    column_names: Vec<String>,
    #[serde(default)]
    rows: Vec<Map<String, Value>>,
    #[serde(default)]
    row_count: usize,
    #[serde(default)]
    truncated: bool,
    #[serde(default)]
    truncation_reason: Option<String>,
    #[serde(default)]
    returned_bytes: usize,
    #[serde(default)]
    artifacts: Vec<SidecarArtifact>,
    #[serde(default)]
    error: Option<SidecarError>,
}

#[derive(Debug, Deserialize)]
struct SidecarArtifact {
    name: String,
    size: u64,
    sha256: String,
}

#[derive(Debug, Deserialize)]
struct SidecarError {
    code: String,
    message: String,
}

#[derive(Clone)]
pub struct JdbcManager {
    app: Option<AppHandle>,
    sessions: Arc<Mutex<HashMap<String, Arc<JdbcSidecarSession>>>>,
}

struct JdbcSidecarSession {
    bundle_id: String,
    process: Mutex<JdbcSidecarProcess>,
    next_request_id: AtomicU64,
}

#[derive(Debug, Clone, Default)]
struct ProcessStats {
    memory_bytes: u64,
    cpu_percent: f32,
}

struct JdbcSidecarProcess {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
}

impl JdbcManager {
    pub fn new(app: AppHandle) -> Self {
        Self {
            app: Some(app),
            sessions: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    #[cfg(test)]
    pub(crate) fn for_test() -> Self {
        Self {
            app: None,
            sessions: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub async fn status(&self) -> anyhow::Result<JdbcStatus> {
        Ok(JdbcStatus {
            runtime: self.runtime_status().await,
            drivers: self.list_drivers()?,
        })
    }

    pub async fn shutdown_sessions(&self) {
        let sessions = std::mem::take(&mut *self.sessions.lock().await);
        for session in sessions.into_values() {
            session.shutdown().await;
        }
    }

    fn emit_install_progress(&self, operation: &str, phase: &str, progress: Option<u8>) {
        let Some(app) = &self.app else {
            return;
        };
        if let Err(error) = app.emit(
            JDBC_INSTALL_PROGRESS_EVENT,
            JdbcInstallProgress {
                operation: operation.to_string(),
                phase: phase.to_string(),
                progress,
            },
        ) {
            eprintln!("failed to emit JDBC install progress: {error}");
        }
    }

    pub fn list_drivers(&self) -> anyhow::Result<Vec<JdbcDriverBundle>> {
        let root = self.drivers_root()?;
        fs::create_dir_all(&root)?;
        let mut drivers = Vec::new();
        for entry in fs::read_dir(root)? {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with('.') || !valid_bundle_id(&name) {
                continue;
            }
            if let Ok(bundle) = read_manifest(&entry.path()) {
                if bundle.bundle_id == name && verify_bundle_files(&entry.path(), &bundle).is_ok() {
                    drivers.push(bundle);
                }
            }
        }
        drivers.sort_by(|left, right| right.installed_at.cmp(&left.installed_at));
        Ok(drivers)
    }

    pub async fn install_driver(
        &self,
        input: InstallJdbcDriverInput,
    ) -> anyhow::Result<JdbcDriverBundle> {
        self.emit_install_progress("install", "preparing", Some(0));
        let display_name = validate_display_name(&input.display_name)?;
        let coordinate = input.maven_coordinate.trim();
        if !valid_maven_coordinate(coordinate) {
            return Err(anyhow::anyhow!(
                "Maven coordinate must use groupId:artifactId:version"
            ));
        }
        let repository_url = validate_repository_url(input.repository_url.as_deref())?;

        let root = self.drivers_root()?;
        fs::create_dir_all(&root)?;
        let staging = tempfile::Builder::new()
            .prefix(".install-")
            .tempdir_in(&root)?;
        let jars = staging.path().join("jars");
        fs::create_dir_all(&jars)?;
        let local_repository = self
            .app
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("JDBC manager is unavailable in this test context"))?
            .path()
            .app_data_dir()?
            .join("maven-repository");
        let local_repository = local_repository.to_string_lossy().to_string();
        let output_directory = jars.to_string_lossy().to_string();
        self.emit_install_progress("install", "downloading", None);
        let resolution = self
            .run_sidecar_once(
                "resolve_maven",
                staging.path(),
                "",
                "",
                "",
                "",
                &SidecarQuery {
                    maven_coordinate: Some(coordinate),
                    maven_repository: Some(&repository_url),
                    maven_local_repository: Some(&local_repository),
                    maven_output_directory: Some(&output_directory),
                    ..SidecarQuery::default()
                },
                MAVEN_INSTALL_TIMEOUT,
            )
            .await?;
        if resolution.artifacts.is_empty() {
            return Err(anyhow::anyhow!(
                "Maven completed without producing JDBC driver JAR files"
            ));
        }
        self.emit_install_progress("install", "verifying", Some(65));
        let mut jar_paths = fs::read_dir(&jars)?
            .filter_map(Result::ok)
            .filter_map(|entry| {
                entry
                    .file_type()
                    .ok()
                    .filter(|kind| kind.is_file())
                    .map(|_| entry.path())
            })
            .filter(|path| {
                path.extension()
                    .and_then(|value| value.to_str())
                    .is_some_and(|value| value.eq_ignore_ascii_case("jar"))
            })
            .collect::<Vec<_>>();
        jar_paths.sort();
        if jar_paths.is_empty() {
            return Err(anyhow::anyhow!(
                "Maven completed without producing JDBC driver JAR files"
            ));
        }
        let resolved_artifacts = resolution
            .artifacts
            .iter()
            .map(|artifact| (artifact.name.as_str(), artifact))
            .collect::<HashMap<_, _>>();
        if resolved_artifacts.len() != jar_paths.len() {
            return Err(anyhow::anyhow!(
                "Maven resolver returned an unexpected set of JAR files"
            ));
        }
        for path in &jar_paths {
            let name = path
                .file_name()
                .and_then(|value| value.to_str())
                .ok_or_else(|| anyhow::anyhow!("Maven resolver returned an invalid file name"))?;
            let artifact = resolved_artifacts.get(name).ok_or_else(|| {
                anyhow::anyhow!("Maven resolver output is missing artifact {name}")
            })?;
            let metadata = fs::metadata(path)?;
            if metadata.len() != artifact.size || sha256_file(path)? != artifact.sha256 {
                return Err(anyhow::anyhow!(
                    "Maven resolver output failed integrity verification for {name}"
                ));
            }
        }

        self.emit_install_progress("install", "inspecting", Some(80));
        let inspection = self
            .run_sidecar_once(
                "inspect",
                staging.path(),
                "",
                "",
                "",
                "",
                &SidecarQuery::default(),
                SIDECAR_START_TIMEOUT,
            )
            .await?;
        if inspection.driver_classes.is_empty() {
            return Err(anyhow::anyhow!(
                "The installed bundle does not expose any JDBC Driver implementations"
            ));
        }

        let mut files = Vec::with_capacity(jar_paths.len());
        let mut total_size = 0_u64;
        for path in jar_paths {
            let metadata = fs::metadata(&path)?;
            total_size = total_size.saturating_add(metadata.len());
            files.push(JdbcDriverFile {
                name: path
                    .file_name()
                    .and_then(|value| value.to_str())
                    .ok_or_else(|| anyhow::anyhow!("JDBC driver JAR has an invalid file name"))?
                    .to_string(),
                size: metadata.len(),
                sha256: sha256_file(&path)?,
            });
        }

        let bundle_id = Uuid::new_v4().to_string();
        let bundle = JdbcDriverBundle {
            schema_version: 1,
            bundle_id: bundle_id.clone(),
            display_name: display_name.to_string(),
            maven_coordinate: coordinate.to_string(),
            repository_url: repository_url.clone(),
            installed_at: Utc::now().to_rfc3339(),
            driver_classes: inspection.driver_classes,
            files,
            total_size,
            source: "maven".to_string(),
        };
        let mut manifest = serde_json::to_vec_pretty(&bundle)?;
        manifest.push(b'\n');
        self.emit_install_progress("install", "finalizing", Some(92));
        fs::write(staging.path().join("manifest.json"), manifest)?;
        let final_path = root.join(&bundle_id);
        fs::rename(staging.path(), &final_path)?;
        Ok(bundle)
    }

    pub async fn import_driver(
        &self,
        input: ImportJdbcDriverInput,
    ) -> anyhow::Result<JdbcDriverBundle> {
        self.emit_install_progress("import", "preparing", Some(0));
        let display_name = validate_display_name(&input.display_name)?;
        if input.paths.is_empty() {
            return Err(anyhow::anyhow!(
                "Select at least one JDBC driver JAR or directory"
            ));
        }

        let source_paths = input
            .paths
            .iter()
            .filter(|value| !value.trim().is_empty())
            .map(|value| PathBuf::from(value.trim()))
            .collect::<Vec<_>>();
        let jar_paths = collect_local_jar_paths(&source_paths)?;
        let root = self.drivers_root()?;
        fs::create_dir_all(&root)?;
        let staging = tempfile::Builder::new()
            .prefix(".import-")
            .tempdir_in(&root)?;
        let jars = staging.path().join("jars");
        fs::create_dir_all(&jars)?;

        let mut names = HashSet::new();
        let mut total_size = 0_u64;
        self.emit_install_progress("import", "copying", Some(30));
        for path in &jar_paths {
            let metadata = fs::metadata(path)?;
            total_size = total_size.saturating_add(metadata.len());
            if total_size > MAX_LOCAL_DRIVER_BYTES {
                return Err(anyhow::anyhow!(
                    "Local JDBC driver files exceed the 1 GiB limit"
                ));
            }
            let name = path
                .file_name()
                .and_then(|value| value.to_str())
                .ok_or_else(|| anyhow::anyhow!("JDBC driver JAR has an invalid file name"))?;
            if !names.insert(name.to_ascii_lowercase()) {
                return Err(anyhow::anyhow!(
                    "Local JDBC driver files contain duplicate file names: {name}"
                ));
            }
            fs::copy(path, jars.join(name))?;
        }

        self.emit_install_progress("import", "inspecting", Some(65));
        let inspection = self
            .run_sidecar_once(
                "inspect",
                staging.path(),
                "",
                "",
                "",
                "",
                &SidecarQuery::default(),
                SIDECAR_START_TIMEOUT,
            )
            .await?;
        if inspection.driver_classes.is_empty() {
            return Err(anyhow::anyhow!(
                "The imported bundle does not expose any JDBC Driver implementations"
            ));
        }

        let mut files = Vec::with_capacity(jar_paths.len());
        for path in fs::read_dir(&jars)?
            .filter_map(Result::ok)
            .map(|entry| entry.path())
        {
            let metadata = fs::metadata(&path)?;
            files.push(JdbcDriverFile {
                name: path
                    .file_name()
                    .and_then(|value| value.to_str())
                    .ok_or_else(|| anyhow::anyhow!("JDBC driver JAR has an invalid file name"))?
                    .to_string(),
                size: metadata.len(),
                sha256: sha256_file(&path)?,
            });
        }
        files.sort_by(|left, right| left.name.cmp(&right.name));
        self.emit_install_progress("import", "verifying", Some(84));

        let bundle_id = Uuid::new_v4().to_string();
        let bundle = JdbcDriverBundle {
            schema_version: 1,
            bundle_id: bundle_id.clone(),
            display_name: display_name.to_string(),
            maven_coordinate: String::new(),
            repository_url: String::new(),
            installed_at: Utc::now().to_rfc3339(),
            driver_classes: inspection.driver_classes,
            files,
            total_size,
            source: "local".to_string(),
        };
        let mut manifest = serde_json::to_vec_pretty(&bundle)?;
        manifest.push(b'\n');
        fs::write(staging.path().join("manifest.json"), manifest)?;

        let final_path = root.join(&bundle_id);
        self.emit_install_progress("import", "finalizing", Some(92));
        fs::rename(staging.path(), &final_path)?;
        Ok(bundle)
    }

    pub async fn storage_status(&self) -> anyhow::Result<JdbcStorageStatus> {
        let app = self
            .app
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("JDBC manager is unavailable in this test context"))?;
        let storage_root = app.path().app_data_dir()?;
        let drivers_root = self.drivers_root()?;
        let runtime_path = self.java_executable().ok().and_then(|(java, _)| {
            java.parent()
                .and_then(|path| path.parent())
                .map(|path| path.to_path_buf())
        });
        let runtime_bytes = runtime_path
            .as_ref()
            .map(|path| path_size_bytes(path))
            .unwrap_or(0);
        let items = vec![
            ("drivers", "JDBC drivers", drivers_root.clone()),
            (
                "maven",
                "Maven repository",
                storage_root.join("maven-repository"),
            ),
            ("audit", "Audit database", storage_root.join("audit.db")),
            (
                "access",
                "Access control",
                storage_root.join("access-control.db"),
            ),
            ("config", "Configuration", storage_root.join("config.toml")),
        ];
        let mut items = items
            .into_iter()
            .map(|(id, label, path)| JdbcStorageItem {
                id: id.to_string(),
                label: label.to_string(),
                bytes: path_size_bytes(&path),
                path: path.to_string_lossy().to_string(),
            })
            .collect::<Vec<_>>();
        let runtime_inside_storage = runtime_path
            .as_ref()
            .is_some_and(|path| path.starts_with(&storage_root));
        let total_bytes = path_size_bytes(&storage_root)
            .saturating_sub(runtime_inside_storage.then_some(runtime_bytes).unwrap_or(0));
        let known_bytes = items.iter().map(|item| item.bytes).sum::<u64>();
        let other_bytes = total_bytes.saturating_sub(known_bytes);
        if other_bytes > 0 {
            items.push(JdbcStorageItem {
                id: "other".to_string(),
                label: "Other files".to_string(),
                path: storage_root.to_string_lossy().to_string(),
                bytes: other_bytes,
            });
        }
        let drivers = self.list_drivers()?;
        let runtimes = self.collect_driver_runtime_info(&drivers).await;
        Ok(JdbcStorageStatus {
            storage_root: storage_root.to_string_lossy().to_string(),
            total_bytes,
            items,
            runtimes,
        })
    }

    pub fn clear_maven_cache(&self) -> anyhow::Result<()> {
        let app = self
            .app
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("JDBC manager is unavailable in this test context"))?;
        let cache_path = app.path().app_data_dir()?.join("maven-repository");
        let metadata = match fs::symlink_metadata(&cache_path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(error.into()),
        };
        if metadata.file_type().is_symlink() {
            return Err(anyhow::anyhow!(
                "Maven repository path must not be a symbolic link"
            ));
        }
        if !metadata.is_dir() {
            return Err(anyhow::anyhow!("Maven repository path is not a directory"));
        }
        fs::remove_dir_all(&cache_path)?;
        fs::create_dir_all(cache_path)?;
        Ok(())
    }

    pub fn delete_driver(
        &self,
        bundle_id: &str,
        connections: &[ConnectionConfig],
    ) -> anyhow::Result<()> {
        if !valid_bundle_id(bundle_id) {
            return Err(anyhow::anyhow!("Invalid JDBC driver bundle ID"));
        }
        if connections.iter().any(|connection| {
            connection.kind == DbKind::Jdbc
                && connection.jdbc_bundle_id.as_deref() == Some(bundle_id)
        }) {
            return Err(anyhow::anyhow!(
                "This JDBC driver is referenced by a saved connection and cannot be deleted"
            ));
        }
        let root = self.drivers_root()?;
        let target = root.join(bundle_id);
        if !target.is_dir() {
            return Err(anyhow::anyhow!("JDBC driver bundle was not found"));
        }
        fs::remove_dir_all(target)?;
        Ok(())
    }

    pub fn ensure_bundle_exists(&self, bundle_id: &str) -> anyhow::Result<()> {
        let path = self.bundle_path(bundle_id)?;
        let bundle = read_manifest(&path)?;
        if bundle.bundle_id != bundle_id {
            return Err(anyhow::anyhow!(
                "JDBC driver bundle is incomplete or damaged"
            ));
        }
        verify_bundle_files(&path, &bundle)?;
        Ok(())
    }

    pub async fn test_connection(
        &self,
        config: &ConnectionConfig,
        vault: &CredentialVault,
        password_override: Option<&str>,
    ) -> anyhow::Result<Duration> {
        if config.kind != DbKind::Jdbc {
            return Err(anyhow::anyhow!("Connection is not a JDBC connection"));
        }
        let bundle_id = required(config.jdbc_bundle_id.as_deref(), "JDBC driver")?;
        let jdbc_url = required(config.jdbc_url.as_deref(), "JDBC URL")?;
        let bundle_path = self.bundle_path(bundle_id)?;
        self.ensure_bundle_exists(bundle_id)?;
        let stored_password = if password_override.is_none() {
            match config.credential_ref.as_deref() {
                Some(credential_ref) => vault
                    .get(credential_ref)?
                    .unwrap_or_else(|| Zeroizing::new(String::new())),
                None => Zeroizing::new(String::new()),
            }
        } else {
            Zeroizing::new(String::new())
        };
        let password = password_override.unwrap_or(stored_password.as_str());
        let started = Instant::now();
        let response = self
            .run_sidecar(
                "test_connection",
                &bundle_path,
                jdbc_url,
                config.username.as_deref().unwrap_or(""),
                password,
                config.jdbc_driver_class.as_deref().unwrap_or(""),
                SidecarQuery::default(),
                Duration::from_millis(config.query_timeout_ms.clamp(500, 60_000)),
            )
            .await?;
        if response.database_product.is_empty() {
            return Err(anyhow::anyhow!(
                "JDBC connection did not return database metadata"
            ));
        }
        let _metadata = (
            response.database_version,
            response.driver_name,
            response.driver_version,
        );
        Ok(started.elapsed())
    }

    pub async fn list_schema(
        &self,
        config: &ConnectionConfig,
        vault: &CredentialVault,
    ) -> anyhow::Result<Vec<TableInfo>> {
        let (bundle_path, jdbc_url, username, password, driver_class) =
            self.connection_context(config, vault, None)?;
        let response = self
            .run_sidecar(
                "list_schema",
                &bundle_path,
                &jdbc_url,
                &username,
                &password,
                &driver_class,
                SidecarQuery::default(),
                query_timeout(config),
            )
            .await?;
        Ok(response.tables)
    }

    pub async fn describe_table(
        &self,
        config: &ConnectionConfig,
        vault: &CredentialVault,
        schema: Option<&str>,
        table: &str,
    ) -> anyhow::Result<Vec<ColumnInfo>> {
        validate_identifier(table)?;
        if let Some(schema) = schema {
            validate_identifier(schema)?;
        }
        let (bundle_path, jdbc_url, username, password, driver_class) =
            self.connection_context(config, vault, None)?;
        let response = self
            .run_sidecar(
                "describe_table",
                &bundle_path,
                &jdbc_url,
                &username,
                &password,
                &driver_class,
                SidecarQuery {
                    schema,
                    table: Some(table),
                    ..SidecarQuery::default()
                },
                query_timeout(config),
            )
            .await?;
        Ok(response.columns)
    }

    pub async fn execute_readonly(
        &self,
        config: &ConnectionConfig,
        vault: &CredentialVault,
        sql: &str,
        text: &crate::i18n::BackendText,
    ) -> anyhow::Result<(crate::policy::PolicyCheckResult, Option<QueryResult>)> {
        let policy =
            crate::policy::PolicyEngine::check_with_text(&DbKind::Jdbc, sql, config.max_rows, text);
        if !policy.allowed {
            return Ok((policy, None));
        }
        let rewritten = policy
            .rewritten_sql
            .clone()
            .ok_or_else(|| anyhow::anyhow!("policy accepted SQL without a rewritten statement"))?;
        let result = self
            .query(
                config,
                vault,
                &rewritten,
                rewritten.clone(),
                config.max_rows.max(1),
            )
            .await?;
        Ok((policy, Some(result)))
    }

    async fn query(
        &self,
        config: &ConnectionConfig,
        vault: &CredentialVault,
        sql: &str,
        rewritten_sql: String,
        max_rows: u32,
    ) -> anyhow::Result<QueryResult> {
        let (bundle_path, jdbc_url, username, password, driver_class) =
            self.connection_context(config, vault, None)?;
        let started = Instant::now();
        let response = self
            .run_sidecar(
                "query",
                &bundle_path,
                &jdbc_url,
                &username,
                &password,
                &driver_class,
                SidecarQuery {
                    sql: Some(sql),
                    max_rows: Some(max_rows),
                    max_result_bytes: Some(config.max_result_bytes),
                    query_timeout_ms: Some(config.query_timeout_ms),
                    ..SidecarQuery::default()
                },
                query_timeout(config),
            )
            .await?;
        Ok(QueryResult {
            columns: response.column_names,
            rows: response.rows,
            row_count: response.row_count,
            truncated: response.truncated,
            truncation_reason: response.truncation_reason,
            returned_bytes: response.returned_bytes,
            elapsed_ms: started.elapsed().as_millis() as u64,
            rewritten_sql,
        })
    }

    fn connection_context(
        &self,
        config: &ConnectionConfig,
        vault: &CredentialVault,
        password_override: Option<&str>,
    ) -> anyhow::Result<(PathBuf, String, String, String, String)> {
        if config.kind != DbKind::Jdbc {
            return Err(anyhow::anyhow!("Connection is not a JDBC connection"));
        }
        let bundle_id = required(config.jdbc_bundle_id.as_deref(), "JDBC driver")?;
        let jdbc_url = required(config.jdbc_url.as_deref(), "JDBC URL")?;
        let bundle_path = self.bundle_path(bundle_id)?;
        self.ensure_bundle_exists(bundle_id)?;
        let stored_password = if password_override.is_none() {
            match config.credential_ref.as_deref() {
                Some(credential_ref) => vault
                    .get(credential_ref)?
                    .unwrap_or_else(|| Zeroizing::new(String::new())),
                None => Zeroizing::new(String::new()),
            }
        } else {
            Zeroizing::new(String::new())
        };
        let password = password_override
            .unwrap_or(stored_password.as_str())
            .to_string();
        Ok((
            bundle_path,
            jdbc_url.to_string(),
            config.username.clone().unwrap_or_default(),
            password,
            config.jdbc_driver_class.clone().unwrap_or_default(),
        ))
    }

    async fn runtime_status(&self) -> JdbcRuntimeStatus {
        let sidecar_available = self.sidecar_path().is_ok();
        let Ok((java, source)) = self.java_executable() else {
            return JdbcRuntimeStatus {
                available: false,
                source: "unavailable".to_string(),
                java_version: None,
                sidecar_available,
            };
        };
        let java = normalize_java_path(java);
        let mut java_version = None;
        for _ in 0..2 {
            let mut command = Command::new(&java);
            command
                .arg("-version")
                .stdout(Stdio::null())
                .stderr(Stdio::piped());
            configure_hidden_process(&mut command);
            command.kill_on_drop(true);
            let output = timeout(JAVA_PROBE_TIMEOUT, command.output()).await;
            java_version = output
                .ok()
                .and_then(Result::ok)
                .filter(|result| result.status.success())
                .and_then(|result| {
                    String::from_utf8(result.stderr)
                        .ok()
                        .and_then(|text| text.lines().next().map(sanitize_runtime_text))
                });
            if java_version.is_some() {
                break;
            }
        }
        JdbcRuntimeStatus {
            available: sidecar_available,
            source,
            java_version,
            sidecar_available,
        }
    }

    #[allow(clippy::too_many_arguments)]
    async fn run_sidecar(
        &self,
        action: &str,
        bundle_path: &Path,
        jdbc_url: &str,
        username: &str,
        password: &str,
        driver_class: &str,
        query: SidecarQuery<'_>,
        operation_timeout: Duration,
    ) -> anyhow::Result<SidecarResponse> {
        let session_key = jdbc_session_key(bundle_path, jdbc_url, username, password, driver_class);
        for attempt in 0..2 {
            let result = self
                .run_sidecar_session(
                    &session_key,
                    action,
                    bundle_path,
                    jdbc_url,
                    username,
                    password,
                    driver_class,
                    &query,
                    operation_timeout,
                )
                .await;
            match result {
                Ok(response) => return Ok(response),
                Err(error) if retryable_sidecar_error(&error) => {
                    self.invalidate_session(&session_key).await;
                    if attempt == 0 {
                        tokio::time::sleep(Duration::from_millis(150)).await;
                    } else {
                        return Err(error);
                    }
                }
                Err(error) => return Err(error),
            }
        }
        unreachable!("sidecar retry loop must return on every attempt")
    }

    #[allow(clippy::too_many_arguments)]
    async fn run_sidecar_session(
        &self,
        session_key: &str,
        action: &str,
        bundle_path: &Path,
        jdbc_url: &str,
        username: &str,
        password: &str,
        driver_class: &str,
        query: &SidecarQuery<'_>,
        operation_timeout: Duration,
    ) -> anyhow::Result<SidecarResponse> {
        let session = self.get_or_create_session(session_key, bundle_path).await?;
        let request_id = session
            .next_request_id
            .fetch_add(1, Ordering::Relaxed)
            .to_string();
        let bundle_path = normalize_java_path(bundle_path.to_path_buf());
        let bundle_path = bundle_path
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("JDBC driver bundle path is not valid UTF-8"))?;
        let request = SidecarRequest {
            protocol_version: PROTOCOL_VERSION,
            request_id,
            action,
            bundle_path,
            jdbc_url,
            username,
            password,
            driver_class,
            sql: query.sql,
            schema: query.schema,
            table: query.table,
            max_rows: query.max_rows,
            max_result_bytes: query.max_result_bytes,
            query_timeout_ms: query.query_timeout_ms,
            maven_coordinate: query.maven_coordinate,
            maven_repository: query.maven_repository,
            maven_local_repository: query.maven_local_repository,
            maven_output_directory: query.maven_output_directory,
        };
        session.invoke(request, operation_timeout).await
    }

    async fn get_or_create_session(
        &self,
        session_key: &str,
        bundle_path: &Path,
    ) -> anyhow::Result<Arc<JdbcSidecarSession>> {
        let mut sessions = self.sessions.lock().await;
        if let Some(session) = sessions.get(session_key) {
            return Ok(session.clone());
        }
        let (java, _) = self.java_executable()?;
        let java = normalize_java_path(java);
        let sidecar = normalize_java_path(self.sidecar_path()?);
        let mut command = Command::new(&java);
        command
            .arg("-Dfile.encoding=UTF-8")
            .arg("-Dsun.stdout.encoding=UTF-8")
            .arg("-Dsun.stderr.encoding=UTF-8")
            .arg("-jar")
            .arg(&sidecar)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        configure_hidden_process(&mut command);
        command.kill_on_drop(true);
        let mut child = command.spawn().map_err(|error| {
            anyhow::anyhow!(
                "JDBC runtime could not be started: {}",
                sanitize_error(&error.to_string())
            )
        })?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| anyhow::anyhow!("JDBC sidecar stdin was unavailable"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| anyhow::anyhow!("JDBC sidecar stdout was unavailable"))?;
        if let Some(stderr) = child.stderr.take() {
            tokio::spawn(async move {
                let mut reader = BufReader::new(stderr);
                let mut line = String::new();
                while reader.read_line(&mut line).await.unwrap_or(0) > 0 {
                    line.clear();
                }
            });
        }
        let session = Arc::new(JdbcSidecarSession {
            bundle_id: bundle_path
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or("unknown")
                .to_string(),
            process: Mutex::new(JdbcSidecarProcess {
                child,
                stdin,
                stdout: BufReader::new(stdout),
            }),
            next_request_id: AtomicU64::new(1),
        });
        // A successful spawn is enough to publish the session. The first real
        // request is serialized through the same process lock and will surface
        // startup failures to the caller.
        sessions.insert(session_key.to_string(), session.clone());
        Ok(session)
    }

    async fn collect_driver_runtime_info(
        &self,
        drivers: &[JdbcDriverBundle],
    ) -> Vec<JdbcDriverRuntimeInfo> {
        let sessions = self.sessions.lock().await;
        let mut pids_by_bundle = HashMap::<String, Vec<u32>>::new();
        for session in sessions.values() {
            let process = session.process.lock().await;
            if let Some(pid) = process.child.id() {
                pids_by_bundle
                    .entry(session.bundle_id.clone())
                    .or_default()
                    .push(pid);
            }
        }
        let all_pids = pids_by_bundle
            .values()
            .flatten()
            .copied()
            .collect::<HashSet<_>>();
        let stats = collect_process_stats(all_pids).await;
        drivers
            .iter()
            .map(|driver| {
                let pids = pids_by_bundle
                    .get(&driver.bundle_id)
                    .cloned()
                    .unwrap_or_default();
                let mut memory_bytes = 0_u64;
                let mut cpu_percent = 0_f32;
                let mut running = 0_usize;
                for pid in &pids {
                    if let Some(stat) = stats.get(pid) {
                        running += 1;
                        memory_bytes = memory_bytes.saturating_add(stat.memory_bytes);
                        cpu_percent += stat.cpu_percent;
                    }
                }
                let status = if pids.is_empty() {
                    "stopped"
                } else if running == pids.len() {
                    "running"
                } else {
                    "error"
                };
                JdbcDriverRuntimeInfo {
                    bundle_id: driver.bundle_id.clone(),
                    display_name: driver.display_name.clone(),
                    status: status.to_string(),
                    health: if status == "running" {
                        "healthy".to_string()
                    } else {
                        status.to_string()
                    },
                    process_count: running,
                    memory_bytes,
                    cpu_percent,
                }
            })
            .collect()
    }

    async fn invalidate_session(&self, session_key: &str) {
        let session = self.sessions.lock().await.remove(session_key);
        if let Some(session) = session {
            session.shutdown().await;
        }
    }

    #[allow(clippy::too_many_arguments)]
    async fn run_sidecar_once(
        &self,
        action: &str,
        bundle_path: &Path,
        jdbc_url: &str,
        username: &str,
        password: &str,
        driver_class: &str,
        query: &SidecarQuery<'_>,
        operation_timeout: Duration,
    ) -> anyhow::Result<SidecarResponse> {
        let (java, _) = self.java_executable()?;
        let java = normalize_java_path(java);
        let sidecar = normalize_java_path(self.sidecar_path()?);
        let bundle_path = normalize_java_path(bundle_path.to_path_buf());
        let bundle_path = bundle_path
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("JDBC driver bundle path is not valid UTF-8"))?;
        let request_id = Uuid::new_v4().to_string();
        let request = SidecarRequest {
            protocol_version: PROTOCOL_VERSION,
            request_id: request_id.clone(),
            action,
            bundle_path,
            jdbc_url,
            username,
            password,
            driver_class,
            sql: query.sql,
            schema: query.schema,
            table: query.table,
            max_rows: query.max_rows,
            max_result_bytes: query.max_result_bytes,
            query_timeout_ms: query.query_timeout_ms,
            maven_coordinate: query.maven_coordinate,
            maven_repository: query.maven_repository,
            maven_local_repository: query.maven_local_repository,
            maven_output_directory: query.maven_output_directory,
        };
        let mut payload = Zeroizing::new(serde_json::to_vec(&request)?);
        payload.push(b'\n');

        let mut command = Command::new(&java);
        command
            .arg("-Dfile.encoding=UTF-8")
            .arg("-Dsun.stdout.encoding=UTF-8")
            .arg("-Dsun.stderr.encoding=UTF-8")
            .arg("-jar")
            .arg(&sidecar)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        configure_hidden_process(&mut command);
        command.kill_on_drop(true);
        let mut child = command.spawn().map_err(|error| {
            anyhow::anyhow!(
                "JDBC runtime could not be started: {}",
                sanitize_error(&error.to_string())
            )
        })?;
        child
            .stdin
            .take()
            .ok_or_else(|| anyhow::anyhow!("JDBC sidecar stdin was unavailable"))?
            .write_all(payload.as_slice())
            .await?;
        payload.zeroize();

        let output = timeout(operation_timeout, child.wait_with_output())
            .await
            .map_err(|_| anyhow::anyhow!("JDBC operation timed out"))??;
        if output.stdout.len() > MAX_SIDECAR_RESPONSE_BYTES {
            return Err(anyhow::anyhow!(
                "JDBC sidecar response exceeded the size limit"
            ));
        }
        if !output.status.success() && output.stdout.is_empty() {
            return Err(anyhow::anyhow!(
                "JDBC sidecar exited unexpectedly: {} (java={}, sidecar={})",
                sanitize_error(&String::from_utf8_lossy(&output.stderr)),
                java.display(),
                sidecar.display()
            ));
        }
        let response: SidecarResponse = serde_json::from_slice(&output.stdout)
            .map_err(|_| anyhow::anyhow!("JDBC sidecar returned an invalid response"))?;
        if response.protocol_version != PROTOCOL_VERSION || response.request_id != request_id {
            return Err(anyhow::anyhow!("JDBC sidecar protocol validation failed"));
        }
        if !response.ok {
            let error = response.error.unwrap_or(SidecarError {
                code: "sidecar_error".to_string(),
                message: "JDBC operation failed".to_string(),
            });
            return Err(anyhow::anyhow!(
                "JDBC {}: {}",
                sanitize_error(&error.code),
                sanitize_error(&error.message)
            ));
        }
        Ok(response)
    }

    fn drivers_root(&self) -> anyhow::Result<PathBuf> {
        Ok(self
            .app
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("JDBC manager is unavailable in this test context"))?
            .path()
            .app_data_dir()?
            .join("jdbc-drivers"))
    }

    fn bundle_path(&self, bundle_id: &str) -> anyhow::Result<PathBuf> {
        if !valid_bundle_id(bundle_id) {
            return Err(anyhow::anyhow!("Invalid JDBC driver bundle ID"));
        }
        Ok(self.drivers_root()?.join(bundle_id))
    }

    fn java_executable(&self) -> anyhow::Result<(PathBuf, String)> {
        let executable = if cfg!(windows) { "java.exe" } else { "java" };
        if let Some(app) = self.app.as_ref() {
            if let Ok(config) = ConfigStore::new(app).and_then(|store| store.load()) {
                if let Some(home) = config.settings.jdbc_java_home {
                    let configured = resolve_java_path(PathBuf::from(home), executable);
                    if configured.is_file() {
                        return Ok((normalize_java_path(configured), "external".to_string()));
                    }
                    return Err(anyhow::anyhow!(
                        "The selected external Java Runtime is unavailable"
                    ));
                }
            }
            for root in runtime_roots(app) {
                for relative in [
                    ["jdbc-runtime", "bin", executable].as_slice(),
                    ["jdbc-runtime", executable].as_slice(),
                ] {
                    let embedded = normalize_java_path(
                        relative
                            .iter()
                            .fold(root.clone(), |path, part| path.join(part)),
                    );
                    if embedded.is_file() {
                        return Ok((normalize_java_path(embedded), "embedded".to_string()));
                    }
                }
            }
        }
        Err(anyhow::anyhow!(
            "Bundled Java Runtime is unavailable. Prepare the bundled runtime or select an external Java Runtime."
        ))
    }

    fn sidecar_path(&self) -> anyhow::Result<PathBuf> {
        if let Some(path) = std::env::var_os("DATANEXA_JDBC_SIDECAR") {
            let path = PathBuf::from(path);
            if path.is_file() {
                return Ok(normalize_java_path(path));
            }
        }
        if let Some(app) = self.app.as_ref() {
            for root in runtime_roots(app) {
                for relative in [
                    ["jdbc-runtime", "lib", "datanexa-jdbc-sidecar.jar"].as_slice(),
                    ["jdbc-runtime", "datanexa-jdbc-sidecar.jar"].as_slice(),
                ] {
                    let embedded = normalize_java_path(
                        relative
                            .iter()
                            .fold(root.clone(), |path, part| path.join(part)),
                    );
                    if embedded.is_file() {
                        return Ok(normalize_java_path(embedded));
                    }
                }
            }
        }
        let development = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("jdbc-sidecar")
            .join("target")
            .join("datanexa-jdbc-sidecar.jar");
        if development.is_file() {
            return Ok(normalize_java_path(development));
        }
        Err(anyhow::anyhow!(
            "JDBC sidecar is unavailable. Build jdbc-sidecar/pom.xml or prepare the bundled runtime."
        ))
    }
}

impl JdbcSidecarSession {
    async fn invoke(
        &self,
        request: SidecarRequest<'_>,
        operation_timeout: Duration,
    ) -> anyhow::Result<SidecarResponse> {
        let request_id = request.request_id.clone();
        let mut payload = Zeroizing::new(serde_json::to_vec(&request)?);
        payload.push(b'\n');
        let invoke = async {
            let mut process = self.process.lock().await;
            process
                .stdin
                .write_all(payload.as_slice())
                .await
                .map_err(|error| anyhow::anyhow!("JDBC sidecar stdin failed: {error}"))?;
            process
                .stdin
                .flush()
                .await
                .map_err(|error| anyhow::anyhow!("JDBC sidecar stdin flush failed: {error}"))?;
            let mut response_bytes = Vec::new();
            let read = process
                .stdout
                .read_until(b'\n', &mut response_bytes)
                .await
                .map_err(|error| anyhow::anyhow!("JDBC sidecar stdout failed: {error}"))?;
            if read == 0 {
                let status = process
                    .child
                    .try_wait()
                    .ok()
                    .flatten()
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "unknown status".to_string());
                return Err(anyhow::anyhow!(
                    "JDBC sidecar exited unexpectedly: {status}"
                ));
            }
            if response_bytes.len() > MAX_SIDECAR_RESPONSE_BYTES {
                return Err(anyhow::anyhow!(
                    "JDBC sidecar response exceeded the size limit"
                ));
            }
            let response: SidecarResponse = serde_json::from_slice(&response_bytes)
                .map_err(|_| anyhow::anyhow!("JDBC sidecar returned an invalid response"))?;
            if response.protocol_version != PROTOCOL_VERSION || response.request_id != request_id {
                return Err(anyhow::anyhow!("JDBC sidecar protocol validation failed"));
            }
            if !response.ok {
                let error = response.error.unwrap_or(SidecarError {
                    code: "sidecar_error".to_string(),
                    message: "JDBC operation failed".to_string(),
                });
                return Err(anyhow::anyhow!(
                    "JDBC {}: {}",
                    sanitize_error(&error.code),
                    sanitize_error(&error.message)
                ));
            }
            Ok(response)
        };
        let result = timeout(operation_timeout, invoke)
            .await
            .map_err(|_| anyhow::anyhow!("JDBC operation timed out"))?;
        payload.zeroize();
        result
    }

    async fn shutdown(&self) {
        let mut process = self.process.lock().await;
        let _ = process.child.kill().await;
    }
}

fn jdbc_session_key(
    bundle_path: &Path,
    jdbc_url: &str,
    username: &str,
    password: &str,
    driver_class: &str,
) -> String {
    let mut digest = Sha256::new();
    for value in [
        bundle_path.to_string_lossy().as_ref(),
        jdbc_url,
        username,
        password,
        driver_class,
    ] {
        digest.update(value.as_bytes());
        digest.update([0]);
    }
    format!("jdbc-{:x}", digest.finalize())
}

fn retryable_sidecar_error(error: &anyhow::Error) -> bool {
    let message = error.to_string();
    message.contains("JDBC operation timed out")
        || message.contains("JDBC runtime could not be started")
        || message.contains("JDBC sidecar exited unexpectedly")
        || message.contains("JDBC sidecar stdin failed")
        || message.contains("JDBC sidecar stdin flush failed")
        || message.contains("JDBC sidecar stdout failed")
        || message.contains("JDBC sidecar protocol validation failed")
        || message.contains("JDBC sidecar returned an invalid response")
        || message.contains("JDBC connection_lost:")
}

fn runtime_roots(app: &AppHandle) -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Ok(resource_dir) = app.path().resource_dir() {
        roots.push(normalize_java_path(resource_dir));
    }
    if let Ok(executable) = std::env::current_exe() {
        if let Some(parent) = executable.parent() {
            let parent = normalize_java_path(parent.to_path_buf());
            if !roots.iter().any(|root| root == &parent) {
                roots.push(parent);
            }
        }
    }
    roots
}

fn resolve_java_path(path: PathBuf, executable: &str) -> PathBuf {
    if path.is_file() {
        return path;
    }
    path.join("bin").join(executable)
}

#[cfg(windows)]
fn normalize_java_path(path: PathBuf) -> PathBuf {
    let value = path.to_string_lossy();
    if let Some(value) = value.strip_prefix(r"\\?\UNC\") {
        return PathBuf::from(format!(r"\\{value}"));
    }
    if let Some(value) = value.strip_prefix(r"\\?\") {
        return PathBuf::from(value);
    }
    path
}

#[cfg(not(windows))]
fn normalize_java_path(path: PathBuf) -> PathBuf {
    path
}

fn configure_hidden_process(_command: &mut Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        _command.as_std_mut().creation_flags(0x08000000);
    }
}

fn read_manifest(bundle_path: &Path) -> anyhow::Result<JdbcDriverBundle> {
    let bytes = fs::read(bundle_path.join("manifest.json"))?;
    let bundle = serde_json::from_slice::<JdbcDriverBundle>(&bytes)?;
    if bundle.schema_version != 1 || !valid_bundle_id(&bundle.bundle_id) {
        return Err(anyhow::anyhow!("Unsupported JDBC driver bundle manifest"));
    }
    Ok(bundle)
}

fn default_driver_source() -> String {
    "maven".to_string()
}

fn validate_display_name(value: &str) -> anyhow::Result<&str> {
    let value = value.trim();
    if value.is_empty() || value.chars().count() > 80 {
        return Err(anyhow::anyhow!(
            "Driver display name must contain between 1 and 80 characters"
        ));
    }
    Ok(value)
}

fn validate_repository_url(value: Option<&str>) -> anyhow::Result<String> {
    let value = value.unwrap_or(MAVEN_CENTRAL).trim();
    if !(value.starts_with("https://") || value.starts_with("http://localhost"))
        || value.chars().count() > 300
        || value.chars().any(|character| {
            character.is_whitespace() || matches!(character, '<' | '>' | '"' | '\'' | '&')
        })
    {
        return Err(anyhow::anyhow!("Maven repository must be an HTTPS URL"));
    }
    Ok(value.trim_end_matches('/').to_string() + "/")
}

fn collect_local_jar_paths(paths: &[PathBuf]) -> anyhow::Result<Vec<PathBuf>> {
    let mut output = Vec::new();
    for path in paths {
        if !path.exists() {
            return Err(anyhow::anyhow!(
                "JDBC driver path does not exist: {}",
                path.display()
            ));
        }
        let metadata = fs::symlink_metadata(path)?;
        if metadata.is_file() {
            if is_jar_path(path) {
                output.push(path.clone());
            }
            continue;
        }
        if metadata.is_dir() {
            collect_local_jars_recursive(path, &mut output)?;
        }
    }
    output.sort();
    output.dedup();
    if output.is_empty() {
        return Err(anyhow::anyhow!(
            "No .jar files were found in the selected paths"
        ));
    }
    if output.len() > MAX_LOCAL_DRIVER_FILES {
        return Err(anyhow::anyhow!(
            "Local JDBC driver import contains too many JAR files"
        ));
    }
    Ok(output)
}

fn collect_local_jars_recursive(path: &Path, output: &mut Vec<PathBuf>) -> anyhow::Result<()> {
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let entry_path = entry.path();
        let file_type = entry.file_type()?;
        if file_type.is_file() && is_jar_path(&entry_path) {
            output.push(entry_path);
        } else if file_type.is_dir() {
            collect_local_jars_recursive(&entry_path, output)?;
        }
    }
    Ok(())
}

fn is_jar_path(path: &Path) -> bool {
    path.extension()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.eq_ignore_ascii_case("jar"))
}

fn path_size_bytes(path: &Path) -> u64 {
    let Ok(metadata) = fs::symlink_metadata(path) else {
        return 0;
    };
    if metadata.is_file() {
        return metadata.len();
    }
    if !metadata.is_dir() {
        return 0;
    }
    fs::read_dir(path)
        .map(|entries| {
            entries
                .flatten()
                .map(|entry| path_size_bytes(&entry.path()))
                .sum()
        })
        .unwrap_or(0)
}

async fn collect_process_stats(pids: HashSet<u32>) -> HashMap<u32, ProcessStats> {
    if pids.is_empty() {
        return HashMap::new();
    }
    tokio::task::spawn_blocking(move || {
        let mut system = System::new_with_specifics(
            RefreshKind::new().with_processes(ProcessRefreshKind::new().with_cpu().with_memory()),
        );
        system.refresh_processes_specifics(
            ProcessesToUpdate::All,
            true,
            ProcessRefreshKind::new().with_cpu().with_memory(),
        );
        pids.into_iter()
            .filter_map(|pid| {
                system.process(Pid::from_u32(pid)).map(|process| {
                    (
                        pid,
                        ProcessStats {
                            memory_bytes: process.memory(),
                            cpu_percent: process.cpu_usage(),
                        },
                    )
                })
            })
            .collect()
    })
    .await
    .unwrap_or_default()
}

fn verify_bundle_files(bundle_path: &Path, bundle: &JdbcDriverBundle) -> anyhow::Result<()> {
    if bundle.files.is_empty() {
        return Err(anyhow::anyhow!(
            "JDBC driver bundle contains no driver files"
        ));
    }

    let mut total_size = 0_u64;
    let mut expected_files = HashSet::with_capacity(bundle.files.len());
    for file in &bundle.files {
        if file.name.is_empty() || file.name.contains('/') || file.name.contains('\\') {
            return Err(anyhow::anyhow!(
                "JDBC driver bundle contains an invalid file name"
            ));
        }
        if !expected_files.insert(file.name.as_str()) {
            return Err(anyhow::anyhow!(
                "JDBC driver bundle manifest contains duplicate files"
            ));
        }
        let path = bundle_path.join("jars").join(&file.name);
        let metadata = fs::symlink_metadata(&path)
            .map_err(|_| anyhow::anyhow!("JDBC driver bundle is missing file {}", file.name))?;
        if !metadata.is_file() || metadata.len() != file.size {
            return Err(anyhow::anyhow!(
                "JDBC driver bundle file {} has an unexpected size",
                file.name
            ));
        }
        let actual_sha256 = sha256_file(&path)?;
        if !actual_sha256.eq_ignore_ascii_case(&file.sha256) {
            return Err(anyhow::anyhow!(
                "JDBC driver bundle file {} failed SHA-256 verification",
                file.name
            ));
        }
        total_size = total_size.saturating_add(metadata.len());
    }
    let jars_directory = bundle_path.join("jars");
    for entry in fs::read_dir(&jars_directory)
        .map_err(|_| anyhow::anyhow!("JDBC driver bundle does not contain a jars directory"))?
    {
        let entry = entry?;
        if !entry.file_type()?.is_file() {
            return Err(anyhow::anyhow!(
                "JDBC driver bundle contains an unexpected directory entry"
            ));
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if !expected_files.remove(name.as_str()) {
            return Err(anyhow::anyhow!(
                "JDBC driver bundle contains an unlisted file"
            ));
        }
    }
    if !expected_files.is_empty() {
        return Err(anyhow::anyhow!(
            "JDBC driver bundle is missing a manifest file"
        ));
    }
    if total_size != bundle.total_size {
        return Err(anyhow::anyhow!(
            "JDBC driver bundle total size does not match its manifest"
        ));
    }
    Ok(())
}

fn valid_bundle_id(bundle_id: &str) -> bool {
    Uuid::parse_str(bundle_id).is_ok() && bundle_id.len() == 36
}

fn valid_maven_coordinate(coordinate: &str) -> bool {
    Regex::new(r"^[A-Za-z0-9_.-]+:[A-Za-z0-9_.-]+:[A-Za-z0-9_.-]+$")
        .expect("valid Maven coordinate regex")
        .is_match(coordinate)
}

fn sha256_file(path: &Path) -> anyhow::Result<String> {
    let mut file = fs::File::open(path)?;
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }
    Ok(format!("{:x}", digest.finalize()))
}

fn required<'a>(value: Option<&'a str>, field: &str) -> anyhow::Result<&'a str> {
    value
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| anyhow::anyhow!("{field} is required"))
}

fn query_timeout(config: &ConnectionConfig) -> Duration {
    Duration::from_millis(config.query_timeout_ms.clamp(500, 60_000))
}

fn validate_identifier(value: &str) -> anyhow::Result<()> {
    if value.is_empty()
        || value.len() > 128
        || !value.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '_' | '$' | '-')
        })
    {
        return Err(anyhow::anyhow!("invalid table identifier"));
    }
    Ok(())
}

fn sanitize_runtime_text(value: &str) -> String {
    value
        .chars()
        .filter(|value| !value.is_control())
        .take(160)
        .collect::<String>()
}

fn sanitize_error(value: &str) -> String {
    let url_user_info = Regex::new(r"(?i)((?:[A-Za-z][A-Za-z0-9+.-]*:)+//)([^/@\s:]+):([^/@\s]+)@")
        .expect("valid JDBC URL user-info sanitizer regex");
    let value = url_user_info.replace_all(value, "$1$2:[REDACTED]@");
    let secret = Regex::new(r"(?i)(password|passwd|pwd|token|secret)=([^&;\s]+)")
        .expect("valid secret sanitizer regex");
    secret
        .replace_all(&value, "$1=[REDACTED]")
        .chars()
        .filter(|value| !matches!(value, '\r' | '\n' | '\t'))
        .take(600)
        .collect::<String>()
        .trim()
        .to_string()
}
