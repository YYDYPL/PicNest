use rusqlite::params;

use crate::{
    db::{sync_search_row, AppState},
    error::{AppError, AppResult},
};

#[cfg(target_os = "windows")]
fn recognize_path(path: &str) -> AppResult<String> {
    use windows::{
        core::HSTRING,
        Graphics::Imaging::BitmapDecoder,
        Media::Ocr::OcrEngine,
        Storage::{FileAccessMode, StorageFile},
    };

    let file = StorageFile::GetFileFromPathAsync(&HSTRING::from(path))?
        .get()
        .map_err(|error| AppError::Message(format!("Windows OCR 无法打开图片：{error}")))?;
    let stream = file
        .OpenAsync(FileAccessMode::Read)?
        .get()
        .map_err(|error| AppError::Message(format!("Windows OCR 无法读取图片：{error}")))?;
    let decoder = BitmapDecoder::CreateAsync(&stream)?
        .get()
        .map_err(|error| AppError::Message(format!("Windows OCR 无法解码图片：{error}")))?;
    let bitmap = decoder
        .GetSoftwareBitmapAsync()?
        .get()
        .map_err(|error| AppError::Message(format!("Windows OCR 无法准备图片：{error}")))?;
    let engine = OcrEngine::TryCreateFromUserProfileLanguages()?;
    let result = engine
        .RecognizeAsync(&bitmap)?
        .get()
        .map_err(|error| AppError::Message(format!("Windows OCR 识别失败：{error}")))?;
    Ok(result.Text()?.to_string())
}

#[cfg(not(target_os = "windows"))]
fn recognize_path(_path: &str) -> AppResult<String> {
    Err(AppError::Message("本地 OCR 当前仅支持 Windows".to_string()))
}

pub fn recognize_asset(state: &AppState, asset_id: i64) -> AppResult<String> {
    let connection = state.connection()?;
    let path: String = connection.query_row(
        "SELECT path FROM asset_locations WHERE asset_id = ?1 AND available = 1 ORDER BY needs_organize DESC, id LIMIT 1",
        params![asset_id],
        |row| row.get(0),
    )?;
    let text = recognize_path(&path)?;
    connection.execute(
        "UPDATE assets SET ocr_text = ?1 WHERE id = ?2",
        params![text, asset_id],
    )?;
    sync_search_row(&connection, asset_id)?;
    Ok(text)
}
