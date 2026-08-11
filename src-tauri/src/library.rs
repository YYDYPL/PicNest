use std::{
    collections::HashSet,
    fs::{self, File},
    io::{BufReader, Read},
    path::{Path, PathBuf},
    sync::atomic::Ordering,
    time::UNIX_EPOCH,
};

use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use chrono::{DateTime, Local, NaiveDateTime, TimeZone, Utc};
use exif::{In, Reader as ExifReader, Tag};
use image::{DynamicImage, GenericImageView, ImageFormat, ImageReader};
use rusqlite::{params, params_from_iter, types::Value, OptionalExtension};
use walkdir::WalkDir;

use crate::{
    db::{add_activity, load_settings, perceptual_segments, sync_search_row, AppState},
    error::{AppError, AppResult},
    models::{Asset, AssetPage, AssetQuery, ScanResult},
};

const SUPPORTED_EXTENSIONS: &[&str] = &[
    "jpg", "jpeg", "png", "webp", "gif", "bmp", "tif", "tiff", "heic", "heif",
];

#[derive(Default)]
struct ImageMetadata {
    width: u32,
    height: u32,
    captured_at: Option<String>,
    camera: Option<String>,
    location: Option<String>,
    perceptual_hash: Option<String>,
    thumbnail_path: Option<String>,
}

pub fn scan_paths(state: &AppState, paths: &[String]) -> AppResult<ScanResult> {
    if state.scan_running.swap(true, Ordering::SeqCst) {
        return Err(AppError::Message("已有扫描任务正在运行".to_string()));
    }
    state.scan_cancelled.store(false, Ordering::SeqCst);
    let _guard = ScanGuard(&state.scan_running);
    let connection = state.connection()?;
    let settings = load_settings(&connection)?;
    let mut result = ScanResult::default();

    for source in paths {
        if state.scan_cancelled.load(Ordering::SeqCst) {
            result.cancelled = true;
            break;
        }
        let root = PathBuf::from(source);
        if !root.exists() || !root.is_dir() {
            result.failed += 1;
            continue;
        }
        let scan_token = Utc::now().to_rfc3339();

        for entry in WalkDir::new(&root).follow_links(false).into_iter() {
            if state.scan_cancelled.load(Ordering::SeqCst) {
                result.cancelled = true;
                break;
            }
            let entry = match entry {
                Ok(entry) => entry,
                Err(_) => {
                    result.failed += 1;
                    continue;
                }
            };
            if !entry.file_type().is_file() {
                continue;
            }
            let path = entry.path();
            if !has_supported_extension(path) {
                continue;
            }
            if !is_supported_image(path) {
                result.unsupported += 1;
                continue;
            }
            result.discovered += 1;
            let stamp = match file_stamp(path) {
                Ok(stamp) => stamp,
                Err(_) => {
                    result.failed += 1;
                    continue;
                }
            };
            if refresh_unchanged_location(
                &connection,
                path,
                &root,
                &settings.library_path,
                &scan_token,
                stamp,
            )? {
                result.indexed += 1;
                result.skipped += 1;
                continue;
            }
            match index_file(
                state,
                &connection,
                path,
                &root,
                &settings.library_path,
                &scan_token,
                stamp,
            ) {
                Ok(was_duplicate) => {
                    result.indexed += 1;
                    if was_duplicate {
                        result.duplicates += 1;
                    }
                }
                Err(error) => {
                    log::warn!("Failed to index an image: {error}");
                    result.failed += 1;
                }
            }
        }

        if result.cancelled {
            break;
        }

        mark_unseen_under_root(&connection, &root, &scan_token)?;

        connection.execute(
            "INSERT INTO source_roots(path, enabled, added_at, last_scan_at) VALUES (?1, 1, ?2, ?2)
             ON CONFLICT(path) DO UPDATE SET enabled = 1, last_scan_at = excluded.last_scan_at",
            params![source, Utc::now().to_rfc3339()],
        )?;
    }

    add_activity(
        &connection,
        "scan",
        "扫描了图片文件夹",
        &format!(
            "发现 {} 张图片，索引 {} 张，跳过 {} 张未变化文件，{} 张需要复查{}",
            result.discovered,
            result.indexed,
            result.skipped,
            result.failed,
            if result.cancelled {
                "，扫描已取消"
            } else {
                ""
            }
        ),
        false,
        None,
    )?;
    Ok(result)
}

