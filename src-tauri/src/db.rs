use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};

use chrono::Utc;
use notify::RecommendedWatcher;
use rusqlite::{backup::Backup, functions::FunctionFlags, params, Connection, OptionalExtension};
use tauri::{AppHandle, Manager};

use crate::{
    error::{AppError, AppResult},
    models::{ActivityItem, Album, AppSettings, LibraryStats},
};

const SCHEMA_VERSION: i64 = 2;

#[derive(Clone)]
pub struct AppState {
    pub app_handle: AppHandle,
    pub db_path: PathBuf,
    pub thumbnail_dir: PathBuf,
    pub scan_cancelled: Arc<AtomicBool>,
    pub scan_running: Arc<AtomicBool>,
    pub watcher: Arc<Mutex<Option<RecommendedWatcher>>>,
}

impl AppState {
    pub fn initialize(app: &AppHandle) -> AppResult<Self> {
        let data_dir = app
            .path()
            .app_local_data_dir()
            .map_err(|error| AppError::Message(format!("无法确定应用数据目录：{error}")))?;
        let thumbnail_dir = data_dir.join("cache").join("thumbnails");
        fs::create_dir_all(&thumbnail_dir)?;
        let db_path = data_dir.join("picnest.db");
        let database_existed = db_path.exists();
        let state = Self {
            app_handle: app.clone(),
            db_path,
            thumbnail_dir,
            scan_cancelled: Arc::new(AtomicBool::new(false)),
            scan_running: Arc::new(AtomicBool::new(false)),
            watcher: Arc::new(Mutex::new(None)),
        };
        let connection = state.connection()?;
        migrate(&connection, &state.db_path, database_existed)?;
        seed(&connection)?;
        drop(connection);
        crate::organize::recover_interrupted_jobs(&state)?;
        state.refresh_watcher()?;
        Ok(state)
    }

    pub fn connection(&self) -> AppResult<Connection> {
        let connection = Connection::open(&self.db_path)?;
        connection.busy_timeout(Duration::from_secs(5))?;
        connection.pragma_update(None, "foreign_keys", "ON")?;
        connection.pragma_update(None, "journal_mode", "WAL")?;
        connection.pragma_update(None, "synchronous", "NORMAL")?;
        connection.create_scalar_function(
            "hamming_hex",
            2,
            FunctionFlags::SQLITE_DETERMINISTIC,
            |context| {
                let left = context.get::<String>(0)?;
                let right = context.get::<String>(1)?;
                Ok(hamming_hex(&left, &right))
            },
        )?;
        Ok(connection)
    }

    pub fn cancel_scan(&self) -> bool {
        let running = self.scan_running.load(Ordering::SeqCst);
        if running {
            self.scan_cancelled.store(true, Ordering::SeqCst);
        }
        running
    }

    pub fn refresh_watcher(&self) -> AppResult<()> {
        let connection = self.connection()?;
        let settings = load_settings(&connection)?;
        let watcher = if settings.configured && !settings.source_paths.is_empty() {
            let rules = crate::sources::source_rules(&settings);
            Some(crate::watch::create_watcher(self.clone(), &rules)?)
        } else {
            None
        };
        let mut slot = self
            .watcher
            .lock()
            .map_err(|_| AppError::Message("目录监听状态不可用".to_string()))?;
        *slot = watcher;
        Ok(())
    }
}

fn hamming_hex(left: &str, right: &str) -> i64 {
    match (
        u64::from_str_radix(left, 16),
        u64::from_str_radix(right, 16),
    ) {
        (Ok(left), Ok(right)) => (left ^ right).count_ones() as i64,
        _ => 64,
    }
}

