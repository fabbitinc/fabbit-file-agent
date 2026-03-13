use notify_debouncer_mini::{new_debouncer, DebouncedEventKind};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::Emitter;

#[derive(Clone, serde::Serialize)]
struct PendingFiles {
    count: usize,
    files: Vec<String>,
}

pub fn start_watching(app_handle: tauri::AppHandle, watch_path: PathBuf) {
    let pending: Arc<Mutex<std::collections::HashSet<PathBuf>>> =
        Arc::new(Mutex::new(std::collections::HashSet::new()));

    let pending_clone = pending.clone();
    let app = app_handle.clone();

    std::thread::spawn(move || {
        let (tx, rx) = std::sync::mpsc::channel();

        let mut debouncer = new_debouncer(Duration::from_secs(2), tx)
            .expect("Failed to create file watcher");

        debouncer
            .watcher()
            .watch(&watch_path, notify::RecursiveMode::Recursive)
            .expect("Failed to watch directory");

        loop {
            match rx.recv() {
                Ok(Ok(events)) => {
                    let mut pending = pending_clone.lock().unwrap();
                    for event in events {
                        if event.kind == DebouncedEventKind::Any {
                            // 임시 파일, 숨김 파일 무시
                            if let Some(name) = event.path.file_name() {
                                let name = name.to_string_lossy();
                                if name.starts_with('.') || name.starts_with('~') {
                                    continue;
                                }
                            }
                            if event.path.is_file() {
                                pending.insert(event.path.clone());
                            }
                        }
                    }

                    if !pending.is_empty() {
                        let payload = PendingFiles {
                            count: pending.len(),
                            files: pending
                                .iter()
                                .map(|p| p.display().to_string())
                                .collect(),
                        };
                        let _ = app.emit("files-pending-upload", &payload);
                    }
                }
                Ok(Err(e)) => {
                    eprintln!("File watch error: {:?}", e);
                }
                Err(_) => break,
            }
        }
    });
}
