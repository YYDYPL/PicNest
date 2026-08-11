use std::collections::HashMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub configured: bool,
    pub library_path: String,
    pub source_paths: Vec<String>,
    #[serde(default)]
    pub source_recursive: HashMap<String, bool>,
    pub locale: String,
    pub cloud_ai_enabled: bool,
    pub ai_base_url: String,
    pub vision_model: String,
    pub embedding_model: String,
    pub ai_batch_limit: u32,
    pub api_key_configured: bool,
    #[serde(default)]
    pub telemetry_enabled: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveSettingsInput {
    pub configured: bool,
    pub library_path: String,
    pub source_paths: Vec<String>,
    #[serde(default)]
    pub source_recursive: HashMap<String, bool>,
    pub locale: String,
    pub cloud_ai_enabled: bool,
    pub ai_base_url: String,
    pub vision_model: String,
    pub embedding_model: String,
    pub ai_batch_limit: u32,
    pub api_key_configured: bool,
    pub api_key: Option<String>,
    #[serde(default)]
    pub telemetry_enabled: bool,
}

impl From<SaveSettingsInput> for AppSettings {
    fn from(value: SaveSettingsInput) -> Self {
        let mut source_recursive = value.source_recursive;
        for path in &value.source_paths {
            source_recursive.entry(path.clone()).or_insert(true);
        }
        Self {
            configured: value.configured,
            library_path: value.library_path,
            source_paths: value.source_paths,
            source_recursive,
            locale: value.locale,
            cloud_ai_enabled: value.cloud_ai_enabled,
            ai_base_url: value.ai_base_url,
            vision_model: value.vision_model,
            embedding_model: value.embedding_model,
            ai_batch_limit: value.ai_batch_limit.clamp(1, 100),
            api_key_configured: value.api_key_configured,
            telemetry_enabled: value.telemetry_enabled,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryStats {
    pub total: i64,
    pub inbox: i64,
    pub favorites: i64,
    pub duplicates: i64,
    pub missing: i64,
    pub albums: i64,
    pub storage_bytes: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Album {
    pub id: i64,
    pub name: String,
    pub kind: String,
    pub count: i64,
    pub cover_thumbnail: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivityItem {
    pub id: i64,
    pub kind: String,
    pub title: String,
    pub detail: String,
    pub created_at: String,
    pub reversible: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BootstrapPayload {
    pub settings: AppSettings,
    pub stats: LibraryStats,
    pub albums: Vec<Album>,
    pub recent_activity: Vec<ActivityItem>,
    pub demo_mode: bool,
    pub recovery_jobs: Vec<RecoveryJob>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Asset {
    pub id: i64,
    pub filename: String,
    pub path: String,
    pub thumbnail_data_url: Option<String>,
    pub width: i64,
    pub height: i64,
    pub captured_at: String,
    pub imported_at: String,
    pub file_size: i64,
    pub source: String,
    pub category: String,
    pub favorite: bool,
    pub missing: bool,
    pub needs_organize: bool,
    pub duplicate_count: i64,
    pub similar_count: i64,
    pub content_hash: String,
    pub camera: Option<String>,
    pub location: Option<String>,
    pub description: Option<String>,
    pub ocr_text: Option<String>,
    pub tags: Vec<String>,
    pub album_ids: Vec<i64>,
    pub ai_analyzed: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetQuery {
    pub view: String,
    pub search: Option<String>,
    pub limit: Option<u32>,
    pub cursor: Option<u32>,
    pub album_id: Option<i64>,
    pub date_from: Option<String>,
    pub date_to: Option<String>,
    pub category: Option<String>,
    pub source: Option<String>,
    pub location: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetPage {
    pub items: Vec<Asset>,
    pub next_cursor: Option<u32>,
    pub total: i64,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ScanResult {
    pub discovered: u32,
    pub indexed: u32,
    pub duplicates: u32,
    pub unsupported: u32,
    pub failed: u32,
    pub skipped: u32,
    pub cancelled: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OrganizePlanItem {
    pub asset_id: i64,
    pub filename: String,
    pub source_path: String,
    pub target_path: String,
    pub reason: String,
    pub conflict: bool,
    pub bytes: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OrganizePlan {
    pub id: String,
    pub items: Vec<OrganizePlanItem>,
    pub total_bytes: i64,
    pub conflicts: i64,
    pub required_copy_bytes: i64,
    pub available_bytes: i64,
    pub disk_space_ok: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OrganizeResult {
    pub job_id: String,
    pub moved: u32,
    pub failed: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryJob {
    pub plan_id: String,
    pub moved: i64,
    pub remaining: i64,
    pub failed: i64,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionTestResult {
    pub ok: bool,
    pub latency_ms: u128,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticsResult {
    pub path: String,
    pub bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoveSourcePreviewEntry {
    pub monitored_count: usize,
    pub index_count: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoveSourcePreview {
    pub path: String,
    pub current: RemoveSourcePreviewEntry,
    pub with_subdirs: RemoveSourcePreviewEntry,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoveSourceResult {
    pub removed_paths: Vec<String>,
    pub removed_indexes: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetLocation {
    pub id: i64,
    pub path: String,
    pub source: String,
    pub available: bool,
    pub needs_organize: bool,
    pub file_size: i64,
    pub modified_at: i64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiConnectionInput {
    pub base_url: String,
    pub model: String,
    pub api_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiAnalysis {
    pub description: String,
    pub tags: Vec<String>,
    pub image_type: String,
    pub scene: String,
    pub objects: Vec<String>,
    pub confidence: f64,
    #[serde(default)]
    pub model: String,
}