struct ScanGuard<'a>(&'a std::sync::atomic::AtomicBool);

impl Drop for ScanGuard<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

#[derive(Clone, Copy)]
struct FileStamp {
    size: i64,
    modified_at: i64,
}

fn file_stamp(path: &Path) -> AppResult<FileStamp> {
    let metadata = fs::metadata(path)?;
    let size = i64::try_from(metadata.len()).unwrap_or(i64::MAX);
    let modified_at = metadata
        .modified()
        .ok()
        .and_then(|value| value.duration_since(UNIX_EPOCH).ok())
        .map(|value| value.as_secs() as i64)
        .unwrap_or(0);
    Ok(FileStamp { size, modified_at })
}

fn refresh_unchanged_location(
    connection: &rusqlite::Connection,
    path: &Path,
    source_root: &Path,
    library_root: &str,
    seen_at: &str,
    stamp: FileStamp,
) -> AppResult<bool> {
    let normalized_path = path.to_string_lossy().to_string();
    let existing: Option<(i64, i64, i64)> = connection
        .query_row(
            "SELECT asset_id, file_size, modified_at FROM asset_locations WHERE path = ?1",
            params![normalized_path],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?;
    let Some((_asset_id, size, modified_at)) = existing else {
        return Ok(false);
    };
    if size != stamp.size || modified_at != stamp.modified_at {
        return Ok(false);
    }
    let category = detect_category(path);
    let source = source_label(source_root, &category);
    let needs_organize = !path_starts_with(path, Path::new(library_root));
    connection.execute(
        "UPDATE asset_locations
         SET source = ?1, available = 1, needs_organize = ?2, last_seen_at = ?3,
             root_path = ?4
         WHERE path = ?5",
        params![
            source,
            needs_organize as i64,
            seen_at,
            source_root.to_string_lossy(),
            normalized_path
        ],
    )?;
    Ok(true)
}

fn index_file(
    state: &AppState,
    connection: &rusqlite::Connection,
    path: &Path,
    source_root: &Path,
    library_root: &str,
    seen_at: &str,
    stamp: FileStamp,
) -> AppResult<bool> {
    let content_hash = hash_file(path)?;
    let existing_id: Option<i64> = connection
        .query_row(
            "SELECT id FROM assets WHERE content_hash = ?1",
            params![content_hash],
            |row| row.get(0),
        )
        .optional()?;
    let modified_at = DateTime::<Utc>::from_timestamp(stamp.modified_at, 0)
        .unwrap_or_else(Utc::now)
        .to_rfc3339();
    let filename = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("untitled-image")
        .to_string();
    let category = detect_category(path);
    let source = source_label(source_root, &category);
    let now = Utc::now().to_rfc3339();
    let asset_id = if let Some(asset_id) = existing_id {
        asset_id
    } else {
        let metadata = extract_metadata(state, path, &content_hash, &modified_at)?;
        let captured_at = metadata.captured_at.unwrap_or_else(|| modified_at.clone());
        let initial_tags = serde_json::to_string(&initial_tags(&category))?;
        let segments = metadata
            .perceptual_hash
            .as_deref()
            .and_then(perceptual_segments);
        connection.execute(
            "INSERT INTO assets(
                content_hash, perceptual_hash, phash0, phash1, phash2, phash3,
                filename, width, height, captured_at, imported_at, file_size, category,
                camera, location, tags_json, thumbnail_path
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)",
            params![
                content_hash,
                metadata.perceptual_hash,
                segments.map(|value| value[0]),
                segments.map(|value| value[1]),
                segments.map(|value| value[2]),
                segments.map(|value| value[3]),
                filename,
                metadata.width as i64,
                metadata.height as i64,
                captured_at,
                now,
                stamp.size,
                category,
                metadata.camera,
                metadata.location,
                initial_tags,
                metadata.thumbnail_path,
            ],
        )?;
        connection.last_insert_rowid()
    };

    let normalized_path = path.to_string_lossy().to_string();
    let prior_asset_id: Option<i64> = connection
        .query_row(
            "SELECT asset_id FROM asset_locations WHERE path = ?1",
            params![normalized_path],
            |row| row.get(0),
        )
        .optional()?;
    let needs_organize = !path_starts_with(path, Path::new(library_root));
    connection.execute(
        "INSERT INTO asset_locations(
            asset_id, path, source, available, needs_organize, last_seen_at,
            file_size, modified_at, root_path
         ) VALUES (?1, ?2, ?3, 1, ?4, ?5, ?6, ?7, ?8)
         ON CONFLICT(path) DO UPDATE SET asset_id = excluded.asset_id, source = excluded.source,
             available = 1, needs_organize = excluded.needs_organize,
             last_seen_at = excluded.last_seen_at, file_size = excluded.file_size,
             modified_at = excluded.modified_at, root_path = excluded.root_path",
        params![
            asset_id,
            normalized_path,
            source,
            needs_organize as i64,
            seen_at,
            stamp.size,
            stamp.modified_at,
            source_root.to_string_lossy()
        ],
    )?;
    if prior_asset_id.is_some_and(|value| value != asset_id) {
        sync_search_row(connection, prior_asset_id.unwrap_or_default())?;
    }
    sync_search_row(connection, asset_id)?;
    Ok(existing_id.is_some())
}

