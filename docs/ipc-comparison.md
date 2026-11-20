# 웹 ↔ Rust 통신 방식 비교 (gpui + CEF OSR)

목적: Workspace 브라우저 패널을 Rust(gpui+CEF)로 이식할 때, **웹 페이지(렌더러)와 Rust(브라우저 프로세스)가 어떻게 통신할지** 선택한다.
기존 Electron의 `preload` + `contextBridge` + `ipcRenderer/ipcMain` + `webContents.send` + `protocol.handle`에 해당하는 대체물이 필요하다.

구현·측정 코드: `crates/spike-cef/src/ipc_browser.rs`(브라우저 쪽), `src/bin/helper.rs`(렌더러 쪽), 측정 `SPIKE_IPCBENCH=1 SPIKE_HEADLESS=1`(창 없음), 벤치 페이지 `testsite /ipcbench`.

## 후보

| # | 방식 | 요약 |
|---|---|---|
| A | **프로세스 메시지 + V8 브리지** | 렌더러에서 V8 함수(`__ws_send`)를 노출 → `ProcessMessage`로 브라우저 프로세스(Rust)에 전달 → 응답을 `ProcessMessage`로 되돌려 JS 콜백 호출. `window.workspace.invoke()` 형태. Electron의 `contextBridge`+`ipcRenderer.invoke`에 가장 가까움 |
| B | **커스텀 스킴 핸들러** | `workspace://…`를 Rust `ResourceHandler`가 처리. 페이지는 `fetch`/`XHR`/`<img src>`로 호출. Electron `protocol.handle`에 해당 |
| C | **로컬 HTTP / WebSocket 서버** | Rust가 `127.0.0.1:PORT`로 서버를 띄우고 페이지가 `fetch`/WS 사용 |
| D | **Rust → 페이지 푸시 (`execute_java_script`)** | 단방향(Rust→JS). 이벤트 전달용 |
| E | `cefQuery`(MessageRouter) | CEF C++ 래퍼의 표준 방식. **cef-rs 156.7에는 없음**(C API가 아니라 C++ 래퍼 소속). 쓰려면 A와 같은 구조를 직접 만들어야 함 |
| F | 페이지 제목/콘솔 메시지 가로채기 | 테스트 하니스에서만 사용(`document.title` 읽기). 제품 사용 부적합 |

## 실측 (이 PC, macOS arm64, CEF 156, 창 없는 헤드리스, 루프백)

페이지에서 **순차 `await` 왕복**(요청 → Rust가 N바이트 문자열 응답 → 페이지가 수신). 단위 ms. `n`: 64B 500회 / 1MB 40회(웜업 2회 제외).

| 방식 | 64B p50 / p99 / 평균 | 1MB p50 / p99 / 평균 |
|---|---|---|
| **A 프로세스 메시지** | 0.0* / 4.7 / **0.79** | 6.1 / 39.8 / **7.0** |
| **B 커스텀 스킴** | 0.2 / 0.5 / **0.21** | 1.6 / 2.7 / **1.55** |
| **C 로컬 HTTP** | 0.6 / 3.3 / **0.71** | 1.5 / 3.4 / **1.55** |

\* `performance.now()` 해상도 한계로 p50이 0으로 찍힘 → **평균을 기준**으로 볼 것.

| 항목 | 결과 |
|---|---|
| **D Rust → 페이지 푸시** | `execute_java_script` 2000회: 큐잉 2.7ms, 전부 반영까지 115ms (호출당 약 58µs) |
| **오리진 게이팅(A)** | 신뢰 오리진(`127.0.0.1`)에서는 `window.workspace` 존재, 비신뢰 오리진(`localhost`)에서는 `undefined` — 렌더러 쪽 주입 게이팅이 동작함 |
| **B/C의 노출 범위** | 비신뢰 오리진에서도 **스킴 fetch와 HTTP fetch가 그대로 성공**(핸들러가 오리진을 검사하지 않기 때문). → 별도 방어 필요 |

측정 한계: Rust 빌드가 **debug 프로파일**(실제 release는 더 빠를 수 있음), 순차 요청만 측정(동시성/처리량 미측정), 요청 본문 업로드 방향과 공유 메모리(`SharedMemoryRegion`) 미측정, 페이로드는 ASCII 문자열(바이너리/ArrayBuffer 미측정).

## 비교표

