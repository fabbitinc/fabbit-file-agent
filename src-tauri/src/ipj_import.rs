use crate::local_server::SharedAuthState;
#[cfg(not(feature = "mock"))]
use reqwest::blocking::Client;
use roxmltree::Document;
use serde::Serialize;
#[cfg(not(feature = "mock"))]
use std::collections::HashMap;
use std::{
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};
use tauri::{AppHandle, Emitter, State};
use tauri_plugin_dialog::{DialogExt, FilePath};
use walkdir::WalkDir;

#[cfg(not(feature = "mock"))]
const API_URL: &str = match option_env!("FABBIT_API_URL") {
    Some(v) => v,
    None => "https://api.fabbit.io",
};

#[derive(Clone, Copy, Default)]
enum ImportStatus {
    #[default]
    Idle,
    Ready,
    Uploading,
    Completed,
    Failed,
}

impl ImportStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Ready => "ready",
            Self::Uploading => "uploading",
            Self::Completed => "completed",
            Self::Failed => "failed",
        }
    }
}

#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ImportFileType {
    Part,
    Assembly,
    Drawing,
    Attachment,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportManifestFile {
    pub path: String,
    #[serde(rename = "type")]
    pub file_type: ImportFileType,
    pub size_bytes: u64,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ImportManifest {
    pub project_name: String,
    pub ipj_path: String,
    pub inventor_version: Option<String>,
    pub files: Vec<ImportManifestFile>,
}

#[cfg_attr(feature = "mock", allow(dead_code))]
#[derive(Clone)]
struct ScannedFile {
    pub absolute_path: PathBuf,
    pub manifest: ImportManifestFile,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportSummary {
    pub total_files: usize,
    pub total_bytes: u64,
    pub part_count: usize,
    pub assembly_count: usize,
    pub drawing_count: usize,
    pub attachment_count: usize,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportAnalysis {
    pub selected_root: String,
    pub ipj_path: String,
    pub workspace_path: String,
    pub project_name: String,
    pub project_type: Option<String>,
    pub inventor_version: Option<String>,
    pub summary: ImportSummary,
    pub warnings: Vec<String>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UploadProgress {
    pub total_files: usize,
    pub uploaded_files: usize,
    pub total_bytes: u64,
    pub uploaded_bytes: u64,
    pub current_file: Option<String>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportStateSnapshot {
    pub status: String,
    pub analysis: Option<ImportAnalysis>,
    pub progress: Option<UploadProgress>,
    pub last_error: Option<String>,
}

#[cfg_attr(feature = "mock", allow(dead_code))]
#[derive(Clone)]
struct PreparedImport {
    pub analysis: ImportAnalysis,
    pub manifest: ImportManifest,
    pub files: Vec<ScannedFile>,
}

#[derive(Default)]
pub struct ImportState {
    status: ImportStatus,
    analysis: Option<ImportAnalysis>,
    progress: Option<UploadProgress>,
    last_error: Option<String>,
    prepared: Option<PreparedImport>,
}

pub type SharedImportState = Arc<Mutex<ImportState>>;

#[derive(Default)]
struct ParsedIpj {
    project_name: Option<String>,
    project_type: Option<String>,
    workspace_path: Option<String>,
    library_paths: Vec<String>,
    content_center_paths: Vec<String>,
    inventor_version: Option<String>,
}

#[cfg(not(feature = "mock"))]
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct ImportInitResponse {
    #[serde(
        default,
        alias = "uploadTargets",
        alias = "upload_targets",
        alias = "uploads"
    )]
    files: Vec<UploadTarget>,
}

#[cfg(not(feature = "mock"))]
#[derive(Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct UploadTarget {
    path: Option<String>,
    #[serde(alias = "upload_url", alias = "presignedUrl", alias = "url")]
    upload_url: String,
    #[serde(default)]
    method: Option<String>,
    #[serde(default)]
    headers: HashMap<String, String>,
}

impl ImportState {
    fn snapshot(&self) -> ImportStateSnapshot {
        ImportStateSnapshot {
            status: self.status.as_str().to_string(),
            analysis: self.analysis.clone(),
            progress: self.progress.clone(),
            last_error: self.last_error.clone(),
        }
    }
}

#[tauri::command]
pub fn pick_import_folder(app: AppHandle) -> Result<Option<String>, String> {
    Ok(app
        .dialog()
        .file()
        .set_title("Inventor 프로젝트 폴더 선택")
        .blocking_pick_folder()
        .map(file_path_to_string))
}

#[tauri::command]
pub fn analyze_import_folder(
    folder_path: String,
    app: AppHandle,
    import_state: State<'_, SharedImportState>,
) -> Result<ImportAnalysis, String> {
    let shared = import_state.inner().clone();

    {
        let state = shared.lock().unwrap();
        if matches!(state.status, ImportStatus::Uploading) {
            return Err("업로드가 진행 중입니다. 완료 후 다시 시도해 주세요.".to_string());
        }
    }

    let prepared = build_prepared_import(&folder_path)?;
    let analysis = prepared.analysis.clone();

    {
        let mut state = shared.lock().unwrap();
        state.status = ImportStatus::Ready;
        state.analysis = Some(analysis.clone());
        state.progress = None;
        state.last_error = None;
        state.prepared = Some(prepared);
    }

    emit_state(&app, &shared);

    Ok(analysis)
}

#[tauri::command]
pub fn start_import_upload(
    app: AppHandle,
    import_state: State<'_, SharedImportState>,
    auth_state: State<'_, SharedAuthState>,
) -> Result<ImportStateSnapshot, String> {
    let shared = import_state.inner().clone();
    let auth = auth_state.inner().clone();

    let access_token = {
        let state = auth.lock().unwrap();
        state.access_token.clone()
    }
    .ok_or_else(|| "로그인이 필요합니다. 트레이 메뉴에서 먼저 로그인해 주세요.".to_string())?;

    let prepared = {
        let mut state = shared.lock().unwrap();

        if matches!(state.status, ImportStatus::Uploading) {
            return Err("이미 업로드가 진행 중입니다.".to_string());
        }

        let prepared = state
            .prepared
            .clone()
            .ok_or_else(|| "먼저 IPJ 폴더를 분석해 주세요.".to_string())?;

        let total_bytes = prepared
            .files
            .iter()
            .map(|file| file.manifest.size_bytes)
            .sum();

        state.status = ImportStatus::Uploading;
        state.last_error = None;
        state.progress = Some(UploadProgress {
            total_files: prepared.files.len(),
            uploaded_files: 0,
            total_bytes,
            uploaded_bytes: 0,
            current_file: None,
        });

        prepared
    };

    let initial_snapshot = snapshot_from_shared(&shared);
    let _ = app.emit("ipj-import-state", &initial_snapshot);

    std::thread::spawn(move || {
        let result = upload_prepared(&app, &shared, &prepared, &access_token);

        if let Err(message) = result {
            {
                let mut state = shared.lock().unwrap();
                state.status = ImportStatus::Failed;
                state.last_error = Some(message);
            }
            let snapshot = snapshot_from_shared(&shared);
            let _ = app.emit("ipj-import-state", &snapshot);
            let _ = app.emit("ipj-upload-failed", &snapshot);
        }
    });

    Ok(initial_snapshot)
}

#[tauri::command]
pub fn get_import_state(
    import_state: State<'_, SharedImportState>,
) -> Result<ImportStateSnapshot, String> {
    Ok(snapshot_from_shared(import_state.inner()))
}

pub fn snapshot_from_shared(shared: &SharedImportState) -> ImportStateSnapshot {
    shared.lock().unwrap().snapshot()
}

fn emit_state(app: &AppHandle, shared: &SharedImportState) {
    let snapshot = snapshot_from_shared(shared);
    let _ = app.emit("ipj-import-state", &snapshot);
}

fn build_prepared_import(folder_path: &str) -> Result<PreparedImport, String> {
    let selected_root = PathBuf::from(folder_path);
    if !selected_root.exists() || !selected_root.is_dir() {
        return Err("선택한 폴더를 찾을 수 없습니다.".to_string());
    }

    let ipj_files = find_ipj_files(&selected_root)?;
    let ipj_path = match ipj_files.as_slice() {
        [] => {
            return Err("선택한 폴더에서 IPJ 파일을 찾을 수 없습니다.".to_string());
        }
        [path] => path.clone(),
        _ => {
            return Err(
                "선택한 폴더에서 여러 IPJ 파일이 발견되었습니다. 1차 버전은 단일 IPJ 폴더만 지원합니다."
                    .to_string(),
            );
        }
    };

    let xml =
        fs::read_to_string(&ipj_path).map_err(|e| format!("IPJ 파일을 읽지 못했습니다: {e}"))?;
    let parsed = parse_ipj(&xml)?;

    let mut warnings = Vec::new();
    let ipj_dir = ipj_path
        .parent()
        .ok_or_else(|| "IPJ 파일의 부모 폴더를 확인할 수 없습니다.".to_string())?;

    let workspace_root =
        resolve_workspace_root(ipj_dir, parsed.workspace_path.as_deref(), &mut warnings);
    let project_name = parsed.project_name.clone().unwrap_or_else(|| {
        ipj_path
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string()
    });

    if parsed.project_name.is_none() {
        warnings.push("IPJ에서 프로젝트 이름을 찾지 못해 파일명으로 대체했습니다.".to_string());
    }

    if parsed.inventor_version.is_none() {
        warnings.push("Inventor 버전을 IPJ에서 찾지 못했습니다.".to_string());
    }

    if !parsed.library_paths.is_empty() || !parsed.content_center_paths.is_empty() {
        warnings.push("1차 버전에서는 workspace만 스캔하고 library/content center 경로는 업로드하지 않습니다.".to_string());
    }

    let files = scan_workspace(&workspace_root, &ipj_path)?;
    let summary = summarize_files(&files);
    let analysis = ImportAnalysis {
        selected_root: selected_root.display().to_string(),
        ipj_path: relative_display_path(&selected_root, &ipj_path),
        workspace_path: workspace_root.display().to_string(),
        project_name: project_name.clone(),
        project_type: parsed.project_type.clone(),
        inventor_version: parsed.inventor_version.clone(),
        summary,
        warnings,
    };

    let manifest = ImportManifest {
        project_name,
        ipj_path: relative_display_path(&selected_root, &ipj_path),
        inventor_version: parsed.inventor_version,
        files: files.iter().map(|file| file.manifest.clone()).collect(),
    };

    Ok(PreparedImport {
        analysis,
        manifest,
        files,
    })
}

fn find_ipj_files(root: &Path) -> Result<Vec<PathBuf>, String> {
    let mut matches = Vec::new();

    for entry in WalkDir::new(root).follow_links(false) {
        let entry = entry.map_err(|e| format!("폴더를 탐색하지 못했습니다: {e}"))?;
        if entry.file_type().is_file() && has_extension(entry.path(), "ipj") {
            matches.push(entry.into_path());
        }
    }

    matches.sort();
    Ok(matches)
}

fn parse_ipj(xml: &str) -> Result<ParsedIpj, String> {
    let doc = Document::parse(xml).map_err(|e| format!("IPJ XML 파싱에 실패했습니다: {e}"))?;
    let mut parsed = ParsedIpj::default();

    for node in doc.descendants().filter(|node| node.is_element()) {
        let node_name = normalize_name(node.tag_name().name());
        let path = normalized_node_path(node);

        if parsed.inventor_version.is_none() {
            for attr in node.attributes() {
                let attr_name = normalize_name(attr.name());
                if (attr_name.contains("inventor") && attr_name.contains("version"))
                    || attr_name == "version"
                {
                    let value = attr.value().trim();
                    if !value.is_empty() {
                        parsed.inventor_version = Some(value.to_string());
                        break;
                    }
                }
            }
        }

        let Some(text) = node.text().map(str::trim).filter(|text| !text.is_empty()) else {
            continue;
        };

        if parsed.workspace_path.is_none() && looks_like_workspace_field(&node_name, &path) {
            parsed.workspace_path = Some(text.to_string());
            continue;
        }

        if looks_like_library_field(&node_name, &path) {
            push_unique(&mut parsed.library_paths, text);
            continue;
        }

        if looks_like_content_center_field(&node_name, &path) {
            push_unique(&mut parsed.content_center_paths, text);
            continue;
        }

        if parsed.project_name.is_none() && looks_like_project_name_field(&node_name, &path) {
            parsed.project_name = Some(text.to_string());
            continue;
        }

        if parsed.project_type.is_none() && looks_like_project_type_field(&node_name, &path) {
            parsed.project_type = Some(text.to_string());
            continue;
        }

        if parsed.inventor_version.is_none() && looks_like_inventor_version_field(&node_name, &path)
        {
            parsed.inventor_version = Some(text.to_string());
        }
    }

    Ok(parsed)
}

fn resolve_workspace_root(
    ipj_dir: &Path,
    workspace_value: Option<&str>,
    warnings: &mut Vec<String>,
) -> PathBuf {
    let Some(raw_workspace) = workspace_value
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        warnings.push("workspace path가 없어 IPJ 폴더를 기준으로 스캔합니다.".to_string());
        return ipj_dir.to_path_buf();
    };

    let candidate = PathBuf::from(raw_workspace);
    let resolved = if candidate.is_absolute() {
        candidate
    } else {
        ipj_dir.join(candidate)
    };

    if resolved.exists() {
        resolved
    } else {
        warnings.push(format!(
            "workspace path '{}' 를 찾지 못해 IPJ 폴더를 기준으로 스캔합니다.",
            raw_workspace
        ));
        ipj_dir.to_path_buf()
    }
}

fn scan_workspace(workspace_root: &Path, ipj_path: &Path) -> Result<Vec<ScannedFile>, String> {
    let mut files = Vec::new();

    for entry in WalkDir::new(workspace_root).follow_links(false) {
        let entry = entry.map_err(|e| format!("workspace를 스캔하지 못했습니다: {e}"))?;
        if !entry.file_type().is_file() {
            continue;
        }

        let path = entry.path();
        if path == ipj_path || should_skip_file(path) {
            continue;
        }

        let metadata = entry
            .metadata()
            .map_err(|e| format!("파일 메타데이터를 읽지 못했습니다: {e}"))?;
        let relative_path = normalize_relative_path(
            path.strip_prefix(workspace_root)
                .unwrap_or(path)
                .to_path_buf(),
        );

        files.push(ScannedFile {
            absolute_path: path.to_path_buf(),
            manifest: ImportManifestFile {
                path: relative_path,
                file_type: classify_file(path),
                size_bytes: metadata.len(),
            },
        });
    }

    files.sort_by(|left, right| left.manifest.path.cmp(&right.manifest.path));
    Ok(files)
}

fn summarize_files(files: &[ScannedFile]) -> ImportSummary {
    let mut summary = ImportSummary {
        total_files: files.len(),
        total_bytes: 0,
        part_count: 0,
        assembly_count: 0,
        drawing_count: 0,
        attachment_count: 0,
    };

    for file in files {
        summary.total_bytes += file.manifest.size_bytes;
        match file.manifest.file_type {
            ImportFileType::Part => summary.part_count += 1,
            ImportFileType::Assembly => summary.assembly_count += 1,
            ImportFileType::Drawing => summary.drawing_count += 1,
            ImportFileType::Attachment => summary.attachment_count += 1,
        }
    }

    summary
}

fn classify_file(path: &Path) -> ImportFileType {
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_lowercase());

    match extension.as_deref() {
        Some("ipt") => ImportFileType::Part,
        Some("iam") => ImportFileType::Assembly,
        Some("idw") | Some("dwg") => ImportFileType::Drawing,
        _ => ImportFileType::Attachment,
    }
}

