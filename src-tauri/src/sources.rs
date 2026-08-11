use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

use rusqlite::{params, Connection};

use crate::{
    db::{add_activity, load_settings, sync_search_row, write_settings, AppState},
    error::{AppError, AppResult},
    library::path_starts_with,
    models::{AppSettings, RemoveSourcePreview, RemoveSourcePreviewEntry, RemoveSourceResult},
};

#[derive(Debug, Clone)]
pub struct SourceRule {
    pub path: String,
    pub recursive: bool,
}

pub fn source_rules(settings: &AppSettings) -> Vec<SourceRule> {
    settings
        .source_paths
        .iter()
        .map(|path| SourceRule {
            path: path.clone(),
            recursive: settings.source_recursive.get(path).copied().unwrap_or(true),
        })
        .collect()
}

pub fn path_in_scope(path: &Path, root: &Path, recursive: bool) -> bool {
    if !path_starts_with(path, root) {
        return false;
    }
    if recursive {
        return true;
    }
    let path = normalized(path.to_string_lossy().as_ref());
    let root = normalized(root.to_string_lossy().as_ref());
    let rest = path.strip_prefix(&root).unwrap_or_default();
    let rest = rest.trim_start_matches(['\\', '/']);
    !rest.is_empty() && !rest.contains('\\') && !rest.contains('/')
}

pub fn preview_remove_source(state: &AppState, path: &str) -> AppResult<RemoveSourcePreview> {
    let connection = state.connection()?;
    let settings = load_settings(&connection)?;
    preview_remove(&connection, &settings, path)
}

pub fn remove_source(
    state: &AppState,
    path: &str,
    include_subdirs: bool,
) -> AppResult<RemoveSourceResult> {
    let connection = state.connection()?;
    let mut settings = load_settings(&connection)?;
    let result = remove_sources(&connection, &mut settings, path, include_subdirs)?;
    drop(connection);
    state.refresh_watcher()?;
    Ok(result)
}

fn preview_remove(
    connection: &Connection,
    settings: &AppSettings,
    path: &str,
) -> AppResult<RemoveSourcePreview> {
    ensure_source_exists(settings, path)?;
    let current_paths = collect_removed_roots(settings, path, false);
    let with_subdirs = collect_removed_roots(settings, path, true);
    let current = preview_entry(connection, settings, &current_paths)?;
    let with_subdirs = preview_entry(connection, settings, &with_subdirs)?;
    Ok(RemoveSourcePreview {
        path: path.to_string(),
        current,
        with_subdirs,
    })
}

fn preview_entry(
    connection: &Connection,
    settings: &AppSettings,
    removed_paths: &[String],
) -> AppResult<RemoveSourcePreviewEntry> {
    let removed_rules = rules_for_paths(settings, removed_paths);
    let removed_set = normalized_set(removed_paths);
    let remaining_paths = settings
        .source_paths
        .iter()
        .filter(|path| !removed_set.contains(&normalized(path)))
        .cloned()
        .collect::<Vec<_>>();
    let remaining_rules = rules_for_paths(settings, &remaining_paths);
    Ok(RemoveSourcePreviewEntry {
        monitored_count: removed_paths.len(),
        index_count: count_indexed_locations(connection, &removed_rules, &remaining_rules)?,
    })
}

fn remove_sources(
    connection: &Connection,
    settings: &mut AppSettings,
    path: &str,
    include_subdirs: bool,
) -> AppResult<RemoveSourceResult> {
    ensure_source_exists(settings, path)?;
    let removed_paths = collect_removed_roots(settings, path, include_subdirs);
    let removed_rules = rules_for_paths(settings, &removed_paths);
    let removed_set = normalized_set(&removed_paths);
    let remaining_paths = settings
        .source_paths
        .iter()
        .filter(|path| !removed_set.contains(&normalized(path)))
        .cloned()
        .collect::<Vec<_>>();
    let remaining_rules = rules_for_paths(settings, &remaining_paths);

    let transaction = connection.unchecked_transaction()?;
    let mut statement = transaction.prepare("SELECT id, asset_id, path FROM asset_locations")?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    drop(statement);

    let mut removed_indexes = 0_i64;
    let mut affected_assets = HashSet::<i64>::new();
    for (location_id, asset_id, location_path) in rows {
        let covered_by_removed = removed_rules.iter().any(|rule| {
            path_in_scope(
                Path::new(&location_path),
                Path::new(&rule.path),
                rule.recursive,
            )
        });
        let covered_by_remaining = remaining_rules.iter().any(|rule| {
            path_in_scope(
                Path::new(&location_path),
                Path::new(&rule.path),
                rule.recursive,
            )
        });
        if covered_by_removed && !covered_by_remaining {
            transaction.execute(
                "DELETE FROM asset_locations WHERE id = ?1",
                params![location_id],
            )?;
            removed_indexes += 1;
            affected_assets.insert(asset_id);
        }
    }

    for asset_id in affected_assets {
        let remaining_locations: i64 = transaction.query_row(
            "SELECT COUNT(*) FROM asset_locations WHERE asset_id = ?1",
            params![asset_id],
            |row| row.get(0),
        )?;
        if remaining_locations == 0 {
            transaction.execute(
                "DELETE FROM asset_search WHERE asset_id = ?1",
                params![asset_id],
            )?;
            transaction.execute("DELETE FROM assets WHERE id = ?1", params![asset_id])?;
        } else {
            sync_search_row(&transaction, asset_id)?;
        }
    }

    settings
        .source_paths
        .retain(|candidate| !removed_set.contains(&normalized(candidate)));
    settings
        .source_recursive
        .retain(|candidate, _| !removed_set.contains(&normalized(candidate)));
    write_settings(&transaction, settings)?;
    add_activity(
        &transaction,
        "source",
        "移除了监控文件夹",
        &format!(
            "移除了 {} 个监控目录，清理 {} 条本地索引",
            removed_paths.len(),
            removed_indexes
        ),
        false,
        None,
    )?;
    transaction.commit()?;

    Ok(RemoveSourceResult {
        removed_paths,
        removed_indexes,
    })
}

