# gpui + 브라우저 임베딩 Feasibility 검증

목표: 기존 Workspace(Electron/Chromium)의 **브라우저 패널만** Rust(gpui)로 점진 이식할 수 있는지 검증한다.
알려진 문제: (1) z-index — gpui UI를 웹뷰 위에 못 올림, (2) 로그인이 제대로 안 됨.

비교 대상 두 방식:

| | A. `spike-wry` | B. `spike-cef` |
|---|---|---|
| 엔진 | WKWebView (macOS 시스템 WebKit) | Chromium (CEF 151, Electron과 동일 계열) |
| 합성 방식 | 네이티브 NSView를 gpui 창 위에 child로 부착 | off-screen rendering → 픽셀을 gpui 씬에 직접 그림 |

판정 기준은 두 스파이크가 동일한 로컬 테스트 사이트(`crates/testsite`)를 쓰며,
페이지가 스스로 결과를 `POST /report`로 보고한다(엔진 무관, 사람이 눈으로 판단하지 않음).
z-index 항목은 실제 화면을 `screencapture`로 찍어 픽셀을 검사한다.

결과 표기: ✅ 통과 / ❌ 실패 / ⚠️ 부분·조건부 / ⬜ 미검증. 결과 칸은 실제 실행 후에만 채운다.

## 체크리스트

### Z — 합성 / z-index
| ID | 항목 | 판정 방법 |
|---|---|---|
| Z1 | gpui 오버레이(드롭다운/모달)가 웹뷰 영역 **위에** 그려짐 | 스크린샷 픽셀: 웹뷰 배경(빨강) 위 오버레이 영역이 오버레이 색(파랑)인가 |
| Z2 | 오버레이 영역 클릭이 웹뷰가 아닌 gpui로 전달됨 | 웹뷰 페이지의 click 리스너 미호출 + gpui 클릭 카운터 증가 |
| Z3 | 레이아웃 변경(사이드바 폭 조절)에 웹뷰가 즉시 동기화 | 변경 후 스크린샷에서 웹뷰 좌측 경계 위치 |
| Z4 | 모서리 둥근 클립 / gpui 요소가 웹뷰 일부를 가림 | 스크린샷 픽셀 (모서리 바깥은 배경색) |
| Z5 | 탭 전환(웹뷰 hide/show) 시 깜빡임/잔상 없음 | 전환 직후 스크린샷 |

### L — 로그인 / 세션
| ID | 항목 | 판정 방법 |
|---|---|---|
| L1 | 폼 로그인 → Set-Cookie 세션 유지 | `/me`가 서버 쪽에서 쿠키 수신 확인 |
| L2 | 영속 쿠키가 앱 재시작 후에도 유지 | 2회차 실행 시 `/me`에서 `psid` 유지, 세션 쿠키 `ssid` 정책 기록 |
| L3 | OAuth 팝업(`window.open` + `postMessage` + `window.close`) | 팝업 → opener로 code 전달 |
| L4 | `target=_blank` 링크 처리 | 새 창 요청 핸들링 |
| L5 | 3rd-party 쿠키(SameSite=None, cross-site iframe) | iframe이 쿠키를 다시 보내는지 |
| L6 | UA / 자동화 탐지 표면 (`navigator.userAgent`, `window.chrome`, `userAgentData`) | Google 등 "안전하지 않은 브라우저" 차단 회피 가능성 |
| L7 | 실제 `accounts.google.com` 진입 시 차단 문구 여부 | 페이지 텍스트 (자격증명 입력은 안 함) |

### I — 입력
| ID | 항목 | 판정 방법 |
|---|---|---|
| I1 | 영문 키 입력, Cmd+A/C/V | 페이지의 input value |
| I2 | 한글 IME 조합 (compositionstart/update/end) | 페이지의 composition 이벤트 로그 |
| I3 | 마우스 클릭/스크롤/hover | 페이지 이벤트 카운터 |
| I4 | gpui 단축키 ↔ 웹뷰 포커스 간 라우팅 | 포커스 상태별 단축키 수신 위치 |

### P — 성능 / 운영
| ID | 항목 | 판정 방법 |
|---|---|---|
| P1 | 리사이즈 중 프레임 | 측정값 기록 |
| P2 | 프레임 업로드 비용 (B) | ms/frame |
| P3 | 번들 크기 / 배포 복잡도 | 실측 |
| P4 | 다중 웹뷰(탭) | 동시 N개 |

