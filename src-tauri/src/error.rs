use std::io;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("数据库错误：{0}")]
    Database(#[from] rusqlite::Error),
    #[error("文件操作失败：{0}")]
    Io(#[from] io::Error),
    #[error("图片无法读取：{0}")]
    Image(#[from] image::ImageError),
    #[error("数据格式错误：{0}")]
    Json(#[from] serde_json::Error),
    #[error("网络请求失败：{0}")]
    Network(#[from] reqwest::Error),
    #[error("目录监听失败：{0}")]
    Watch(#[from] notify::Error),
    #[error("{0}")]
    Message(String),
}

impl From<keyring::Error> for AppError {
    fn from(value: keyring::Error) -> Self {
        Self::Message(format!("无法访问 Windows 凭据管理器：{value}"))
    }
}

#[cfg(target_os = "windows")]
impl From<windows::core::Error> for AppError {
    fn from(value: windows::core::Error) -> Self {
        Self::Message(format!("Windows 系统接口错误：{value}"))
    }
}

pub type AppResult<T> = Result<T, AppError>;

pub fn command_error(error: AppError) -> String {
    error.to_string()
}
