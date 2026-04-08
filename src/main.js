const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const importState = {
  status: "idle",
  analysis: null,
  progress: null,
  lastError: null,
  workingFolder: "불러오는 중...",
};

let elements = {};

function formatBytes(bytes) {
  if (!bytes) return "0 B";
  const units = ["B", "KB", "MB", "GB", "TB"];
  let value = bytes;
  let unitIndex = 0;

  while (value >= 1024 && unitIndex < units.length - 1) {
    value /= 1024;
    unitIndex += 1;
  }

  const formatted = value >= 10 || unitIndex === 0 ? value.toFixed(0) : value.toFixed(1);
  return `${formatted} ${units[unitIndex]}`;
}

function getStatusMeta(status) {
  switch (status) {
    case "ready":
      return { label: "분석 완료", className: "status-ready" };
    case "uploading":
      return { label: "업로드 중", className: "status-uploading" };
    case "completed":
      return { label: "업로드 완료", className: "status-completed" };
    case "failed":
      return { label: "오류", className: "status-failed" };
    default:
      return { label: "대기 중", className: "status-idle" };
  }
}

function getProgressPercent(progress) {
  if (!progress) return 0;
  if (progress.totalBytes > 0) {
    return Math.min(100, Math.round((progress.uploadedBytes / progress.totalBytes) * 100));
  }
  if (progress.totalFiles > 0) {
    return Math.min(100, Math.round((progress.uploadedFiles / progress.totalFiles) * 100));
  }
  return 0;
}

function normalizeError(error) {
  if (typeof error === "string") return error;
  if (error && typeof error === "object" && "message" in error) return error.message;
  return "알 수 없는 오류가 발생했습니다.";
}

function applySnapshot(snapshot) {
  importState.status = snapshot?.status ?? "idle";
  importState.analysis = snapshot?.analysis ?? null;
  importState.progress = snapshot?.progress ?? null;
  importState.lastError = snapshot?.lastError ?? null;
  render();
}

function renderWarnings(warnings) {
  elements.warningsList.innerHTML = "";

  if (!warnings || warnings.length === 0) {
    const item = document.createElement("li");
    item.textContent = "경고 없이 분석이 완료되었습니다.";
    elements.warningsList.append(item);
    return;
  }

  warnings.forEach((warning) => {
    const item = document.createElement("li");
    item.textContent = warning;
    elements.warningsList.append(item);
  });
}

function render() {
  const { analysis, progress, status, lastError, workingFolder } = importState;
  const statusMeta = getStatusMeta(status);
  const isUploading = status === "uploading";

  elements.statusBadge.textContent = statusMeta.label;
  elements.statusBadge.className = `status-badge ${statusMeta.className}`;
  elements.workingFolderPath.textContent = workingFolder || "설정되지 않음";

  elements.selectedRoot.textContent = analysis?.selectedRoot ?? "아직 선택되지 않음";
  elements.projectName.textContent = analysis?.projectName ?? "-";
  elements.projectType.textContent = analysis?.projectType ?? "-";
  elements.ipjPath.textContent = analysis?.ipjPath ?? "-";
  elements.workspacePath.textContent = analysis?.workspacePath ?? "-";
  elements.inventorVersion.textContent = analysis?.inventorVersion ?? "확인 불가";

  const summary = analysis?.summary;
  elements.totalFiles.textContent = `${summary?.totalFiles ?? 0}개`;
  elements.totalBytes.textContent = formatBytes(summary?.totalBytes ?? 0);
  elements.partCount.textContent = `${summary?.partCount ?? 0}`;
  elements.assemblyCount.textContent = `${summary?.assemblyCount ?? 0}`;
  elements.drawingCount.textContent = `${summary?.drawingCount ?? 0}`;
  elements.attachmentCount.textContent = `${summary?.attachmentCount ?? 0}`;

  renderWarnings(analysis?.warnings ?? []);

  if (lastError) {
    elements.errorPanel.hidden = false;
    elements.errorMessage.textContent = lastError;
  } else {
    elements.errorPanel.hidden = true;
    elements.errorMessage.textContent = "";
  }

  const percent = getProgressPercent(progress);
  elements.progressFill.style.width = `${percent}%`;
  elements.progressLabel.textContent =
    status === "completed"
      ? "업로드가 완료되었습니다."
      : status === "uploading"
        ? `${percent}% 업로드 중`
        : "업로드 대기 중";
  elements.progressCount.textContent = `${progress?.uploadedFiles ?? 0} / ${progress?.totalFiles ?? 0} 파일`;
  elements.currentFile.textContent = progress?.currentFile
    ? `현재 파일: ${progress.currentFile}`
    : "현재 업로드 중인 파일이 없습니다.";

  elements.selectFolderBtn.disabled = isUploading;
  elements.selectFolderBtn.textContent = isUploading ? "업로드 중..." : "IPJ 폴더 선택";
  elements.uploadBtn.disabled = !analysis || isUploading;
  elements.uploadBtn.textContent = isUploading ? "업로드 중..." : "업로드 시작";
  elements.selectWorkingFolderBtn.disabled = isUploading;
  elements.openWorkingFolderBtn.disabled = isUploading;
}

