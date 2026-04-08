# Fabbit File Agent

**Tauri 2.0 (Rust)** 기반의 크로스플랫폼 데스크탑 앱으로, Fabbit 웹 앱과 로컬 파일 시스템을 연결합니다. Windows 탐색기에 가상 폴더를 등록하고, 로컬 파일 변경을 감시하며, localhost HTTP 서버를 통해 브라우저와 네이티브 OS 기능을 브리지합니다.

---

## 기술 스택

| 계층 | 기술 |
|---|---|
| 데스크탑 프레임워크 | Tauri 2.0 |
| 백엔드 | Rust |
| 프론트엔드 (UI) | HTML / JavaScript / CSS |
| OS 통합 | Windows Shell Folder (레지스트리 CLSID), 시스템 트레이 |
| 인증 | OAuth 2.0 + PKCE (퍼블릭 클라이언트) |
| 토큰 저장 | OS 자격증명 저장소 (Keychain / Credential Manager) |
| 자동 업데이트 | Tauri updater 플러그인 + GitHub Releases |

---

## 아키텍처

핵심 설계 패턴은 Fabbit 웹 앱(브라우저)과 네이티브 데스크탑 앱 사이의 통신 브리지 역할을 하는 **localhost HTTP 서버**입니다.

```
브라우저 (fabbit.app)
        │
        │  localhost:52847으로 HTTP 요청
        ▼
┌───────────────────────────────────────┐
│       Localhost HTTP 서버             │  local_server.rs
│  /status  /auth/callback  /download   │
│  /upload/status  /update/check        │
└───────────┬───────────────────────────┘
            │
   ┌────────┴──────────┐
   │                   │
   ▼                   ▼
파일 감시기         인증 & 토큰
(file_watcher.rs)  (OS 자격증명 저장소)
   │
   ▼
Fabbit 폴더  (%USERPROFILE%\Fabbit\)
  Shell Folder로 Windows 탐색기에 등록
  shell_folder.rs (레지스트리 CLSID)
```

---

## 모듈 구성

| 파일 | 책임 |
|---|---|
| `lib.rs` | 앱 진입점, 시스템 트레이 메뉴, 윈도우 라이프사이클 |
| `local_server.rs` | `localhost:52847` 내장 HTTP 서버 |
| `shell_folder.rs` | 레지스트리 CLSID를 통한 Windows 탐색기 Shell Folder 등록 |
| `file_watcher.rs` | 파일 시스템 모니터링, 미업로드 변경 감지 |
| `autostart.rs` | Windows 시작 프로그램 등록 토글 |
| `ipj_import.rs` | 레거시 `.ipj` 프로젝트 파일 파싱 |
| `updater.rs` | GitHub Releases 기반 자가 업데이트 확인 |

---

## 인증 (OAuth 2.0 + PKCE)

데스크탑 앱은 웹 앱과 별도의 OAuth 클라이언트를 사용하여 독립적인 세션을 관리합니다.

```
1. 사용자가 브라우저에서 "데스크탑 앱 연결" 클릭
2. 브라우저가 다음 주소로 리다이렉트:
     http://localhost:52847/auth/callback?code=<일회성 인증 코드>
3. Rust 서버가 인증 코드 수신
4. 앱이 API로 POST /oauth/token { code, client_id: "fabbit-agent" } 전송
5. API가 { access_token, refresh_token } 반환
6. 토큰을 OS 자격증명 저장소에 보관 (평문 파일 아님)
```

주요 속성:
- **PKCE** 적용 — 퍼블릭 클라이언트에 안전 (클라이언트 시크릿 없음)
- 인증 코드는 단일 사용, 짧은 TTL
- `fabbit-agent` 클라이언트는 `fabbit-web`보다 긴 리프레시 토큰 유효기간
- 웹 로그아웃과 데스크탑 로그아웃은 독립적인 세션

---

## 파일 다운로드 흐름

```
브라우저                 Localhost 서버           API 서버
   │                          │                        │
   │  POST /download           │                        │
   │  { fileId: "abc123" }     │                        │
   │─────────────────────────►│                        │
   │                          │  GET /api/files/abc123  │
   │                          │  Authorization: Bearer  │
   │                          │────────────────────────►│
   │                          │◄────────────────────────│
   │                          │  Fabbit 폴더에 저장     │
   │◄─────────────────────────│                        │
   │  { success: true }        │                        │
```

브라우저는 파일 바이트를 직접 처리하지 않습니다. `fileId`만 전송하면 Rust 프로세스가 인증된 다운로드를 수행하고 로컬 폴더에 저장합니다.

---

## 미업로드 변경 감지

`file_watcher.rs`는 OS 파일 시스템 API로 Fabbit 폴더를 모니터링합니다. 파일이 로컬에서 수정된 후 일정 임계 시간 내에 업로드되지 않으면, 앱이 시스템 트레이 알림을 표시하여 동기화를 안내합니다.

---

## 보안

- Localhost 서버는 `Origin` 헤더를 검증 — `*.fabbit.io` 요청만 허용
- 데스크탑의 모든 API 호출은 OS 자격증명 저장소에 보관된 토큰 사용 (평문 설정 파일 없음)
- 파일 다운로드 요청에는 `fileId`만 포함되며, 실제 파일 접근 권한은 서버 측에서 검증

---

## 크로스플랫폼 전략

Shell Folder 등록은 OS별로 다릅니다. 트레이, HTTP 서버, 파일 감시, 자동 업데이트는 Tauri를 통해 공유됩니다.

| OS | 탐색기 통합 |
|---|---|
| Windows | 레지스트리 CLSID + Shell Folder API |
| macOS | Finder Sync Extension (예정) |
| Linux | GVFS / KIO (예정) |

---

## 로컬 실행

```bash
pnpm install
pnpm tauri dev
```

**프로덕션 빌드:**

```bash
pnpm tauri build
```
