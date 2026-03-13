mod autostart;
mod file_watcher;
mod local_server;
mod shell_folder;
mod updater;

use std::sync::{Arc, Mutex};
use tauri::{
    menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem},
    tray::TrayIconBuilder,
    Emitter, Listener, Manager,
};

#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! Fabbit에서 인사드립니다!", name)
}

#[tauri::command]
fn unregister_shell_folder() -> Result<(), String> {
    shell_folder::unregister().map_err(|e| e.to_string())
}

#[tauri::command]
fn install_update(app: tauri::AppHandle) -> Result<(), String> {
    let info = updater::check();
    if info.update_available {
        updater::run_installer(&app, &info);
        Ok(())
    } else {
        Err("No update available".to_string())
    }
}

fn create_badge_icon(app: &tauri::AppHandle, mandatory: bool) -> Option<tauri::image::Image<'static>> {
    let icon = app.default_window_icon()?;
    let w = icon.width();
    let h = icon.height();
    let mut rgba = icon.rgba().to_vec();

    let (cr, cg, cb) = if mandatory {
        (220u8, 38, 38)    // 빨간색 (필수)
    } else {
        (245u8, 158, 11)   // 주황색 (선택)
    };

    // 우하단에 원 그리기 (아이콘의 1/4 크기)
    let r = (w.min(h) / 4) as i32;
    let cx = w as i32 - r - 1;
    let cy = h as i32 - r - 1;

    for y in 0..h as i32 {
        for x in 0..w as i32 {
            let dx = x - cx;
            let dy = y - cy;
            if dx * dx + dy * dy <= r * r {
                let i = ((y as u32 * w + x as u32) * 4) as usize;
                rgba[i] = cr;
                rgba[i + 1] = cg;
                rgba[i + 2] = cb;
                rgba[i + 3] = 255;
            }
        }
    }

    Some(tauri::image::Image::new_owned(rgba, w, h))
}