fn should_skip_file(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
        return false;
    };

    name.starts_with('.') || name.starts_with('~')
}

fn relative_display_path(root: &Path, path: &Path) -> String {
    match path.strip_prefix(root) {
        Ok(relative) => normalize_relative_path(relative.to_path_buf()),
        Err(_) => path.display().to_string(),
    }
}

fn file_path_to_string(path: FilePath) -> String {
    match path {
        FilePath::Path(path) => path.to_string_lossy().into_owned(),
        FilePath::Url(url) => url.to_string(),
    }
}

fn normalize_relative_path(path: PathBuf) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn has_extension(path: &Path, expected: &str) -> bool {
    path.extension()
        .and_then(|value| value.to_str())
        .map(|value| value.eq_ignore_ascii_case(expected))
        .unwrap_or(false)
}

fn normalize_name(name: &str) -> String {
    name.chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .collect::<String>()
        .to_ascii_lowercase()
}

fn normalized_node_path(node: roxmltree::Node<'_, '_>) -> String {
    let mut parts = node
        .ancestors()
        .filter(|ancestor| ancestor.is_element())
        .map(|ancestor| normalize_name(ancestor.tag_name().name()))
        .collect::<Vec<_>>();
    parts.reverse();
    parts.join("/")
}

fn push_unique(target: &mut Vec<String>, value: &str) {
    if !target.iter().any(|existing| existing == value) {
        target.push(value.to_string());
    }
}