pub(crate) fn migrate(
    connection: &Connection,
    db_path: &Path,
    database_existed: bool,
) -> AppResult<()> {
    let version: i64 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
    if database_existed && version < SCHEMA_VERSION {
        backup_database(connection, db_path)?;
    }
    connection.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS app_settings (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS source_roots (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            path TEXT NOT NULL UNIQUE,
            enabled INTEGER NOT NULL DEFAULT 1,
            recursive INTEGER NOT NULL DEFAULT 1,
            added_at TEXT NOT NULL,
            last_scan_at TEXT
        );

        CREATE TABLE IF NOT EXISTS assets (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            content_hash TEXT NOT NULL UNIQUE,
            perceptual_hash TEXT,
            phash0 INTEGER,
            phash1 INTEGER,
            phash2 INTEGER,
            phash3 INTEGER,
            filename TEXT NOT NULL,
            width INTEGER NOT NULL DEFAULT 0,
            height INTEGER NOT NULL DEFAULT 0,
            captured_at TEXT NOT NULL,
            imported_at TEXT NOT NULL,
            file_size INTEGER NOT NULL DEFAULT 0,
            category TEXT NOT NULL DEFAULT 'other',
            camera TEXT,
            location TEXT,
            description TEXT,
            ocr_text TEXT,
            tags_json TEXT NOT NULL DEFAULT '[]',
            favorite INTEGER NOT NULL DEFAULT 0,
            ai_analyzed INTEGER NOT NULL DEFAULT 0,
            thumbnail_path TEXT
        );

        CREATE INDEX IF NOT EXISTS idx_assets_captured_at ON assets(captured_at DESC);
        CREATE INDEX IF NOT EXISTS idx_assets_category ON assets(category);
        CREATE INDEX IF NOT EXISTS idx_assets_perceptual_hash ON assets(perceptual_hash);

        CREATE TABLE IF NOT EXISTS asset_locations (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            asset_id INTEGER NOT NULL REFERENCES assets(id) ON DELETE CASCADE,
            path TEXT NOT NULL UNIQUE,
            source TEXT NOT NULL,
            available INTEGER NOT NULL DEFAULT 1,
            needs_organize INTEGER NOT NULL DEFAULT 1,
            last_seen_at TEXT NOT NULL,
            file_size INTEGER NOT NULL DEFAULT 0,
            modified_at INTEGER NOT NULL DEFAULT 0,
            root_path TEXT
        );

        CREATE INDEX IF NOT EXISTS idx_asset_locations_asset ON asset_locations(asset_id);
        CREATE INDEX IF NOT EXISTS idx_asset_locations_organize ON asset_locations(needs_organize, available);

        CREATE TABLE IF NOT EXISTS albums (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT NOT NULL,
            kind TEXT NOT NULL DEFAULT 'manual',
            rule_json TEXT,
            created_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS album_assets (
            album_id INTEGER NOT NULL REFERENCES albums(id) ON DELETE CASCADE,
            asset_id INTEGER NOT NULL REFERENCES assets(id) ON DELETE CASCADE,
            PRIMARY KEY(album_id, asset_id)
        );

        CREATE TABLE IF NOT EXISTS organize_plans (
            id TEXT PRIMARY KEY,
            status TEXT NOT NULL,
            total_bytes INTEGER NOT NULL,
            created_at TEXT NOT NULL,
            completed_at TEXT
        );

        CREATE TABLE IF NOT EXISTS organize_plan_items (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            plan_id TEXT NOT NULL REFERENCES organize_plans(id) ON DELETE CASCADE,
            asset_id INTEGER NOT NULL REFERENCES assets(id),
            source_path TEXT NOT NULL,
            target_path TEXT NOT NULL,
            reason TEXT NOT NULL,
            bytes INTEGER NOT NULL,
            conflict INTEGER NOT NULL DEFAULT 0,
            status TEXT NOT NULL DEFAULT 'planned',
            error TEXT
        );

        CREATE TABLE IF NOT EXISTS activity (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            kind TEXT NOT NULL,
            title TEXT NOT NULL,
            detail TEXT NOT NULL,
            created_at TEXT NOT NULL,
            reversible INTEGER NOT NULL DEFAULT 0,
            plan_id TEXT
        );

        CREATE TABLE IF NOT EXISTS ai_analysis (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            asset_id INTEGER NOT NULL REFERENCES assets(id) ON DELETE CASCADE,
            description TEXT NOT NULL,
            tags_json TEXT NOT NULL,
            image_type TEXT NOT NULL,
            scene TEXT NOT NULL,
            objects_json TEXT NOT NULL,
            confidence REAL NOT NULL,
            model TEXT NOT NULL,
            prompt_version TEXT NOT NULL,
            created_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS embeddings (
            asset_id INTEGER PRIMARY KEY REFERENCES assets(id) ON DELETE CASCADE,
            model TEXT NOT NULL,
            dimensions INTEGER NOT NULL,
            vector_json TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );

        CREATE VIRTUAL TABLE IF NOT EXISTS asset_search USING fts5(
            asset_id UNINDEXED,
            filename,
            path,
            description,
            ocr_text,
            tags,
            camera,
            location,
            captured_at,
            source,
            albums,
            tokenize = 'unicode61 remove_diacritics 2'
        );
        "#,
    )?;
    ensure_column(connection, "assets", "phash0", "INTEGER")?;
    ensure_column(connection, "assets", "phash1", "INTEGER")?;
    ensure_column(connection, "assets", "phash2", "INTEGER")?;
    ensure_column(connection, "assets", "phash3", "INTEGER")?;
    connection.execute_batch(
        r#"
        CREATE INDEX IF NOT EXISTS idx_assets_phash0 ON assets(phash0);
        CREATE INDEX IF NOT EXISTS idx_assets_phash1 ON assets(phash1);
        CREATE INDEX IF NOT EXISTS idx_assets_phash2 ON assets(phash2);
        CREATE INDEX IF NOT EXISTS idx_assets_phash3 ON assets(phash3);
        "#,
    )?;
    ensure_column(
        connection,
        "asset_locations",
        "file_size",
        "INTEGER NOT NULL DEFAULT 0",
    )?;
    ensure_column(
        connection,
        "asset_locations",
        "modified_at",
        "INTEGER NOT NULL DEFAULT 0",
    )?;
    ensure_column(connection, "asset_locations", "root_path", "TEXT")?;
    ensure_column(
        connection,
        "source_roots",
        "recursive",
        "INTEGER NOT NULL DEFAULT 1",
    )?;
    connection.execute_batch(
        "CREATE INDEX IF NOT EXISTS idx_asset_locations_root ON asset_locations(root_path, last_seen_at);",
    )?;

    if version < SCHEMA_VERSION {
        connection.execute_batch(
            r#"
            DROP TABLE IF EXISTS asset_search;
            CREATE VIRTUAL TABLE asset_search USING fts5(
                asset_id UNINDEXED,
                filename,
                path,
                description,
                ocr_text,
                tags,
                camera,
                location,
                captured_at,
                source,
                albums,
                tokenize = 'unicode61 remove_diacritics 2'
            );
            PRAGMA user_version = 2;
            "#,
        )?;
        backfill_perceptual_segments(connection)?;
        rebuild_search_index(connection)?;
    }
    Ok(())
}

fn ensure_column(
    connection: &Connection,
    table: &str,
    column: &str,
    definition: &str,
) -> AppResult<()> {
    let mut statement = connection.prepare(&format!("PRAGMA table_info({table})"))?;
    let columns = statement
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<Result<Vec<_>, _>>()?;
    if !columns.iter().any(|value| value == column) {
        connection.execute_batch(&format!(
            "ALTER TABLE {table} ADD COLUMN {column} {definition};"
        ))?;
    }
    Ok(())
}

fn backup_database(connection: &Connection, db_path: &Path) -> AppResult<()> {
    let timestamp = Utc::now().format("%Y%m%d-%H%M%S");
    let backup_path =
        db_path.with_file_name(format!("picnest-before-v{SCHEMA_VERSION}-{timestamp}.db"));
    let mut destination = Connection::open(backup_path)?;
    let backup = Backup::new(connection, &mut destination)?;
    backup.run_to_completion(128, Duration::from_millis(10), None)?;
    Ok(())
}

fn backfill_perceptual_segments(connection: &Connection) -> AppResult<()> {
    let mut statement = connection
        .prepare("SELECT id, perceptual_hash FROM assets WHERE perceptual_hash IS NOT NULL")?;
    let rows = statement
        .query_map([], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    drop(statement);
    for (asset_id, hash) in rows {
        if let Some([a, b, c, d]) = perceptual_segments(&hash) {
            connection.execute(
                "UPDATE assets SET phash0 = ?1, phash1 = ?2, phash2 = ?3, phash3 = ?4 WHERE id = ?5",
                params![a, b, c, d, asset_id],
            )?;
        }
    }
    Ok(())
}

pub fn perceptual_segments(hash: &str) -> Option<[i64; 4]> {
    if hash.len() != 16 {
        return None;
    }
    Some([
        i64::from_str_radix(&hash[0..4], 16).ok()?,
        i64::from_str_radix(&hash[4..8], 16).ok()?,
        i64::from_str_radix(&hash[8..12], 16).ok()?,
        i64::from_str_radix(&hash[12..16], 16).ok()?,
    ])
}

fn seed(connection: &Connection) -> AppResult<()> {
    let album_count: i64 =
        connection.query_row("SELECT COUNT(*) FROM albums", [], |row| row.get(0))?;
    if album_count == 0 {
        let now = Utc::now().to_rfc3339();
        connection.execute(
            "INSERT INTO albums(name, kind, rule_json, created_at) VALUES (?1, 'smart', ?2, ?3)",
            params!["包含文字", r#"{"hasOcr":true}"#, now],
        )?;
        connection.execute(
            "INSERT INTO albums(name, kind, rule_json, created_at) VALUES (?1, 'smart', ?2, ?3)",
            params!["本月收藏", r#"{"favorite":true,"dateRange":"month"}"#, now],
        )?;
    }
    Ok(())
}

pub fn default_settings() -> AppSettings {
    let pictures =
        dirs::picture_dir().unwrap_or_else(|| PathBuf::from("C:\\Users\\Public\\Pictures"));
    let mut sources = Vec::new();
    if let Some(desktop) = dirs::desktop_dir() {
        sources.push(desktop.to_string_lossy().to_string());
    }
    if let Some(downloads) = dirs::download_dir() {
        sources.push(downloads.to_string_lossy().to_string());
    }
    let source_recursive = sources
        .iter()
        .map(|path| (path.clone(), true))
        .collect::<HashMap<_, _>>();
    AppSettings {
        configured: false,
        library_path: pictures
            .join("PicNest Library")
            .to_string_lossy()
            .to_string(),
        source_paths: sources,
        source_recursive,
        locale: "zh-CN".to_string(),
        cloud_ai_enabled: false,
        ai_base_url: "https://api.openai.com/v1".to_string(),
        vision_model: "gpt-4.1-mini".to_string(),
        embedding_model: "text-embedding-3-small".to_string(),
        ai_batch_limit: 20,
        api_key_configured: false,
        telemetry_enabled: false,
    }
}

pub fn load_settings(connection: &Connection) -> AppResult<AppSettings> {
    let value: Option<String> = connection
        .query_row(
            "SELECT value FROM app_settings WHERE key = 'settings'",
            [],
            |row| row.get(0),
        )
        .optional()?;
    let mut settings = match value {
        Some(value) => serde_json::from_str(&value)?,
        None => default_settings(),
    };
    for path in &settings.source_paths {
        settings
            .source_recursive
            .entry(path.clone())
            .or_insert(true);
    }
    Ok(settings)
}

pub fn store_settings(connection: &Connection, settings: &AppSettings) -> AppResult<()> {
    let transaction = connection.unchecked_transaction()?;
    write_settings(&transaction, settings)?;
    transaction.commit()?;
    Ok(())
}

pub(crate) fn write_settings(connection: &Connection, settings: &AppSettings) -> AppResult<()> {
    connection.execute(
        "INSERT INTO app_settings(key, value, updated_at) VALUES ('settings', ?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
        params![serde_json::to_string(settings)?, Utc::now().to_rfc3339()],
    )?;
    connection.execute("UPDATE source_roots SET enabled = 0", [])?;
    for path in &settings.source_paths {
        let recursive = settings.source_recursive.get(path).copied().unwrap_or(true);
        connection.execute(
            "INSERT INTO source_roots(path, enabled, recursive, added_at) VALUES (?1, 1, ?2, ?3)
             ON CONFLICT(path) DO UPDATE SET enabled = 1, recursive = excluded.recursive",
            params![path, recursive as i64, Utc::now().to_rfc3339()],
        )?;
    }
    Ok(())
}

pub fn library_stats(connection: &Connection) -> AppResult<LibraryStats> {
    Ok(LibraryStats {
        total: connection.query_row("SELECT COUNT(*) FROM assets", [], |row| row.get(0))?,
        inbox: connection.query_row(
            "SELECT COUNT(DISTINCT asset_id) FROM asset_locations WHERE available = 1 AND needs_organize = 1",
            [],
            |row| row.get(0),
        )?,
        favorites: connection.query_row("SELECT COUNT(*) FROM assets WHERE favorite = 1", [], |row| row.get(0))?,
        duplicates: connection.query_row(
            "SELECT COUNT(*) FROM assets a
             WHERE (SELECT COUNT(*) FROM asset_locations l WHERE l.asset_id = a.id AND l.available = 1) > 1
                OR EXISTS (
                    SELECT 1 FROM assets b
                    WHERE b.id != a.id
                      AND a.perceptual_hash IS NOT NULL AND b.perceptual_hash IS NOT NULL
                      AND (a.phash0 = b.phash0 OR a.phash1 = b.phash1 OR a.phash2 = b.phash2 OR a.phash3 = b.phash3)
                      AND hamming_hex(a.perceptual_hash, b.perceptual_hash) <= 3
                )",
            [],
            |row| row.get(0),
        )?,
        missing: connection.query_row(
            "SELECT COUNT(*) FROM assets a WHERE NOT EXISTS (
                SELECT 1 FROM asset_locations l WHERE l.asset_id = a.id AND l.available = 1
            )",
            [],
            |row| row.get(0),
        )?,
        albums: connection.query_row("SELECT COUNT(*) FROM albums", [], |row| row.get(0))?,
        storage_bytes: connection.query_row("SELECT COALESCE(SUM(file_size), 0) FROM assets", [], |row| row.get(0))?,
    })
}

pub fn list_albums(connection: &Connection) -> AppResult<Vec<Album>> {
    let mut statement = connection
        .prepare("SELECT id, name, kind, rule_json FROM albums ORDER BY created_at DESC")?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    drop(statement);
    let mut albums = Vec::new();
    for (id, name, kind, rule_json) in rows {
        let count = if kind == "manual" {
            connection.query_row(
                "SELECT COUNT(*) FROM album_assets WHERE album_id = ?1",
                params![id],
                |row| row.get(0),
            )?
        } else {
            let rule: serde_json::Value = rule_json
                .as_deref()
                .and_then(|value| serde_json::from_str(value).ok())
                .unwrap_or_default();
            if rule.get("hasOcr").and_then(|value| value.as_bool()) == Some(true) {
                connection.query_row(
                    "SELECT COUNT(*) FROM assets WHERE LENGTH(TRIM(COALESCE(ocr_text, ''))) > 0",
                    [],
                    |row| row.get(0),
                )?
            } else if rule.get("favorite").and_then(|value| value.as_bool()) == Some(true)
                && rule.get("dateRange").and_then(|value| value.as_str()) == Some("month")
            {
                connection.query_row(
                    "SELECT COUNT(*) FROM assets WHERE favorite = 1 AND strftime('%Y-%m', captured_at) = strftime('%Y-%m', 'now', 'localtime')",
                    [],
                    |row| row.get(0),
                )?
            } else {
                0
            }
        };
        albums.push(Album {
            id,
            name,
            kind,
            count,
            cover_thumbnail: None,
        });
    }
    Ok(albums)
}

pub fn list_activity(connection: &Connection) -> AppResult<Vec<ActivityItem>> {
    let mut statement = connection.prepare(
        "SELECT id, kind, title, detail, created_at, reversible FROM activity ORDER BY id DESC LIMIT 30",
    )?;
    let rows = statement.query_map([], |row| {
        Ok(ActivityItem {
            id: row.get(0)?,
            kind: row.get(1)?,
            title: row.get(2)?,
            detail: row.get(3)?,
            created_at: row.get(4)?,
            reversible: row.get::<_, i64>(5)? != 0,
        })
    })?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

pub fn add_activity(
    connection: &Connection,
    kind: &str,
    title: &str,
    detail: &str,
    reversible: bool,
    plan_id: Option<&str>,
) -> AppResult<()> {
    connection.execute(
        "INSERT INTO activity(kind, title, detail, created_at, reversible, plan_id) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![kind, title, detail, Utc::now().to_rfc3339(), reversible as i64, plan_id],
    )?;
    Ok(())
}

pub fn sync_search_row(connection: &Connection, asset_id: i64) -> AppResult<()> {
    connection.execute(
        "DELETE FROM asset_search WHERE asset_id = ?1",
        params![asset_id],
    )?;
    connection.execute(
        "INSERT INTO asset_search(
            asset_id, filename, path, description, ocr_text, tags, camera, location,
            captured_at, source, albums
         )
         SELECT a.id, a.filename,
                COALESCE((SELECT GROUP_CONCAT(l.path, ' ') FROM asset_locations l WHERE l.asset_id = a.id AND l.available = 1), ''),
                COALESCE(a.description, ''), COALESCE(a.ocr_text, ''), COALESCE(a.tags_json, '[]'),
                COALESCE(a.camera, ''), COALESCE(a.location, ''), a.captured_at,
                COALESCE((SELECT GROUP_CONCAT(l.source, ' ') FROM asset_locations l WHERE l.asset_id = a.id AND l.available = 1), ''),
                COALESCE((SELECT GROUP_CONCAT(al.name, ' ') FROM album_assets aa JOIN albums al ON al.id = aa.album_id WHERE aa.asset_id = a.id), '')
         FROM assets a WHERE a.id = ?1",
        params![asset_id],
    )?;
    Ok(())
}

fn rebuild_search_index(connection: &Connection) -> AppResult<()> {
    connection.execute("DELETE FROM asset_search", [])?;
    connection.execute(
        "INSERT INTO asset_search(
            asset_id, filename, path, description, ocr_text, tags, camera, location,
            captured_at, source, albums
         )
         SELECT a.id, a.filename,
                COALESCE((SELECT GROUP_CONCAT(l.path, ' ') FROM asset_locations l WHERE l.asset_id = a.id AND l.available = 1), ''),
                COALESCE(a.description, ''), COALESCE(a.ocr_text, ''), COALESCE(a.tags_json, '[]'),
                COALESCE(a.camera, ''), COALESCE(a.location, ''), a.captured_at,
                COALESCE((SELECT GROUP_CONCAT(l.source, ' ') FROM asset_locations l WHERE l.asset_id = a.id AND l.available = 1), ''),
                COALESCE((SELECT GROUP_CONCAT(al.name, ' ') FROM album_assets aa JOIN albums al ON al.id = aa.album_id WHERE aa.asset_id = a.id), '')
         FROM assets a",
        [],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_incremental_scan_and_search_schema() {
        let directory = tempfile::tempdir().expect("temp directory");
        let path = directory.path().join("picnest.db");
        let connection = Connection::open(&path).expect("database");

        migrate(&connection, &path, false).expect("migration");

        let version: i64 = connection
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .expect("schema version");
        assert_eq!(version, SCHEMA_VERSION);
        let mut statement = connection
            .prepare("PRAGMA table_info(asset_locations)")
            .expect("table info");
        let columns = statement
            .query_map([], |row| row.get::<_, String>(1))
            .expect("columns")
            .collect::<Result<Vec<_>, _>>()
            .expect("column list");
        assert!(columns.contains(&"modified_at".to_string()));
        assert!(columns.contains(&"root_path".to_string()));
        let mut source_columns = connection
            .prepare("PRAGMA table_info(source_roots)")
            .expect("source table info");
        let source_columns = source_columns
            .query_map([], |row| row.get::<_, String>(1))
            .expect("source columns")
            .collect::<Result<Vec<_>, _>>()
            .expect("source column list");
        assert!(source_columns.contains(&"recursive".to_string()));
        let search_exists: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'asset_search'",
                [],
                |row| row.get(0),
            )
            .expect("search table");
        assert_eq!(search_exists, 1);
    }

    #[test]
    fn splits_perceptual_hash_into_indexable_segments() {
        assert_eq!(
            perceptual_segments("0123abcd4567ffff"),
            Some([0x0123, 0xabcd, 0x4567, 0xffff])
        );
        assert_eq!(hamming_hex("0000000000000000", "0000000000000007"), 3);
    }
}
