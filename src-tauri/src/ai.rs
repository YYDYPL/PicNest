use std::{
    fs,
    io::Cursor,
    path::Path,
    time::{Duration, Instant},
};

use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use image::{codecs::jpeg::JpegEncoder, GenericImageView};
use reqwest::Client;
use rusqlite::{params, OptionalExtension};
use serde_json::{json, Value};

use crate::{
    db::{add_activity, load_settings, sync_search_row, AppState},
    error::{AppError, AppResult},
    models::{AiAnalysis, AiConnectionInput, ConnectionTestResult},
};

const KEYRING_SERVICE: &str = "PicNest";
const KEYRING_USER: &str = "cloud-ai";
const PROMPT_VERSION: &str = "visual-index-v1";

pub fn api_key_configured() -> bool {
    keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER)
        .and_then(|entry| entry.get_password())
        .is_ok_and(|value| !value.is_empty())
}

pub fn store_api_key(value: &str) -> AppResult<()> {
    if value.trim().is_empty() {
        return Ok(());
    }
    keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER)?.set_password(value.trim())?;
    Ok(())
}

pub fn delete_api_key() -> AppResult<()> {
    let entry = keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER)?;
    match entry.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(error) => Err(error.into()),
    }
}

pub async fn test_connection(
    _state: &AppState,
    input: AiConnectionInput,
) -> AppResult<ConnectionTestResult> {
    let api_key = match input.api_key.as_deref().map(str::trim) {
        Some(value) if !value.is_empty() => value.to_string(),
        _ => keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER)?
            .get_password()
            .map_err(|_| AppError::Message("请先输入或保存 API Key".to_string()))?,
    };
    let base_url = input.base_url.trim().trim_end_matches('/');
    if base_url.is_empty() || input.model.trim().is_empty() {
        return Err(AppError::Message("接口地址和模型名称不能为空".to_string()));
    }
    let started = Instant::now();
    let response = Client::builder()
        .timeout(Duration::from_secs(15))
        .build()?
        .get(format!("{base_url}/models"))
        .bearer_auth(api_key)
        .send()
        .await?;
    let latency_ms = started.elapsed().as_millis();
    let status = response.status();
    if status.is_success() {
        Ok(ConnectionTestResult {
            ok: true,
            latency_ms,
            message: format!("连接成功，可使用模型 {}", input.model.trim()),
        })
    } else {
        Ok(ConnectionTestResult {
            ok: false,
            latency_ms,
            message: format!("服务返回 HTTP {}", status.as_u16()),
        })
    }
}