fn has_supported_extension(path: &Path) -> bool {
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .map(str::to_ascii_lowercase);
    extension
        .as_deref()
        .is_some_and(|value| SUPPORTED_EXTENSIONS.contains(&value))
}

fn is_supported_image(path: &Path) -> bool {
    if !has_supported_extension(path) {
        return false;
    }
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .map(str::to_ascii_lowercase);

    let mut header = [0_u8; 32];
    let read = File::open(path)
        .and_then(|mut file| file.read(&mut header))
        .unwrap_or(0);
    match infer::get(&header[..read]) {
        Some(kind) => kind.mime_type().starts_with("image/"),
        None => extension.as_deref().is_some_and(|value| {
            value == "heic" || value == "heif" || value == "tif" || value == "tiff"
        }),
    }
}

fn mark_unseen_under_root(
    connection: &rusqlite::Connection,
    root: &Path,
    seen_at: &str,
) -> AppResult<()> {
    let mut statement = connection.prepare(
        "SELECT id, asset_id, path, last_seen_at FROM asset_locations WHERE available = 1",
    )?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    drop(statement);

    let mut affected = HashSet::new();
    for (location_id, asset_id, path, last_seen_at) in rows {
        if last_seen_at != seen_at && path_starts_with(Path::new(&path), root) {
            connection.execute(
                "UPDATE asset_locations SET available = 0 WHERE id = ?1",
                params![location_id],
            )?;
            affected.insert(asset_id);
        }
    }
    for asset_id in affected {
        sync_search_row(connection, asset_id)?;
    }
    Ok(())
}

fn mark_missing_path(connection: &rusqlite::Connection, missing_path: &Path) -> AppResult<()> {
    let mut statement =
        connection.prepare("SELECT id, asset_id, path FROM asset_locations WHERE available = 1")?;
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

    let mut affected = HashSet::new();
    for (location_id, asset_id, path) in rows {
        let path = Path::new(&path);
        if path == missing_path || path_starts_with(path, missing_path) {
            connection.execute(
                "UPDATE asset_locations SET available = 0 WHERE id = ?1",
                params![location_id],
            )?;
            affected.insert(asset_id);
        }
    }
    for asset_id in affected {
        sync_search_row(connection, asset_id)?;
    }
    Ok(())
}

