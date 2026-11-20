# UI·렌더링 기반 선택 검토 (gpui / egui / iced / Bevy / Xilem)

작성: 2026-11-20. 비전: Figma·Blender·CAD·시뮬레이션처럼 2D/3D 캔버스가 중심인 공간형 워크스페이스. 개발자 1인, 주 타깃 Windows(내장 GPU), Electron/TypeScript 탈피, 한국어 사용자.
**근거 수준**: 로컬 소스 확인(gpui 0.2.2), 공개 문서·커밋·README 확인, 나머지는 일반 지식. 성능·IME 실동작은 **이 문서에서 직접 측정하지 않았다**(아래 "미검증").

## 확인된 사실

| 사실 | 출처 |
|---|---|
| crates.io 의 gpui 0.2.2 는 wgpu 를 쓰지 않는다. 소스에 wgpu 언급이 없고 플랫폼별 백엔드(`mac`, `windows`, `linux`+`blade`)만 있다. 그리는 단위는 Shadow/Quad/Path/Underline/MonochromeSprite/PolychromeSprite/Surface 뿐이다 | 로컬 소스(`gpui-0.2.2/src/scene.rs`, `Cargo.toml`) |
| 업스트림 Zed 는 2026-02-13 에 **Linux 렌더러만** blade → wgpu 로 교체. macOS 는 Metal 유지, Windows 는 별도 변경으로 DirectX 11 | [Zed 커밋 #46758](https://git.secluded.site/zed/commit/af8ea0d6c26192c45f44f473c0d4a7d6f72ed018), [Windows DX11 #34374](https://git.secluded.site/zed/commit/15ad9863296d427966098f6b9864d5b819725101) |
| gpui 에 wgpu 뷰포트를 넣는 **커뮤니티 시도**가 있다: `gpui_interop`(외부에서 만든 픽셀을 복사 없이 장면에 합성, 의존성이 `bite-gpui` 계열 포크와 wgpu 30), WGPUI(gpui-ce 포크, 렌더링·윈도잉을 wgpu+winit 단일 백엔드로 교체, `WgpuSurface` 로 wgpu 렌더 패스를 UI 트리에 삽입) | [gpui_interop 0.2.0](https://docs.rs/gpui_interop/0.2.0), [WGPUI README](https://raw.githubusercontent.com/Far-Beyond-Pulsar/WGPUI/main/README.md) |
| **egui**: 커스텀 wgpu 렌더링을 UI 안에 넣는 공식 예제(`custom3d_wgpu`), AccessKit 접근성 기본 지원(네이티브 플랫폼) | [egui custom3d_wgpu](https://github-redirect.dependabot.com/emilk/egui/blob/041f2e64bac778c9095fbf4316dc1f7c7bceb670/crates/egui_demo_app/src/apps/custom3d_wgpu.rs), [egui accessibility](https://raw.githubusercontent.com/emilk/egui/main/docs/accessibility.md) |
| **실사례**: SolidCraft(파라메트릭 CAD, `egui 0.37`/`eframe`/`egui-wgpu`, `kernel`·`viewport-gpu`·`viewport-egui` 크레이트 분리, 데스크톱+웹), Rerun(egui+wgpu 3D 뷰어, 일반 지식) | [SolidCraft Cargo.toml](https://raw.githubusercontent.com/tangency-labs/solidcraft/main/Cargo.toml), [README](https://raw.githubusercontent.com/tangency-labs/solidcraft/d20e582bd95df2ef9b6c9664fb3f55f8d7976aae/README.md) |
| **iced**: `Shader` 위젯으로 iced 의 wgpu 디바이스에 커스텀 렌더링(3D 뷰포트용으로 논의됨). 도킹 레이아웃은 확인 못 함 | [iced Shader](https://docs.iced.rs/iced/widget/struct.Shader.html) |
| **Bevy**: `EditableText` 에 CJK IME 지원이 0.19 RC 문서에 있음(안정판 여부 미확인). 서드파티 IME 위젯 일부는 Windows 전용 | [Bevy IME 예제](https://bevy.org/examples/ui-user-interface/ime-support/), [bevy_text 0.19.0-rc.2](https://docs.rs/crate/bevy_text/0.19.0-rc.2/source/src/editing.rs) |
| **Xilem/Masonry(Vello)**: 2026 Q1 기준 새 레이아웃·위젯 추가, IME 통합은 진행 중(완료로 표기되지 않음) | [Linebender 2026 Q1](https://linebender.org/blog/tmil-25/) |
| winit 계열(egui, iced, Bevy, Masonry)은 macOS 한국어 IME 관련 버그 수정 PR(#4478)이 있었다는 정보가 있다(미러 출처, **원본 미확인**). gpui 는 winit 이 아니라 자체 플랫폼 레이어를 쓴다 | [winit #4478](https://github.com/rust-windowing/winit/pull/4478) |

## 후보 비교

평가는 근거 수준이 다르므로 ◎○△✗ 로만 표시한다(검증된 것과 추정 혼재, 아래 비고 참고).

| 기준 | gpui(업스트림) | gpui + wgpu(포크/interop) | **egui + wgpu** | iced + wgpu | Bevy | Xilem/Masonry |
|---|---|---|---|---|---|---|
| 3D/커스텀 GPU 뷰포트 임베딩 | ✗ 훅 없음 | △ 가능, 포크·실험적 | ◎ 공식 예제·실사례 | ○ Shader 위젯 | ◎ 엔진 자체 | △ |
| UI를 텍스처로 렌더(3D 공간의 패널) | ✗ 창에 묶임 | △ | ◎ (일반 지식: 임의 렌더 타깃에 그림) | ○ | ○ | △ |
| 에디터급 텍스트·큰 앱 UI | ◎ Zed 가 증명 | ◎ | △ 즉시 모드, 에디터 위젯 빈약 | ○ | △ | △ |
| 한국어 IME 신뢰도 | ○ 자체 플랫폼 레이어(macOS 조합 이벤트는 이번 PoC 로 확인) | △ winit 사용 포크는 winit 이슈 영향 | △ winit 의존(미확인) | △ winit 의존 | △ 최신 RC 에서 개선 중 | △ 진행 중 |
| 접근성 | ✗(미확인) | ✗ | ◎ AccessKit | △ | △ | ○ |
| 성숙도/생태계 | ○ 그러나 API 불안정, crates.io 0.2.2 는 오래됨 | ✗ 소규모 프로젝트 | ◎ 매우 큼 | ○ | ◎ (엔진) | △ 초기 |
| 웹(wasm) 동시 지원 | ✗ | ○ (WGPUI 주장) | ◎ (SolidCraft 가 증명) | ○ | ○ | △ |
| Windows 내장 GPU 성능 | ○ DX11, Zed 가 해당 PC 에서 빠름(사용자 보고) | 미확인 | 미확인 | 미확인 | 미확인 | 미확인 |

## 판단

**"뭐가 제일 나은가"는 무엇을 중심에 두느냐로 갈린다.**

1. **캔버스·3D·공간형 UI 가 중심 → egui + wgpu 를 기반으로 권장.**
   - 뷰포트 임베딩이 1급이고 SolidCraft·Rerun 이 같은 구조로 증명했다. UI 를 임의 렌더 타깃(텍스처)에 그릴 수 있어 "3D 공간 속 패널"에도 맞고, **PoC 에서 정한 CEF 오프스크린(텍스처) 방식이 그대로 합류한다**(cef-rs 공식 예제가 wgpu 로 CEF 공유 텍스처를 가져온다).
   - 약점: 즉시 모드라 UI 가 커지면 비용이 늘고, 코드 에디터·터미널급 텍스트 편집은 직접 만들거나 포기해야 하며, **한국어 IME 가 winit 에 의존**한다.
2. **IDE/에디터형 앱 UI 가 중심 → gpui 가 최선.** 에디터·텍스트·IME·성능에서 Zed 가 증명했다. 그러나 3D 뷰포트는 포크/interop(실험적)에 의존해야 한다.
3. **gpui + wgpu 포크(WGPUI, interop)** 는 두 장점을 합치려는 시도지만, 작은 커뮤니티 프로젝트에 핵심 기반을 거는 위험이 크다. **본 기반으로는 권하지 않고, 관찰 대상**으로 둔다.
4. Bevy 는 시뮬레이션·3D 에는 강하지만 일반 앱 UI·에디터 용도로는 추가 작업이 크다. Xilem 은 아직 이르다.

### 권고 아키텍처 (선택을 되돌릴 수 있게)

SolidCraft 구조를 참고해 **UI 툴킷과 분리된 층**으로 나눈다.

```
도메인 크레이트(순수 Rust: 지오메트리/커널/문서/시뮬레이션)
        ↓
렌더러(wgpu: 씬, 뷰포트, 2D 벡터, 텍스처 합성)   ← CEF 공유 텍스처, 패널 텍스처가 여기로
        ↓
UI 어댑터(egui 기본; 필요하면 gpui/iced 어댑터를 추가)
```

이렇게 하면 UI 툴킷 선택이 틀려도 도메인·렌더러 자산은 남는다.

## 미검증 (결정 전에 반드시 확인)

1. **Windows 내장 GPU(i5 8세대급)에서 각 후보의 프레임·메모리** — 측정하지 않았다. 특히 egui+wgpu 의 DX12/Vulkan 선택과 효과 비용.
2. **egui(winit) 한국어 IME 실동작** — 검색으로는 egui 특정 정보를 찾지 못했다. 직접 입력 테스트 필요.
3. gpui 접근성 지원 여부, WGPUI·`gpui_interop` 의 성숙도·Windows 동작.
4. Slint, Makepad, Dioxus Native 등은 이번에 조사하지 않았다.

## 제안: 결정 스파이크 (1~2일)

같은 Windows 내장 GPU PC 에서 다음 3가지를 egui+wgpu 와 (가능하면) gpui 쪽으로 각각 측정한다.
1. 3D 뷰포트 하나 + 패널 UI, 프레임 시간
2. **한글 입력**(텍스트 칸·에디터)
3. CEF 오프스크린 텍스처를 패널로 표시(우리 PoC 의 헤드리스 경험 재사용)
