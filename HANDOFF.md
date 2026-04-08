# HANDOFF

## Goal

현재 `file-agent`를 디자인 문서 기준의 Inventor IPJ import 앱으로 발전시키는 작업 중이다. 다만 다음 단계의 우선순위는 **Windows 탐색기 연동을 macOS에서 억지로 맞추는 것이 아니라, macOS에서 먼저 자연스럽게 실행 가능한 앱 형태로 정리하는 것**이다.

구체적으로는:
- 기존 Windows 탐색기/셸 폴더 코드는 **건드리지 않는다**.
- macOS에서는 Finder 확장 같은 복잡한 기능은 제거/보류한다.
- 대신 macOS에서 필요한 최소 기능만 제공한다.
  - 앱 UI 정상 표시
  - 상태바(tray/status bar) 아이콘 정상 동작
  - 사용자가 임의의 로컬 경로를 설정해서 그 경로 기준으로 동작
- IPJ import 기능 자체(IPJ 선택 → 분석 → 업로드)는 유지한다.

## Current Progress

### 이미 완료된 것

#### 1. IPJ import 1차 구현 추가
- `src-tauri/src/ipj_import.rs`
  - IPJ 폴더 선택 command
  - IPJ 탐색 (0개면 에러, 2개 이상이면 에러)
  - IPJ XML 파싱
  - workspace path 해석
  - workspace 기준 파일 스캔
  - 파일 분류
    - `.ipt` → `PART`
    - `.iam` → `ASSEMBLY`
    - `.idw`, `.dwg` → `DRAWING`
    - 기타 → `ATTACHMENT`
  - manifest 생성
  - import 상태 관리
  - mock 업로드/real 업로드 뼈대
  - 진행률 이벤트 emit

#### 2. Tauri 앱 wiring 완료
- `src-tauri/src/lib.rs`
  - `tauri_plugin_dialog` 등록
  - import 관련 command 등록
  - `SharedImportState` 관리

#### 3. OAuth 토큰 교환 real 경로 보강
- `src-tauri/src/local_server.rs`
  - non-mock `exchange_token()` 구현 추가
  - `POST {API_URL}/oauth/token` 호출
  - `access_token`을 `AuthState`에 저장

#### 4. import UI로 프론트 교체
- `src/index.html`
- `src/main.js`
- `src/styles.css`

현재 UI는 다음 흐름을 제공한다.
- 폴더 선택
- 분석 결과 표시
- 경고 표시
- 업로드 시작
- 진행률 표시
- 완료/실패 상태 표시

#### 5. dialog 권한/의존성 추가
- `src-tauri/Cargo.toml`
  - `tauri-plugin-dialog`
  - `reqwest`
  - `roxmltree`
  - `walkdir`
- `src-tauri/capabilities/default.json`
  - `dialog:default`

#### 6. Rust 검증 완료
실제로 아래 명령을 실행했고 모두 통과함.
- `cargo test --features mock`
- `cargo clippy --features mock -- -D warnings`

## What Worked

### 1. macOS에서도 Rust 레벨 검증은 가능했다
비록 Windows 전용 UX 전체를 재현할 수는 없지만, Rust 레벨에서 다음은 문제없이 검증 가능했다.
- IPJ 파싱
- workspace 스캔
- manifest 생성
- mock 업로드 흐름
- Tauri command 컴파일

### 2. non-Windows fallback 구조는 이미 일부 존재한다
- `src-tauri/src/shell_folder.rs:119`
  - non-Windows에서는 단순히 `~/Fabbit` 폴더를 target folder로 사용
  - register/unregister/is_registered 도 noop 또는 단순 처리
- `src-tauri/src/autostart.rs:38`
  - non-Windows에서는 noop fallback 존재

즉, **Windows 전용 기능을 완전히 없애지 않고도 macOS용 경량 동작을 별도로 정리할 기반은 이미 있다.**

### 3. import 기능 자체는 OS 독립적으로 분리할 수 있다
`src-tauri/src/ipj_import.rs`는 로컬 경로 선택, XML 파싱, 스캔, manifest 생성 중심이라 OS 종속성이 낮다. 다음 단계에서 macOS 지원을 정리할 때도 이 모듈은 대부분 재사용 가능하다.

## What Didn’t Work

### 1. Windows 탐색기와 동일한 모델을 macOS에 그대로 맞추는 접근
이 프로젝트는 기존 구조상 다음 전제가 강하다.
- 셸 폴더 등록
- 탐색기 통합
- tray 중심의 백그라운드 앱
- Windows 자동 시작

이걸 macOS Finder에 그대로 대응시키려 하면 복잡도가 급격히 올라간다.
특히 Finder extension, 상태 동기화, 경로 노출 방식을 당장 맞추려는 시도는 현재 단계에서 비효율적이다.

### 2. `open_folder` / shell integration을 그대로 UX 중심에 두는 것
현재 앱은 `src-tauri/src/lib.rs:162` 부근에서 “폴더 열기” 메뉴가 있고, Windows 쪽 `shell_uri()` 전제를 깔고 있다. macOS에서는 이 UX가 핵심이 아니며, 오히려 **사용자가 직접 작업 경로를 지정하는 방식**이 더 단순하고 맞다.