pub fn process_watcher_paths(state: &AppState, paths: &[PathBuf]) -> AppResult<()> {
    let connection = state.connection()?;
    let settings = load_settings(&connection)?;
    let mut candidates = HashSet::<PathBuf>::new();

    for path in paths {
        if !path.exists() {
            mark_missing_path(&connection, path)?;
            continue;
        }
        if path.is_dir() {
            for entry in WalkDir::new(path)
                .follow_links(false)
                .into_iter()
                .filter_map(Result::ok)
                .filter(|entry| entry.file_type().is_file())
            {
                candidates.insert(entry.into_path());
            }
        } else {
            candidates.insert(path.clone());
        }
    }

    for path in candidates {
        if !has_supported_extension(&path) || !is_supported_image(&path) {
            continue;
        }
        let Some(root) = settings
            .source_paths
            .iter()
            .map(PathBuf::from)
            .find(|root| path_starts_with(&path, root))
        else {
            continue;
        };
        let stamp = match file_stamp(&path) {
            Ok(stamp) => stamp,
            Err(error) => {
                log::warn!("Unable to read a changed photo: {error}");
                continue;
            }
        };
        let seen_at = Utc::now().to_rfc3339();
        if !refresh_unchanged_location(
            &connection,
            &path,
            &root,
            &settings.library_path,
            &seen_at,
            stamp,
        )? {
            if let Err(error) = index_file(
                state,
                &connection,
                &path,
                &root,
                &settings.library_path,
                &seen_at,
                stamp,
            ) {
                log::warn!("Unable to index a changed photo: {error}");
            }
        }
    }
    Ok(())
}

pub fn hash_file(path: &Path) -> AppResult<String> {
    let mut file = File::open(path)?;
    let mut hasher = blake3::Hasher::new();
    let mut buffer = [0_u8; 1024 * 128];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hasher.finalize().to_hex().to_string())
}

fn extract_metadata(
    state: &AppState,
    path: &Path,
    content_hash: &str,
    fallback_date: &str,
) -> AppResult<ImageMetadata> {
    let mut metadata = ImageMetadata {
        captured_at: Some(fallback_date.to_string()),
        ..Default::default()
    };
    if let Ok(file) = File::open(path) {
        if let Ok(exif) = ExifReader::new().read_from_container(&mut BufReader::new(file)) {
            metadata.captured_at = exif_datetime(&exif, Tag::DateTimeOriginal)
                .or_else(|| exif_datetime(&exif, Tag::DateTimeDigitized))
                .or_else(|| exif_datetime(&exif, Tag::DateTime))
                .or_else(|| Some(fallback_date.to_string()));
            let make = exif_text(&exif, Tag::Make);
            let model = exif_text(&exif, Tag::Model);
            metadata.camera = match (make, model) {
                (Some(make), Some(model))
                    if !model.to_lowercase().contains(&make.to_lowercase()) =>
                {
                    Some(format!("{make} {model}"))
                }
                (_, Some(model)) => Some(model),
                (Some(make), None) => Some(make),
                _ => None,
            };
            metadata.location = gps_text(&exif);
        }
    }

    let reader = match ImageReader::open(path).and_then(|reader| reader.with_guessed_format()) {
        Ok(reader) => reader,
        Err(_) => return Ok(metadata),
    };
    let image = match reader.decode() {
        Ok(image) => image,
        Err(_) => return Ok(metadata),
    };
    let (width, height) = image.dimensions();
    metadata.width = width;
    metadata.height = height;
    metadata.perceptual_hash = Some(difference_hash(&image));

    let thumbnail_path = state
        .thumbnail_dir
        .join(format!("{content_hash}-grid.webp"));
    if !thumbnail_path.exists() {
        image
            .thumbnail(560, 560)
            .save_with_format(&thumbnail_path, ImageFormat::WebP)?;
    }
    metadata.thumbnail_path = Some(thumbnail_path.to_string_lossy().to_string());
    Ok(metadata)
}

fn difference_hash(image: &DynamicImage) -> String {
    let resized = image.thumbnail_exact(9, 8).to_luma8();
    let mut hash = 0_u64;
    for y in 0..8 {
        for x in 0..8 {
            hash <<= 1;
            if resized.get_pixel(x, y)[0] > resized.get_pixel(x + 1, y)[0] {
                hash |= 1;
            }
        }
    }
    format!("{hash:016x}")
}

fn exif_datetime(exif: &exif::Exif, tag: Tag) -> Option<String> {
    let value = exif
        .get_field(tag, In::PRIMARY)?
        .display_value()
        .to_string();
    let value = value.trim_matches(|character| character == '"' || character == ' ');
    let naive = NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S")
        .or_else(|_| NaiveDateTime::parse_from_str(value, "%Y:%m:%d %H:%M:%S"))
        .ok()?;
    Local
        .from_local_datetime(&naive)
        .single()
        .map(|value| value.to_rfc3339())
        .or_else(|| Some(DateTime::<Utc>::from_naive_utc_and_offset(naive, Utc).to_rfc3339()))
}

