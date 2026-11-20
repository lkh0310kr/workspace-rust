# rte-engine

`crates/radial-tree/reference/index.html`(원본)의 로직을 같은 연산 순서로 옮긴 순수 Rust 엔진. UI 툴킷과 무관하다.

## 원본과의 동일성 검증 (창 없이)

```bash
crates/rte-engine/tools/run_compare.sh 1100x800        # 전 시나리오, 지정 크기
crates/rte-engine/tools/run_compare.sh 600x700 initial  # 특정 시나리오만
```

- 원본 JS 를 CEF 헤드리스에서 **가짜 시계**(`performance.now`, `setTimeout`, `requestAnimationFrame`)로 구동하고, 같은 시나리오를 Rust 엔진에 넣어 **프레임별 상태 덤프**(위치, 불투명도, 모드, 텍스트, 링, 링크 풀 등)를 비교한다.
- 시나리오: `initial`, `navigate`, `dial`(휠·키·드래그·반동), `fuzz1~8`(무작위 입력). 텍스트 폭은 폰트 비의존 스텁을 양쪽에 동일하게 쓴다.
- 원본 파일은 수정하지 않는다. 하니스(`tools/js_prelude.js`)는 서빙 시 `<head>` 에 주입된다.
- 비교 허용오차: 값 상대 1e-9, `style.opacity`(브라우저가 6자리로 직렬화) 절대 2e-6.

이 검증이 **보장하는 것**: 레이아웃/전환/회전·반동 물리/말줄임 결정/입력 핸들러 로직이 원본과 같다.
**보장하지 않는 것**: 실제 폰트 폭, 글리프 렌더링, 색, 안티앨리어싱(그리기 계층은 gpui 에서 별도 검증).
