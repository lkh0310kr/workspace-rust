# gpui + 브라우저 임베딩 Feasibility 검증

목표: 기존 Workspace(Electron/Chromium)의 **브라우저 패널**을 Rust(gpui)로 점진 이식할 수 있는지 검증한다.
알려진 문제는 (1) z-index(gpui UI를 웹뷰 위에 못 올림), (2) 로그인 불안정이었다.

**이 문서가 검증의 유일한 산출물이다.** 표 3개로 구성한다: ① 검증표(케이스 목록·계획·엔진별 최신 결과) ② CEF 결과 ③ wry 결과. 앞으로 할 일(TODO)은 검증표와 CEF 표에 `⬜ TODO` 로 들어 있다.
관련 문서: [IPC 방식 비교](ipc-comparison.md) · [백로그](BACKLOG.md)

## 결론

- **CEF(Chromium) off-screen rendering을 권장한다.** 웹 콘텐츠가 gpui 씬의 이미지이므로 z-order·클립·히트테스트가 gpui 규칙을 따르고(COMP-001~005), 엔진이 기존 Electron과 같은 Chromium이라 로그인 관련 항목이 통과한다(AUTH).
- **wry(WKWebView)는 z-index를 구조적으로 해결하지 못한다**(네이티브 뷰가 gpui 위에 얹힘). 팝업·3rd-party 쿠키·Chrome UA도 막힌다.
- **CEF 151.3.x는 Google 로그인(아이디 → Next)에서 100% 크래시**한다(AUTH-009, CEF 내부 버그). **CEF 156.0.31로 올리면 해결**된다. 비밀번호 로그인과 세션 유지는 사용자가 수동으로 확인했다. passkey는 지원하지 않는다(요구사항 아님).

## 표 읽는 법

| 항목 | 설명 |
|---|---|
| ID | `분류-번호`. 같은 케이스는 엔진이 달라도 ID가 같다(표 1에서 엔진별 비교, 표 2·3에서 상세) |
| 분류 | COMP 합성 · AUTH 로그인 · STORE 저장소 · INPUT 입력 · NAV 내비게이션 · UI 브라우저 UI 연동 · WEB 웹 플랫폼 · IPC 통신 · PERF 성능 · STAB 안정성 · SEC 보안 · PKG 배포 |
| 결과 | ✅ PASS · ❌ FAIL · ℹ️ INFO(판정 없이 측정/관찰) · 🚫 UNSUPPORTED(지원하지 않음, 결정됨) · ⬜ TODO(아직 안 함) |
| 방법 | `auto-headless` 창 없는 자동 실행 · `auto-gui` gpui 창이 필요한 자동 실행 · `manual` 사람이 직접 · `plan:*` TODO 의 실행 계획 |
| 커밋 | 검증을 수행한 코드 버전. **`*` 는 커밋 전 작업트리에서 실행**했고 그 내용이 이 커밋이 됐다는 뜻. `+dirty` 는 미커밋 변경이 있는 상태 |
| 날짜 | 검증 실행일 |
| 우선 | 검증표의 우선순위(P0 필수, P1 일상 품질, P2 있으면 좋음, `완료` 는 이미 실행된 케이스) |
| 엔진 | `wry (WKWebView)`, `CEF 151.3.18`, `CEF 156.0.31`(cef-rs 156.7.0). **CEF 151 결과는 156에서 재검증하지 않았다**(GUI 회귀는 창이 필요해 보류) |

데이터 원본은 `docs/verification/results.csv`이고, 표는 `scripts/results.py render`로 생성한다(직접 수정하지 말 것).

<!-- RESULTS:BEGIN (scripts/results.py render 로 생성, 직접 수정 금지) -->
### 요약

| 분류 | 설명 | 케이스 수 | CEF 결과(PASS/FAIL/INFO/UNSUP/TODO) | wry 결과(PASS/FAIL/INFO) |
|---|---|---|---|---|
| COMP | 합성·레이아웃 (z-index, 클립, 리사이즈) | 13 | 9/0/0/0/4 | 3/2/0 |
| AUTH | 로그인·세션 | 14 | 9/1/2/1/1 | 4/3/1 |
| STORE | 쿠키·저장소·프로필 | 6 | 4/0/0/0/2 | — |
| INPUT | 입력 (키보드, 마우스, IME, 스크롤, 줌, 커서) | 37 | 25/0/1/0/11 | 2/0/0 |
| NAV | 내비게이션 (히스토리, 탭, 팝업, 찾기, 로딩 상태) | 15 | 10/0/1/0/4 | — |
| UI | 브라우저 UI 연동 (메뉴, 다이얼로그, 다운로드, DevTools, 권한) | 17 | 7/0/0/0/10 | — |
| WEB | 웹 플랫폼·미디어·렌더링 | 4 | 0/0/0/0/4 | — |
| IPC | 웹 ↔ Rust 통신 | 8 | 1/1/4/0/2 | — |
| PERF | 성능 | 6 | 1/0/2/0/3 | — |
| STAB | 안정성·프로세스·종료 | 5 | 1/0/1/0/3 | — |
| SEC | 보안 | 2 | 0/0/0/0/2 | — |
| PKG | 패키징·배포 | 4 | 0/0/2/0/3 | — |

(케이스 131개 / 결과 행 147개)

## 표 1. 검증표 (케이스 목록)