fn exif_text(exif: &exif::Exif, tag: Tag) -> Option<String> {
    let value = exif
        .get_field(tag, In::PRIMARY)?
        .display_value()
        .to_string();
    let cleaned = value
        .trim_matches(|character| character == '"' || character == ' ')
        .to_string();
    (!cleaned.is_empty()).then_some(cleaned)
}

fn gps_text(exif: &exif::Exif) -> Option<String> {
    let latitude = exif.get_field(Tag::GPSLatitude, In::PRIMARY)?;
    let longitude = exif.get_field(Tag::GPSLongitude, In::PRIMARY)?;
    Some(format!(
        "{} · {}",
        latitude.display_value(),
        longitude.display_value()
    ))
}

fn detect_category(path: &Path) -> String {
    let value = path.to_string_lossy().to_lowercase();
    if ["screenshot", "screen shot", "snipping", "截图", "截屏"]
        .iter()
        .any(|term| value.contains(term))
    {
        "screenshot".to_string()
    } else if ["wechat", "weixin", "微信", "mmexport"]
        .iter()
        .any(|term| value.contains(term))
    {
        "wechat".to_string()
    } else if ["downloads", "download", "下载"]
        .iter()
        .any(|term| value.contains(term))
    {
        "download".to_string()
    } else if ["paper", "document", "论文", "文档", "invoice"]
        .iter()
        .any(|term| value.contains(term))
    {
        "document".to_string()
    } else if ["dcim", "img_", "dsc_", "pxl_", "camera"]
        .iter()
        .any(|term| value.contains(term))
    {
        "camera".to_string()
    } else {
        "other".to_string()
    }
}

fn initial_tags(category: &str) -> Vec<String> {
    match category {
        "screenshot" => vec!["截图".to_string()],
        "wechat" => vec!["微信图片".to_string()],
        "download" => vec!["下载图片".to_string()],
        "camera" => vec!["相机照片".to_string()],
        "document" => vec!["文档图片".to_string()],
        _ => Vec::new(),
    }
}

fn source_label(root: &Path, category: &str) -> String {
    match category {
        "wechat" => "微信图片".to_string(),
        "camera" => "相机导入".to_string(),
        "screenshot" => "截图".to_string(),
        _ => root
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("本地文件夹")
            .to_string(),
    }
}

fn path_starts_with(path: &Path, root: &Path) -> bool {
    let path = path.to_string_lossy().to_lowercase();
    let root = root
        .to_string_lossy()
        .trim_end_matches(['\\', '/'])
        .to_lowercase();
    path == root || path.starts_with(&format!("{root}\\")) || path.starts_with(&format!("{root}/"))
}

