use std::{
    collections::HashSet,
    fs,
    path::{Component, Path, PathBuf},
};

use chrono::{DateTime, Datelike, Utc};
use fs2::available_space;
use rusqlite::{params, OptionalExtension};
use uuid::Uuid;

use crate::{
    db::{add_activity, load_settings, sync_search_row, AppState},
    error::{AppError, AppResult},
    library::hash_file,
    models::{OrganizePlan, OrganizePlanItem, OrganizeResult, RecoveryJob},
};

pub fn create_plan(state: &AppState, asset_ids: &[i64]) -> AppResult<OrganizePlan> {
    let mut connection = state.connection()?;
    let settings = load_settings(&connection)?;
    let library_root = PathBuf::from(&settings.library_path);
    fs::create_dir_all(&library_root)?;
    let id = Uuid::new_v4().to_string();
    let mut items = Vec::new();
    let mut reserved_targets = HashSet::new();

    for asset_id in asset_ids {
        let asset: Option<(String, String, String, String, i64, String)> = connection
            .query_row(
                "SELECT a.filename, a.captured_at, a.content_hash, a.category, a.file_size,
                        COALESCE((SELECT l.path FROM asset_locations l WHERE l.asset_id = a.id AND l.available = 1 AND l.needs_organize = 1 ORDER BY l.id LIMIT 1), '')
                 FROM assets a WHERE a.id = ?1",
                params![asset_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?)),
            )
            .optional()?;
        let Some((filename, captured_at, content_hash, category, bytes, source_path)) = asset
        else {
            continue;
        };
        if source_path.is_empty() {
            continue;
        }
        let captured = DateTime::parse_from_rfc3339(&captured_at)
            .map(|value| value.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now());
        let safe_name = sanitize_filename(&filename);
        let target_dir = library_root
            .join(captured.year().to_string())
            .join(format!("{:02}", captured.month()));
        let base_target = target_dir.join(safe_name);
        let conflict = path_reserved(&connection, &base_target, &reserved_targets)?;
        let target_path =
            unique_target(&connection, &base_target, &content_hash, &reserved_targets)?;
        reserved_targets.insert(path_key(&target_path));
        let reason = match category.as_str() {
            "screenshot" => "检测到截图特征，按拍摄月份归档".to_string(),
            "wechat" => "来自微信图片目录，按拍摄月份归档".to_string(),
            "download" => "来自下载目录，按拍摄月份归档".to_string(),
            _ => format!(
                "按拍摄时间 {}-{:02} 归档",
                captured.year(),
                captured.month()
            ),
        };
        items.push(OrganizePlanItem {
            asset_id: *asset_id,
            filename,
            source_path,
            target_path: target_path.to_string_lossy().to_string(),
            reason,
            conflict,
            bytes,
        });
    }

    let total_bytes = items.iter().map(|item| item.bytes).sum();
    let conflicts = items.iter().filter(|item| item.conflict).count() as i64;
    let required_copy_bytes = items
        .iter()
        .filter(|item| !same_volume(Path::new(&item.source_path), Path::new(&item.target_path)))
        .map(|item| item.bytes)
        .sum::<i64>();
    let available_bytes = i64::try_from(available_space(&library_root)?).unwrap_or(i64::MAX);
    let disk_space_ok = available_bytes >= required_copy_bytes;
    let transaction = connection.transaction()?;
    transaction.execute(
        "INSERT INTO organize_plans(id, status, total_bytes, created_at) VALUES (?1, 'planned', ?2, ?3)",
        params![id, total_bytes, Utc::now().to_rfc3339()],
    )?;
    for item in &items {
        transaction.execute(
            "INSERT INTO organize_plan_items(plan_id, asset_id, source_path, target_path, reason, bytes, conflict)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![id, item.asset_id, item.source_path, item.target_path, item.reason, item.bytes, item.conflict as i64],
        )?;
    }
    transaction.commit()?;
    Ok(OrganizePlan {
        id,
        items,
        total_bytes,
        conflicts,
        required_copy_bytes,
        available_bytes,
        disk_space_ok,
    })
}