| 기준 | A 프로세스 메시지 | B 커스텀 스킴 | C 로컬 HTTP/WS | D `execute_java_script` |
|---|---|---|---|---|
| 작은 메시지 지연(평균) | 0.8ms | **0.2ms** | 0.7ms | (단방향) 약 0.06ms/호출 |
| 1MB 응답(평균) | 7.0ms (꼬리 40ms) | **1.6ms** | **1.6ms** | 부적합(문자열 삽입) |
| 방향 | 양방향 RPC + 이벤트 | 요청/응답(+스트리밍) | 양방향(WS) | Rust→JS만 |
| 대용량·바이너리 | 보통(문자열 복사, 공유 메모리 API 있음) | **좋음**(스트리밍, `<img src>`/`<video src>` 직접 사용) | 좋음 | 나쁨 |
| 스트리밍/부분 응답 | 직접 구현 | **가능**(Read 콜백) | 가능(WS/SSE) | 불가 |
| **오리진 통제** | **강함**: 렌더러 주입 게이팅 + 브라우저에서 프레임 URL 검증 가능 | 중간: 핸들러에서 `Origin/Referer` 검사 필요, 기본은 모든 페이지에 열림 | **약함**: 같은 PC의 다른 프로세스/탭/악성 페이지가 접근 가능(토큰·Host·Origin 검사 필수) | 해당 없음(Rust가 주도) |
| 서드파티 페이지(로그인 등)에서의 위험 | 브리지를 주입하지 않으면 노출 0 | 스킴 요청은 가능하므로 응답 정책 필요 | 포트 스캔/CSRF 위험 | 문자열 인젝션 주의 |
| Rust 쪽 구현량 | 중간(렌더러 헬퍼 + 브라우저 핸들러 둘 다) | 중간(핸들러만, 헬퍼는 스킴 등록만) | **낮음**(익숙한 서버) | **낮음** |
| 헬퍼(렌더러) 코드 필요 | **필요** | 스킴 등록만 | 불필요 | 불필요 |
| cef-rs 지원 | 있음(V8, ProcessMessage) | 있음(Scheme/Resource) | n/a | 있음 |
| Electron 대응 | `contextBridge`+`ipcRenderer.invoke`, `webContents.send` | `protocol.handle` | `app.listen`류 | `webContents.executeJavaScript` |
| 디버깅 | 로그 필요(DevTools 네트워크 탭에 안 보임) | **DevTools 네트워크 탭에 보임** | 보임 | 콘솔 |
| 플랫폼 이식 | 동일 코드(CEF 공통) | 동일 | 동일 | 동일 |
| 백프레셔/취소 | 직접 구현 | 요청 취소 지원(`cancel`) | 프로토콜 의존 | 없음 |

## 권고: 용도별 혼합

1. **제어/RPC·이벤트(앱이 소유한 신뢰 페이지)** → **A 프로세스 메시지 브리지**.
   - Electron의 `contextBridge`와 같은 모델이라 기존 코드 이식 매핑이 가장 자연스럽다.
   - 오리진 통제가 가장 강하다. 단, **렌더러 쪽 게이팅만 믿지 말고**, 브라우저 프로세스 `on_process_message_received`에서도 `frame.url()`/오리진을 검증해야 한다(침해된 렌더러가 직접 메시지를 보낼 수 있음).
2. **대용량·바이너리·스트리밍·리소스 로딩(파일, 이미지, 큰 JSON)** → **B 커스텀 스킴**.
   - 지연이 가장 낮고 URL 한 줄로 `<img>`/`<video>`/`fetch`에서 모두 사용 가능. 핸들러에서 오리진/토큰 검사를 반드시 넣는다(현재 구현은 검사 없음).
3. **Rust → 페이지 이벤트** → 구조화된 이벤트는 **A의 역방향**(브라우저→렌더러 `ProcessMessage` → `CustomEvent` 디스패치), 가벼운 상태 갱신은 **D**.
4. **C 로컬 HTTP/WS는 지양.** 지연상 이점이 없고(B와 동일 수준) 같은 PC의 다른 프로세스·페이지에 열리는 공격면이 가장 크다. 꼭 필요하면(예: 외부 도구 연동) 랜덤 포트 + 세션 토큰 + `Host`/`Origin` 검증을 필수로 한다.
5. E(`cefQuery`)는 cef-rs에 없으므로 A로 대체한다.

## 이식 시 매핑 초안

| Electron | 대체 |
|---|---|
| `ipcRenderer.invoke(ch, args)` / `ipcMain.handle` | `window.workspace.invoke(ch, args)` → Rust 핸들러(채널 라우팅 테이블) |
| `ipcRenderer.send` / `on` | 동일 브리지의 fire-and-forget + 역방향 이벤트 |
| `webContents.send(ch, data)` | 브라우저→렌더러 `ProcessMessage` → `window.dispatchEvent(new CustomEvent(...))` |
| `contextBridge.exposeInMainWorld` | 렌더러 `on_context_created`에서 V8 함수 노출(신뢰 오리진만) |
| `protocol.handle('app', …)` | `register_scheme_handler_factory` |
| preload 스크립트 | 렌더러 헬퍼의 `on_context_created`에서 JS 주입 |
| 세션/파티션 | `RequestContext` 별도 생성(별도 캐시 경로) — 별도 검증 필요(TODO) |

## 다음 단계(미완)

- 업로드 방향(요청 본문 큰 경우), `SharedMemoryRegion` 기반 대용량 전달, ArrayBuffer 직접 전달 측정.
- 동시 요청/처리량, release 빌드 재측정, 메시지 직렬화 형식(JSON vs 바이너리) 비교.
- 브라우저 프로세스의 프레임/오리진 검증 구현과 우회 시도 테스트(보안).
- 채널 라우팅/권한 모델(채널별 허용 오리진) 설계.
