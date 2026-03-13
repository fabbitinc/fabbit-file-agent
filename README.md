# Fabbit

Tauri 2.0 기반 데스크톱 에이전트. 파일 탐색기 "내 PC" 하위에 Fabbit 폴더를 등록하고, 웹과 연동하여 파일을 관리한다.

## 구조

```
fabbit-file-agent/
├── src/                        # 프론트엔드 (HTML/JS/CSS)
└── src-tauri/                  # Rust 백엔드
    └── src/
        ├── lib.rs              # 앱 진입점, 트레이 메뉴
        ├── shell_folder.rs     # 파일 탐색기 셸 폴더 등록 (레지스트리 CLSID)
        ├── file_watcher.rs     # 폴더 내 파일 변경 감시
        ├── autostart.rs        # Windows 시작 시 자동 실행
        └── local_server.rs     # Localhost HTTP 서버 (예정)
```

## 구현된 기능

- 시스템 트레이 상주 (더블클릭 → 창 열기, 우클릭 → 메뉴)
- 파일 탐색기 "내 PC" 하위 셸 폴더 등록 (`C:\Users\{user}\Fabbit\`)
- 파일 변경 감시 → 프론트엔드 이벤트 발행 (`files-pending-upload`)
- Windows 시작 시 자동 실행 토글

## Localhost HTTP 서버

앱이 시작되면 `localhost:52847`에 HTTP 서버를 상시 띄운다.
웹과의 모든 상호작용은 이 서버를 통해 이루어진다. (포트 충돌 시 fallback)

### 엔드포인트

| 엔드포인트 | 메서드 | 설명 |
|---|---|---|
| `/status` | GET | 앱 상태 확인 (버전, 로그인 여부, 실행 상태) |
| `/auth/callback` | GET | OAuth 인증 콜백 (code 수신) |
| `/download` | POST | 파일 다운로드 트리거 |
| `/upload/status` | GET | 미업로드 파일 목록 조회 |
| `/update/check` | GET | 앱 업데이트 확인 |

### 웹에서 앱 설치/실행 확인

```javascript
// 웹에서 앱 상태 확인
fetch("http://localhost:52847/status")
  .then(r => r.json())
  .then(data => {
    // 앱 실행 중 → { version: "0.1.0", loggedIn: true, user: "홍길동" }
  })
  .catch(() => {
    // 앱 미설치 또는 미실행 → 설치 안내 페이지 표시
  });
```

### 보안

- `Origin` 헤더 검증: `fabbit.io` 도메인만 허용
- CORS 설정: 허용된 origin만 접근 가능
- 인증이 필요한 엔드포인트는 앱 내부 토큰 검증

## 인증 (로그인)

Localhost HTTP 서버를 통한 OAuth 방식.

### 흐름

```
1. 웹에서 "데스크톱 앱 연동" 클릭
2. 웹 → http://localhost:52847/auth/callback?code={일회용_auth_code} 리다이렉트
3. 앱이 code 수신
4. 앱 → API 서버: POST /oauth/token { code, client_id=fabbit-agent }
5. API 서버 → 앱: { access_token, refresh_token }
6. 앱이 토큰을 OS credential store에 저장
```

### 토큰 관리

- 웹과 앱은 같은 OAuth 서버를 사용하되, **별도 세션**(별도 토큰)으로 관리
- `client_id`로 구분: 웹 = `fabbit-web`, 앱 = `fabbit-agent`
- 웹 로그아웃 ≠ 앱 로그아웃 (독립적)
- refresh_token 수명은 앱이 더 길게 설정 가능

### 보안

- PKCE(Proof Key for Code Exchange) 적용 (public client)
- auth code는 일회용, 짧은 TTL
- 토큰은 OS credential store에 저장 (평문 파일 X)

## 다운로드 (웹 → 앱)

Localhost HTTP 서버를 통해 웹에서 앱으로 다운로드를 트리거한다.

### 흐름

```
1. 웹에서 다운로드 버튼 클릭
2. 웹 → POST http://localhost:52847/download { fileId: "abc123" }
3. 앱이 저장된 access_token으로 API 호출: GET /api/files/abc123
4. API 서버가 권한 검증 후 파일 데이터 응답
5. 앱이 Fabbit 폴더에 파일 저장
6. 파일 감시(file_watcher)가 변경 감지 → 상태 추적 시작
7. 앱 → 웹 응답: { success: true, path: "..." }
```

### 수정 후 미업로드 경고

```
1. file_watcher가 파일 수정 감지
2. 수정된 파일을 pending 목록에 추가
3. 일정 시간 경과 후에도 업로드되지 않으면 트레이 알림 표시
4. 유저가 업로드하면 pending 목록에서 제거
```

### 보안

- 요청에는 fileId만 포함 (파일 내용 X)
- 실제 다운로드는 앱이 API에 인증된 요청으로 수행
- 파일 접근 권한은 서버에서 검증

## 자동 업데이트

### 흐름

```
1. 앱 시작 시 (또는 주기적으로) GET http://localhost:52847/update/check 내부 호출
   또는 앱이 직접 업데이트 서버에 확인: GET https://releases.fabbit.io/latest.json
2. 새 버전 있으면 트레이 알림 표시
3. 유저 승인 시 다운로드 → 자동 설치 (Tauri updater 플러그인 사용)
```

### 업데이트 서버 응답 형식

```json
{
  "version": "0.2.0",
  "url": "https://releases.fabbit.io/fabbit-0.2.0-setup.exe",
  "notes": "버그 수정 및 성능 개선"
}
```

GitHub Releases를 사용하면 호스팅 자동화 가능.

## 개발

```bash
npm install
npm run tauri dev
```

## 크로스 플랫폼 전략

셸 폴더 등록은 OS별 네이티브 API 사용, 나머지는 Tauri로 공유:

| OS | 탐색기 통합 방식 |
|---------|-------------------------------|
| Windows | 레지스트리 CLSID + Shell Folder |
| macOS | Finder Sync Extension (예정) |
| Linux | GVFS / KIO (예정) |
