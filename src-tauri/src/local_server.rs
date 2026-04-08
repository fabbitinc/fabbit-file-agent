#[cfg(not(feature = "mock"))]
use reqwest::blocking::Client;
use std::sync::{Arc, Mutex};
use tauri::Emitter;
use tiny_http::{Header, Response, Server};

#[cfg(not(feature = "mock"))]
pub const API_URL: &str = match option_env!("FABBIT_API_URL") {
    Some(v) => v,
    None => "https://api.fabbit.io",
};
const DEFAULT_PORT: u16 = 52847;
const ALLOWED_ORIGINS: &[&str] = &["https://fabbit.io", "http://localhost:3000"];

#[derive(Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct AuthState {
    pub logged_in: bool,
    pub username: String,
    pub access_token: Option<String>,
}

pub type SharedAuthState = Arc<Mutex<AuthState>>;

pub fn start(app_handle: tauri::AppHandle, auth_state: SharedAuthState) -> u16 {
    let port = find_available_port();

    let app = app_handle.clone();
    std::thread::spawn(move || {
        let addr = format!("127.0.0.1:{}", port);
        let server = Server::http(&addr).expect("Failed to start local HTTP server");
        println!("Local server listening on {}", addr);

        for mut request in server.incoming_requests() {
            let origin = request
                .headers()
                .iter()
                .find(|h| h.field.as_str() == "Origin")
                .map(|h| h.value.as_str().to_string());

            let cors_headers = build_cors_headers(origin.as_deref());

            // Preflight
            let method_str = format!("{}", request.method());
            if method_str == "OPTIONS" {
                let mut response = Response::from_string("").with_status_code(204);
                for h in &cors_headers {
                    response.add_header(h.clone());
                }
                let _ = request.respond(response);
                continue;
            }

            let url = request.url().to_string();
            let (status, body) = match (method_str.as_str(), url.as_str()) {
                ("GET", "/status") => handle_status(&auth_state),
                ("GET", path) if path.starts_with("/auth/callback") => {
                    handle_auth_callback(path, &auth_state, &app)
                }
                ("POST", "/download") => {
                    let mut req_body = String::new();
                    let _ = request.as_reader().read_to_string(&mut req_body);
                    handle_download(&req_body, &auth_state, &app)
                }
                ("GET", "/update/check") => handle_update_check(),
                ("GET", "/upload/status") => handle_upload_status(&app),
                _ => (404, json_response("error", "Not found")),
            };

            let mut response = Response::from_string(&body)
                .with_status_code(status)
                .with_header(Header::from_bytes("Content-Type", "application/json").unwrap());
            for h in &cors_headers {
                response.add_header(h.clone());
            }
            let _ = request.respond(response);
        }
    });

    port
}

fn find_available_port() -> u16 {
    // 기본 포트 시도, 실패 시 OS가 할당
    if std::net::TcpListener::bind(format!("127.0.0.1:{}", DEFAULT_PORT)).is_ok() {
        return DEFAULT_PORT;
    }
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.local_addr().unwrap().port()
}

fn build_cors_headers(origin: Option<&str>) -> Vec<Header> {
    let allowed = origin
        .filter(|o| ALLOWED_ORIGINS.iter().any(|a| a == o))
        .unwrap_or(ALLOWED_ORIGINS[0]);

    vec![
        Header::from_bytes("Access-Control-Allow-Origin", allowed).unwrap(),
        Header::from_bytes("Access-Control-Allow-Methods", "GET, POST, OPTIONS").unwrap(),
        Header::from_bytes("Access-Control-Allow-Headers", "Content-Type").unwrap(),
    ]
}

// GET /status
fn handle_status(auth_state: &SharedAuthState) -> (i32, String) {
    let state = auth_state.lock().unwrap();
    let body = serde_json::json!({
        "version": env!("CARGO_PKG_VERSION"),
        "running": true,
        "loggedIn": state.logged_in,
        "user": state.username,
    });
    (200, body.to_string())
}

// GET /auth/callback?code=xxx
fn handle_auth_callback(
    path: &str,
    auth_state: &SharedAuthState,
    app: &tauri::AppHandle,
) -> (i32, String) {
    let code = path.split('?').nth(1).and_then(|q| {
        q.split('&')
            .find(|p| p.starts_with("code="))
            .map(|p| p.trim_start_matches("code=").to_string())
    });

    let Some(code) = code else {
        return (400, json_response("error", "Missing code parameter"));
    };

    println!("Auth callback received code: {}", code);

    exchange_token(code, auth_state, app)
}

#[cfg(feature = "mock")]
fn exchange_token(
    code: String,
    auth_state: &SharedAuthState,
    app: &tauri::AppHandle,
) -> (i32, String) {
    let mut state = auth_state.lock().unwrap();
    state.logged_in = true;
    state.username = "홍길동".to_string();
    state.access_token = Some(format!("mock_access_token_{}", code));

    let _ = app.emit(
        "auth-changed",
        serde_json::json!({
            "loggedIn": true,
            "user": &state.username,
        }),
    );

    (
        200,
        json_response("success", "로그인 완료. 이 창을 닫아도 됩니다."),
    )
}