fn looks_like_workspace_field(node_name: &str, path: &str) -> bool {
    node_name == "workspacepath"
        || node_name == "workspace"
        || (path.contains("workspace") && (node_name == "path" || node_name == "location"))
}

fn looks_like_library_field(node_name: &str, path: &str) -> bool {
    node_name == "librarypath"
        || node_name == "library"
        || (path.contains("library") && (node_name == "path" || node_name == "location"))
}

fn looks_like_content_center_field(node_name: &str, path: &str) -> bool {
    node_name == "contentcenterpath"
        || node_name == "contentcenter"
        || (path.contains("contentcenter") && (node_name == "path" || node_name == "location"))
}

fn looks_like_project_name_field(node_name: &str, path: &str) -> bool {
    node_name == "projectname"
        || (node_name == "name" && path.contains("project"))
        || path.ends_with("/project/name")
}

fn looks_like_project_type_field(node_name: &str, path: &str) -> bool {
    node_name == "projecttype"
        || (node_name == "type" && path.contains("project"))
        || path.ends_with("/project/type")
}

fn looks_like_inventor_version_field(node_name: &str, path: &str) -> bool {
    node_name == "inventorversion"
        || (node_name == "version" && path.contains("inventor"))
        || path.ends_with("/inventor/version")
}

