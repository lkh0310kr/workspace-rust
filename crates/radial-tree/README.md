# radial-tree (gpui 포팅)

원본: `reference/index.html` (수정하지 않음). 로직은 `../rte-engine`(원본 JS 와 수치 동일성 검증됨), 이 크레이트는 gpui 로 **그리기와 입력 연결**만 한다.

```bash
cargo run -p radial-tree                       # 실행 (시스템 다크/라이트 따름)
RTE_THEME=dark RTE_SIZE=1100x800 cargo run -p radial-tree
# 개발용 캡처(창이 잠깐 뜸): RTE_SHOT=out.png RTE_SHOT_MS=2500 RTE_GO=1 cargo run -p radial-tree
```

## GPUI 컴포넌트로 사용

필수 콘텐츠 입력은 tree JSON이다. 컴포넌트 크기는 부모 레이아웃을 따르며, 테마,
시작 경로, 텍스트 후광 품질은 `TreeViewOptions`로 설정한다. GPUI 호스트는 텍스트
시스템과 포커스 핸들을 전달한다.
`RTE_*` 환경변수는 실행용 데모에만 적용된다.

```rust,ignore
use gpui::{prelude::*, App, Application, Bounds, FocusHandle, Window, WindowOptions};
use radial_tree::{register_fonts, RadialTree, TreeViewOptions};

Application::new().run(|cx: &mut App| {
    register_fonts(cx.text_system());
    let tree_data: serde_json::Value = load_tree_data();
    cx.open_window(WindowOptions::default(), move |window: &mut Window, cx| {
        let focus: FocusHandle = cx.focus_handle();
        let tree = RadialTree::new(tree_data, window, focus.clone(), TreeViewOptions::default());
        window.focus(&focus);
        cx.new(|_| tree)
    }).unwrap();
});
```

`RadialTree`는 GPUI `Render` 컴포넌트이므로 기존 GPUI 화면에서도 엔티티로 생성해
배치할 수 있다. 트리 JSON 구조는 `crates/rte-engine/data/data.json`을 참고한다.

## 비교 기준 이미지 (창 없이 생성)

```bash
SPIKE_SCALE=2 crates/radial-tree/tools_shot_orig.sh target/shots/ref/lang-dark.png 1100x800 dark 'go(N[1])'
python3 crates/radial-tree/tools_diff.py target/shots/ref/lang-dark.png target/shots/gpui-lang-dark.png target/shots/cmp-lang-dark
```

## 포팅 범위

| 항목 | 상태 |
|---|---|
| 레이아웃·전환·회전/반동 물리·드래그·휠·키 로직 | rte-engine 으로 원본과 수치 동일(4개 창 크기 × 11개 시나리오) |
| 링, 링크, 노드 원, 다이얼(트랙+엄지 호), 크럼, 힌트, 다크/라이트 | 구현. 시각 비교는 창 실행이 필요(미수행) |
| 텍스트: Montserrat 변수 폰트 번들, letter-spacing .01em, 한글 폴백 | 구현. 폰트 두께·폴백 일치는 시각 확인 필요 |
| SVG `paint-order: stroke` 텍스트 후광 | bg 색 글리프를 원주 24점에 겹쳐 그려 **근사**(정확한 외곽선 스트로크는 gpui 에 없음) |
| 포커스(Tab)·focus-visible·hover·커서 | 구현. 실사용 확인 필요 |
| 그룹 불투명도(`<g opacity>`) | 요소별 알파로 근사 → 전환 중 일부 겹침 영역이 미세하게 다를 수 있음 |
| SVG `<title>` 툴팁, aria/role=tree 접근성 | **미구현**(gpui 에 접근성 트리 없음) |