pub fn apply_plan(state: &AppState, plan_id: &str) -> AppResult<OrganizeResult> {
    let connection = state.connection()?;
    let status: Option<String> = connection
        .query_row(
            "SELECT status FROM organize_plans WHERE id = ?1",
            params![plan_id],
            |row| row.get(0),
        )
        .optional()?;
    if !matches!(status.as_deref(), Some("planned" | "partial" | "failed")) {
        return Err(AppError::Message("整理预案不存在或已经执行".to_string()));
    }
    ensure_plan_disk_space(&connection, plan_id)?;
    let allow_resume = status.as_deref() != Some("planned");
    connection.execute(
        "UPDATE organize_plans SET status = 'running' WHERE id = ?1",
        params![plan_id],
    )?;

    let mut statement = connection.prepare(
        "SELECT i.id, i.asset_id, i.source_path, i.target_path, a.content_hash
         FROM organize_plan_items i JOIN assets a ON a.id = i.asset_id
         WHERE i.plan_id = ?1 AND i.status IN ('planned', 'failed') ORDER BY i.id",
    )?;
    let rows = statement.query_map(params![plan_id], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, i64>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, String>(4)?,
        ))
    })?;
    let work = rows.collect::<Result<Vec<_>, _>>()?;
    drop(statement);

    let mut moved = 0_u32;
    let mut failed = 0_u32;
    for (item_id, asset_id, source, target, expected_hash) in work {
        match safe_move_or_resume(
            Path::new(&source),
            Path::new(&target),
            &expected_hash,
            allow_resume,
        ) {
            Ok(()) => {
                connection.execute(
                    "UPDATE asset_locations SET path = ?1, source = 'PicNest 图库', needs_organize = 0, available = 1, last_seen_at = ?2 WHERE asset_id = ?3 AND path = ?4",
                    params![target, Utc::now().to_rfc3339(), asset_id, source],
                )?;
                connection.execute(
                    "UPDATE organize_plan_items SET status = 'moved', error = NULL WHERE id = ?1",
                    params![item_id],
                )?;
                sync_search_row(&connection, asset_id)?;
                moved += 1;
            }
            Err(error) => {
                connection.execute(
                    "UPDATE organize_plan_items SET status = 'failed', error = ?1 WHERE id = ?2",
                    params![error.to_string(), item_id],
                )?;
                failed += 1;
            }
        }
    }

    let remaining: i64 = connection.query_row(
        "SELECT COUNT(*) FROM organize_plan_items WHERE plan_id = ?1 AND status IN ('planned', 'failed')",
        params![plan_id],
        |row| row.get(0),
    )?;
    let total_moved: i64 = connection.query_row(
        "SELECT COUNT(*) FROM organize_plan_items WHERE plan_id = ?1 AND status = 'moved'",
        params![plan_id],
        |row| row.get(0),
    )?;
    let final_status = if remaining == 0 {
        "completed"
    } else if total_moved > 0 {
        "partial"
    } else {
        "failed"
    };
    connection.execute(
        "UPDATE organize_plans SET status = ?1, completed_at = ?2 WHERE id = ?3",
        params![final_status, Utc::now().to_rfc3339(), plan_id],
    )?;
    if total_moved > 0 {
        connection.execute(
            "UPDATE activity SET reversible = 0 WHERE reversible = 1",
            [],
        )?;
    }
    add_activity(
        &connection,
        "organize",
        &format!("整理了 {moved} 张图片"),
        if failed == 0 {
            "已按拍摄月份归档到 PicNest 图库"
        } else {
            "部分文件未移动，原文件仍保留"
        },
        total_moved > 0,
        Some(plan_id),
    )?;
    Ok(OrganizeResult {
        job_id: plan_id.to_string(),
        moved,
        failed,
    })
}