fn upload_prepared(
    app: &AppHandle,
    shared: &SharedImportState,
    prepared: &PreparedImport,
    access_token: &str,
) -> Result<(), String> {
    #[cfg(feature = "mock")]
    {
        let _ = access_token;
        upload_mock(app, shared, prepared)
    }

    #[cfg(not(feature = "mock"))]
    {
        upload_real(app, shared, prepared, access_token)
    }
}

#[cfg(feature = "mock")]
fn upload_mock(
    app: &AppHandle,
    shared: &SharedImportState,
    prepared: &PreparedImport,
) -> Result<(), String> {
    let mut uploaded_bytes = 0u64;
    let total_bytes = prepared
        .files
        .iter()
        .map(|file| file.manifest.size_bytes)
        .sum();

    for (index, file) in prepared.files.iter().enumerate() {
        std::thread::sleep(Duration::from_millis(60));
        uploaded_bytes += file.manifest.size_bytes;

        let progress = UploadProgress {
            total_files: prepared.files.len(),
            uploaded_files: index + 1,
            total_bytes,
            uploaded_bytes,
            current_file: Some(file.manifest.path.clone()),
        };

        {
            let mut state = shared.lock().unwrap();
            state.progress = Some(progress.clone());
        }

        let snapshot = snapshot_from_shared(shared);
        let _ = app.emit("ipj-import-state", &snapshot);
        let _ = app.emit("ipj-upload-progress", &progress);
    }

    {
        let mut state = shared.lock().unwrap();
        state.status = ImportStatus::Completed;
        state.last_error = None;
        state.progress = Some(UploadProgress {
            total_files: prepared.files.len(),
            uploaded_files: prepared.files.len(),
            total_bytes,
            uploaded_bytes: total_bytes,
            current_file: None,
        });
    }

    let snapshot = snapshot_from_shared(shared);
    let _ = app.emit("ipj-import-state", &snapshot);
    let _ = app.emit("ipj-upload-completed", &snapshot);
    Ok(())
}