## 결과 기록

실행일 2026-11-20, macOS 26.6 / arm64. 전부 `scripts/verify.sh`로 재현 가능(자동 하니스, 스크린샷 픽셀 + 페이지 자체 보고).
Z1/Z2는 오버레이가 실제로 그려졌는지 대조군(사이드바 쪽 파랑 픽셀)을 함께 확인했다.

| ID | A. wry (WKWebView) | B. CEF OSR (Chromium 151) | 비고 |
|---|---|---|---|
| Z1 오버레이가 웹뷰 위에 그려짐 | ❌ 웹뷰가 위 (오버레이 중심 픽셀 빨강) | ✅ 파랑 | A는 네이티브 NSView가 gpui Metal 레이어 위에 있음 |
| Z2 오버레이 클릭 라우팅 | ❌ gpui 0회, 웹뷰가 클릭 수신 | ✅ gpui 1회, 웹뷰 미수신 | 합성 NSEvent → `NSWindow.sendEvent` 경로 |
| Z3 레이아웃 동기화 | ✅ | ✅ | 사이드바 폭 200→400 후 좌측 경계 408 |
| Z4 둥근 모서리 클립 | ⚠️ 기본은 직각. `CALayer.cornerRadius` 우회로 ✅ | ✅ `paint_image` corner_radii | |
| Z5 hide/show | ✅ | ✅ | 깜빡임은 측정 안 함 |
| L1 폼 로그인 쿠키 | ✅ | ✅ | |
| L2 재시작 후 영속 쿠키 | ✅ (`data_store_identifier` 지정 시) | ✅ **단, 종료 전 `flush_store` 필요** | flush 없이 종료하면 CEF는 쿠키 유실(실측) |
| L3 OAuth 팝업 postMessage/close | ❌ 팝업이 열리지 않음 (`Allow`만으로는 불가, 별도 구현 필요) | ✅ 팝업을 별도 windowless 브라우저로 받아 gpui에 그림, opener 유지 | |
| L4 `target=_blank` | ✅ | ✅ | |
| L5 3rd-party 쿠키 | ❌ BLOCKED | ✅ SENT | WebKit ITP / Secure 쿠키 정책 |
| L6 UA/Chrome 표면 | ❌ Safari UA, `window.chrome` 없음 | ✅ Chrome/151, `chrome`·`userAgentData` 존재 | |
| L7 Google 로그인 페이지 | ✅ 페이지 로드(자격증명 미입력) | ✅ 페이지 로드(스크린샷 확인) | 계정 입력 후 "안전하지 않은 브라우저" 차단 여부는 **미검증** |
| I1 영문 입력 | ✅ | ✅ | |
| I2 한글 IME 조합 이벤트 | ✅ | ✅ | **실제 한글 키보드가 아니라** `NSTextInputClient`(`setMarkedText`/`insertText`)를 직접 호출해 모사 |
| I3 마우스 클릭 | — | ✅ | |
| P2 프레임 비용 (B) | — | on_paint 복사 평균 0.9ms, RenderImage+paint_image 1.2~1.7ms (CPU 경로, GPU 업로드 제외) | 984x744 논리 / 2x |
| P3 번들 크기 | 바이너리만 | 번들 370MB (CEF 프레임워크 343MB) | Electron과 동급 |
| P4 다중 브라우저 | — | ✅ 메인 + 팝업 동시 | |

### 결론

- **A(wry)는 z-index 요구를 구조적으로 만족하지 못한다.** 네이티브 뷰는 gpui 위에 항상 얹힌다. 로그인 쪽도 WebKit 한계(3rd-party 쿠키, 팝업 처리, Safari UA)가 겹친다. 우회(오버레이를 별도 창으로 분리, 스냅샷 교체 등)는 가능하나 모든 오버레이마다 필요하고 이번에는 검증하지 않았다.
- **B(CEF OSR)는 두 문제를 모두 해소했다.** 웹 콘텐츠가 gpui 씬의 이미지이므로 z-order/클립/히트테스트가 gpui 규칙을 따르고, 엔진이 기존 Electron과 같은 Chromium이라 로그인 관련 항목이 통과했다.
- 점진 이식 관점에서는 **B를 권장**한다. 단, 아래 미해결 항목이 제품화 전 필수 작업이다.

