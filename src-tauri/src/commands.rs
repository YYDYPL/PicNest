use std::{collections::HashSet, fs};

use chrono::Utc;
use rusqlite::params;
use tauri::State;

use crate::{
    ai,
    db::{
        library_stats, list_activity, list_albums, load_settings, store_settings, sync_search_row,
        AppState,
    },
    diagnostics,
    error::{command_error, AppError, AppResult},
    library,
    models::{
        AiAnalysis, AiConnectionInput, AppSettings, AssetLocation, AssetPage, AssetQuery,
        BootstrapPayload, ConnectionTestResult, DiagnosticsResult, OrganizePlan, OrganizeResult,
        SaveSettingsInput, ScanResult,
    },
    ocr, organize,
};

async fn blocking<T, F>(operation: F) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce() -> AppResult<T> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(operation)
        .await
        .map_err(|error| format!("后台任务异常结束：{error}"))?
        .map_err(command_error)
}

#[tauri::command]
pub fn bootstrap(state: State<'_, AppState>) -> Result<BootstrapPayload, String> {
    let connection = state.connection().map_err(command_error)?;
    let mut settings = load_settings(&connection).map_err(command_error)?;
    settings.api_key_configured = ai::api_key_configured();
    Ok(BootstrapPayload {
        settings,
        stats: library_stats(&connection).map_err(command_error)?,
        albums: list_albums(&connection).map_err(command_error)?,
        recent_activity: list_activity(&connection).map_err(command_error)?,
        demo_mode: false,
        recovery_jobs: organize::list_recovery_jobs(state.inner()).map_err(command_error)?,
    })
}

#[tauri::command]
pub async fn list_assets(
    state: State<'_, AppState>,
    query: AssetQuery,
) -> Result<AssetPage, String> {
    let state = state.inner().clone();
    blocking(move || library::list_assets(&state, &query)).await
}

#[tauri::command]
pub async fn scan_paths(
    state: State<'_, AppState>,
    paths: Vec<String>,
) -> Result<ScanResult, String> {
    let state = state.inner().clone();
    blocking(move || library::scan_paths(&state, &paths)).await
}

#[tauri::command]
pub fn cancel_scan(state: State<'_, AppState>) -> bool {
    state.cancel_scan()
}

#[tauri::command]
pub async fn get_asset_thumbnail(
    state: State<'_, AppState>,
    asset_id: i64,
) -> Result<Option<String>, String> {
    let state = state.inner().clone();
    blocking(move || library::asset_image_data_url(&state, asset_id, false)).await
}

#[tauri::command]
pub async fn get_asset_preview(
    state: State<'_, AppState>,
    asset_id: i64,
) -> Result<Option<String>, String> {
    let state = state.inner().clone();
    blocking(move || library::asset_image_data_url(&state, asset_id, true)).await
}

#[tauri::command]
pub async fn create_organize_plan(
    state: State<'_, AppState>,
    asset_ids: Vec<i64>,
) -> Result<OrganizePlan, String> {
    let state = state.inner().clone();
    blocking(move || organize::create_plan(&state, &asset_ids)).await
}

#[tauri::command]
pub async fn apply_organize_plan(
    state: State<'_, AppState>,
    plan_id: String,
) -> Result<OrganizeResult, String> {
    let state = state.inner().clone();
    blocking(move || organize::apply_plan(&state, &plan_id)).await
}

#[tauri::command]
pub async fn undo_last_operation(state: State<'_, AppState>) -> Result<u32, String> {
    let state = state.inner().clone();
    blocking(move || organize::undo_last(&state)).await
}

#[tauri::command]
pub async fn rollback_organize_plan(
    state: State<'_, AppState>,
    plan_id: String,
) -> Result<u32, String> {
    let state = state.inner().clone();
    blocking(move || organize::rollback_plan(&state, &plan_id)).await
}

