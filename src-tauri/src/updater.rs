use std::time::Duration;
use tauri::Emitter;
use tauri_plugin_notification::NotificationExt;

#[cfg(not(feature = "mock"))]
const RELEASES_URL: &str = match option_env!("FABBIT_RELEASES_URL") {
    Some(v) => v,
    None => "https://releases.fabbit.io/latest.json",
};
const CHECK_INTERVAL_SECS: u64 = 60 * 60; // 1시간마다 확인
const INITIAL_DELAY_SECS: u64 = 3;

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct UpdateInfo {
    pub current_version: String,
    pub latest_version: String,
    pub update_available: bool,
    pub mandatory: bool,
    pub download_url: String,
    pub release_notes: String,
}

#[cfg(feature = "mock")]
pub fn check() -> UpdateInfo {
    let current = env!("CARGO_PKG_VERSION").to_string();
    let latest = "0.2.0".to_string();
    let update_available = latest != current;

    let installer_path = dirs::home_dir()
        .unwrap()
        .join("Fabbit")
        .join("_updates")
        .join("Fabbit_0.2.0_x64-setup.exe");

    UpdateInfo {
        current_version: current,
        latest_version: latest,
        update_available,
        mandatory: false,
        download_url: installer_path.to_string_lossy().to_string(),
        release_notes: "새 기능: 파일 자동 업로드, 버그 수정".to_string(),
    }
}

#[cfg(not(feature = "mock"))]
pub fn check() -> UpdateInfo {
    // TODO: GET https://releases.fabbit.io/latest.json
    let current = env!("CARGO_PKG_VERSION").to_string();
    UpdateInfo {
        current_version: current.clone(),
        latest_version: current,
        update_available: false,
        mandatory: false,
        download_url: String::new(),
        release_notes: String::new(),
    }
}

pub fn start_periodic_check(app_handle: tauri::AppHandle) {
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(INITIAL_DELAY_SECS));

        loop {
            let info = check();

            if info.update_available {
                let _ = app_handle.emit("update-available", &info);

                if info.mandatory {
                    handle_mandatory_update(&app_handle, &info);
                } else {
                    handle_optional_update(&app_handle, &info);
                }
            }

            std::thread::sleep(Duration::from_secs(CHECK_INTERVAL_SECS));
        }
    });
}

fn handle_mandatory_update(app: &tauri::AppHandle, info: &UpdateInfo) {
    let _ = app
        .notification()
        .builder()
        .title("Fabbit 필수 업데이트")
        .body(format!(
            "보안 업데이트가 필요합니다. (v{} → v{})",
            info.current_version, info.latest_version
        ))
        .show();

    // 강제: 창을 띄워서 차단 UI 표시 (사용자가 버튼 클릭 시 설치)
    // run_installer는 프론트엔드에서 Tauri command로 호출
}

fn handle_optional_update(app: &tauri::AppHandle, info: &UpdateInfo) {
    let _ = app
        .notification()
        .builder()
        .title("Fabbit 업데이트 가능")
        .body(format!(
            "v{} → v{}\n{}",
            info.current_version, info.latest_version, info.release_notes
        ))
        .show();
}

pub fn run_installer(app: &tauri::AppHandle, info: &UpdateInfo) {
    let installer_path = &info.download_url;

    // TODO: 실제 구현 시 download_url이 HTTP URL이면 다운로드 후 실행
    // mock에서는 로컬 파일 경로를 직접 사용
    if std::path::Path::new(installer_path).exists() {
        println!("업데이트 설치 시작: {}", installer_path);

        // /P = passive 모드 (진행바만 표시, 사용자 입력 없음)
        // /R = 설치 후 앱 재시작
        let _ = std::process::Command::new(installer_path)
            .args(["/P", "/R"])
            .spawn();

        // 현재 앱 종료 → installer가 업데이트 후 재시작
        app.exit(0);
    } else {
        eprintln!("Installer not found: {}", installer_path);
    }
}
