use notify_debouncer_mini::{new_debouncer, DebouncedEventKind};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::Emitter;

#[derive(Default)]
pub struct WatchController {
    stop_tx: Option<std::sync::mpsc::Sender<()>>,
    current_path: Option<PathBuf>,
}

pub type SharedWatchController = Arc<Mutex<WatchController>>;

#[derive(Clone, serde::Serialize)]
struct PendingFiles {
    count: usize,
    files: Vec<String>,
}

pub fn update_watch_path(
    app_handle: tauri::AppHandle,
    watch_state: SharedWatchController,
    watch_path: PathBuf,
) -> Result<(), String> {
    std::fs::create_dir_all(&watch_path).map_err(|e| e.to_string())?;

    let mut state = watch_state.lock().unwrap();
    if state.current_path.as_ref() == Some(&watch_path) {
        return Ok(());
    }

    if let Some(stop_tx) = state.stop_tx.take() {
        let _ = stop_tx.send(());
    }

    let (stop_tx, stop_rx) = std::sync::mpsc::channel();
    state.stop_tx = Some(stop_tx);
    state.current_path = Some(watch_path.clone());
    drop(state);

    let pending: Arc<Mutex<std::collections::HashSet<PathBuf>>> =
        Arc::new(Mutex::new(std::collections::HashSet::new()));

    let pending_clone = pending.clone();
    let app = app_handle.clone();

    std::thread::spawn(move || {
        let (tx, rx) = std::sync::mpsc::channel();

        let mut debouncer =
            new_debouncer(Duration::from_secs(2), tx).expect("Failed to create file watcher");

        debouncer
            .watcher()
            .watch(&watch_path, notify::RecursiveMode::Recursive)
            .expect("Failed to watch directory");

        loop {
            if stop_rx.try_recv().is_ok() {
                break;
            }

            match rx.recv_timeout(Duration::from_millis(500)) {
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
                            files: pending.iter().map(|p| p.display().to_string()).collect(),
                        };
                        let _ = app.emit("files-pending-upload", &payload);
                    }
                }
                Ok(Err(e)) => {
                    eprintln!("File watch error: {:?}", e);
                }
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
            }
        }
    });

    Ok(())
}