#[tauri::command]
pub fn set_favorite(
    state: State<'_, AppState>,
    asset_id: i64,
    favorite: bool,
) -> Result<(), String> {
    let connection = state.connection().map_err(command_error)?;
    connection
        .execute(
            "UPDATE assets SET favorite = ?1 WHERE id = ?2",
            params![favorite as i64, asset_id],
        )
        .map_err(AppError::from)
        .map_err(command_error)?;
    Ok(())
}

#[tauri::command]
pub async fn analyze_asset(
    state: State<'_, AppState>,
    asset_id: i64,
) -> Result<AiAnalysis, String> {
    ai::analyze_asset(state.inner(), asset_id)
        .await
        .map_err(command_error)
}

#[tauri::command]
pub async fn test_ai_connection(
    state: State<'_, AppState>,
    input: AiConnectionInput,
) -> Result<ConnectionTestResult, String> {
    ai::test_connection(state.inner(), input)
        .await
        .map_err(command_error)
}

#[tauri::command]
pub async fn clear_ai_results(
    state: State<'_, AppState>,
    asset_ids: Vec<i64>,
) -> Result<u32, String> {
    let state = state.inner().clone();
    blocking(move || ai::clear_analysis(&state, &asset_ids)).await
}

#[tauri::command]
pub fn delete_api_key(state: State<'_, AppState>) -> Result<AppSettings, String> {
    ai::delete_api_key().map_err(command_error)?;
    let connection = state.connection().map_err(command_error)?;
    let mut settings = load_settings(&connection).map_err(command_error)?;
    settings.api_key_configured = false;
    store_settings(&connection, &settings).map_err(command_error)?;
    Ok(settings)
}

#[tauri::command]
pub async fn ocr_asset(state: State<'_, AppState>, asset_id: i64) -> Result<String, String> {
    let state = state.inner().clone();
    blocking(move || ocr::recognize_asset(&state, asset_id)).await
}

#[tauri::command]
pub fn save_settings(
    state: State<'_, AppState>,
    input: SaveSettingsInput,
) -> Result<AppSettings, String> {
    if input.library_path.trim().is_empty() {
        return Err("图库位置不能为空".to_string());
    }
    fs::create_dir_all(input.library_path.trim())
        .map_err(AppError::from)
        .map_err(command_error)?;
    if let Some(api_key) = input
        .api_key
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        ai::store_api_key(api_key).map_err(command_error)?;
    }
    let mut settings: AppSettings = input.into();
    settings.configured = true;
    settings.api_key_configured = ai::api_key_configured();
    let connection = state.connection().map_err(command_error)?;
    store_settings(&connection, &settings).map_err(command_error)?;
    drop(connection);
    state.refresh_watcher().map_err(command_error)?;
    Ok(settings)
}

#[tauri::command]
pub fn create_album(state: State<'_, AppState>, name: String) -> Result<i64, String> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > 80 {
        return Err("相册名称需要包含 1 至 80 个字符".to_string());
    }
    let connection = state.connection().map_err(command_error)?;
    connection
        .execute(
            "INSERT INTO albums(name, kind, created_at) VALUES (?1, 'manual', ?2)",
            params![name, Utc::now().to_rfc3339()],
        )
        .map_err(AppError::from)
        .map_err(command_error)?;
    Ok(connection.last_insert_rowid())
}

#[tauri::command]
pub fn assign_asset_to_album(
    state: State<'_, AppState>,
    album_id: i64,
    asset_id: i64,
) -> Result<(), String> {
    let connection = state.connection().map_err(command_error)?;
    connection
        .execute(
            "INSERT OR IGNORE INTO album_assets(album_id, asset_id) VALUES (?1, ?2)",
            params![album_id, asset_id],
        )
        .map_err(AppError::from)
        .map_err(command_error)?;
    sync_search_row(&connection, asset_id).map_err(command_error)?;
    Ok(())
}