async function handleSelectFolder() {
  try {
    const folderPath = await invoke("pick_import_folder");
    if (!folderPath) return;

    importState.lastError = null;
    render();

    const analysis = await invoke("analyze_import_folder", { folderPath });
    importState.status = "ready";
    importState.analysis = analysis;
    importState.progress = null;
    importState.lastError = null;
    render();
  } catch (error) {
    importState.status = "failed";
    importState.lastError = normalizeError(error);
    render();
  }
}

async function handleUpload() {
  try {
    importState.status = "uploading";
    importState.lastError = null;
    render();
    await invoke("start_import_upload");
  } catch (error) {
    importState.status = "failed";
    importState.lastError = normalizeError(error);
    render();
  }
}

async function handleSelectWorkingFolder() {
  try {
    const folderPath = await invoke("pick_working_folder");
    if (!folderPath) return;

    importState.workingFolder = folderPath;
    importState.lastError = null;
    render();
  } catch (error) {
    importState.lastError = normalizeError(error);
    render();
  }
}

async function handleOpenWorkingFolder() {
  try {
    await invoke("open_working_folder");
  } catch (error) {
    importState.lastError = normalizeError(error);
    render();
  }
}

async function loadInitialState() {
  try {
    const [snapshot, workingFolder] = await Promise.all([
      invoke("get_import_state"),
      invoke("get_working_folder"),
    ]);

    importState.workingFolder = workingFolder;
    applySnapshot(snapshot);
  } catch (error) {
    importState.status = "failed";
    importState.lastError = normalizeError(error);
    render();
  }
}

window.addEventListener("DOMContentLoaded", async () => {
  elements = {
    statusBadge: document.getElementById("status-badge"),
    selectFolderBtn: document.getElementById("select-folder-btn"),
    uploadBtn: document.getElementById("upload-btn"),
    selectWorkingFolderBtn: document.getElementById("select-working-folder-btn"),
    openWorkingFolderBtn: document.getElementById("open-working-folder-btn"),
    workingFolderPath: document.getElementById("working-folder-path"),
    selectedRoot: document.getElementById("selected-root"),
    errorPanel: document.getElementById("error-panel"),
    errorMessage: document.getElementById("error-message"),
    projectName: document.getElementById("project-name"),
    projectType: document.getElementById("project-type"),
    ipjPath: document.getElementById("ipj-path"),
    workspacePath: document.getElementById("workspace-path"),
    inventorVersion: document.getElementById("inventor-version"),
    totalFiles: document.getElementById("total-files"),
    totalBytes: document.getElementById("total-bytes"),
    partCount: document.getElementById("part-count"),
    assemblyCount: document.getElementById("assembly-count"),
    drawingCount: document.getElementById("drawing-count"),
    attachmentCount: document.getElementById("attachment-count"),
    warningsList: document.getElementById("warnings-list"),
    progressLabel: document.getElementById("progress-label"),
    progressCount: document.getElementById("progress-count"),
    progressFill: document.getElementById("progress-fill"),
    currentFile: document.getElementById("current-file"),
  };

  elements.selectFolderBtn.addEventListener("click", handleSelectFolder);
  elements.uploadBtn.addEventListener("click", handleUpload);
  elements.selectWorkingFolderBtn.addEventListener("click", handleSelectWorkingFolder);
  elements.openWorkingFolderBtn.addEventListener("click", handleOpenWorkingFolder);

  listen("mandatory-update", (event) => {
    const info = event.payload;
    const overlay = document.getElementById("mandatory-update-overlay");
    const versionEl = document.getElementById("update-version");
    const notesEl = document.getElementById("update-notes");
    const button = document.getElementById("update-btn");

    versionEl.textContent = `v${info.current_version} → v${info.latest_version}`;
    notesEl.textContent = info.release_notes;
    overlay.style.display = "flex";

    button.onclick = async () => {
      button.textContent = "설치 중...";
      button.disabled = true;
      try {
        await invoke("install_update");
      } catch (error) {
        button.textContent = "업데이트 설치";
        button.disabled = false;
        console.error("Update failed:", error);
      }
    };
  });

  listen("ipj-import-state", (event) => applySnapshot(event.payload));
  listen("ipj-upload-progress", (event) => {
    importState.status = "uploading";
    importState.progress = event.payload;
    render();
  });
  listen("ipj-upload-completed", (event) => applySnapshot(event.payload));
  listen("ipj-upload-failed", (event) => applySnapshot(event.payload));

  render();
  await loadInitialState();
});