### 3. macOS에서 Windows 기능 parity를 먼저 맞추려는 시도
사용자 판단에 따르면 지금 우선순위는 parity가 아니라 **macOS에서 제대로 뜨고, tray/status bar와 기본 UI가 잘 보이고, 경로를 지정해서 import 기능을 쓸 수 있게 만드는 것**이다.

## Next Steps

### 우선순위 1: macOS용 동작 모델을 먼저 명확히 정리
다음 작업은 **Windows 코드를 수정하지 않고**, macOS에서만 동작을 단순화하는 방향으로 간다.

권장 방향:
1. macOS에서는 Finder 통합을 목표에서 제외
2. 사용자 지정 작업 경로(app-managed local folder path)를 도입
3. 상태바 아이콘 + 메인 UI를 중심 UX로 전환
4. import 기능은 해당 경로와 독립적으로 사용 가능하게 유지

### 우선순위 2: 경로를 사용자가 임의 설정할 수 있게 만들기
현재 non-Windows target folder는 `~/Fabbit`로 고정이다.
- 파일: `src-tauri/src/shell_folder.rs:119`

다음 에이전트가 할 일:
1. macOS/non-Windows용 설정 저장 방식 추가
   - 예: app config/state에 사용자 지정 경로 저장
2. `target_folder()`가 macOS에서는 저장된 사용자 경로를 우선 사용하도록 수정
3. 설정되지 않았으면 fallback으로 `~/Fabbit`
4. UI에서 “작업 폴더 선택”을 제공

중요:
- Windows 경로/CLSID/Explorer 등록 로직은 건드리지 않는다.
- `#[cfg(target_os = "windows")]` 블록은 그대로 유지한다.

### 우선순위 3: lib.rs에서 macOS용 분기 최소화
- `src-tauri/src/lib.rs`
  - tray 메뉴의 “폴더 열기”는 macOS에서도 쓸 수 있지만, shell URI 개념이 아니라 단순 로컬 폴더 열기여야 함
  - 필요하면 `open::that(shell_folder::target_folder())` 형태로 OS별 분기 정리
- 현재 `explorer` 직접 호출 부분은 Windows 전용 분기로 빼는 것이 맞다.
  - 파일: `src-tauri/src/lib.rs:239`

### 우선순위 4: macOS에서 보이는 앱 UX 정리
- 현재 창은 `tauri.conf.json`에서 다음 상태임
  - `visible: false`
  - `skipTaskbar: true`
  - `alwaysOnTop: true`
- 파일: `src-tauri/tauri.conf.json:11`

macOS에서 먼저 확인할 것:
1. 처음 실행 시 창을 보여줄지 여부
2. 상태바 아이콘만으로 충분한지
3. Dock/taskbar/activation UX가 자연스러운지
4. `skipTaskbar: true`가 macOS에서 원하는 UX인지

즉, macOS에서 먼저 UX를 다듬을 때는 `tauri.conf.json`과 `lib.rs`의 초기 창/트레이 동작을 함께 검토해야 한다.

### 우선순위 5: import 기능을 macOS 기준으로 실제 실행 확인
이미 Rust test/clippy는 통과했으므로 다음은 실행 검증이다.

권장 순서:
1. `npm run dev-mock`으로 앱 실행
2. UI가 정상 표시되는지 확인
3. tray/status bar 아이콘이 macOS에서 정상 동작하는지 확인
4. 폴더 선택 다이얼로그가 정상 동작하는지 확인
5. 샘플 IPJ 폴더로 분석/업로드 mock 흐름 확인

### 우선순위 6: handoff 이후 첫 구현 범위 제안
다음 에이전트는 아래 범위를 먼저 처리하는 것이 좋다.

1. **Windows 코드 untouched 원칙 유지**
2. `shell_folder`에 macOS/non-Windows 사용자 지정 경로 저장 추가
3. UI에 “작업 폴더 선택” 추가
4. `open_folder` 메뉴를 OS별로 안전하게 정리
5. macOS에서 앱 창/상태바 UX 조정
6. 이후 `npm run dev-mock`으로 실제 실행 검증

## Important File References

- `src-tauri/src/ipj_import.rs` — IPJ import 핵심 로직
- `src-tauri/src/lib.rs` — 앱 초기화, tray/menu, command 등록
- `src-tauri/src/local_server.rs` — OAuth/local server
- `src-tauri/src/shell_folder.rs` — Windows 셸 폴더 + non-Windows fallback
- `src-tauri/src/autostart.rs` — Windows 자동시작 + non-Windows fallback
- `src-tauri/tauri.conf.json` — 창/앱 UX 설정
- `src/index.html` — import UI
- `src/main.js` — import UI 상태 관리
- `src/styles.css` — import UI 스타일

## Recommended First Prompt for Next Agent

"`/Users/moonseongha/code/projects/fabbit/file-agent/HANDOFF.md` 를 읽고, Windows 코드는 그대로 두면서 macOS에서 먼저 자연스럽게 동작하도록 정리해줘. Finder 통합은 빼고, 상태바 아이콘 + UI + 사용자 지정 작업 경로 중심으로 바꿔줘. 그 다음 dev-mock 실행까지 확인해줘."