### 추가 검증 (CEF OSR 미검증 항목 순차 진행)

| ID | 항목 | 결과 | 비고 |
|---|---|---|---|
| U1 | 마우스 휠 스크롤 | ✅ | 합성 CGEvent 스크롤: 6×(-60px) → scrollY=360, 3×(+60px) → 180 |
| U2 | 일반 문자 JS `keydown` + 텍스트 | ✅ | `keydown:h`, 값 "hi". IME commit이 만드는 `keydown:Unidentified`가 문자당 1회 추가로 발생(실제 IME 입력과 동일) |
| U2 | Cmd+A/C/V | ✅ | OSR엔 메뉴가 없어 `Frame.select_all/copy/paste`로 직접 전달해야 동작 |
| I4 | gpui 단축키(Cmd+K) vs 웹뷰 포커스 | ✅ | gpui 액션만 실행, 페이지는 keydown을 못 받음 |
| U3 | `<select>` popup widget | ✅ | `OnPopupShow/Size` + `PET_POPUP` 별도 레이어로 그리면 렌더/선택 모두 동작 |
| U3b | 배경 미지정 페이지 | ✅(수정 후) | **OSR 기본 배경은 투명** → gpui 뒷면이 비침. `BrowserSettings.background_color=0xFFFFFFFF` 필수 |

| ID | 항목 | 결과 | 비고 |
|---|---|---|---|
| U4 | 커서 모양 | ✅ | 링크 위 hand, input 위 ibeam. `on_cursor_change` → gpui `.cursor()` |
| U4 | 우클릭 컨텍스트 메뉴 | ✅ | CEF 기본 메뉴를 비우고 gpui가 메뉴를 그림(오버레이이므로 z-order 문제 없음). "Select all" 클릭 시 선택·메뉴 닫힘 |
| U4 | 드래그 텍스트 선택 | ✅ | |
| U6 | 다운로드 | ✅ | `DownloadHandler`로 앱이 경로 지정, 내용 일치 |
| U6 | 파일 선택(`<input type=file>`) | ✅ | 테스트에서는 지정 경로 반환. **실제 앱은 네이티브 열기 대화상자를 연결해야 함** |
| P1 | 애니메이션 프레임레이트 | ✅ 60fps | rAF 182회/3s, CEF on_paint 60.2/s, gpui paint 60.2/s (CPU 경로, 984x744 논리·2x) |
| P1 | 리사이즈 지연 | 측정 | 사이드바 토글 → 올바른 크기의 프레임 도착까지 평균 32ms, 최대 67ms (10회). `was_resized` 뒤 `invalidate` 호출 필요(없으면 일부 토글에서 새 크기 프레임이 오지 않음) |
| — | 권한 프롬프트(카메라/위치 등) | 미구현 | — |

하니스 정리: 합성 우클릭은 `NSEvent.buttonNumber`가 0이라 gpui가 좌클릭으로 해석하므로 CGEvent 경로로 만들어야 한다(제품 문제가 아니라 테스트 방식의 한계). 최종 전체 실행: 34 PASS / 0 FAIL.

검증 중 발견한 환경 제약:

- **gpui는 실제로 보이고 활성화된 창이 있어야 프레임을 그린다.** 화면 밖 / 거의 투명(alpha 0.03~0.3) / 비활성(Accessory) 창에서는 캡처뿐 아니라 레이아웃·입력 핸들러 갱신까지 멈춘다(App Nap 해제, `isKeyWindow` 오버라이드로도 해결 안 됨). 따라서 GUI 자동 검증은 창이 뜨고 포커스를 가져가며, 실행 중 사용자가 다른 앱을 조작하면 간헐 실패한다(측정: 고정 sleep → 조건 대기로 바꾼 뒤 연속 4회 25/25 통과).
- 페이지 전환/레이아웃 변경 직후에는 이전 크기의 프레임이 잠깐 보이는 구간이 있다(OSR은 변화 시에만 프레임을 줌).

### Google 로그인 크래시 — 원인 확정: CEF 151.3.x 버그, CEF 156으로 해결

증상: `google` → 아이디 → Next 시 앱이 즉시 종료(`SIGSEGV`, 브라우저 프로세스 UI 스레드의 CEF 내부 널 포인터 역참조, 주소 `0x10`, 모든 리포트에서 CEF 오프셋 동일).