fn ensure_source_exists(settings: &AppSettings, path: &str) -> AppResult<()> {
    if settings
        .source_paths
        .iter()
        .any(|candidate| paths_equal(candidate, path))
    {
        Ok(())
    } else {
        Err(AppError::Message("该文件夹不在监控列表中".to_string()))
    }
}

fn collect_removed_roots(settings: &AppSettings, path: &str, include_subdirs: bool) -> Vec<String> {
    let root = PathBuf::from(path);
    settings
        .source_paths
        .iter()
        .filter(|candidate| {
            let candidate_path = Path::new(candidate);
            paths_equal(candidate, path)
                || (include_subdirs && path_starts_with(candidate_path, &root))
        })
        .cloned()
        .collect()
}

fn rules_for_paths(settings: &AppSettings, paths: &[String]) -> Vec<SourceRule> {
    let set = normalized_set(paths);
    source_rules(settings)
        .into_iter()
        .filter(|rule| set.contains(&normalized(&rule.path)))
        .collect()
}

fn count_indexed_locations(
    connection: &Connection,
    removed_rules: &[SourceRule],
    remaining_rules: &[SourceRule],
) -> AppResult<i64> {
    let mut statement = connection.prepare("SELECT path FROM asset_locations")?;
    let rows = statement
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows
        .iter()
        .filter(|path| {
            let path = Path::new(path);
            let covered_by_removed = removed_rules
                .iter()
                .any(|rule| path_in_scope(path, Path::new(&rule.path), rule.recursive));
            let covered_by_remaining = remaining_rules
                .iter()
                .any(|rule| path_in_scope(path, Path::new(&rule.path), rule.recursive));
            covered_by_removed && !covered_by_remaining
        })
        .count() as i64)
}

fn paths_equal(left: &str, right: &str) -> bool {
    normalized(left) == normalized(right)
}

fn normalized_set(paths: &[String]) -> HashSet<String> {
    paths
        .iter()
        .map(|path| normalized(path))
        .collect::<HashSet<_>>()
}