| ID | 분류 | 케이스 | 우선 | 계획 방법 | CEF 최신 | wry |
|---|---|---|---|---|---|---|
| COMP-001 | COMP | gpui 오버레이(모달/드롭다운)가 웹 콘텐츠 위에 그려짐 | 완료 | auto-gui | ✅ PASS (151.3.18) | ❌ FAIL |
| COMP-002 | COMP | 오버레이 영역 클릭이 gpui 로 전달됨(웹뷰가 가로채지 않음) | 완료 | auto-gui | ✅ PASS (151.3.18) | ❌ FAIL |
| COMP-003 | COMP | 레이아웃 변경(사이드바 폭)에 웹뷰 영역이 동기화됨 | 완료 | auto-gui | ✅ PASS (151.3.18) | ✅ PASS |
| COMP-004 | COMP | 둥근 모서리 클립 | 완료 | auto-gui | ✅ PASS (151.3.18) | ✅ PASS |
| COMP-005 | COMP | 웹뷰 숨김/표시(탭 전환) | 완료 | auto-gui | ✅ PASS (151.3.18) | ✅ PASS |
| COMP-006 | COMP | 배경 미지정 페이지가 불투명 흰색으로 그려짐(OSR 기본은 투명) | 완료 | auto-gui | ✅ PASS (151.3.18) | — |
| COMP-007 | COMP | <select> 팝업 위젯 렌더링 | 완료 | auto-gui | ✅ PASS (151.3.18) | — |
| COMP-008 | COMP | <select> 항목 선택 → change 이벤트·팝업 닫힘 | 완료 | auto-gui | ✅ PASS (151.3.18) | — |
| COMP-009 | COMP | 메인 + 팝업 브라우저 동시 렌더(팝업을 gpui 레이어로 그림) | 완료 | auto-gui | ✅ PASS (151.3.18) | — |
| COMP-010 | COMP | 리사이즈/스플리터 드래그 중 깜빡임·잔상 | P1 | gui | ⬜ TODO | — |
| COMP-011 | COMP | gpui 오버레이 위 입력 차단, 웹뷰 영역 밖으로 나가는 드롭다운 | P1 | gui | ⬜ TODO | — |
| COMP-012 | COMP | 고DPI/다중 모니터 DPI 변경, 텍스트 선명도 | P1 | gui | ⬜ TODO | — |
| COMP-013 | COMP | 다크모드(prefers-color-scheme) 연동 | P2 | headless | ⬜ TODO | — |
| AUTH-001 | AUTH | 폼 로그인 → Set-Cookie 세션 유지 | 완료 | auto-gui | ✅ PASS (151.3.18) | ✅ PASS |
| AUTH-002 | AUTH | 영속 쿠키가 앱 재시작 후에도 유지 | 완료 | auto-gui | ✅ PASS (151.3.18) | ✅ PASS |
| AUTH-003 | AUTH | 세션 쿠키 재시작 후 정책(정보) | 완료 | auto-gui | ℹ️ INFO (151.3.18) | ℹ️ INFO |
| AUTH-004 | AUTH | OAuth 팝업(window.open + postMessage + close) | 완료 | auto-gui | ✅ PASS (151.3.18) | ❌ FAIL |
| AUTH-005 | AUTH | target=_blank 링크 처리 | 완료 | auto-gui | ✅ PASS (151.3.18) | ✅ PASS |
| AUTH-006 | AUTH | 3rd-party 쿠키(SameSite=None, cross-site iframe) | 완료 | auto-gui | ✅ PASS (151.3.18) | ❌ FAIL |
| AUTH-007 | AUTH | UA·자동화 탐지 표면(Chrome UA, window.chrome, userAgentData) | 완료 | auto-gui | ✅ PASS (151.3.18) | ❌ FAIL |
| AUTH-008 | AUTH | Google 로그인 페이지 로드 | 완료 | auto-gui | — | ✅ PASS |
| AUTH-009 | AUTH | Google 로그인: 아이디 입력 → Next 에서 크래시 없음(passkey 단계 도달) | 완료 | auto-headless | ✅ PASS (156.0.31) | — |
| AUTH-010 | AUTH | Google 비밀번호 로그인 완주(passkey 화면에서 'Try another way') | 완료 | manual | ✅ PASS (156.0.31) | — |
| AUTH-011 | AUTH | 앱 재시작 후 로그인 상태 유지(주기적 쿠키 flush) | 완료 | manual | ✅ PASS (156.0.31) | — |
| AUTH-012 | AUTH | passkey(Touch ID/Bluetooth) 인증 완료 | 완료 | auto-headless | 🚫 UNSUPPORTED (156.0.31) | — |
| AUTH-013 | AUTH | 종료 직전 쿠키 flush 필요(없으면 CEF 영속 쿠키 유실) | 완료 | auto-gui | ℹ️ INFO (151.3.18) | — |
| AUTH-014 | AUTH | Google 외 서드파티 로그인(GitHub/Apple/Microsoft/네이버/카카오), 2단계 인증 | P0 | manual | ⬜ TODO | — |
| STORE-001 | STORE | 쿠키: JS 가시성 vs 서버 수신(HttpOnly), 세션/영속 | 완료 | auto-headless | ✅ PASS (156.0.31) | — |
| STORE-002 | STORE | Rust(cookie manager)에서 쿠키 설정 → 서버가 수신 | 완료 | auto-headless | ✅ PASS (156.0.31) | — |
| STORE-003 | STORE | 쿠키 개별 삭제 | 완료 | auto-headless | ✅ PASS (156.0.31) | — |
| STORE-004 | STORE | 쿠키 전체 삭제 | 완료 | auto-headless | ✅ PASS (156.0.31) | — |
| STORE-005 | STORE | 프로필/세션 분리(RequestContext 별 캐시 경로, Electron partition 대응), 시크릿(메모리) 모드 | P0 | headless | ⬜ TODO | — |
| STORE-006 | STORE | localStorage/IndexedDB 영속, 쿠키 SameSite 정책별 전송 | P1 | headless | ⬜ TODO | — |
| INPUT-001 | INPUT | 영문 텍스트 입력 | 완료 | auto-gui | ✅ PASS (151.3.18) | ✅ PASS |
| INPUT-002 | INPUT | 한글 IME 조합 이벤트(NSTextInputClient 호출 모사) | 완료 | auto-gui | ✅ PASS (151.3.18) | ✅ PASS |
| INPUT-003 | INPUT | 마우스 클릭 | 완료 | auto-gui | ✅ PASS (151.3.18) | — |
| INPUT-004 | INPUT | 휠 스크롤(gpui 경로) | 완료 | auto-gui | ✅ PASS (151.3.18) | — |
| INPUT-005 | INPUT | 일반 문자 JS keydown + 텍스트 삽입 | 완료 | auto-gui | ✅ PASS (151.3.18) | — |
| INPUT-006 | INPUT | Cmd+A / Cmd+C(클립보드) | 완료 | auto-gui | ✅ PASS (151.3.18) | — |
| INPUT-007 | INPUT | Cmd+V 붙여넣기 | 완료 | auto-gui | ✅ PASS (151.3.18) | — |
| INPUT-008 | INPUT | gpui 단축키가 웹뷰 포커스 중에도 gpui 로 감 | 완료 | auto-gui | ✅ PASS (151.3.18) | — |
| INPUT-009 | INPUT | 커서 모양(링크 hand / 입력칸 ibeam) | 완료 | auto-gui | ✅ PASS (151.3.18) | — |
| INPUT-010 | INPUT | 드래그 텍스트 선택 | 완료 | auto-gui | ✅ PASS (151.3.18) | — |
| INPUT-011 | INPUT | 키보드 스크롤(↓/PageDown/End/Home/Space) | 완료 | auto-headless | ✅ PASS (156.0.31) | — |
| INPUT-012 | INPUT | 편집 키: 입력/Backspace/Delete/← | 완료 | auto-headless | ✅ PASS (156.0.31) | — |
| INPUT-013 | INPUT | Shift+Home 선택 | 완료 | auto-headless | ✅ PASS (156.0.31) | — |
| INPUT-014 | INPUT | Cmd+← 줄 처음(OSR 미전달 → Home 으로 매핑) | 완료 | auto-headless | ✅ PASS (156.0.31) | — |
| INPUT-015 | INPUT | Opt+→ 단어 이동 | 완료 | auto-headless | ✅ PASS (156.0.31) | — |
| INPUT-016 | INPUT | 전체 선택·실행 취소(frame API) | 완료 | auto-headless | ✅ PASS (156.0.31) | — |
| INPUT-017 | INPUT | Tab / Shift+Tab 포커스 순서 | 완료 | auto-headless | ✅ PASS (156.0.31) | — |
| INPUT-018 | INPUT | Enter 로 폼 제출 | 완료 | auto-headless | ✅ PASS (156.0.31) | — |
| INPUT-019 | INPUT | Escape keydown 전달 | 완료 | auto-headless | ✅ PASS (156.0.31) | — |
| INPUT-020 | INPUT | 더블클릭 단어 선택 | 완료 | auto-headless | ✅ PASS (156.0.31) | — |
| INPUT-021 | INPUT | 트리플클릭 문단 선택 | 완료 | auto-headless | ✅ PASS (156.0.31) | — |
| INPUT-022 | INPUT | Shift+클릭 범위 선택 | 완료 | auto-headless | ✅ PASS (156.0.31) | — |
| INPUT-023 | INPUT | 줌 레벨 변경 시 레이아웃 반영 | 완료 | auto-headless | ✅ PASS (156.0.31) | — |
| INPUT-024 | INPUT | 줌 상태에서 클릭 좌표 정확도(앱이 줌 배율로 변환해야 함) | 완료 | auto-headless | ✅ PASS (156.0.31) | — |
| INPUT-025 | INPUT | 줌 레벨이 같은 오리진에서 유지 | 완료 | auto-headless | ✅ PASS (156.0.31) | — |
| INPUT-026 | INPUT | 키 입력 인코딩 규칙(RAWKEYDOWN 단독은 2회 동작 → 기능 문자 필요, Cmd+방향키 미전달) | 완료 | auto-headless | ℹ️ INFO (156.0.31) | — |
| INPUT-027 | INPUT | 실제 한글 키보드 입력(자모 조합·영한 전환·후보창 위치), 일본어/중국어 IME | P0 | manual | ⬜ TODO | — |
| INPUT-028 | INPUT | 핀치 줌(트랙패드 magnify → ctrl+wheel 또는 zoom level), 한계값, 중심점 | P0 | gui | ⬜ TODO | — |
| INPUT-029 | INPUT | 키보드 줌(Cmd +/−/0), 도메인별 기억 연동 | P0 | headless | ⬜ TODO | — |
| INPUT-030 | INPUT | 휠 스크롤 정확도(픽셀/라인, 관성, 가로, 중첩 스크롤), 부드러운 스크롤 | P0 | gui | ⬜ TODO | — |
| INPUT-031 | INPUT | 키 반복·수식키 조합·keydown/up 순서 | P1 | headless | ⬜ TODO | — |
| INPUT-032 | INPUT | 호버/mouseleave, 패널 밖으로 나갈 때 leave 이벤트 | P1 | headless | ⬜ TODO | — |
| INPUT-033 | INPUT | 중간 버튼/Cmd+클릭(새 탭), 마우스 뒤로/앞으로 버튼 | P1 | gui | ⬜ TODO | — |
| INPUT-034 | INPUT | 드래그&드롭(페이지 내, 링크/이미지, 외부 파일 드롭) | P1 | gui | ⬜ TODO | — |
| INPUT-035 | INPUT | 웹뷰↔gpui 입력칸 간 포커스 이동, 창 비활성/활성 시 캐럿·:focus | P0 | gui | ⬜ TODO | — |
| INPUT-036 | INPUT | preventDefault 한 wheel/키가 gpui 단축키와 충돌하지 않음 | P1 | gui | ⬜ TODO | — |
| INPUT-037 | INPUT | 이모지/특수문자 입력, 서식 붙여넣기 | P2 | manual | ⬜ TODO | — |
| NAV-001 | NAV | 뒤로/앞으로 이동 | 완료 | auto-headless | ✅ PASS (156.0.31) | — |
| NAV-002 | NAV | can_go_back / can_go_forward 상태 콜백 | 완료 | auto-headless | ✅ PASS (156.0.31) | — |
| NAV-003 | NAV | 새로고침 | 완료 | auto-headless | ✅ PASS (156.0.31) | — |
| NAV-004 | NAV | SPA pushState 주소 변경 + 뒤로 | 완료 | auto-headless | ✅ PASS (156.0.31) | — |
| NAV-005 | NAV | 제목·주소·파비콘 이벤트 | 완료 | auto-headless | ✅ PASS (156.0.31) | — |
| NAV-006 | NAV | 로드 실패(연결 거부) 오류 콜백 | 완료 | auto-headless | ✅ PASS (156.0.31) | — |
| NAV-007 | NAV | 페이지 내 찾기(개수, 다음, 대소문자 구분) | 완료 | auto-headless | ✅ PASS (156.0.31) | — |
| NAV-008 | NAV | 사용자 제스처 없는 window.open 은 Chromium 이 기본 차단 | 완료 | auto-headless | ✅ PASS (156.0.31) | — |
| NAV-009 | NAV | window.open(제스처) features(크기)·opener 유지 | 완료 | auto-headless | ✅ PASS (156.0.31) | — |
| NAV-010 | NAV | noopener 팝업은 opener 없음 | 완료 | auto-headless | ✅ PASS (156.0.31) | — |
| NAV-011 | NAV | 팝업 허용/차단 정책 훅(on_before_popup) | 완료 | auto-headless | ℹ️ INFO (156.0.31) | — |
| NAV-012 | NAV | 탭: 다중 브라우저 생성·전환·닫기, 백그라운드 탭 렌더 중지(was_hidden), 탭별 크기/포커스 | P0 | headless | ⬜ TODO | — |
| NAV-013 | NAV | 탭 닫기 시 메모리 해제, 탭 복원 | P1 | headless | ⬜ TODO | — |
| NAV-014 | NAV | 외부 프로토콜(mailto:/tel:) → OS 열기 정책 | P2 | headless | ⬜ TODO | — |
| NAV-015 | NAV | 로딩 진행률(on_loading_progress_change) | P1 | headless | ⬜ TODO | — |
| UI-001 | UI | 우클릭 컨텍스트 메뉴를 gpui 가 그림(기본 메뉴 제거) | 완료 | auto-gui | ✅ PASS (151.3.18) | — |
| UI-002 | UI | gpui 메뉴 항목('Select all') 동작·메뉴 닫힘 | 완료 | auto-gui | ✅ PASS (151.3.18) | — |
| UI-003 | UI | JS alert/confirm/prompt 처리 | 완료 | auto-headless | ✅ PASS (156.0.31) | — |
| UI-004 | UI | beforeunload 다이얼로그 | 완료 | auto-headless | ✅ PASS (156.0.31) | — |
| UI-005 | UI | 다운로드(경로 지정, 완료 콜백, 내용 일치) | 완료 | auto-gui | ✅ PASS (151.3.18) | — |
| UI-006 | UI | 파일 선택 <input type=file>(테스트 고정 경로) | 완료 | auto-gui | ✅ PASS (151.3.18) | — |
| UI-007 | UI | 원격 디버깅 엔드포인트(/json/version, /json/list, WS URL) | 완료 | auto-headless | ✅ PASS (156.0.31) | — |
| UI-008 | UI | DevTools UI: gpui 패널(두 번째 windowless 브라우저)에 렌더. 참고: 응답의 frontend URL 은 외부 호스팅(chrome-devtools-frontend.appspot.com) | P0 | headless | ⬜ TODO | — |
| UI-009 | UI | 컨텍스트 메뉴 완성(링크/이미지/선택 텍스트/입력칸/페이지 항목별 동작) | P1 | gui | ⬜ TODO | — |
| UI-010 | UI | 이미지 저장/복사, download_image, 진행률·취소·재시도 | P1 | headless | ⬜ TODO | — |
| UI-011 | UI | 권한 프롬프트(카메라/마이크/위치/알림/클립보드/화면 공유) | P1 | gui | ⬜ TODO | — |
| UI-012 | UI | HTTP 인증 프롬프트, 인증서 오류 경고/예외, 혼합 콘텐츠 | P1 | headless | ⬜ TODO | — |
| UI-013 | UI | 클립보드 API(navigator.clipboard), 이미지 붙여넣기 | P1 | headless | ⬜ TODO | — |
| UI-014 | UI | 전체화면 API(비디오/페이지), Esc 해제와 gpui 레이아웃 연동 | P1 | gui | ⬜ TODO | — |
| UI-015 | UI | 네이티브 파일 열기/저장 대화상자 연동(현재는 테스트용 고정 경로) | P1 | gui | ⬜ TODO | — |
| UI-016 | UI | 인쇄/PDF 저장, 내장 PDF 뷰어 | P2 | headless | ⬜ TODO | — |
| UI-017 | UI | 접근성(스크린 리더)·키보드만으로 조작 | P2 | manual | ⬜ TODO | — |
| WEB-001 | WEB | 미디어: <video>/<audio> 재생, 자동재생 정책, 코덱(H.264/AAC 지원 여부), 음소거, PiP | P0 | headless | ⬜ TODO | — |
| WEB-002 | WEB | WebSocket, Service Worker, WebGL/WebGPU, WebRTC, 웹폰트 | P1 | headless | ⬜ TODO | — |
| WEB-003 | WEB | 한글/이모지 폰트 폴백, 텍스트 렌더링 품질 | P1 | gui | ⬜ TODO | — |
| WEB-004 | WEB | User-Agent/언어/프록시 설정, 다운로드 폴더 정책 | P2 | headless | ⬜ TODO | — |
| IPC-001 | IPC | 프로세스 메시지 브리지(window.workspace.invoke) 왕복 지연 | 완료 | auto-headless | ℹ️ INFO (156.0.31) | — |
| IPC-002 | IPC | 커스텀 스킴(workspace://) 핸들러 왕복 지연 | 완료 | auto-headless | ℹ️ INFO (156.0.31) | — |
| IPC-003 | IPC | 로컬 HTTP 왕복 지연 | 완료 | auto-headless | ℹ️ INFO (156.0.31) | — |
| IPC-004 | IPC | Rust → 페이지 푸시(execute_java_script) | 완료 | auto-headless | ℹ️ INFO (156.0.31) | — |
| IPC-005 | IPC | 브리지는 신뢰 오리진에만 노출(비신뢰 오리진에서는 undefined) | 완료 | auto-headless | ✅ PASS (156.0.31) | — |
| IPC-006 | IPC | 커스텀 스킴/HTTP 는 비신뢰 오리진에서도 접근 가능(오리진 검사 없음) → 핸들러 검사 필요 | 완료 | auto-headless | ❌ FAIL (156.0.31) | — |
| IPC-007 | IPC | 업로드 방향/SharedMemoryRegion/ArrayBuffer/동시성, release 빌드 재측정 | P1 | headless | ⬜ TODO | — |
| IPC-008 | IPC | 브라우저 프로세스 쪽 프레임/오리진 검증 및 우회 시도 테스트 | P0 | headless | ⬜ TODO | — |
| PERF-001 | PERF | 애니메이션 프레임레이트(rAF) | 완료 | auto-gui | ℹ️ INFO (151.3.18) | — |
| PERF-002 | PERF | 리사이즈 지연(토글 → 올바른 크기 프레임 도착) | 완료 | auto-gui | ✅ PASS (151.3.18) | — |
| PERF-003 | PERF | 프레임 파이프라인 비용(CPU 복사 + RenderImage) | 완료 | auto-gui | ℹ️ INFO (151.3.18) | — |
| PERF-004 | PERF | 탭 다수(10~30개) 메모리/CPU, 숨긴 탭 프레임 전달 중지 | P0 | headless | ⬜ TODO | — |
| PERF-005 | PERF | 프레임 전달: CPU 복사 vs IOSurface 공유 텍스처, 4K/고주사율 | P1 | gui | ⬜ TODO | — |
| PERF-006 | PERF | 영상 재생·고해상도 스크롤 프레임 안정성 | P1 | gui | ⬜ TODO | — |
| STAB-001 | STAB | 종료 후 헬퍼 프로세스가 남지 않음 | 완료 | auto-headless | ✅ PASS (156.0.31) | — |
| STAB-002 | STAB | 렌더러 크래시 복구(on_render_process_terminated → 다시 로드), 행 처리 | P0 | headless | ⬜ TODO | — |
| STAB-003 | STAB | 장시간 구동 메모리 누수, 프레임 버퍼 해제 | P1 | headless | ⬜ TODO | — |
| STAB-004 | STAB | 악성/대용량 페이지(무한 루프, 거대 이미지)에서 UI 응답성 | P1 | headless | ⬜ TODO | — |
| STAB-005 | STAB | 번들 덮어쓰기 시 코드서명 캐시 불일치(SIGKILL Code Signature Invalid) 주의 | 완료 | auto-headless | ℹ️ INFO (156.0.31) | — |
| SEC-001 | SEC | IPC 브리지 오리진 통제(비신뢰 오리진/iframe 호출 불가) | P0 | headless | ⬜ TODO | — |
| SEC-002 | SEC | 스킴 핸들러·로컬 서버 오리진/토큰 검사 | P0 | headless | ⬜ TODO | — |
| PKG-001 | PKG | 번들 크기 | 완료 | auto-gui | ℹ️ INFO (156.0.31) | — |
| PKG-002 | PKG | 샌드박스 켜기(현재 no_sandbox) | P2 | backlog | ⬜ TODO | — |
| PKG-003 | PKG | 코드서명·공증, 엔타이틀먼트(passkey 포함) | P2 | backlog | ⬜ TODO | — |
| PKG-004 | PKG | Windows/Linux, Intel Mac | P2 | backlog | ⬜ TODO | — |

## 표 2. CEF 결과 (gpui + CEF off-screen rendering)

| ID | 케이스 | 엔진 버전 | 결과 | 방법 | 상세 | 커밋 | 날짜 |
|---|---|---|---|---|---|---|---|
| COMP-001 | gpui 오버레이(모달/드롭다운)가 웹 콘텐츠 위에 그려짐 | 151.3.18 | ✅ PASS | auto-gui | pixel at overlay center (over web content)=[0, 0, 245]; blue=gpui on top, red=webview on top | `2b5b01e*` | 2026-11-20 |
| COMP-002 | 오버레이 영역 클릭이 gpui 로 전달됨(웹뷰가 가로채지 않음) | 151.3.18 | ✅ PASS | auto-gui | gpui overlay_clicks=1, webview_clicks=Some("1") (gpui=1 & webview stays 1 expected) | `2b5b01e*` | 2026-11-20 |
| COMP-003 | 레이아웃 변경(사이드바 폭)에 웹뷰 영역이 동기화됨 | 151.3.18 | ✅ PASS | auto-gui | webview left edge x=Some(408), expected≈408; browser logical size now 784x744 (canvas 784.0x744.0) | `2b5b01e*` | 2026-11-20 |
| COMP-004 | 둥근 모서리 클립 | 151.3.18 | ✅ PASS | auto-gui | corner pixel=[76, 167, 48] (not red = clipped by gpui rounded mask) | `2b5b01e*` | 2026-11-20 |
| COMP-005 | 웹뷰 숨김/표시(탭 전환) | 151.3.18 | ✅ PASS | auto-gui | hidden px=[76, 167, 48], shown px=[234, 51, 35] | `2b5b01e*` | 2026-11-20 |
| COMP-006 | 배경 미지정 페이지가 불투명 흰색으로 그려짐(OSR 기본은 투명) | 151.3.18 | ✅ PASS | auto-gui | page without CSS background pixel=[255, 255, 255] (white expected; green = transparent OSR bg leaks gpui) | `2b5b01e*` | 2026-11-20 |
| COMP-007 | <select> 팝업 위젯 렌더링 | 151.3.18 | ✅ PASS | auto-gui | widget_visible=true rect=(36, 62, 68, 93) | `2b5b01e*` | 2026-11-20 |
| COMP-008 | <select> 항목 선택 → change 이벤트·팝업 닫힘 | 151.3.18 | ✅ PASS | auto-gui | change report=Some("two") popup_closed=true | `2b5b01e*` | 2026-11-20 |
| COMP-009 | 메인 + 팝업 브라우저 동시 렌더(팝업을 gpui 레이어로 그림) | 151.3.18 | ✅ PASS | auto-gui | OAuth 팝업이 gpui 중앙 레이어로 렌더, opener 유지, 닫힘 처리 | `2b5b01e*` | 2026-11-20 |
| COMP-010 | 리사이즈/스플리터 드래그 중 깜빡임·잔상 |  | ⬜ TODO | plan:gui |  |  |  |
| COMP-011 | gpui 오버레이 위 입력 차단, 웹뷰 영역 밖으로 나가는 드롭다운 |  | ⬜ TODO | plan:gui |  |  |  |
| COMP-012 | 고DPI/다중 모니터 DPI 변경, 텍스트 선명도 |  | ⬜ TODO | plan:gui |  |  |  |
| COMP-013 | 다크모드(prefers-color-scheme) 연동 |  | ⬜ TODO | plan:headless |  |  |  |
| AUTH-001 | 폼 로그인 → Set-Cookie 세션 유지 | 151.3.18 | ✅ PASS | auto-gui | /me={"psid":true,"ssid":true} | `2b5b01e*` | 2026-11-20 |
| AUTH-002 | 영속 쿠키가 앱 재시작 후에도 유지 | 151.3.18 | ✅ PASS | auto-gui | after restart /me={"psid":true,"ssid":true} | `2b5b01e*` | 2026-11-20 |
| AUTH-003 | 세션 쿠키 재시작 후 정책(정보) | 151.3.18 | ℹ️ INFO | auto-gui | ssid kept after restart: true | `2b5b01e*` | 2026-11-20 |
| AUTH-004 | OAuth 팝업(window.open + postMessage + close) | 151.3.18 | ✅ PASS | auto-gui | clicked=true popup_shown=true popup_clicked=true report=Some("OAUTH_OK:abc123") popup_closed_count=1 | `2b5b01e*` | 2026-11-20 |
| AUTH-005 | target=_blank 링크 처리 | 151.3.18 | ✅ PASS | auto-gui | clicked=true report=Some("OPENED") | `2b5b01e*` | 2026-11-20 |
| AUTH-006 | 3rd-party 쿠키(SameSite=None, cross-site iframe) | 151.3.18 | ✅ PASS | auto-gui | report=Some("TP:SENT") | `2b5b01e*` | 2026-11-20 |
| AUTH-007 | UA·자동화 탐지 표면(Chrome UA, window.chrome, userAgentData) | 151.3.18 | ✅ PASS | auto-gui | {"ua":"Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/151.0.0.0 Safari/537.36","webdriver":false,"chrome":"object","uaData":"object","brands":["Chromium","Not=A?Brand"],"vendor":"Google Inc.","platform":"MacIntel","lang":"en-US"} | `2b5b01e*` | 2026-11-20 |
| AUTH-009 | Google 로그인: 아이디 입력 → Next 에서 크래시 없음(passkey 단계 도달) | 151.3.18 | ❌ FAIL | auto-headless | 10/10 크래시(창 없는 헤드리스). 브라우저 프로세스 UI 스레드 CEF 내부 널 역참조(SIGSEGV, 주소 0x10, 오프셋 0x19330e0). 구글의 조건부 passkey 요청(rpId google.com, allowCredentials [], timeout 600000) 직후. WebAuthn API 제거·feature 비활성·prefs 비활성으로 해결 안 됨 | `f164c33*` | 2026-11-20 |
| AUTH-009 | Google 로그인: 아이디 입력 → Next 에서 크래시 없음(passkey 단계 도달) | 156.0.31 | ✅ PASS | auto-headless | 0/13 크래시, 모두 'Complete sign-in using your passkey' 화면 도달(PNG 확인) → 151 버그, 156에서 해결 | `f164c33*` | 2026-11-20 |
| AUTH-010 | Google 비밀번호 로그인 완주(passkey 화면에서 'Try another way') | 156.0.31 | ✅ PASS | manual | 사용자가 수동으로 로그인 성공 | `cc43ae1*` | 2026-11-20 |
| AUTH-011 | 앱 재시작 후 로그인 상태 유지(주기적 쿠키 flush) | 156.0.31 | ✅ PASS | manual | 사용자가 수동 확인 | `cc43ae1*` | 2026-11-20 |
| AUTH-012 | passkey(Touch ID/Bluetooth) 인증 완료 | 156.0.31 | 🚫 UNSUPPORTED | auto-headless | keychain-access-groups 엔타이틀먼트·Bluetooth Info.plist 권한 없이는 불가. 비밀번호 폴백으로 충분(요구사항 아님). BACKLOG | `f164c33*` | 2026-11-20 |
| AUTH-013 | 종료 직전 쿠키 flush 필요(없으면 CEF 영속 쿠키 유실) | 151.3.18 | ℹ️ INFO | auto-gui | flush 없이 종료 시 psid 유실을 실측 → 주기 flush/종료 시 flush 필수 | `2b5b01e*` | 2026-11-20 |
| AUTH-014 | Google 외 서드파티 로그인(GitHub/Apple/Microsoft/네이버/카카오), 2단계 인증 |  | ⬜ TODO | plan:manual |  |  |  |
| STORE-001 | 쿠키: JS 가시성 vs 서버 수신(HttpOnly), 세션/영속 | 156.0.31 | ✅ PASS | auto-headless | document.cookie="pers=1; sess=2" server saw="pers=1; sess=2; hon=3" (HttpOnly hidden from JS, visible to server) | `218b7f5` | 2026-11-20 |
| STORE-002 | Rust(cookie manager)에서 쿠키 설정 → 서버가 수신 | 156.0.31 | ✅ PASS | auto-headless | set_cookie->1 server saw="pers=1; sess=2; hon=3; mgr=fromrust" | `218b7f5` | 2026-11-20 |
| STORE-003 | 쿠키 개별 삭제 | 156.0.31 | ✅ PASS | auto-headless | after delete mgr: "pers=1; sess=2; hon=3" | `218b7f5` | 2026-11-20 |
| STORE-004 | 쿠키 전체 삭제 | 156.0.31 | ✅ PASS | auto-headless | after delete all: "" | `218b7f5` | 2026-11-20 |
| STORE-005 | 프로필/세션 분리(RequestContext 별 캐시 경로, Electron partition 대응), 시크릿(메모리) 모드 |  | ⬜ TODO | plan:headless |  |  |  |
| STORE-006 | localStorage/IndexedDB 영속, 쿠키 SameSite 정책별 전송 |  | ⬜ TODO | plan:headless |  |  |  |
| INPUT-001 | 영문 텍스트 입력 | 151.3.18 | ✅ PASS | auto-gui | value="abc한" | `2b5b01e*` | 2026-11-20 |
| INPUT-002 | 한글 IME 조합 이벤트(NSTextInputClient 호출 모사) | 151.3.18 | ✅ PASS | auto-gui | responder=GPUIView value="abc한" log=["input:abc","compositionstart:","compositionupdate:ㅎ","input:ㅎ","compositionupdate:하","input:하","compositionupdate:한","input:한","compositionupdate:한","input:한","compositionend:한"] | `2b5b01e*` | 2026-11-20 |
| INPUT-003 | 마우스 클릭 | 151.3.18 | ✅ PASS | auto-gui | mouse report={"click":1,"wheel":0,"move":0} | `2b5b01e*` | 2026-11-20 |
| INPUT-004 | 휠 스크롤(gpui 경로) | 151.3.18 | ✅ PASS | auto-gui | after 6x(-60px) scrollY=Some("360"); after 3x(+60px) scrollY=Some("180") | `2b5b01e*` | 2026-11-20 |
| INPUT-005 | 일반 문자 JS keydown + 텍스트 삽입 | 151.3.18 | ✅ PASS | auto-gui | value="hi" log=["keydown:h","keydown:Unidentified","input:h","keydown:i","keydown:Unidentified","input:i"] handler_calls=["key_down handler key=\"h\" vk=72", "key_down handler key=\"i\" vk=73"] | `2b5b01e*` | 2026-11-20 |
| INPUT-006 | Cmd+A / Cmd+C(클립보드) | 151.3.18 | ✅ PASS | auto-gui | clipboard after Cmd+A,Cmd+C="hi" (sentinel was SENTINEL) | `2b5b01e*` | 2026-11-20 |
| INPUT-007 | Cmd+V 붙여넣기 | 151.3.18 | ✅ PASS | auto-gui | input value after Cmd+V="PASTED" | `2b5b01e*` | 2026-11-20 |
| INPUT-008 | gpui 단축키가 웹뷰 포커스 중에도 gpui 로 감 | 151.3.18 | ✅ PASS | auto-gui | gpui action count 0->1; page saw k keydown: false | `2b5b01e*` | 2026-11-20 |
| INPUT-009 | 커서 모양(링크 hand / 입력칸 ibeam) | 151.3.18 | ✅ PASS | auto-gui | (element, got, want)=[("hand", 1, 1), ("ibeam", 2, 2)] (0=arrow 1=hand 2=ibeam) | `2b5b01e*` | 2026-11-20 |
| INPUT-010 | 드래그 텍스트 선택 | 151.3.18 | ✅ PASS | auto-gui | selection after drag="drag select this sentence ple" | `2b5b01e*` | 2026-11-20 |
| INPUT-011 | 키보드 스크롤(↓/PageDown/End/Home/Space) | 156.0.31 | ✅ PASS | auto-headless | start=0 down×3=120 pagedown=880 end=4200 home=0 space=760 | `218b7f5` | 2026-11-20 |
| INPUT-012 | 편집 키: 입력/Backspace/Delete/← | 156.0.31 | ✅ PASS | auto-headless | typed="hello world" backspace="hello worl" left×2+delete="hello wol" | `218b7f5` | 2026-11-20 |
| INPUT-013 | Shift+Home 선택 | 156.0.31 | ✅ PASS | auto-headless | selection after Shift+Home="hello wo" | `218b7f5` | 2026-11-20 |
| INPUT-014 | Cmd+← 줄 처음(OSR 미전달 → Home 으로 매핑) | 156.0.31 | ✅ PASS | auto-headless | caret after End, Cmd+←=0 (0 expected) | `218b7f5` | 2026-11-20 |
| INPUT-015 | Opt+→ 단어 이동 | 156.0.31 | ✅ PASS | auto-headless | caret after Opt+→=5 (5 expected: end of 'hello') | `218b7f5` | 2026-11-20 |
| INPUT-016 | 전체 선택·실행 취소(frame API) | 156.0.31 | ✅ PASS | auto-headless | select_all="hello wol" after undo="hello worl" | `218b7f5` | 2026-11-20 |
| INPUT-017 | Tab / Shift+Tab 포커스 순서 | 156.0.31 | ✅ PASS | auto-headless | a→b→c, shift+tab→b (start a) | `218b7f5` | 2026-11-20 |
| INPUT-018 | Enter 로 폼 제출 | 156.0.31 | ✅ PASS | auto-headless | submit events=1 | `218b7f5` | 2026-11-20 |
| INPUT-019 | Escape keydown 전달 | 156.0.31 | ✅ PASS | auto-headless | Escape keydown events=1 | `218b7f5` | 2026-11-20 |
| INPUT-020 | 더블클릭 단어 선택 | 156.0.31 | ✅ PASS | auto-headless | double-click selection="beta" | `218b7f5` | 2026-11-20 |
| INPUT-021 | 트리플클릭 문단 선택 | 156.0.31 | ✅ PASS | auto-headless | triple-click selection="alpha beta gamma delta" | `218b7f5` | 2026-11-20 |
| INPUT-022 | Shift+클릭 범위 선택 | 156.0.31 | ✅ PASS | auto-headless | selection after click + shift-click="lpha beta gamma" | `218b7f5` | 2026-11-20 |
| INPUT-023 | 줌 레벨 변경 시 레이아웃 반영 | 156.0.31 | ✅ PASS | auto-headless | level=3.80 innerWidth 1100→550 (ratio 2.00, ≈2 expected) | `218b7f5` | 2026-11-20 |
| INPUT-024 | 줌 상태에서 클릭 좌표 정확도(앱이 줌 배율로 변환해야 함) | 156.0.31 | ✅ PASS | auto-headless | css center=(90,90), factor 2.00: click at css×factor hits=true, at raw css coords hits=false → 앱이 줌 배율로 좌표를 변환해야 함 | `218b7f5` | 2026-11-20 |
| INPUT-025 | 줌 레벨이 같은 오리진에서 유지 | 156.0.31 | ✅ PASS | auto-headless | level after navigating same-origin page=3.80 | `218b7f5` | 2026-11-20 |
| INPUT-026 | 키 입력 인코딩 규칙(RAWKEYDOWN 단독은 2회 동작 → 기능 문자 필요, Cmd+방향키 미전달) | 156.0.31 | ℹ️ INFO | auto-headless | 실험(keyexp): RAWKEYDOWN 단독=편집키 2회 동작, +macOS 기능 문자=1회, Space는 +CHAR 필요, Cmd+←/→ 는 어떤 인코딩도 무효 → keys.rs 에 규칙·매핑 반영 | `218b7f5` | 2026-11-20 |
| INPUT-027 | 실제 한글 키보드 입력(자모 조합·영한 전환·후보창 위치), 일본어/중국어 IME |  | ⬜ TODO | plan:manual |  |  |  |
| INPUT-028 | 핀치 줌(트랙패드 magnify → ctrl+wheel 또는 zoom level), 한계값, 중심점 |  | ⬜ TODO | plan:gui |  |  |  |
| INPUT-029 | 키보드 줌(Cmd +/−/0), 도메인별 기억 연동 |  | ⬜ TODO | plan:headless |  |  |  |
| INPUT-030 | 휠 스크롤 정확도(픽셀/라인, 관성, 가로, 중첩 스크롤), 부드러운 스크롤 |  | ⬜ TODO | plan:gui |  |  |  |
| INPUT-031 | 키 반복·수식키 조합·keydown/up 순서 |  | ⬜ TODO | plan:headless |  |  |  |
| INPUT-032 | 호버/mouseleave, 패널 밖으로 나갈 때 leave 이벤트 |  | ⬜ TODO | plan:headless |  |  |  |
| INPUT-033 | 중간 버튼/Cmd+클릭(새 탭), 마우스 뒤로/앞으로 버튼 |  | ⬜ TODO | plan:gui |  |  |  |
| INPUT-034 | 드래그&드롭(페이지 내, 링크/이미지, 외부 파일 드롭) |  | ⬜ TODO | plan:gui |  |  |  |
| INPUT-035 | 웹뷰↔gpui 입력칸 간 포커스 이동, 창 비활성/활성 시 캐럿·:focus |  | ⬜ TODO | plan:gui |  |  |  |
| INPUT-036 | preventDefault 한 wheel/키가 gpui 단축키와 충돌하지 않음 |  | ⬜ TODO | plan:gui |  |  |  |
| INPUT-037 | 이모지/특수문자 입력, 서식 붙여넣기 |  | ⬜ TODO | plan:manual |  |  |  |
| NAV-001 | 뒤로/앞으로 이동 | 156.0.31 | ✅ PASS | auto-headless | back→http://127.0.0.1:18080/hist/2 back→http://127.0.0.1:18080/hist/1 fwd×2→http://127.0.0.1:18080/hist/3 | `218b7f5` | 2026-11-20 |
| NAV-002 | can_go_back / can_go_forward 상태 콜백 | 156.0.31 | ✅ PASS | auto-headless | at /hist/3 back=true fwd=false; at /hist/1 back=true (이전 항목 존재) fwd=true | `218b7f5` | 2026-11-20 |
| NAV-003 | 새로고침 | 156.0.31 | ✅ PASS | auto-headless | loading events 46→48 | `218b7f5` | 2026-11-20 |
| NAV-004 | SPA pushState 주소 변경 + 뒤로 | 156.0.31 | ✅ PASS | auto-headless | pushState→http://127.0.0.1:18080/spa/x (title "spa-x") back→http://127.0.0.1:18080/spa | `218b7f5` | 2026-11-20 |
| NAV-005 | 제목·주소·파비콘 이벤트 | 156.0.31 | ✅ PASS | auto-headless | title="My Titled Page" address=http://127.0.0.1:18080/titled favicon events=1 | `218b7f5` | 2026-11-20 |
| NAV-006 | 로드 실패(연결 거부) 오류 콜백 | 156.0.31 | ✅ PASS | auto-headless | load errors=["Errorcode(ERR_CONNECTION_REFUSED) ERR_CONNECTION_REFUSED http://127.0.0.1:54329/"] | `218b7f5` | 2026-11-20 |
| NAV-007 | 페이지 내 찾기(개수, 다음, 대소문자 구분) | 156.0.31 | ✅ PASS | auto-headless | case-insensitive count=6 initial active ordinal=0 (CEF 는 첫 검색에서 0); find-next → 1; case-sensitive count=4 (6 / +1 / 4 expected) | `218b7f5` | 2026-11-20 |
| NAV-008 | 사용자 제스처 없는 window.open 은 Chromium 이 기본 차단 | 156.0.31 | ✅ PASS | auto-headless | popup opened=false on_before_popup calls=0 (Chromium 이 기본 차단하므로 핸들러까지 오지 않음) | `218b7f5` | 2026-11-20 |
| NAV-009 | window.open(제스처) features(크기)·opener 유지 | 156.0.31 | ✅ PASS | auto-headless | log=["popup url=http://127.0.0.1:18080/popchild gesture=1 features=400x300"] child saw=Some("OPENER") | `218b7f5` | 2026-11-20 |
| NAV-010 | noopener 팝업은 opener 없음 | 156.0.31 | ✅ PASS | auto-headless | child saw=Some("NO_OPENER") | `218b7f5` | 2026-11-20 |
| NAV-011 | 팝업 허용/차단 정책 훅(on_before_popup) | 156.0.31 | ℹ️ INFO | auto-headless | on_before_popup 에서 user_gesture/features/URL 로 허용·차단을 결정할 수 있음(차단 시 1 반환) — 코드 경로 존재, 제스처 있는 차단 시나리오는 미검증 | `218b7f5` | 2026-11-20 |
| NAV-012 | 탭: 다중 브라우저 생성·전환·닫기, 백그라운드 탭 렌더 중지(was_hidden), 탭별 크기/포커스 |  | ⬜ TODO | plan:headless |  |  |  |
| NAV-013 | 탭 닫기 시 메모리 해제, 탭 복원 |  | ⬜ TODO | plan:headless |  |  |  |
| NAV-014 | 외부 프로토콜(mailto:/tel:) → OS 열기 정책 |  | ⬜ TODO | plan:headless |  |  |  |
| NAV-015 | 로딩 진행률(on_loading_progress_change) |  | ⬜ TODO | plan:headless |  |  |  |
| UI-001 | 우클릭 컨텍스트 메뉴를 gpui 가 그림(기본 메뉴 제거) | 151.3.18 | ✅ PASS | auto-gui | ctx=Some((120, 60, "")) | `2b5b01e*` | 2026-11-20 |
| UI-002 | gpui 메뉴 항목('Select all') 동작·메뉴 닫힘 | 151.3.18 | ✅ PASS | auto-gui | selection after gpui menu 'Select all'="select me with context menu" menu_closed=true | `2b5b01e*` | 2026-11-20 |
| UI-003 | JS alert/confirm/prompt 처리 | 156.0.31 | ✅ PASS | auto-headless | page result={"a":"ok","c":true,"p":"spike-input"} handler log=["JsdialogType(JSDIALOGTYPE_ALERT):A", "JsdialogType(JSDIALOGTYPE_CONFIRM):C", "JsdialogType(JSDIALOGTYPE_PROMPT):P"] | `218b7f5` | 2026-11-20 |
| UI-004 | beforeunload 다이얼로그 | 156.0.31 | ✅ PASS | auto-headless | handler log after navigating away=["beforeunload:Is it OK to leave/reload this page?"] | `218b7f5` | 2026-11-20 |
| UI-005 | 다운로드(경로 지정, 완료 콜백, 내용 일치) | 151.3.18 | ✅ PASS | auto-gui | file content="hello download" log=["start spike.txt", "complete 14 bytes"] | `2b5b01e*` | 2026-11-20 |
| UI-006 | 파일 선택 <input type=file>(테스트 고정 경로) | 151.3.18 | ✅ PASS | auto-gui | clicked=true report={"name":"upload-sample.txt","size":10} | `2b5b01e*` | 2026-11-20 |
| UI-007 | 원격 디버깅 엔드포인트(/json/version, /json/list, WS URL) | 156.0.31 | ✅ PASS | auto-headless | version ok=true list has page target=true (port 9222); version head="HTTP/1.1 200 OK\r\nContent-Security-Policy:frame-ancestors 'none'\r\nContent-Length:451\r\nContent-Type:application/json; charset=UTF-8\r\n\r\n{\n   \"Browser\": \"Chrome/156" | `218b7f5` | 2026-11-20 |
| UI-008 | DevTools UI: gpui 패널(두 번째 windowless 브라우저)에 렌더. 참고: 응답의 frontend URL 은 외부 호스팅(chrome-devtools-frontend.appspot.com) |  | ⬜ TODO | plan:headless |  |  |  |
| UI-009 | 컨텍스트 메뉴 완성(링크/이미지/선택 텍스트/입력칸/페이지 항목별 동작) |  | ⬜ TODO | plan:gui |  |  |  |
| UI-010 | 이미지 저장/복사, download_image, 진행률·취소·재시도 |  | ⬜ TODO | plan:headless |  |  |  |
| UI-011 | 권한 프롬프트(카메라/마이크/위치/알림/클립보드/화면 공유) |  | ⬜ TODO | plan:gui |  |  |  |
| UI-012 | HTTP 인증 프롬프트, 인증서 오류 경고/예외, 혼합 콘텐츠 |  | ⬜ TODO | plan:headless |  |  |  |
| UI-013 | 클립보드 API(navigator.clipboard), 이미지 붙여넣기 |  | ⬜ TODO | plan:headless |  |  |  |
| UI-014 | 전체화면 API(비디오/페이지), Esc 해제와 gpui 레이아웃 연동 |  | ⬜ TODO | plan:gui |  |  |  |
| UI-015 | 네이티브 파일 열기/저장 대화상자 연동(현재는 테스트용 고정 경로) |  | ⬜ TODO | plan:gui |  |  |  |
| UI-016 | 인쇄/PDF 저장, 내장 PDF 뷰어 |  | ⬜ TODO | plan:headless |  |  |  |
| UI-017 | 접근성(스크린 리더)·키보드만으로 조작 |  | ⬜ TODO | plan:manual |  |  |  |
| WEB-001 | 미디어: <video>/<audio> 재생, 자동재생 정책, 코덱(H.264/AAC 지원 여부), 음소거, PiP |  | ⬜ TODO | plan:headless |  |  |  |
| WEB-002 | WebSocket, Service Worker, WebGL/WebGPU, WebRTC, 웹폰트 |  | ⬜ TODO | plan:headless |  |  |  |
| WEB-003 | 한글/이모지 폰트 폴백, 텍스트 렌더링 품질 |  | ⬜ TODO | plan:gui |  |  |  |
| WEB-004 | User-Agent/언어/프록시 설정, 다운로드 폴더 정책 |  | ⬜ TODO | plan:headless |  |  |  |
| IPC-001 | 프로세스 메시지 브리지(window.workspace.invoke) 왕복 지연 | 156.0.31 | ℹ️ INFO | auto-headless | 64B 평균 0.79ms, 1MB 평균 7.0ms(p99 39.8ms). debug 빌드, 순차 요청 | `191dd55*` | 2026-11-20 |
| IPC-002 | 커스텀 스킴(workspace://) 핸들러 왕복 지연 | 156.0.31 | ℹ️ INFO | auto-headless | 64B 평균 0.21ms, 1MB 평균 1.55ms | `191dd55*` | 2026-11-20 |
| IPC-003 | 로컬 HTTP 왕복 지연 | 156.0.31 | ℹ️ INFO | auto-headless | 64B 평균 0.71ms, 1MB 평균 1.55ms | `191dd55*` | 2026-11-20 |
| IPC-004 | Rust → 페이지 푸시(execute_java_script) | 156.0.31 | ℹ️ INFO | auto-headless | 2000회 큐잉 2.7ms, 전부 반영 115ms(호출당 약 58µs) | `191dd55*` | 2026-11-20 |
| IPC-005 | 브리지는 신뢰 오리진에만 노출(비신뢰 오리진에서는 undefined) | 156.0.31 | ✅ PASS | auto-headless | 127.0.0.1: window.workspace=object, localhost: undefined | `191dd55*` | 2026-11-20 |
| IPC-006 | 커스텀 스킴/HTTP 는 비신뢰 오리진에서도 접근 가능(오리진 검사 없음) → 핸들러 검사 필요 | 156.0.31 | ❌ FAIL | auto-headless | 의도적으로 검사 없이 구현한 상태에서 비신뢰 오리진 fetch 성공을 확인(위험 확인용) | `191dd55*` | 2026-11-20 |
| IPC-007 | 업로드 방향/SharedMemoryRegion/ArrayBuffer/동시성, release 빌드 재측정 |  | ⬜ TODO | plan:headless |  |  |  |
| IPC-008 | 브라우저 프로세스 쪽 프레임/오리진 검증 및 우회 시도 테스트 |  | ⬜ TODO | plan:headless |  |  |  |
| PERF-001 | 애니메이션 프레임레이트(rAF) | 151.3.18 | ℹ️ INFO | auto-gui | page rAF={"raf":182,"ms":3011}; CEF on_paint=181 in 3.0s (60.2/s); gpui paints=181 (60.2/s) | `2b5b01e*` | 2026-11-20 |
| PERF-002 | 리사이즈 지연(토글 → 올바른 크기 프레임 도착) | 151.3.18 | ✅ PASS | auto-gui | toggle→correctly-sized frame: avg=32ms max=67ms samples=[22.036584, 23.698666, 55.897458, 16.855666, 16.730999999999998, 66.711958, 16.449917000000003, 16.896125, 66.552209, 17.185916] (측정 실패=-1) | `2b5b01e*` | 2026-11-20 |
| PERF-003 | 프레임 파이프라인 비용(CPU 복사 + RenderImage) | 151.3.18 | ℹ️ INFO | auto-gui | on_paint callbacks=292 avg copy=1.00ms; gpui paint closures=335 avg RenderImage+paint_image build=2.22ms (GPU upload excluded) | `2b5b01e*` | 2026-11-20 |
| PERF-004 | 탭 다수(10~30개) 메모리/CPU, 숨긴 탭 프레임 전달 중지 |  | ⬜ TODO | plan:headless |  |  |  |
| PERF-005 | 프레임 전달: CPU 복사 vs IOSurface 공유 텍스처, 4K/고주사율 |  | ⬜ TODO | plan:gui |  |  |  |
| PERF-006 | 영상 재생·고해상도 스크롤 프레임 안정성 |  | ⬜ TODO | plan:gui |  |  |  |
| STAB-001 | 종료 후 헬퍼 프로세스가 남지 않음 | 156.0.31 | ✅ PASS | auto-headless | 스위트 종료 2초 뒤 spike-cef 프로세스 0개 | `218b7f5` | 2026-11-20 |
| STAB-002 | 렌더러 크래시 복구(on_render_process_terminated → 다시 로드), 행 처리 |  | ⬜ TODO | plan:headless |  |  |  |
| STAB-003 | 장시간 구동 메모리 누수, 프레임 버퍼 해제 |  | ⬜ TODO | plan:headless |  |  |  |
| STAB-004 | 악성/대용량 페이지(무한 루프, 거대 이미지)에서 UI 응답성 |  | ⬜ TODO | plan:headless |  |  |  |
| STAB-005 | 번들 덮어쓰기 시 코드서명 캐시 불일치(SIGKILL Code Signature Invalid) 주의 | 156.0.31 | ℹ️ INFO | auto-headless | 번들 삭제 후 재생성 + ad-hoc 재서명으로 해결(스크립트 반영) | `beace55` | 2026-11-20 |
| SEC-001 | IPC 브리지 오리진 통제(비신뢰 오리진/iframe 호출 불가) |  | ⬜ TODO | plan:headless |  |  |  |
| SEC-002 | 스킴 핸들러·로컬 서버 오리진/토큰 검사 |  | ⬜ TODO | plan:headless |  |  |  |
| PKG-001 | 번들 크기 | 151.3.18 | ℹ️ INFO | auto-gui | 370MB(CEF 프레임워크 343MB) | `2b5b01e*` | 2026-11-20 |
| PKG-001 | 번들 크기 | 156.0.31 | ℹ️ INFO | auto-headless | 381MB | `218b7f5` | 2026-11-20 |
| PKG-002 | 샌드박스 켜기(현재 no_sandbox) |  | ⬜ TODO | plan:backlog |  |  |  |
| PKG-003 | 코드서명·공증, 엔타이틀먼트(passkey 포함) |  | ⬜ TODO | plan:backlog |  |  |  |
| PKG-004 | Windows/Linux, Intel Mac |  | ⬜ TODO | plan:backlog |  |  |  |

## 표 3. wry 결과 (gpui + wry/WKWebView child view)

| ID | 케이스 | 엔진 버전 | 결과 | 방법 | 상세 | 커밋 | 날짜 |
|---|---|---|---|---|---|---|---|
| COMP-001 | gpui 오버레이(모달/드롭다운)가 웹 콘텐츠 위에 그려짐 | macOS 26.6 WebKit | ❌ FAIL | auto-gui | pixel at overlay center=[234, 51, 35]; blue=gpui on top, red=native webview on top | `9389bb2*` | 2026-11-20 |
| COMP-002 | 오버레이 영역 클릭이 gpui 로 전달됨(웹뷰가 가로채지 않음) | macOS 26.6 WebKit | ❌ FAIL | auto-gui | gpui overlay_clicks=0, webview_clicks=Some("2") (gpui=1 & webview stays 1 expected) | `9389bb2*` | 2026-11-20 |
| COMP-003 | 레이아웃 변경(사이드바 폭)에 웹뷰 영역이 동기화됨 | macOS 26.6 WebKit | ✅ PASS | auto-gui | webview left edge x=Some(408), expected≈408 | `9389bb2*` | 2026-11-20 |
| COMP-004 | 둥근 모서리 클립 | macOS 26.6 WebKit | ✅ PASS | auto-gui | [우회 필요: 네이티브 CALayer.cornerRadius 를 직접 적용해야 함] corner pixel before radius=[234, 51, 35] (red = squared), after CALayer.cornerRadius=[76, 167, 48] | `9389bb2*` | 2026-11-20 |
| COMP-005 | 웹뷰 숨김/표시(탭 전환) | macOS 26.6 WebKit | ✅ PASS | auto-gui | hidden px=[76, 167, 48], shown px=[234, 51, 35] | `9389bb2*` | 2026-11-20 |
| AUTH-001 | 폼 로그인 → Set-Cookie 세션 유지 | macOS 26.6 WebKit | ✅ PASS | auto-gui | /me={"psid":true,"ssid":true} | `9389bb2*` | 2026-11-20 |
| AUTH-002 | 영속 쿠키가 앱 재시작 후에도 유지 | macOS 26.6 WebKit | ✅ PASS | auto-gui | after restart /me={"psid":true,"ssid":false} | `9389bb2*` | 2026-11-20 |
| AUTH-003 | 세션 쿠키 재시작 후 정책(정보) | macOS 26.6 WebKit | ℹ️ INFO | auto-gui | ssid kept after restart: false | `9389bb2*` | 2026-11-20 |
| AUTH-004 | OAuth 팝업(window.open + postMessage + close) | macOS 26.6 WebKit | ❌ FAIL | auto-gui | clicked=true report=None | `9389bb2*` | 2026-11-20 |
| AUTH-005 | target=_blank 링크 처리 | macOS 26.6 WebKit | ✅ PASS | auto-gui | clicked=true report=Some("OPENED") | `9389bb2*` | 2026-11-20 |
| AUTH-006 | 3rd-party 쿠키(SameSite=None, cross-site iframe) | macOS 26.6 WebKit | ❌ FAIL | auto-gui | report=Some("TP:BLOCKED") (Chromium allows; WebKit ITP typically blocks) | `9389bb2*` | 2026-11-20 |
| AUTH-007 | UA·자동화 탐지 표면(Chrome UA, window.chrome, userAgentData) | macOS 26.6 WebKit | ❌ FAIL | auto-gui | {"ua":"Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko)","webdriver":false,"chrome":"undefined","uaData":"undefined","brands":null,"vendor":"Apple Computer, Inc.","platform":"MacIntel","lang":"en-US"} | `9389bb2*` | 2026-11-20 |
| AUTH-008 | Google 로그인 페이지 로드 | macOS 26.6 WebKit | ✅ PASS | auto-gui | url="https:\/\/accounts.google.com\/v3\/signin\/identifier?hl=en&flowName=WebLiteSignIn&flowEntry=ServiceLogin&dsh=S-634803707:1795135231417611" text="\nSign in\nUse your Google Account\n\nEmail or phone\nForgot email?\nNot your computer? Use a private browsing window to sign in. Learn more about using Guest mode\nNext\nCreate account\n\nHelpPrivacyTerms" | `9389bb2*` | 2026-11-20 |
| INPUT-001 | 영문 텍스트 입력 | macOS 26.6 WebKit | ✅ PASS | auto-gui | value="abc한" | `9389bb2*` | 2026-11-20 |
| INPUT-002 | 한글 IME 조합 이벤트(NSTextInputClient 호출 모사) | macOS 26.6 WebKit | ✅ PASS | auto-gui | responder=..NSKVONotifying_wry::wkwebview::class::wry_web_view::WryWebView0.57.3 value="abc한" log=["input:abc","compositionstart:","compositionupdate:ㅎ","input:ㅎ","compositionupdate:하","input:하","compositionupdate:한","input:한","input","input:한","compositionend:한"] | `9389bb2*` | 2026-11-20 |
<!-- RESULTS:END -->

## 주요 발견·제약

- **Google 로그인 크래시(AUTH-009)**: 증상은 브라우저 프로세스 UI 스레드의 CEF 내부 널 역참조(`SIGSEGV`, 주소 `0x10`). 구글이 Next 직후 보내는 조건부 passkey 요청(`rpId: google.com`, `allowCredentials: []`, `timeout: 600000`) 직후에 발생한다. 시도했지만 효과 없던 것: WebAuthn API 제거, iCloud 키체인 등 feature 비활성, Blink `WebAuth` 비활성, 비밀번호 관리자·자동완성 prefs 비활성, 런타임 스타일 변경(윈도우리스는 항상 Alloy 로 생성됨). CEF 156 업그레이드로만 해결됐다. 로컬 페이지의 모달·조건부 WebAuthn 호출만으로는 151에서도 크래시하지 않는다.
- **키 입력 인코딩(INPUT)**: `RAWKEYDOWN` 단독은 편집·이동 키가 2번 동작한다. macOS 기능 문자를 함께 보내야 1번 동작하고, Space/Enter는 `CHAR`가 필요하며, Cmd+방향키/Backspace는 OSR에서 전달되지 않아 Home/End 등으로 직접 매핑해야 한다(`crates/spike-cef/src/keys.rs`).
- **gpui는 보이고 활성화된 창이 있어야 프레임을 그린다.** 화면 밖·거의 투명·비활성 창에서는 레이아웃/입력 핸들러 갱신이 멈춘다. 그래서 GUI 검증은 창이 뜨고 포커스를 가져가며(실행 중 사용자 입력이 섞이면 결과가 오염됨), 가능한 항목은 **창 없는 헤드리스**(`scripts/suite-headless.sh`)로 옮겼다.
- **헤드리스 테스트 주의**: 읽기 값이 같으면 `document.title` 이 안 바뀌어 타임아웃되므로 호출마다 고유 번호를 붙인다. 블로킹 소켓 읽기 중에는 CEF 메시지 루프가 멈춰 같은 스레드의 DevTools 서버가 응답하지 못한다(별도 스레드에서 요청).
- **번들 서명**: 번들을 덮어쓰면 macOS 서명 캐시 불일치로 `SIGKILL (Code Signature Invalid)`. 번들 삭제 후 재생성 + ad-hoc 재서명(스크립트 반영).
- **윈도우리스 팝업**: 사용자 제스처 없는 `window.open`은 Chromium이 기본 차단한다(핸들러까지 안 옴).

## 실행 방법

```bash
export CEF_PATH=$HOME/.local/share/cef156        # CEF 156 (cef-rs export-cef-dir 로 설치)

scripts/suite-headless.sh all                     # 창/포커스 없음. input | browser | all
scripts/google-headless.sh <google_id> <횟수>      # 구글 로그인 크래시율(창 없음)
scripts/verify.sh --yes [--colorful]              # gpui 창 필요(포커스 가져감). 기본은 흰색 UI, 픽셀 검증은 --colorful

# 결과를 표에 반영
scripts/results.py update --engine cef156 --method auto-headless target/results/cef-headless-all.json
scripts/results.py render
```
