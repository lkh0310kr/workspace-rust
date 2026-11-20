# workspace-rust

Feasibility proof for gradually porting Workspace browser UI.

gpui(Rust) 앱에 브라우저를 임베딩할 때 두 가지 알려진 문제 — **z-index(gpui UI를 웹뷰 위에 못 올림)** 와 **로그인 불안정** — 이 실제로 해결되는지를 두 방식으로 비교 검증한다.

| | 방식 | 결과 요약 |
|---|---|---|
| `crates/spike-wry` | gpui + wry(WKWebView) child NSView | z-index ❌, 팝업/3rd-party 쿠키/UA ❌ |
| `crates/spike-cef` | gpui + CEF(Chromium) off-screen rendering | z-index ✅, 로그인 관련 항목 ✅ |

- **검증 결과 단일 표(결론·TODO 포함, 이것만 보면 됨)**: [docs/VERIFICATION.md](docs/VERIFICATION.md)
- 웹 ↔ Rust 통신 방식 비교(실측): [docs/ipc-comparison.md](docs/ipc-comparison.md)
- 보류 항목: [docs/BACKLOG.md](docs/BACKLOG.md)
- 테스트 사이트(두 스파이크 공용): `crates/testsite`
- 재현: `scripts/suite-headless.sh all`(창 없음), `scripts/verify.sh --yes`(gpui 창, 포커스 가져감) — 자세한 방법은 VERIFICATION.md. CEF 156 바이너리(`CEF_PATH`, cef-rs `export-cef-dir`로 설치)와 `bundle-cef-app`(cef-rs) 필요.

PoC 코드이며 제품 품질이 아니다.