pub fn list_assets(state: &AppState, query: &AssetQuery) -> AppResult<AssetPage> {
    let connection = state.connection()?;
    let mut where_clauses = Vec::<String>::new();
    let mut values = Vec::<Value>::new();

    match query.view.as_str() {
        "inbox" => where_clauses.push("EXISTS (SELECT 1 FROM asset_locations li WHERE li.asset_id = a.id AND li.available = 1 AND li.needs_organize = 1)".to_string()),
        "recent" => where_clauses.push("datetime(a.imported_at) >= datetime('now', '-30 days')".to_string()),
        "favorites" => where_clauses.push("a.favorite = 1".to_string()),
        "duplicates" => where_clauses.push(
            "((SELECT COUNT(*) FROM asset_locations ld WHERE ld.asset_id = a.id AND ld.available = 1) > 1
              OR EXISTS (
                SELECT 1 FROM assets similar
                WHERE similar.id != a.id
                  AND a.perceptual_hash IS NOT NULL AND similar.perceptual_hash IS NOT NULL
                  AND (a.phash0 = similar.phash0 OR a.phash1 = similar.phash1 OR a.phash2 = similar.phash2 OR a.phash3 = similar.phash3)
                  AND hamming_hex(a.perceptual_hash, similar.perceptual_hash) <= 3
              ))".to_string(),
        ),
        "missing" => where_clauses.push("NOT EXISTS (SELECT 1 FROM asset_locations lm WHERE lm.asset_id = a.id AND lm.available = 1)".to_string()),
        _ => {}
    }

    if let Some(album_id) = query.album_id {
        let album: Option<(String, Option<String>)> = connection
            .query_row(
                "SELECT kind, rule_json FROM albums WHERE id = ?1",
                params![album_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        match album {
            Some((kind, _)) if kind == "manual" => {
                where_clauses.push(
                    "EXISTS (SELECT 1 FROM album_assets qa WHERE qa.asset_id = a.id AND qa.album_id = ?)"
                        .to_string(),
                );
                values.push(Value::Integer(album_id));
            }
            Some((_, Some(rule_json))) => {
                let rule: serde_json::Value = serde_json::from_str(&rule_json).unwrap_or_default();
                if rule.get("hasOcr").and_then(|value| value.as_bool()) == Some(true) {
                    where_clauses.push("LENGTH(TRIM(COALESCE(a.ocr_text, ''))) > 0".to_string());
                }
                if rule.get("favorite").and_then(|value| value.as_bool()) == Some(true) {
                    where_clauses.push("a.favorite = 1".to_string());
                }
                if rule.get("dateRange").and_then(|value| value.as_str()) == Some("month") {
                    where_clauses.push(
                        "strftime('%Y-%m', a.captured_at) = strftime('%Y-%m', 'now', 'localtime')"
                            .to_string(),
                    );
                }
            }
            _ => where_clauses.push("0 = 1".to_string()),
        }
    }
    if let Some(date_from) = query
        .date_from
        .as_ref()
        .filter(|value| !value.trim().is_empty())
    {
        where_clauses.push("date(a.captured_at) >= date(?)".to_string());
        values.push(Value::Text(date_from.clone()));
    }
    if let Some(date_to) = query
        .date_to
        .as_ref()
        .filter(|value| !value.trim().is_empty())
    {
        where_clauses.push("date(a.captured_at) <= date(?)".to_string());
        values.push(Value::Text(date_to.clone()));
    }
    if let Some(category) = query
        .category
        .as_ref()
        .filter(|value| !value.trim().is_empty())
    {
        where_clauses.push("a.category = ?".to_string());
        values.push(Value::Text(category.clone()));
    }
    if let Some(source) = query
        .source
        .as_ref()
        .filter(|value| !value.trim().is_empty())
    {
        where_clauses.push(
            "EXISTS (SELECT 1 FROM asset_locations qs WHERE qs.asset_id = a.id AND qs.available = 1 AND LOWER(qs.source) LIKE ?)"
                .to_string(),
        );
        values.push(Value::Text(format!("%{}%", source.to_lowercase())));
    }
    if let Some(location) = query
        .location
        .as_ref()
        .filter(|value| !value.trim().is_empty())
    {
        where_clauses.push("LOWER(COALESCE(a.location, '')) LIKE ?".to_string());
        values.push(Value::Text(format!("%{}%", location.to_lowercase())));
    }

    let mut rank_query = None;
    if let Some(search) = query
        .search
        .as_ref()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
    {
        let pattern = format!("%{}%", search.to_lowercase());
        let fts = fts_query(search);
        let mut clauses = vec![
            "LOWER(a.filename) LIKE ?",
            "LOWER(COALESCE(a.description, '')) LIKE ?",
            "LOWER(COALESCE(a.ocr_text, '')) LIKE ?",
            "LOWER(a.tags_json) LIKE ?",
            "EXISTS (SELECT 1 FROM asset_locations ls WHERE ls.asset_id = a.id AND ls.available = 1 AND LOWER(ls.path) LIKE ?)",
        ];
        if fts.is_some() {
            clauses.insert(
                0,
                "a.id IN (SELECT asset_id FROM asset_search WHERE asset_search MATCH ?)",
            );
            values.push(Value::Text(fts.clone().unwrap_or_default()));
            rank_query = fts;
        }
        where_clauses.push(format!("({})", clauses.join(" OR ")));
        for _ in 0..5 {
            values.push(Value::Text(pattern.clone()));
        }
    }

    let where_sql = if where_clauses.is_empty() {
        String::new()
    } else {
        format!(" WHERE {}", where_clauses.join(" AND "))
    };
    let count_sql = format!("SELECT COUNT(*) FROM assets a{where_sql}");
    let total: i64 = connection.query_row(&count_sql, params_from_iter(values.clone()), |row| {
        row.get(0)
    })?;
    let limit = query.limit.unwrap_or(200).clamp(1, 500);
    let offset = query.cursor.unwrap_or(0);
    let mut select_values = values;
    let order_sql = if let Some(rank_query) = rank_query {
        select_values.push(Value::Text(rank_query));
        "COALESCE((
            SELECT bm25(asset_search, 8.0, 3.0, 4.0, 5.0, 2.0, 2.0, 1.5, 1.5, 1.0, 1.5, 2.0)
            FROM asset_search
            WHERE asset_search MATCH ? AND asset_id = a.id
        ), 9999.0), datetime(a.captured_at) DESC, a.id DESC"
    } else {
        "datetime(a.captured_at) DESC, a.id DESC"
    };
    select_values.push(Value::Integer(limit as i64));
    select_values.push(Value::Integer(offset as i64));

    let sql = format!(
        "SELECT
            a.id, a.filename,
            COALESCE((SELECT l.path FROM asset_locations l WHERE l.asset_id = a.id ORDER BY l.available DESC, l.needs_organize DESC, l.id LIMIT 1), ''),
            a.thumbnail_path, a.width, a.height, a.captured_at, a.imported_at, a.file_size,
            COALESCE((SELECT l.source FROM asset_locations l WHERE l.asset_id = a.id ORDER BY l.available DESC, l.id LIMIT 1), '本地'),
            a.category, a.favorite,
            CASE WHEN EXISTS (SELECT 1 FROM asset_locations lm WHERE lm.asset_id = a.id AND lm.available = 1) THEN 0 ELSE 1 END,
            CASE WHEN EXISTS (SELECT 1 FROM asset_locations lo WHERE lo.asset_id = a.id AND lo.available = 1 AND lo.needs_organize = 1) THEN 1 ELSE 0 END,
            MAX(0, (SELECT COUNT(*) FROM asset_locations ld WHERE ld.asset_id = a.id AND ld.available = 1) - 1),
            (SELECT COUNT(*) FROM assets similar
             WHERE similar.id != a.id
               AND a.perceptual_hash IS NOT NULL AND similar.perceptual_hash IS NOT NULL
               AND (a.phash0 = similar.phash0 OR a.phash1 = similar.phash1 OR a.phash2 = similar.phash2 OR a.phash3 = similar.phash3)
               AND hamming_hex(a.perceptual_hash, similar.perceptual_hash) <= 3),
            a.content_hash, a.camera, a.location, a.description, a.ocr_text, a.tags_json, a.ai_analyzed,
            COALESCE((SELECT GROUP_CONCAT(aa.album_id) FROM album_assets aa WHERE aa.asset_id = a.id), '')
         FROM assets a{where_sql}
         ORDER BY {order_sql} LIMIT ? OFFSET ?"
    );
    let mut statement = connection.prepare(&sql)?;
    let rows = statement.query_map(params_from_iter(select_values), |row| {
        let tags_json: String = row.get(21)?;
        let album_ids: String = row.get(23)?;
        Ok(Asset {
            id: row.get(0)?,
            filename: row.get(1)?,
            path: row.get(2)?,
            thumbnail_data_url: None,
            width: row.get(4)?,
            height: row.get(5)?,
            captured_at: row.get(6)?,
            imported_at: row.get(7)?,
            file_size: row.get(8)?,
            source: row.get(9)?,
            category: row.get(10)?,
            favorite: row.get::<_, i64>(11)? != 0,
            missing: row.get::<_, i64>(12)? != 0,
            needs_organize: row.get::<_, i64>(13)? != 0,
            duplicate_count: row.get(14)?,
            similar_count: row.get(15)?,
            content_hash: row.get(16)?,
            camera: row.get(17)?,
            location: row.get(18)?,
            description: row.get(19)?,
            ocr_text: row.get(20)?,
            tags: serde_json::from_str(&tags_json).unwrap_or_default(),
            album_ids: album_ids
                .split(',')
                .filter_map(|value| value.parse::<i64>().ok())
                .collect(),
            ai_analyzed: row.get::<_, i64>(22)? != 0,
        })
    })?;
    let items = rows.collect::<Result<Vec<_>, _>>()?;
    let next_cursor =
        (offset as i64 + (items.len() as i64) < total).then_some(offset + items.len() as u32);
    Ok(AssetPage {
        items,
        next_cursor,
        total,
    })
}