#[cfg(not(feature = "mock"))]
fn upload_real(
    app: &AppHandle,
    shared: &SharedImportState,
    prepared: &PreparedImport,
    access_token: &str,
) -> Result<(), String> {
    let client = Client::builder()
        .timeout(Duration::from_secs(120))
        .build()
        .map_err(|e| format!("HTTP 클라이언트를 초기화하지 못했습니다: {e}"))?;

    let init_url = format!("{API_URL}/api/v1/migrations/inventor");
    let init_response = client
        .post(&init_url)
        .bearer_auth(access_token)
        .json(&prepared.manifest)
        .send()
        .map_err(|e| format!("매니페스트 전송에 실패했습니다: {e}"))?;

    let init_status = init_response.status();
    let init_body = init_response
        .text()
        .map_err(|e| format!("매니페스트 응답을 읽지 못했습니다: {e}"))?;

    if !init_status.is_success() {
        return Err(format!(
            "매니페스트 전송이 실패했습니다. ({}) {}",
            init_status.as_u16(),
            init_body
        ));
    }

    let init_payload: ImportInitResponse = serde_json::from_str(&init_body).map_err(|e| {
        format!("업로드 대상 응답을 해석하지 못했습니다: {e}. 응답 본문: {init_body}")
    })?;

    if init_payload.files.is_empty() {
        return Err("업로드 대상 정보가 비어 있습니다.".to_string());
    }

    let uploads_by_path = init_payload
        .files
        .iter()
        .filter_map(|target| target.path.clone().map(|path| (path, target.clone())))
        .collect::<HashMap<_, _>>();

    let total_bytes = prepared
        .files
        .iter()
        .map(|file| file.manifest.size_bytes)
        .sum();
    let mut uploaded_files = 0usize;
    let mut uploaded_bytes = 0u64;

    for (index, file) in prepared.files.iter().enumerate() {
        let target = uploads_by_path
            .get(&file.manifest.path)
            .cloned()
            .or_else(|| init_payload.files.get(index).cloned())
            .ok_or_else(|| {
                format!(
                    "'{}' 파일의 업로드 대상을 찾지 못했습니다.",
                    file.manifest.path
                )
            })?;

        let file_bytes = fs::read(&file.absolute_path)
            .map_err(|e| format!("'{}' 파일을 읽지 못했습니다: {e}", file.manifest.path))?;

        let method = target
            .method
            .as_deref()
            .unwrap_or("PUT")
            .to_ascii_uppercase();
        let mut request = match method.as_str() {
            "POST" => client.post(&target.upload_url),
            _ => client.put(&target.upload_url),
        };

        for (header_name, header_value) in &target.headers {
            request = request.header(header_name, header_value);
        }

        let response = request
            .body(file_bytes)
            .send()
            .map_err(|e| format!("'{}' 파일 업로드에 실패했습니다: {e}", file.manifest.path))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().unwrap_or_default();
            return Err(format!(
                "'{}' 파일 업로드가 실패했습니다. ({}) {}",
                file.manifest.path,
                status.as_u16(),
                body
            ));
        }

        uploaded_files += 1;
        uploaded_bytes += file.manifest.size_bytes;
        let progress = UploadProgress {
            total_files: prepared.files.len(),
            uploaded_files,
            total_bytes,
            uploaded_bytes,
            current_file: Some(file.manifest.path.clone()),
        };

        {
            let mut state = shared.lock().unwrap();
            state.progress = Some(progress.clone());
        }

        let snapshot = snapshot_from_shared(shared);
        let _ = app.emit("ipj-import-state", &snapshot);
        let _ = app.emit("ipj-upload-progress", &progress);
    }

    {
        let mut state = shared.lock().unwrap();
        state.status = ImportStatus::Completed;
        state.last_error = None;
        state.progress = Some(UploadProgress {
            total_files: prepared.files.len(),
            uploaded_files: prepared.files.len(),
            total_bytes,
            uploaded_bytes: total_bytes,
            current_file: None,
        });
    }

    let snapshot = snapshot_from_shared(shared);
    let _ = app.emit("ipj-import-state", &snapshot);
    let _ = app.emit("ipj-upload-completed", &snapshot);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        path::{Path, PathBuf},
        time::{SystemTime, UNIX_EPOCH},
    };

    struct TestDir {
        path: PathBuf,
    }

    impl TestDir {
        fn new(name: &str) -> Self {
            let unique = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let path = std::env::temp_dir().join(format!("fabbit-file-agent-{name}-{unique}"));
            fs::create_dir_all(&path).unwrap();
            Self { path }
        }

        fn path(&self) -> &Path {
            &self.path
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    #[test]
    fn parse_ipj_extracts_core_fields() {
        let xml = r#"
        <Project inventorVersion="2024">
          <ProjectName>Motor Assembly</ProjectName>
          <ProjectType>Vault</ProjectType>
          <WorkspacePath>Workspace</WorkspacePath>
          <LibraryPaths>
            <LibraryPath>Libraries/Common</LibraryPath>
          </LibraryPaths>
          <ContentCenterPaths>
            <ContentCenterPath>ContentCenter</ContentCenterPath>
          </ContentCenterPaths>
        </Project>
        "#;

        let parsed = parse_ipj(xml).unwrap();
        assert_eq!(parsed.project_name.as_deref(), Some("Motor Assembly"));
        assert_eq!(parsed.project_type.as_deref(), Some("Vault"));
        assert_eq!(parsed.workspace_path.as_deref(), Some("Workspace"));
        assert_eq!(parsed.inventor_version.as_deref(), Some("2024"));
        assert_eq!(parsed.library_paths, vec!["Libraries/Common"]);
        assert_eq!(parsed.content_center_paths, vec!["ContentCenter"]);
    }

    #[test]
    fn build_prepared_import_requires_single_ipj() {
        let dir = TestDir::new("single-ipj");
        fs::write(dir.path().join("a.ipj"), "<Project />").unwrap();
        fs::write(dir.path().join("b.ipj"), "<Project />").unwrap();

        let result = build_prepared_import(&dir.path().display().to_string());
        assert!(result.is_err());
        let error = result.err().unwrap();
        assert!(error.contains("여러 IPJ 파일"));
    }

    #[test]
    fn build_prepared_import_scans_workspace_and_classifies_files() {
        let dir = TestDir::new("scan-workspace");
        let workspace = dir.path().join("Workspace");
        fs::create_dir_all(workspace.join("Drawings")).unwrap();
        fs::create_dir_all(workspace.join("Docs")).unwrap();

        fs::write(
            dir.path().join("project.ipj"),
            r#"
            <Project>
              <ProjectName>Motor Assembly</ProjectName>
              <ProjectType>Vault</ProjectType>
              <WorkspacePath>Workspace</WorkspacePath>
            </Project>
            "#,
        )
        .unwrap();
        fs::write(workspace.join("shaft.ipt"), b"part").unwrap();
        fs::write(workspace.join("motor.iam"), b"assembly").unwrap();
        fs::write(workspace.join("Drawings").join("shaft.dwg"), b"drawing").unwrap();
        fs::write(workspace.join("Docs").join("spec.pdf"), b"pdf").unwrap();
        fs::write(workspace.join("~temp.tmp"), b"skip").unwrap();

        let prepared = build_prepared_import(&dir.path().display().to_string()).unwrap();

        assert_eq!(prepared.analysis.project_name, "Motor Assembly");
        assert_eq!(prepared.analysis.summary.total_files, 4);
        assert_eq!(prepared.analysis.summary.part_count, 1);
        assert_eq!(prepared.analysis.summary.assembly_count, 1);
        assert_eq!(prepared.analysis.summary.drawing_count, 1);
        assert_eq!(prepared.analysis.summary.attachment_count, 1);
        assert_eq!(prepared.manifest.files.len(), 4);
        assert!(prepared
            .manifest
            .files
            .iter()
            .any(|file| file.path == "Drawings/shaft.dwg"));
    }

    #[test]
    fn resolve_workspace_root_falls_back_to_ipj_dir_when_missing() {
        let dir = TestDir::new("workspace-fallback");
        let mut warnings = Vec::new();
        let resolved = resolve_workspace_root(dir.path(), Some("Missing"), &mut warnings);

        assert_eq!(resolved, dir.path().to_path_buf());
        assert_eq!(warnings.len(), 1);
    }
}