fn normalized(path: &str) -> String {
    path.trim_end_matches(['\\', '/']).to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn settings_with(root: &str, child: &str) -> AppSettings {
        let mut source_recursive = HashMap::new();
        source_recursive.insert(root.to_string(), true);
        source_recursive.insert(child.to_string(), true);
        AppSettings {
            configured: true,
            library_path: "C:\\Library".to_string(),
            source_paths: vec![root.to_string(), child.to_string()],
            source_recursive,
            locale: "zh-CN".to_string(),
            cloud_ai_enabled: false,
            ai_base_url: String::new(),
            vision_model: String::new(),
            embedding_model: String::new(),
            ai_batch_limit: 20,
            api_key_configured: false,
            telemetry_enabled: false,
        }
    }

    fn insert_asset(connection: &Connection, id: i64, hash: &str, path: &str, root: &str) {
        connection
            .execute(
                "INSERT INTO assets(
                    id, content_hash, filename, width, height, captured_at,
                    imported_at, file_size, category, tags_json
                 ) VALUES (?1, ?2, ?3, 0, 0, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z', 1, 'other', '[]')",
                params![id, hash, "image.jpg"],
            )
            .expect("insert asset");
        connection
            .execute(
                "INSERT INTO asset_locations(
                    asset_id, path, source, available, needs_organize,
                    last_seen_at, file_size, modified_at, root_path
                 ) VALUES (?1, ?2, ?3, 1, 1, '2026-01-01T00:00:00Z', 1, 0, ?4)",
                params![id, path, "测试来源", root],
            )
            .expect("insert location");
    }

    #[test]
    fn source_scope_handles_recursive_and_immediate_paths() {
        let root = Path::new(r"D:\src");
        assert!(path_in_scope(Path::new(r"D:\src\a.jpg"), root, false));
        assert!(!path_in_scope(Path::new(r"D:\src\sub\a.jpg"), root, false));
        assert!(path_in_scope(Path::new(r"D:\src\sub\a.jpg"), root, true));
        assert!(!path_in_scope(Path::new(r"D:\src-old\a.jpg"), root, true));
    }

    #[test]
    fn preview_and_removal_cover_selected_scope() {
        let directory = tempfile::tempdir().expect("temp directory");
        let path = directory.path().join("picnest.db");
        let connection = Connection::open(&path).expect("database");
        crate::db::migrate(&connection, &path, false).expect("migrate");

        let root = r"D:\src";
        let child = r"D:\src\child";
        let settings = settings_with(root, child);
        insert_asset(&connection, 1, "hash-1", r"D:\src\a.jpg", root);
        insert_asset(&connection, 2, "hash-2", r"D:\src\child\b.jpg", child);
        insert_asset(&connection, 3, "hash-3", r"D:\src\other\c.jpg", root);

        let preview = preview_remove(&connection, &settings, root).expect("preview");
        assert_eq!(preview.current.monitored_count, 1);
        assert_eq!(preview.current.index_count, 2);
        assert_eq!(preview.with_subdirs.monitored_count, 2);
        assert_eq!(preview.with_subdirs.index_count, 3);

        let mut settings = settings;
        let result = remove_sources(&connection, &mut settings, root, true).expect("remove");
        assert_eq!(result.removed_paths.len(), 2);
        assert_eq!(result.removed_indexes, 3);
        assert!(settings.source_paths.is_empty());
        let remaining: i64 = connection
            .query_row("SELECT COUNT(*) FROM assets", [], |row| row.get(0))
            .expect("count assets");
        assert_eq!(remaining, 0);
    }

    #[test]
    fn removal_protects_indexes_owned_by_overlapping_child_source() {
        let directory = tempfile::tempdir().expect("temp directory");
        let path = directory.path().join("picnest.db");
        let connection = Connection::open(&path).expect("database");
        crate::db::migrate(&connection, &path, false).expect("migrate");

        let root = r"D:\src";
        let child = r"D:\src\child";
        let mut settings = settings_with(root, child);
        settings.source_recursive.insert(child.to_string(), false);
        insert_asset(&connection, 1, "hash-1", r"D:\src\child\b.jpg", child);

        let preview = preview_remove(&connection, &settings, root).expect("preview");
        assert_eq!(preview.current.index_count, 0);
        assert_eq!(preview.with_subdirs.index_count, 1);

        let result = remove_sources(&connection, &mut settings, root, false).expect("remove");
        assert_eq!(result.removed_indexes, 0);
        assert_eq!(settings.source_paths, vec![child.to_string()]);
        let remaining_locations: i64 = connection
            .query_row("SELECT COUNT(*) FROM asset_locations", [], |row| row.get(0))
            .expect("count locations");
        assert_eq!(remaining_locations, 1);
    }

    #[test]
    fn removal_preserves_organized_photos_and_cleans_orphans() {
        let directory = tempfile::tempdir().expect("temp directory");
        let path = directory.path().join("picnest.db");
        let connection = Connection::open(&path).expect("database");
        crate::db::migrate(&connection, &path, false).expect("migrate");

        let root = r"D:\src";
        let mut settings = settings_with(root, r"D:\unused");
        settings.source_paths = vec![root.to_string()];
        settings.source_recursive.clear();
        settings.source_recursive.insert(root.to_string(), true);

        insert_asset(&connection, 1, "hash-1", r"D:\src\a.jpg", root);
        insert_asset(&connection, 2, "hash-2", r"D:\Library\2026\08\b.jpg", root);
        connection
            .execute(
                "INSERT INTO albums(id, name, kind, created_at)
                 VALUES (1, '测试相册', 'manual', '2026-01-01T00:00:00Z')",
                [],
            )
            .expect("insert album");
        connection
            .execute(
                "INSERT INTO album_assets(album_id, asset_id) VALUES (1, 1)",
                [],
            )
            .expect("insert album asset");
        connection
            .execute(
                "INSERT INTO asset_search(asset_id, filename, path)
                 VALUES (1, 'a.jpg', 'D:\\src\\a.jpg')",
                [],
            )
            .expect("insert search row");

        let result = remove_sources(&connection, &mut settings, root, false).expect("remove");
        assert_eq!(result.removed_indexes, 1);
        let remaining_assets: i64 = connection
            .query_row("SELECT COUNT(*) FROM assets", [], |row| row.get(0))
            .expect("count assets");
        assert_eq!(remaining_assets, 1);
        let remaining_locations: i64 = connection
            .query_row("SELECT COUNT(*) FROM asset_locations", [], |row| row.get(0))
            .expect("count locations");
        assert_eq!(remaining_locations, 1);
        let remaining_album_assets: i64 = connection
            .query_row("SELECT COUNT(*) FROM album_assets", [], |row| row.get(0))
            .expect("count album assets");
        assert_eq!(remaining_album_assets, 0);
        let remaining_search_rows: i64 = connection
            .query_row("SELECT COUNT(*) FROM asset_search", [], |row| row.get(0))
            .expect("count search rows");
        assert_eq!(remaining_search_rows, 0);
    }
}