fn fts_query(search: &str) -> Option<String> {
    let terms = search
        .split_whitespace()
        .map(|term| {
            term.chars()
                .filter(|character| {
                    character.is_alphanumeric() || *character == '_' || *character == '-'
                })
                .collect::<String>()
        })
        .filter(|term| !term.is_empty())
        .map(|term| format!("\"{}\"*", term.replace('"', "\"\"")))
        .collect::<Vec<_>>();
    (!terms.is_empty()).then(|| terms.join(" AND "))
}

pub fn asset_image_data_url(
    state: &AppState,
    asset_id: i64,
    preview: bool,
) -> AppResult<Option<String>> {
    let connection = state.connection()?;
    let row: Option<(String, Option<String>, Option<String>)> = connection
        .query_row(
            "SELECT a.content_hash, a.thumbnail_path,
                    (SELECT l.path FROM asset_locations l WHERE l.asset_id = a.id AND l.available = 1 ORDER BY l.needs_organize DESC, l.id LIMIT 1)
             FROM assets a WHERE a.id = ?1",
            params![asset_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?;
    let Some((content_hash, thumbnail_path, original_path)) = row else {
        return Ok(None);
    };

    let target = if preview {
        state
            .thumbnail_dir
            .join(format!("{content_hash}-preview.webp"))
    } else {
        thumbnail_path.map(PathBuf::from).unwrap_or_else(|| {
            state
                .thumbnail_dir
                .join(format!("{content_hash}-grid.webp"))
        })
    };

    if !target.exists() {
        let Some(original_path) = original_path else {
            return Ok(None);
        };
        let image = match ImageReader::open(&original_path)
            .and_then(|reader| reader.with_guessed_format())
            .ok()
            .and_then(|reader| reader.decode().ok())
        {
            Some(image) => image,
            None => return Ok(None),
        };
        let image = if preview {
            image.thumbnail(1920, 1920)
        } else {
            image.thumbnail(560, 560)
        };
        image.save_with_format(&target, ImageFormat::WebP)?;
        if !preview {
            connection.execute(
                "UPDATE assets SET thumbnail_path = ?1 WHERE id = ?2",
                params![target.to_string_lossy(), asset_id],
            )?;
        }
    }

    let bytes = fs::read(target)?;
    Ok(Some(format!(
        "data:image/webp;base64,{}",
        BASE64.encode(bytes)
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn categorizes_common_sources() {
        assert_eq!(
            detect_category(Path::new(r"C:\\Users\\me\\Desktop\\Screenshot_12.png")),
            "screenshot"
        );
        assert_eq!(
            detect_category(Path::new(r"D:\\WeChat Files\\mmexport123.jpg")),
            "wechat"
        );
        assert_eq!(
            detect_category(Path::new(r"E:\\DCIM\\IMG_0001.jpg")),
            "camera"
        );
    }

    #[test]
    fn library_prefix_is_boundary_aware() {
        assert!(path_starts_with(
            Path::new(r"D:\\Photos\\2026\\08\\a.jpg"),
            Path::new(r"D:\\Photos")
        ));
        assert!(!path_starts_with(
            Path::new(r"D:\\Photos-old\\a.jpg"),
            Path::new(r"D:\\Photos")
        ));
    }

    #[test]
    fn builds_safe_prefix_full_text_queries() {
        assert_eq!(
            fts_query("Docker 报错"),
            Some("\"Docker\"* AND \"报错\"*".to_string())
        );
        assert_eq!(fts_query("!!!"), None);
    }
}
