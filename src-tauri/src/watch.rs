use std::{
    path::{Path, PathBuf},
    sync::mpsc::{self, RecvTimeoutError},
    thread,
    time::Duration,
};

use notify::{Config, Event, RecommendedWatcher, RecursiveMode, Watcher};
use tauri::Emitter;

use crate::{db::AppState, error::AppResult, sources::SourceRule};

pub fn create_watcher(state: AppState, sources: &[SourceRule]) -> AppResult<RecommendedWatcher> {
    let (sender, receiver) = mpsc::channel::<notify::Result<Event>>();
    let mut watcher = RecommendedWatcher::new(
        move |event| {
            let _ = sender.send(event);
        },
        Config::default(),
    )?;

    for source in sources {
        let path = Path::new(&source.path);
        if path.is_dir() {
            watcher.watch(
                path,
                if source.recursive {
                    RecursiveMode::Recursive
                } else {
                    RecursiveMode::NonRecursive
                },
            )?;
        }
    }

    thread::spawn(move || {
        while let Ok(first) = receiver.recv() {
            let mut paths = Vec::<PathBuf>::new();
            collect_paths(first, &mut paths);
            loop {
                match receiver.recv_timeout(Duration::from_millis(650)) {
                    Ok(event) => collect_paths(event, &mut paths),
                    Err(RecvTimeoutError::Timeout) => break,
                    Err(RecvTimeoutError::Disconnected) => return,
                }
            }
            paths.sort();
            paths.dedup();
            if !paths.is_empty() {
                if let Err(error) = crate::library::process_watcher_paths(&state, &paths) {
                    log::warn!("Failed to refresh watched photos: {error}");
                } else {
                    let _ = state.app_handle.emit("library-changed", ());
                }
            }
        }
    });

    Ok(watcher)
}

fn collect_paths(event: notify::Result<Event>, paths: &mut Vec<PathBuf>) {
    match event {
        Ok(event) => paths.extend(event.paths),
        Err(error) => log::warn!("Photo folder watcher reported an error: {error}"),
    }
}
