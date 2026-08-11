use std::{fs, path::Path};

use chrono::Utc;
use serde_json::json;

use crate::{
    db::{library_stats, list_activity, load_settings, AppState},
    error::{AppError, AppResult},
    models::DiagnosticsResult,
};

pub fn export(state: &AppState, directory: &str) -> AppResult<DiagnosticsResult> {
    let directory = Path::new(directory);
    fs::create_dir_all(directory)?;
    if !directory.is_dir() {
        return Err(AppError::Message("诊断包目标位置不是文件夹".to_string()));
    }

    let connection = state.connection()?;
    let settings = load_settings(&connection)?;
    let stats = library_stats(&connection)?;
    let schema_version: i64 =
        connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
    let database_bytes = fs::metadata(&state.db_path)
        .map(|value| value.len())
        .unwrap_or(0);
    let mut thumbnail_count = 0_u64;
    let mut thumbnail_bytes = 0_u64;
    if let Ok(entries) = fs::read_dir(&state.thumbnail_dir) {
        for entry in entries.flatten() {
            if let Ok(metadata) = entry.metadata() {
                if metadata.is_file() {
                    thumbnail_count += 1;
                    thumbnail_bytes = thumbnail_bytes.saturating_add(metadata.len());
                }
            }
        }
    }
    let recent_activity = list_activity(&connection)?
        .into_iter()
        .take(10)
        .map(|item| {
            json!({
                "kind": item.kind,
                "title": item.title,
                "createdAt": item.created_at,
                "reversible": item.reversible,
            })
        })
        .collect::<Vec<_>>();
    let active_jobs: i64 = connection.query_row(
        "SELECT COUNT(*) FROM organize_plans WHERE status IN ('running', 'partial', 'failed', 'rollback_failed')",
        [],
        |row| row.get(0),
    )?;

    let payload = json!({
        "generatedAt": Utc::now().to_rfc3339(),
        "application": {
            "name": "PicNest",
            "version": env!("CARGO_PKG_VERSION"),
            "platform": std::env::consts::OS,
            "architecture": std::env::consts::ARCH,
        },
        "privacy": {
            "containsPaths": false,
            "containsKeys": false,
            "containsImages": false,
            "telemetryEnabled": settings.telemetry_enabled,
        },
        "configuration": {
            "configured": settings.configured,
            "sourceCount": settings.source_paths.len(),
            "locale": settings.locale,
            "cloudAiEnabled": settings.cloud_ai_enabled,
            "apiKeyConfigured": settings.api_key_configured,
            "visionModel": settings.vision_model,
            "embeddingModel": settings.embedding_model,
            "aiBatchLimit": settings.ai_batch_limit,
        },
        "library": {
            "totalAssets": stats.total,
            "inboxAssets": stats.inbox,
            "favoriteAssets": stats.favorites,
            "duplicateAssets": stats.duplicates,
            "missingAssets": stats.missing,
            "albumCount": stats.albums,
            "logicalBytes": stats.storage_bytes,
        },
        "storage": {
            "schemaVersion": schema_version,
            "databaseBytes": database_bytes,
            "thumbnailCount": thumbnail_count,
            "thumbnailBytes": thumbnail_bytes,
            "activeRecoveryJobs": active_jobs,
        },
        "recentActivity": recent_activity,
    });

    let filename = format!(
        "PicNest-diagnostics-{}.json",
        Utc::now().format("%Y%m%d-%H%M%S")
    );
    let path = directory.join(filename);
    let bytes = serde_json::to_vec_pretty(&payload)?;
    fs::write(&path, &bytes)?;
    Ok(DiagnosticsResult {
        path: path.to_string_lossy().to_string(),
        bytes: bytes.len() as u64,
    })
}