#[cfg(not(feature = "mock"))]
fn exchange_token(
    code: String,
    auth_state: &SharedAuthState,
    app: &tauri::AppHandle,
) -> (i32, String) {
    let client = match Client::builder().build() {
        Ok(client) => client,
        Err(error) => {
            return (
                500,
                json_response(
                    "error",
                    &format!("HTTP 클라이언트를 초기화하지 못했습니다: {error}"),
                ),
            )
        }
    };

    let response = match client
        .post(format!("{API_URL}/oauth/token"))
        .json(&serde_json::json!({
            "code": code,
            "client_id": "fabbit-agent",
        }))
        .send()
    {
        Ok(response) => response,
        Err(error) => {
            return (
                502,
                json_response("error", &format!("토큰 교환 요청에 실패했습니다: {error}")),
            )
        }
    };

    let status = response.status();
    let body = match response.text() {
        Ok(body) => body,
        Err(error) => {
            return (
                502,
                json_response("error", &format!("토큰 응답을 읽지 못했습니다: {error}")),
            )
        }
    };

    if !status.is_success() {
        return (
            status.as_u16() as i32,
            json_response("error", &format!("토큰 교환이 실패했습니다: {body}")),
        );
    }

    let parsed: serde_json::Value = match serde_json::from_str(&body) {
        Ok(parsed) => parsed,
        Err(error) => {
            return (
                502,
                json_response(
                    "error",
                    &format!("토큰 응답 JSON 파싱에 실패했습니다: {error}"),
                ),
            )
        }
    };

    let access_token = parsed
        .get("access_token")
        .and_then(|value| value.as_str())
        .map(str::to_string);

    let Some(access_token) = access_token else {
        return (
            502,
            json_response("error", "토큰 응답에 access_token이 없습니다."),
        );
    };

    let username = parsed
        .get("user")
        .and_then(|user| user.get("name").or_else(|| user.get("username")))
        .and_then(|value| value.as_str())
        .unwrap_or("로그인됨")
        .to_string();

    let mut state = auth_state.lock().unwrap();
    state.logged_in = true;
    state.username = username.clone();
    state.access_token = Some(access_token);

    let _ = app.emit(
        "auth-changed",
        serde_json::json!({
            "loggedIn": true,
            "user": username,
        }),
    );

    (
        200,
        json_response("success", "로그인 완료. 이 창을 닫아도 됩니다."),
    )
}

// POST /download { "fileId": "abc123" }
fn handle_download(
    body: &str,
    auth_state: &SharedAuthState,
    app: &tauri::AppHandle,
) -> (i32, String) {
    let state = auth_state.lock().unwrap();
    if !state.logged_in {
        return (401, json_response("error", "Not authenticated"));
    }

    let parsed: Result<serde_json::Value, _> = serde_json::from_str(body);
    let file_id = parsed
        .ok()
        .and_then(|v| v.get("fileId").and_then(|f| f.as_str().map(String::from)));

    let Some(file_id) = file_id else {
        return (400, json_response("error", "Missing fileId"));
    };

    download_file(&file_id, app)
}

#[cfg(feature = "mock")]
fn download_file(file_id: &str, app: &tauri::AppHandle) -> (i32, String) {
    let folder = crate::shell_folder::runtime_target_folder(app);
    let file_path = folder.join(format!("{}.txt", file_id));

    let mock_content = format!(
        "Fabbit 파일 (ID: {})\n다운로드 시각: {:?}\n\n이 파일을 수정한 후 업로드하세요.",
        file_id,
        std::time::SystemTime::now()
    );
    if let Err(e) = std::fs::write(&file_path, &mock_content) {
        return (
            500,
            json_response("error", &format!("File write failed: {}", e)),
        );
    }

    let _ = app.emit(
        "file-downloaded",
        serde_json::json!({
            "fileId": file_id,
            "path": file_path.to_string_lossy(),
        }),
    );

    (
        200,
        serde_json::json!({
            "status": "success",
            "fileId": file_id,
            "path": file_path.to_string_lossy(),
        })
        .to_string(),
    )
}

#[cfg(not(feature = "mock"))]
fn download_file(_file_id: &str, _app: &tauri::AppHandle) -> (i32, String) {
    // TODO: GET https://api.fabbit.io/files/{fileId} Authorization: Bearer {access_token}
    (501, json_response("error", "Not implemented"))
}

// GET /update/check
fn handle_update_check() -> (i32, String) {
    let info = crate::updater::check();
    let body = serde_json::json!({
        "currentVersion": info.current_version,
        "latestVersion": info.latest_version,
        "updateAvailable": info.update_available,
        "mandatory": info.mandatory,
        "downloadUrl": info.download_url,
        "releaseNotes": info.release_notes,
    });
    (200, body.to_string())
}

// GET /upload/status
fn handle_upload_status(app: &tauri::AppHandle) -> (i32, String) {
    // file_watcher에서 pending 파일 목록을 가져와야 하지만
    // 지금은 Fabbit 폴더 내 파일 목록을 반환
    let folder = crate::shell_folder::runtime_target_folder(app);
    let files: Vec<String> = std::fs::read_dir(&folder)
        .map(|entries| {
            entries
                .filter_map(|e| e.ok())
                .filter(|e| e.path().is_file())
                .filter(|e| !e.file_name().to_string_lossy().starts_with('.'))
                .map(|e| e.file_name().to_string_lossy().to_string())
                .collect()
        })
        .unwrap_or_default();

    let _ = app; // 향후 이벤트 발행용
    let body = serde_json::json!({
        "pendingCount": files.len(),
        "files": files,
    });
    (200, body.to_string())
}

fn json_response(key: &str, message: &str) -> String {
    serde_json::json!({ key: message }).to_string()
}