**결론**: CEF 151(바이너리 151.3.18, 크레이트 151.8.1)의 버그이며 **CEF 156.0.31 + `cef` 크레이트 156.7.0에서는 재현되지 않는다.**

| 조건 (창 없는 헤드리스 재현, 아이디 입력 검증됨) | 크래시 / 생존 |
|---|---|
| CEF 151 기본 | **10 / 0** |
| CEF 151 + WebAuthn API 제거(렌더러에서 `navigator.credentials` 삭제) | **10 / 0** |
| CEF 151 + iCloud 키체인/passkey 관련 feature 비활성, `--disable-blink-features=WebAuth`, 프로필 prefs(비밀번호 관리자·자동완성) 비활성 시도 | 크래시(각 1~2회, 결정적 재현이라 효과 없음으로 판단) |
| **CEF 156.0.31 기본** | **0 / 13** (모두 "Complete sign-in using your passkey" 화면 도달 확인) |

크래시 직전 시퀀스(151): 구글 페이지가 조건부(passkey 자동완성) WebAuthn 요청을 보냄(`rpId: google.com`, `allowCredentials: []`, `timeout: 600000`, PRF 확장) → Chrome FIDO 요청 시작 → Touch ID/Bluetooth 인증기 불가 로그 → USB(HID) 장치 열거 → 크래시. 대상 계정에는 passkey가 있어 Next 직후 구글이 이 흐름을 자동 시작한다.

원인 분리 과정에서 배제/정정한 것:
- 로컬 페이지의 모달·조건부 WebAuthn 호출만으로는 151에서도 크래시하지 않는다(구글의 특정 요청 조합에서만 발생).
- 윈도우리스(OSR) 브라우저는 요청한 런타임 스타일과 무관하게 **항상 Alloy**로 생성된다(`chrome` 요청해도 Alloy). 따라서 Alloy/Chrome 비교 실험은 의미가 없었다.
- GUI 실행 중 섞여 보이던 "확률적" 생존은 입력 실패/사용자 입력 간섭/번들 서명 불일치가 섞인 측정 오류였다. 창 없는 환경에서는 결정적(10/10)이다.

**제약/남은 것**:
- CEF 156에서도 이 앱은 Touch ID(keychain 엔타이틀먼트)와 Bluetooth(Info.plist 권한 문구)를 쓸 수 없어 **passkey 인증 자체는 완료되지 않는다**("Try another way"로 비밀번호/다른 방법 필요). 코드서명 + 엔타이틀먼트 설계는 별도 과제.
- 비밀번호/2단계 인증을 통한 로그인 완주, 로그인 후 세션 유지는 아직 미검증.
- 151.3.24(크레이트와 정확히 일치하는 바이너리)에서의 동작은 따로 확인하지 않음(151 계열 전체가 영향인지는 미확인).
- CEF 156으로 올린 뒤 전체 GUI 회귀(`scripts/verify.sh --yes --colorful`)는 아직 실행하지 않음.

재현/측정 도구: `scripts/google-headless.sh <id> <횟수>`(창·포커스 없음, 프레임은 `target/shots/headless-*.png`로 저장).

### 미검증 / 알려진 공백 (통과로 주장하지 않음)

- 드래그&드롭(페이지 간/파일 드롭), 권한 프롬프트, 실제 네이티브 파일 대화상자 연동 — 미구현.
- 일반 문자는 텍스트 삽입을 IME commit 경로로, keydown/keyup만 별도 전달(keypress 없음). 실제 키보드 한글 입력은 사람이 확인 필요.
- 프레임 전달은 CPU 복사 경로. 영상 재생, 고해상도/고주사율 환경의 스크롤은 미측정(리사이즈 지연과 애니메이션 60fps는 측정함). IOSurface 공유 텍스처 경로는 gpui의 `paint_surface`가 YCbCr 전용이라 gpui 수정이 필요할 것으로 보이나 시도하지 않았다.
- 샌드박스 비활성(`no_sandbox`), 코드서명/공증, 번들 구조의 배포 파이프라인 미검증. gpui `GPUIApplication`에 런타임으로 `CefAppProtocol`을 주입(`macapp.rs`) — 동작은 확인했으나 sendEvent 중첩 상태 추적은 단순화됨.
- Google 실제 계정 로그인(자격증명 입력 이후 흐름), 패스키/WebAuthn, 확장 프로그램은 미검증.
- 검증은 macOS arm64 단일 환경.