fn ensure_plan_disk_space(connection: &rusqlite::Connection, plan_id: &str) -> AppResult<()> {
    let settings = load_settings(connection)?;
    let mut statement = connection.prepare(
        "SELECT source_path, target_path, bytes FROM organize_plan_items
         WHERE plan_id = ?1 AND status IN ('planned', 'failed')",
    )?;
    let rows = statement
        .query_map(params![plan_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let required = rows
        .iter()
        .filter(|(source, target, _)| {
            !same_volume(Path::new(source), Path::new(target)) && !Path::new(target).exists()
        })
        .map(|(_, _, bytes)| *bytes)
        .sum::<i64>();
    let available =
        i64::try_from(available_space(Path::new(&settings.library_path))?).unwrap_or(i64::MAX);
    if available < required {
        return Err(AppError::Message(format!(
            "图库磁盘空间不足：还需要约 {} MB",
            (required - available + 1_048_575) / 1_048_576
        )));
    }
    Ok(())
}

fn safe_move_or_resume(
    source: &Path,
    target: &Path,
    expected_hash: &str,
    allow_resume: bool,
) -> AppResult<()> {
    if !target.exists() {
        return safe_move(source, target, expected_hash);
    }
    if !allow_resume {
        return Err(AppError::Message("目标位置已经存在同名文件".to_string()));
    }
    if hash_file(target)? != expected_hash {
        return Err(AppError::Message(
            "中断任务的目标文件内容不匹配，操作已停止".to_string(),
        ));
    }
    if source.exists() {
        if hash_file(source)? != expected_hash {
            return Err(AppError::Message(
                "中断任务的源文件内容已变化，操作已停止".to_string(),
            ));
        }
        fs::remove_file(source)?;
    }
    Ok(())
}

pub fn recover_interrupted_jobs(state: &AppState) -> AppResult<()> {
    let connection = state.connection()?;
    connection.execute(
        "UPDATE organize_plans SET status = 'partial' WHERE status = 'running'",
        [],
    )?;
    let mut plan_statement = connection.prepare(
        "SELECT id FROM organize_plans WHERE status IN ('partial', 'failed', 'rollback_failed')",
    )?;
    let plan_ids = plan_statement
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    drop(plan_statement);

    for plan_id in plan_ids {
        let mut statement = connection.prepare(
            "SELECT i.id, i.asset_id, i.source_path, i.target_path, i.status, a.content_hash
             FROM organize_plan_items i JOIN assets a ON a.id = i.asset_id
             WHERE i.plan_id = ?1",
        )?;
        let items = statement
            .query_map(params![plan_id], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        drop(statement);

        for (item_id, asset_id, source, target, item_status, expected_hash) in items {
            let source_path = Path::new(&source);
            let target_path = Path::new(&target);
            let recovered_status = match (source_path.exists(), target_path.exists()) {
                (false, true) if hash_file(target_path).ok().as_deref() == Some(&expected_hash) => {
                    connection.execute(
                        "UPDATE asset_locations SET path = ?1, source = 'PicNest 图库', needs_organize = 0, available = 1, last_seen_at = ?2 WHERE asset_id = ?3 AND path = ?4",
                        params![target, Utc::now().to_rfc3339(), asset_id, source],
                    )?;
                    sync_search_row(&connection, asset_id)?;
                    "moved"
                }
                (true, false) => "planned",
                (true, true)
                    if hash_file(source_path).ok().as_deref() == Some(&expected_hash)
                        && hash_file(target_path).ok().as_deref() == Some(&expected_hash) =>
                {
                    "planned"
                }
                _ if item_status == "moved" => "failed",
                _ => "failed",
            };
            connection.execute(
                "UPDATE organize_plan_items SET status = ?1 WHERE id = ?2",
                params![recovered_status, item_id],
            )?;
        }

        let (moved, remaining, failed): (i64, i64, i64) = connection.query_row(
            "SELECT
                SUM(CASE WHEN status = 'moved' THEN 1 ELSE 0 END),
                SUM(CASE WHEN status = 'planned' THEN 1 ELSE 0 END),
                SUM(CASE WHEN status = 'failed' THEN 1 ELSE 0 END)
             FROM organize_plan_items WHERE plan_id = ?1",
            params![plan_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )?;
        let status = if remaining + failed == 0 {
            "completed"
        } else if moved > 0 {
            "partial"
        } else {
            "failed"
        };
        connection.execute(
            "UPDATE organize_plans SET status = ?1 WHERE id = ?2",
            params![status, plan_id],
        )?;
    }
    Ok(())
}

pub fn list_recovery_jobs(state: &AppState) -> AppResult<Vec<RecoveryJob>> {
    let connection = state.connection()?;
    let mut statement = connection.prepare(
        "SELECT p.id,
                SUM(CASE WHEN i.status = 'moved' THEN 1 ELSE 0 END),
                SUM(CASE WHEN i.status = 'planned' THEN 1 ELSE 0 END),
                SUM(CASE WHEN i.status = 'failed' THEN 1 ELSE 0 END),
                p.created_at
         FROM organize_plans p JOIN organize_plan_items i ON i.plan_id = p.id
         WHERE p.status IN ('partial', 'failed', 'rollback_failed')
         GROUP BY p.id ORDER BY p.created_at DESC",
    )?;
    let rows = statement.query_map([], |row| {
        Ok(RecoveryJob {
            plan_id: row.get(0)?,
            moved: row.get(1)?,
            remaining: row.get(2)?,
            failed: row.get(3)?,
            created_at: row.get(4)?,
        })
    })?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

pub fn undo_last(state: &AppState) -> AppResult<u32> {
    let connection = state.connection()?;
    let plan_id: Option<String> = connection
        .query_row(
            "SELECT plan_id FROM activity WHERE reversible = 1 AND plan_id IS NOT NULL ORDER BY id DESC LIMIT 1",
            [],
            |row| row.get(0),
        )
        .optional()?;
    let Some(plan_id) = plan_id else {
        return Ok(0);
    };
    drop(connection);
    rollback_plan_internal(state, &plan_id, "undo")
}

pub fn rollback_plan(state: &AppState, plan_id: &str) -> AppResult<u32> {
    rollback_plan_internal(state, plan_id, "recovery")
}

fn rollback_plan_internal(state: &AppState, plan_id: &str, kind: &str) -> AppResult<u32> {
    let connection = state.connection()?;
    let mut statement = connection.prepare(
        "SELECT i.id, i.asset_id, i.source_path, i.target_path, i.status, a.content_hash
         FROM organize_plan_items i JOIN assets a ON a.id = i.asset_id
         WHERE i.plan_id = ?1 AND i.status IN ('moved', 'planned', 'failed') ORDER BY i.id DESC",
    )?;
    let work = statement
        .query_map(params![plan_id], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    drop(statement);

    let mut restored = 0_u32;
    let mut failures = 0_u32;
    for (item_id, asset_id, original, current, _status, expected_hash) in work {
        let source = Path::new(&original);
        let target = Path::new(&current);
        let outcome = match (source.exists(), target.exists()) {
            (true, false) if hash_file(source).ok().as_deref() == Some(&expected_hash) => Ok(()),
            (false, true) => safe_move(target, source, &expected_hash),
            (true, true)
                if hash_file(source).ok().as_deref() == Some(&expected_hash)
                    && hash_file(target).ok().as_deref() == Some(&expected_hash) =>
            {
                trash::delete(target).map_err(|error| {
                    AppError::Message(format!("无法将中断复制件移入回收站：{error}"))
                })
            }
            _ => Err(AppError::Message(
                "源文件或目标文件内容已变化，回滚已停止".to_string(),
            )),
        };

        match outcome {
            Ok(()) => {
                connection.execute(
                    "UPDATE asset_locations SET path = ?1, needs_organize = 1, available = 1, last_seen_at = ?2 WHERE asset_id = ?3 AND path = ?4",
                    params![original, Utc::now().to_rfc3339(), asset_id, current],
                )?;
                connection.execute(
                    "UPDATE organize_plan_items SET status = 'undone', error = NULL WHERE id = ?1",
                    params![item_id],
                )?;
                sync_search_row(&connection, asset_id)?;
                restored += 1;
            }
            Err(error) => {
                connection.execute(
                    "UPDATE organize_plan_items SET status = 'failed', error = ?1 WHERE id = ?2",
                    params![error.to_string(), item_id],
                )?;
                failures += 1;
            }
        }
    }
    let final_status = if failures == 0 {
        "undone"
    } else {
        "rollback_failed"
    };
    connection.execute(
        "UPDATE organize_plans SET status = ?1 WHERE id = ?2",
        params![final_status, plan_id],
    )?;
    connection.execute(
        "UPDATE activity SET reversible = 0 WHERE plan_id = ?1",
        params![plan_id],
    )?;
    add_activity(
        &connection,
        "undo",
        &format!(
            "{} {restored} 张图片的整理",
            if kind == "undo" {
                "撤销了"
            } else {
                "回滚了"
            }
        ),
        if failures == 0 {
            "图片已恢复到原位置"
        } else {
            "部分文件内容发生变化，相关副本均已保留"
        },
        false,
        Some(plan_id),
    )?;
    Ok(restored)
}

pub fn move_duplicate_to_trash(state: &AppState, asset_id: i64, path: &str) -> AppResult<()> {
    let connection = state.connection()?;
    let belongs: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM asset_locations WHERE asset_id = ?1 AND path = ?2 AND available = 1)",
        params![asset_id, path],
        |row| row.get::<_, i64>(0).map(|value| value != 0),
    )?;
    if !belongs {
        return Err(AppError::Message("文件不属于所选重复组".to_string()));
    }
    let available_count: i64 = connection.query_row(
        "SELECT COUNT(*) FROM asset_locations WHERE asset_id = ?1 AND available = 1",
        params![asset_id],
        |row| row.get(0),
    )?;
    if available_count <= 1 {
        return Err(AppError::Message(
            "这是该图片最后一个可用副本，不能移入回收站".to_string(),
        ));
    }
    trash::delete(path)
        .map_err(|error| AppError::Message(format!("无法移入系统回收站：{error}")))?;
    connection.execute(
        "UPDATE asset_locations SET available = 0 WHERE asset_id = ?1 AND path = ?2",
        params![asset_id, path],
    )?;
    sync_search_row(&connection, asset_id)?;
    Ok(())
}

fn safe_move(source: &Path, target: &Path, expected_hash: &str) -> AppResult<()> {
    if !source.exists() {
        return Err(AppError::Message("源文件不存在".to_string()));
    }
    if target.exists() {
        return Err(AppError::Message("目标位置已经存在同名文件".to_string()));
    }
    if hash_file(source)? != expected_hash {
        return Err(AppError::Message(
            "源文件内容已发生变化，操作已停止".to_string(),
        ));
    }
    let parent = target
        .parent()
        .ok_or_else(|| AppError::Message("目标路径无效".to_string()))?;
    fs::create_dir_all(parent)?;

    if same_volume(source, target) && fs::rename(source, target).is_ok() {
        return Ok(());
    }

    let temp = parent.join(format!(".picnest-partial-{}", Uuid::new_v4()));
    if let Err(error) = copy_verify(source, &temp, expected_hash) {
        let _ = fs::remove_file(&temp);
        return Err(error);
    }
    fs::rename(&temp, target)?;
    if let Err(error) = fs::remove_file(source) {
        let _ = fs::remove_file(target);
        return Err(AppError::Io(error));
    }
    Ok(())
}

fn copy_verify(source: &Path, target: &Path, expected_hash: &str) -> AppResult<()> {
    fs::copy(source, target)?;
    if hash_file(target)? != expected_hash {
        return Err(AppError::Message("复制后的内容校验失败".to_string()));
    }
    Ok(())
}

fn same_volume(left: &Path, right: &Path) -> bool {
    fn prefix(path: &Path) -> Option<String> {
        path.components().find_map(|component| match component {
            Component::Prefix(value) => Some(value.as_os_str().to_string_lossy().to_lowercase()),
            _ => None,
        })
    }
    prefix(left) == prefix(right)
}

fn sanitize_filename(filename: &str) -> String {
    let cleaned: String = filename
        .chars()
        .map(|character| match character {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '_',
            _ => character,
        })
        .collect();
    let cleaned = cleaned.trim().trim_end_matches(['.', ' ']);
    if cleaned.is_empty() {
        "untitled-image".to_string()
    } else if is_windows_reserved_name(cleaned) {
        format!("_{cleaned}")
    } else {
        cleaned.to_string()
    }
}

fn is_windows_reserved_name(filename: &str) -> bool {
    let stem = Path::new(filename)
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or(filename)
        .to_ascii_uppercase();
    matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (stem.len() == 4
            && (stem.starts_with("COM") || stem.starts_with("LPT"))
            && stem
                .chars()
                .last()
                .is_some_and(|value| ('1'..='9').contains(&value)))
}

fn path_key(path: &Path) -> String {
    path.to_string_lossy().replace('/', "\\").to_lowercase()
}

fn path_reserved(
    connection: &rusqlite::Connection,
    path: &Path,
    reserved: &HashSet<String>,
) -> AppResult<bool> {
    if reserved.contains(&path_key(path)) {
        return Ok(true);
    }
    if path.exists() {
        return Ok(true);
    }
    let value = path.to_string_lossy();
    Ok(connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM asset_locations WHERE LOWER(path) = LOWER(?1))",
        params![value.as_ref()],
        |row| row.get::<_, i64>(0).map(|value| value != 0),
    )?)
}

fn unique_target(
    connection: &rusqlite::Connection,
    base: &Path,
    hash: &str,
    reserved: &HashSet<String>,
) -> AppResult<PathBuf> {
    if !path_reserved(connection, base, reserved)? {
        return Ok(base.to_path_buf());
    }
    let parent = base.parent().unwrap_or_else(|| Path::new(""));
    let stem = base
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("image");
    let extension = base.extension().and_then(|value| value.to_str());
    for index in 0..10_000 {
        let suffix = if index == 0 {
            hash.chars().take(8).collect::<String>()
        } else {
            format!("{}-{index}", hash.chars().take(8).collect::<String>())
        };
        let filename = match extension {
            Some(extension) => format!("{stem}_{suffix}.{extension}"),
            None => format!("{stem}_{suffix}"),
        };
        let candidate = parent.join(filename);
        if !path_reserved(connection, &candidate, reserved)? {
            return Ok(candidate);
        }
    }
    Err(AppError::Message("无法生成不冲突的目标文件名".to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;

    #[test]
    fn sanitizes_windows_reserved_characters() {
        assert_eq!(sanitize_filename("trip:day/one?.jpg"), "trip_day_one_.jpg");
        assert_eq!(sanitize_filename("CON.jpg"), "_CON.jpg");
    }

    #[test]
    fn compares_windows_volume_prefixes() {
        assert!(same_volume(
            Path::new(r"D:\\one.jpg"),
            Path::new(r"D:\\Photos\\one.jpg")
        ));
        assert!(!same_volume(
            Path::new(r"C:\\one.jpg"),
            Path::new(r"D:\\Photos\\one.jpg")
        ));
    }

    #[test]
    fn verified_move_keeps_content_intact() {
        let directory = tempfile::tempdir().expect("temp directory");
        let source = directory.path().join("source.jpg");
        let target = directory.path().join("archive").join("target.jpg");
        let mut file = File::create(&source).expect("create source");
        file.write_all(b"picnest-photo-fixture")
            .expect("write fixture");
        drop(file);
        let expected_hash = hash_file(&source).expect("hash source");

        safe_move(&source, &target, &expected_hash).expect("safe move");

        assert!(!source.exists());
        assert!(target.exists());
        assert_eq!(hash_file(&target).expect("hash target"), expected_hash);
    }

    #[test]
    fn resume_finishes_only_a_verified_copy() {
        let directory = tempfile::tempdir().expect("temp directory");
        let source = directory.path().join("source.jpg");
        let target = directory.path().join("target.jpg");
        fs::write(&source, b"verified-photo").expect("source");
        fs::write(&target, b"verified-photo").expect("target");
        let expected_hash = hash_file(&source).expect("hash");

        safe_move_or_resume(&source, &target, &expected_hash, true).expect("resume");

        assert!(!source.exists());
        assert_eq!(hash_file(&target).expect("target hash"), expected_hash);
    }

    #[test]
    fn reserved_targets_do_not_collide_within_one_plan() {
        let connection = rusqlite::Connection::open_in_memory().expect("database");
        connection
            .execute_batch("CREATE TABLE asset_locations(path TEXT NOT NULL);")
            .expect("schema");
        let base = Path::new(r"D:\\Photos\\2026\\08\\IMG_1.jpg");
        let mut reserved = HashSet::new();
        let first = unique_target(&connection, base, "abcdef012345", &reserved).expect("first");
        reserved.insert(path_key(&first));
        let second = unique_target(&connection, base, "abcdef012345", &reserved).expect("second");

        assert_eq!(first, base);
        assert_ne!(first, second);
        assert!(second.to_string_lossy().contains("abcdef01"));
    }
}