pub async fn analyze_asset(state: &AppState, asset_id: i64) -> AppResult<AiAnalysis> {
    let connection = state.connection()?;
    let settings = load_settings(&connection)?;
    if !settings.cloud_ai_enabled {
        return Err(AppError::Message("云端 AI 当前未启用".to_string()));
    }
    let api_key = keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER)?
        .get_password()
        .map_err(|_| AppError::Message("尚未保存 AI 服务 API Key".to_string()))?;
    let path: Option<String> = connection
        .query_row(
            "SELECT l.path FROM asset_locations l WHERE l.asset_id = ?1 AND l.available = 1 ORDER BY l.needs_organize DESC, l.id LIMIT 1",
            params![asset_id],
            |row| row.get(0),
        )
        .optional()?;
    let path = path.ok_or_else(|| AppError::Message("找不到可读取的原图".to_string()))?;
    let preview = sanitized_preview(Path::new(&path))?;
    let data_url = format!("data:image/jpeg;base64,{}", BASE64.encode(preview));
    let endpoint = format!(
        "{}/chat/completions",
        settings.ai_base_url.trim_end_matches('/')
    );
    let body = json!({
        "model": settings.vision_model,
        "temperature": 0.1,
        "response_format": { "type": "json_object" },
        "messages": [
            {
                "role": "system",
                "content": "You index a private photo library. Return JSON only with description, tags, imageType, scene, objects, confidence. Use concise Simplified Chinese for description and tags. Do not identify real people."
            },
            {
                "role": "user",
                "content": [
                    { "type": "text", "text": "Describe this image for local search. Include visible text themes when relevant, but do not transcribe secrets or personal identifiers." },
                    { "type": "image_url", "image_url": { "url": data_url, "detail": "low" } }
                ]
            }
        ]
    });
    let response: Value = Client::new()
        .post(endpoint)
        .bearer_auth(api_key)
        .json(&body)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let content = response
        .pointer("/choices/0/message/content")
        .and_then(Value::as_str)
        .ok_or_else(|| AppError::Message("AI 服务返回了无法识别的结果".to_string()))?;
    let cleaned = content
        .trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();
    let mut analysis: AiAnalysis = serde_json::from_str(cleaned)?;
    analysis.model = settings.vision_model;
    analysis.tags.sort();
    analysis.tags.dedup();
    let tags_json = serde_json::to_string(&analysis.tags)?;
    let objects_json = serde_json::to_string(&analysis.objects)?;
    connection.execute(
        "INSERT INTO ai_analysis(
            asset_id, description, tags_json, image_type, scene, objects_json,
            confidence, model, prompt_version, created_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![
            asset_id,
            &analysis.description,
            &tags_json,
            &analysis.image_type,
            &analysis.scene,
            objects_json,
            analysis.confidence,
            &analysis.model,
            PROMPT_VERSION,
            chrono::Utc::now().to_rfc3339(),
        ],
    )?;
    connection.execute(
        "UPDATE assets SET description = ?1, tags_json = ?2, ai_analyzed = 1 WHERE id = ?3",
        params![analysis.description, tags_json, asset_id],
    )?;
    sync_search_row(&connection, asset_id)?;
    add_activity(
        &connection,
        "ai",
        "分析了 1 张图片",
        "已保存描述与标签，上传的压缩预览未在本地保留",
        false,
        None,
    )?;
    Ok(analysis)
}

pub fn clear_analysis(state: &AppState, asset_ids: &[i64]) -> AppResult<u32> {
    let connection = state.connection()?;
    let ids = if asset_ids.is_empty() {
        let mut statement = connection.prepare("SELECT id FROM assets WHERE ai_analyzed = 1")?;
        let rows = statement
            .query_map([], |row| row.get::<_, i64>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        rows
    } else {
        asset_ids.to_vec()
    };
    let mut cleared = 0_u32;
    for asset_id in ids {
        let category: Option<String> = connection
            .query_row(
                "SELECT category FROM assets WHERE id = ?1",
                params![asset_id],
                |row| row.get(0),
            )
            .optional()?;
        let Some(category) = category else {
            continue;
        };
        let tags = match category.as_str() {
            "screenshot" => vec!["截图"],
            "wechat" => vec!["微信图片"],
            "download" => vec!["下载图片"],
            "camera" => vec!["相机照片"],
            "document" => vec!["文档图片"],
            _ => Vec::new(),
        };
        connection.execute(
            "UPDATE assets SET description = NULL, tags_json = ?1, ai_analyzed = 0 WHERE id = ?2",
            params![serde_json::to_string(&tags)?, asset_id],
        )?;
        connection.execute(
            "DELETE FROM ai_analysis WHERE asset_id = ?1",
            params![asset_id],
        )?;
        connection.execute(
            "DELETE FROM embeddings WHERE asset_id = ?1",
            params![asset_id],
        )?;
        sync_search_row(&connection, asset_id)?;
        cleared += 1;
    }
    Ok(cleared)
}

fn sanitized_preview(path: &Path) -> AppResult<Vec<u8>> {
    let bytes = fs::read(path)?;
    let image = image::load_from_memory(&bytes)?;
    let (width, height) = image.dimensions();
    let image = if width.max(height) > 1600 {
        image.thumbnail(1600, 1600)
    } else {
        image
    };
    let rgb = image.to_rgb8();
    let mut output = Cursor::new(Vec::new());
    JpegEncoder::new_with_quality(&mut output, 82).encode_image(&rgb)?;
    Ok(output.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_key_is_not_stored() {
        assert!(store_api_key("   ").is_ok());
    }
}
