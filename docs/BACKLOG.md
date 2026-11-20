# 백로그 (개인 사용 전제, 지금 당장 하지 않음)

이 프로젝트는 당분간 본인만 사용하는 PoC/내부 도구다. 아래는 배포·다인 사용 단계로 넘어갈 때 필요한 항목이다.

## 보안/배포
- [ ] **샌드박스 켜기**: 현재 `no_sandbox`. 헬퍼 앱 구조, `cef::sandbox`(macOS) 초기화 검증.
- [ ] **코드서명 + 공증(notarization)**: Developer ID 서명, 헬퍼 앱 서명, hardened runtime, 엔타이틀먼트 최소화. (지금은 ad-hoc 서명. 번들을 덮어쓰면 서명 캐시 불일치로 `SIGKILL (Code Signature Invalid)` 발생 → 번들 삭제 후 재생성 + 서명 필수.)
- [ ] **번들 크기·구성**: 약 370MB(CEF 프레임워크 343MB). 로케일/리소스 축소, 업데이트 전달 방식.
- [ ] **자동 업데이트**: CEF 버전 추적(보안 패치 주기), 앱 업데이트 채널.
- [ ] **CEF 버전 고정 전략**: 151에서 크래시가 났던 사례(`docs/VERIFICATION.md`)처럼 마이너 버전 간 회귀 가능 → 업그레이드 시 `scripts/google-headless.sh` 같은 회귀 하니스를 CI로.

## passkey / 키체인 (선택)
- [ ] Touch ID passkey: keychain-access-groups 엔타이틀먼트 + 프로비저닝/서명 필요, Bluetooth(hybrid) 사용을 위한 Info.plist 권한 문구(`NSBluetoothAlwaysUsageDescription`).
- [ ] 현재는 passkey 불가 → "Try another way"로 비밀번호 로그인(사용자 확인 완료).
- [ ] macOS 키체인(저장된 비밀번호/자동완성) 연동 여부 결정(`use-mock-keychain` 제거 시 키체인 접근 프롬프트).

## 미디어
- [ ] **H.264/AAC 재생**: CEF 기본 빌드는 상용 코덱 미포함(`canPlayType` no). 필요하면 `proprietary_codecs=true ffmpeg_branding=Chrome` 로 CEF를 직접 빌드(수 시간·수십 GB)하거나 코덱 포함 배포본을 사용. 라이선스 검토 필요.

## 플랫폼
- [ ] **Windows/Linux 지원 여부 결정**: 현재 macOS arm64만 검증. OSR 가속 경로(IOSurface/DXGI/dmabuf), IME, 번들 구조가 플랫폼별로 다름.
- [ ] Intel Mac(x86_64) 빌드.

## 성능
- [ ] **IOSurface 공유 텍스처 경로**: gpui `paint_surface`가 YCbCr 전용이라 gpui 포크/패치 필요. CPU 복사 비용(프레임당 약 2~3ms) 대비 이득 측정 후 결정.
- [ ] 다중 탭에서의 프레임 전달 스로틀링, 숨김 탭 렌더 중지.

## 기능 확장
- [ ] **Chrome 확장 프로그램**: Alloy 스타일은 확장 미지원. 필요한 확장이 있는지 조사.
- [ ] 북마크/히스토리/자동완성/비밀번호 관리자 UI(필요 시).
- [ ] 다운로드 관리자 UI, 인쇄 미리보기, PDF 뷰어 통합.
- [ ] 접근성(VoiceOver) 지원 수준 결정.

## 정리(기술 부채)
- [ ] 하니스 중복 정리: `spike-wry`와 `spike-cef`의 `native.rs`/`autotest.rs` 공통화.
- [ ] 개발용 하니스 전용 코드(`isKeyWindow` 오버라이드, stealth 모드)와 제품 코드 분리/삭제.
- [ ] `ref/cef-rs-src` 참조 클론과 CEF 151 바이너리(`~/.local/share/cef`) 정리.
- [ ] `spike-wry`(WKWebView) 보존 여부 결정(비교 근거로만 유지).
