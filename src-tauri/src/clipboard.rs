use std::{borrow::Cow, mem, os::windows::ffi::OsStrExt, path::PathBuf, ptr, slice};

use windows::{
    core::BOOL,
    Win32::{
        Foundation::{GlobalFree, HANDLE, HGLOBAL},
        System::{
            DataExchange::{
                CloseClipboard, EmptyClipboard, OpenClipboard, RegisterClipboardFormatW,
                SetClipboardData,
            },
            Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE},
            Ole::{CF_HDROP, DROPEFFECT_COPY},
        },
        UI::Shell::{CFSTR_PREFERREDDROPEFFECT, DROPFILES},
    },
};

use crate::error::{AppError, AppResult};

pub fn copy_files(window: &tauri::WebviewWindow, paths: &[PathBuf]) -> AppResult<()> {
    let hwnd = window
        .hwnd()
        .map_err(|error| AppError::Message(format!("无法访问应用窗口：{error}")))?;
    let files = ClipboardMemory::new(&file_drop_bytes(paths)?)?;
    let copy_effect = ClipboardMemory::new(&DROPEFFECT_COPY.0.to_le_bytes())?;

    // Prepare both blocks before replacing the clipboard contents.
    unsafe {
        let effect_format = RegisterClipboardFormatW(CFSTR_PREFERREDDROPEFFECT);
        if effect_format == 0 {
            return Err(windows::core::Error::from_win32().into());
        }
        OpenClipboard(Some(hwnd))
            .map_err(|_| AppError::Message("无法打开系统剪贴板，请重试".to_string()))?;
        let _clipboard = OpenClipboardGuard;
        EmptyClipboard()?;
        copy_effect.transfer(effect_format)?;
        files.transfer(CF_HDROP.0.into())?;
    }
    Ok(())
}

fn file_drop_bytes(paths: &[PathBuf]) -> AppResult<Vec<u8>> {
    let header = DROPFILES {
        pFiles: mem::size_of::<DROPFILES>() as u32,
        fWide: BOOL(1),
        ..Default::default()
    };
    // DROPFILES is packed and all its fields are initialized.
    let mut bytes = unsafe {
        slice::from_raw_parts(
            ptr::addr_of!(header).cast::<u8>(),
            mem::size_of::<DROPFILES>(),
        )
    }
    .to_vec();
    for path in paths {
        let path = if path.is_absolute() {
            Cow::Borrowed(path.as_path())
        } else {
            Cow::Owned(std::env::current_dir()?.join(path))
        };
        bytes.extend(path.as_os_str().encode_wide().flat_map(u16::to_le_bytes));
        bytes.extend_from_slice(&0u16.to_le_bytes());
    }
    bytes.extend_from_slice(&0u16.to_le_bytes());
    Ok(bytes)
}

struct OpenClipboardGuard;

impl Drop for OpenClipboardGuard {
    fn drop(&mut self) {
        let _ = unsafe { CloseClipboard() };
    }
}

struct ClipboardMemory {
    handle: HGLOBAL,
}

impl ClipboardMemory {
    fn new(bytes: &[u8]) -> AppResult<Self> {
        unsafe {
            let memory = Self {
                handle: GlobalAlloc(GMEM_MOVEABLE, bytes.len())?,
            };
            let target = GlobalLock(memory.handle).cast::<u8>();
            if target.is_null() {
                return Err(windows::core::Error::from_win32().into());
            }
            ptr::copy_nonoverlapping(bytes.as_ptr(), target, bytes.len());
            let _ = GlobalUnlock(memory.handle);
            Ok(memory)
        }
    }

    unsafe fn transfer(self, format: u32) -> AppResult<()> {
        SetClipboardData(format, Some(HANDLE(self.handle.0)))?;
        // Windows owns the allocation after SetClipboardData succeeds.
        mem::forget(self);
        Ok(())
    }
}

impl Drop for ClipboardMemory {
    fn drop(&mut self) {
        let _ = unsafe { GlobalFree(Some(self.handle)) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::UI::Shell::{DragQueryFileW, HDROP};

    #[test]
    fn windows_shell_reads_multiple_unicode_paths() {
        let paths = [
            PathBuf::from(r"C:\照片\风景.jpg"),
            PathBuf::from(r"D:\New Folder\a.png"),
        ];
        let bytes = file_drop_bytes(&paths).expect("file drop data");
        let memory = ClipboardMemory::new(&bytes).expect("file drop memory");
        let handle = HDROP(memory.handle.0);
        assert_eq!(unsafe { DragQueryFileW(handle, u32::MAX, None) }, 2);
        for (index, path) in paths.iter().enumerate() {
            let mut buffer = [0; 128];
            let len = unsafe { DragQueryFileW(handle, index as u32, Some(&mut buffer)) };
            assert_eq!(
                &buffer[..len as usize],
                path.as_os_str().encode_wide().collect::<Vec<_>>()
            );
        }
    }
}