#[tauri::command]
pub fn assign_assets_to_album(
    state: State<'_, AppState>,
    album_id: i64,
    asset_ids: Vec<i64>,
) -> Result<u32, String> {
    if asset_ids.is_empty() {
        return Ok(0);
    }
    let mut connection = state.connection().map_err(command_error)?;
    let album_kind: String = connection
        .query_row(
            "SELECT kind FROM albums WHERE id = ?1",
            params![album_id],
            |row| row.get(0),
        )
        .map_err(AppError::from)
        .map_err(command_error)?;
    if album_kind != "manual" {
        return Err("智能相册由规则自动维护，不能手动添加图片".to_string());
    }
    let transaction = connection
        .transaction()
        .map_err(AppError::from)
        .map_err(command_error)?;
    let mut added = 0_u32;
    for asset_id in &asset_ids {
        added += transaction
            .execute(
                "INSERT OR IGNORE INTO album_assets(album_id, asset_id) VALUES (?1, ?2)",
                params![album_id, asset_id],
            )
            .map_err(AppError::from)
            .map_err(command_error)? as u32;
    }
    transaction
        .commit()
        .map_err(AppError::from)
        .map_err(command_error)?;
    for asset_id in asset_ids {
        sync_search_row(&connection, asset_id).map_err(command_error)?;
    }
    Ok(added)
}

#[tauri::command]
pub fn set_asset_tags(
    state: State<'_, AppState>,
    asset_ids: Vec<i64>,
    tags: Vec<String>,
) -> Result<u32, String> {
    let mut seen = HashSet::new();
    let tags = tags
        .into_iter()
        .map(|tag| tag.trim().to_string())
        .filter(|tag| !tag.is_empty() && tag.chars().count() <= 40)
        .filter(|tag| seen.insert(tag.to_lowercase()))
        .take(40)
        .collect::<Vec<_>>();
    let value = serde_json::to_string(&tags)
        .map_err(AppError::from)
        .map_err(command_error)?;
    let connection = state.connection().map_err(command_error)?;
    let mut updated = 0_u32;
    for asset_id in asset_ids {
        updated += connection
            .execute(
                "UPDATE assets SET tags_json = ?1 WHERE id = ?2",
                params![value, asset_id],
            )
            .map_err(AppError::from)
            .map_err(command_error)? as u32;
        sync_search_row(&connection, asset_id).map_err(command_error)?;
    }
    Ok(updated)
}

#[tauri::command]
pub fn list_asset_locations(
    state: State<'_, AppState>,
    asset_id: i64,
) -> Result<Vec<AssetLocation>, String> {
    let connection = state.connection().map_err(command_error)?;
    let mut statement = connection
        .prepare(
            "SELECT id, path, source, available, needs_organize, file_size, modified_at
             FROM asset_locations WHERE asset_id = ?1 ORDER BY available DESC, id",
        )
        .map_err(AppError::from)
        .map_err(command_error)?;
    let rows = statement
        .query_map(params![asset_id], |row| {
            Ok(AssetLocation {
                id: row.get(0)?,
                path: row.get(1)?,
                source: row.get(2)?,
                available: row.get::<_, i64>(3)? != 0,
                needs_organize: row.get::<_, i64>(4)? != 0,
                file_size: row.get(5)?,
                modified_at: row.get(6)?,
            })
        })
        .map_err(AppError::from)
        .map_err(command_error)?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(AppError::from)
        .map_err(command_error)
}

#[tauri::command]
pub async fn move_duplicate_to_trash(
    state: State<'_, AppState>,
    asset_id: i64,
    path: String,
) -> Result<(), String> {
    let state = state.inner().clone();
    blocking(move || organize::move_duplicate_to_trash(&state, asset_id, &path)).await
}

#[tauri::command]
pub async fn export_diagnostics(
    state: State<'_, AppState>,
    directory: String,
) -> Result<DiagnosticsResult, String> {
    let state = state.inner().clone();
    blocking(move || diagnostics::export(&state, &directory)).await
}