fn show_window(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            show_window(app);
        }))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .invoke_handler(tauri::generate_handler![greet, unregister_shell_folder, install_update])
        .setup(|app| {
            // 공유 인증 상태
            let auth_state: local_server::SharedAuthState =
                Arc::new(Mutex::new(local_server::AuthState::default()));

            // 공유 업데이트 정보
            let update_info: Arc<Mutex<Option<updater::UpdateInfo>>> =
                Arc::new(Mutex::new(None));

            // Localhost HTTP 서버 시작
            let port = local_server::start(app.handle().clone(), auth_state.clone());
            println!("Fabbit local server started on port {}", port);

            // 셸 폴더 등록
            let icon_path = app
                .path()
                .resource_dir()
                .map(|d| d.join("icons").join("icon.ico"))
                .unwrap_or_else(|_| {
                    std::env::current_exe()
                        .unwrap()
                        .parent()
                        .unwrap()
                        .join("icons")
                        .join("icon.ico")
                });

            if !shell_folder::is_registered() {
                if let Err(e) = shell_folder::register(&icon_path.to_string_lossy()) {
                    eprintln!("Failed to register shell folder: {}", e);
                }
            }
            let _ = std::fs::create_dir_all(shell_folder::target_folder());

            // 파일 감시 시작
            let watch_path = shell_folder::target_folder();
            file_watcher::start_watching(app.handle().clone(), watch_path);

            // 트레이 메뉴
            let state = auth_state.lock().unwrap();
            let display_name = if state.logged_in {
                state.username.clone()
            } else {
                "로그인 필요".to_string()
            };
            drop(state);

            let app_title = MenuItem::with_id(
                app,
                "app_title",
                format!("Fabbit v{}", env!("CARGO_PKG_VERSION")),
                false,
                None::<&str>,
            )?;
            let sep0 = PredefinedMenuItem::separator(app)?;
            let is_logged_in = auth_state.lock().unwrap().logged_in;
            let user =
                MenuItem::with_id(app, "user", &display_name, !is_logged_in, None::<&str>)?;
            let sep1 = PredefinedMenuItem::separator(app)?;
            let open_folder =
                MenuItem::with_id(app, "open_folder", "폴더 열기", true, None::<&str>)?;
            let sep2 = PredefinedMenuItem::separator(app)?;
            let autostart = CheckMenuItem::with_id(
                app,
                "autostart",
                "Windows 시작 시 자동 실행",
                true,
                autostart::is_enabled(),
                None::<&str>,
            )?;
            let sep3 = PredefinedMenuItem::separator(app)?;
            // 업데이트 메뉴 (처음에는 비활성)
            let update_menu =
                MenuItem::with_id(app, "update", "업데이트 확인 중...", false, None::<&str>)?;
            let sep4 = PredefinedMenuItem::separator(app)?;
            let quit = MenuItem::with_id(app, "quit", "종료", true, None::<&str>)?;
            let menu = Menu::with_items(
                app,
                &[
                    &app_title,
                    &sep0,
                    &user,
                    &sep1,
                    &open_folder,
                    &sep2,
                    &autostart,
                    &sep3,
                    &update_menu,
                    &sep4,
                    &quit,
                ],
            )?;

            // auth-changed 이벤트
            let user_item = user.clone();
            app.listen("auth-changed", move |event| {
                if let Ok(payload) = serde_json::from_str::<serde_json::Value>(event.payload()) {
                    if let Some(name) = payload.get("user").and_then(|u| u.as_str()) {
                        let _ = user_item.set_text(name);
                        let _ = user_item.set_enabled(false);
                    }
                }
            });

            // update-available 이벤트 → 트레이 메뉴 업데이트 + 강제 시 창 표시
            let update_item = update_menu.clone();
            let update_info_for_event = update_info.clone();
            let app_for_event = app.handle().clone();
            app.listen("update-available", move |event| {
                if let Ok(info) =
                    serde_json::from_str::<updater::UpdateInfo>(event.payload())
                {
                    let label = if info.mandatory {
                        format!("⚠ 필수 업데이트 (v{})", info.latest_version)
                    } else {
                        format!("업데이트 설치 (v{})", info.latest_version)
                    };
                    let _ = update_item.set_text(&label);
                    let _ = update_item.set_enabled(true);

                    // 트레이 아이콘에 뱃지 표시
                    if let Some(tray) = app_for_event.tray_by_id("main") {
                        if let Some(badge) = create_badge_icon(&app_for_event, info.mandatory) {
                            let _ = tray.set_icon(Some(badge));
                            let _ = tray.set_tooltip(Some(if info.mandatory {
                                "Fabbit - 필수 업데이트 필요"
                            } else {
                                "Fabbit - 업데이트 가능"
                            }));
                        }
                    }

                    *update_info_for_event.lock().unwrap() = Some(info.clone());

                    // 강제 업데이트: 창을 띄워서 차단 UI 표시
                    if info.mandatory {
                        let _ = app_for_event.emit("mandatory-update", &info);
                        show_window(&app_for_event);
                    }
                }
            });

            let update_info_for_menu = update_info.clone();
            TrayIconBuilder::with_id("main")
                .icon(app.default_window_icon().unwrap().clone())
                .tooltip("Fabbit")
                .show_menu_on_left_click(false)
                .menu(&menu)
                .on_menu_event(move |app, event| match event.id.as_ref() {
                    "user" => {
                        let _ =
                            open::that("http://localhost:52847/auth/callback?code=mock_login");
                    }
                    "open_folder" => {
                        let _ = std::process::Command::new("explorer")
                            .arg(shell_folder::shell_uri())
                            .spawn();
                    }
                    "autostart" => {
                        if let Err(e) = autostart::toggle() {
                            eprintln!("Failed to toggle autostart: {}", e);
                        }
                    }
                    "update" => {
                        let info = update_info_for_menu.lock().unwrap();
                        if let Some(info) = info.as_ref() {
                            updater::run_installer(app, info);
                        }
                    }
                    "quit" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let tauri::tray::TrayIconEvent::DoubleClick { .. } = event {
                        show_window(tray.app_handle());
                    }
                })
                .build(app)?;

            // 업데이트 확인 (시작 시 + 주기적)
            updater::start_periodic_check(app.handle().clone());

            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
