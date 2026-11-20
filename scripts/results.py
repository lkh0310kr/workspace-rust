#!/usr/bin/env python3
"""검증 결과 단일 표 관리.

데이터 원본: docs/verification/results.csv  (ID × 엔진 단위 한 행)
산출물:      docs/VERIFICATION.md 의 표 (이 스크립트가 생성)

사용:
  scripts/results.py init                      # CASES 로 CSV 생성(기존 결과는 덮어씀)
  scripts/results.py update --engine cef156 --method auto-headless target/results/cef-headless-all.json
  scripts/results.py render                    # CSV → docs/VERIFICATION.md
엔진 키: wry | cef151 | cef156
commit 은 기본으로 현재 HEAD(+작업트리가 더러우면 '+dirty'). --commit 으로 지정 가능('*' = 커밋 전 작업트리에서 실행).
"""
import csv, json, os, subprocess, sys, datetime, re, argparse

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
CSV = os.path.join(ROOT, "docs/verification/results.csv")
MD = os.path.join(ROOT, "docs/VERIFICATION.md")
TODAY = datetime.date.today().isoformat()

ENGINES = {
    "wry": ("wry (WKWebView)", "macOS 26.6 WebKit"),
    "cef151": ("CEF", "151.3.18"),
    "cef156": ("CEF", "156.0.31"),
}
CATEGORIES = [
    ("COMP", "합성·레이아웃 (z-index, 클립, 리사이즈)"),
    ("AUTH", "로그인·세션"),
    ("STORE", "쿠키·저장소·프로필"),
    ("INPUT", "입력 (키보드, 마우스, IME, 스크롤, 줌, 커서)"),
    ("NAV", "내비게이션 (히스토리, 탭, 팝업, 찾기, 로딩 상태)"),
    ("UI", "브라우저 UI 연동 (메뉴, 다이얼로그, 다운로드, DevTools, 권한)"),
    ("WEB", "웹 플랫폼·미디어·렌더링"),
    ("IPC", "웹 ↔ Rust 통신"),
    ("PERF", "성능"),
    ("STAB", "안정성·프로세스·종료"),
    ("SEC", "보안"),
    ("PKG", "패키징·배포"),
]
FIELDS = ["id", "category", "case", "engine", "engine_version", "result", "method", "detail", "commit", "date", "priority", "legacy_id"]


def fx(result, detail, method, commit, date=TODAY):
    return dict(result=result, detail=detail, method=method, commit=commit, date=date)


# case: category, name, map={engine: legacy harness id}, fixed={engine: fx(...)}, todo=(priority, planned-method)
def C(cat, name, map=None, fixed=None, todo=None):
    return dict(cat=cat, name=name, map=map or {}, fixed=fixed or {}, todo=todo)


CASES = [
    # ================= COMP =================
    C("COMP", "gpui 오버레이(모달/드롭다운)가 웹 콘텐츠 위에 그려짐", {"wry": "Z1.overlay_above_webview", "cef151": "Z1.overlay_above_webview"}),
    C("COMP", "오버레이 영역 클릭이 gpui 로 전달됨(웹뷰가 가로채지 않음)", {"wry": "Z2.overlay_click_goes_to_gpui", "cef151": "Z2.overlay_click_goes_to_gpui"}),
    C("COMP", "레이아웃 변경(사이드바 폭)에 웹뷰 영역이 동기화됨", {"wry": "Z3.layout_sync", "cef151": "Z3.layout_sync"}),
    C("COMP", "둥근 모서리 클립", {"wry": "Z4.rounded_corner_native_layer", "cef151": "Z4.rounded_corner_clip"}),
    C("COMP", "웹뷰 숨김/표시(탭 전환)", {"wry": "Z5.hide_show", "cef151": "Z5.hide_show"}),
    C("COMP", "배경 미지정 페이지가 불투명 흰색으로 그려짐(OSR 기본은 투명)", {"cef151": "U3b.default_background_opaque_white"}),
    C("COMP", "<select> 팝업 위젯 렌더링", {"cef151": "U3.select_popup_rendered"}),
    C("COMP", "<select> 항목 선택 → change 이벤트·팝업 닫힘", {"cef151": "U3.select_choose_option"}),
    C("COMP", "메인 + 팝업 브라우저 동시 렌더(팝업을 gpui 레이어로 그림)", {}, {"cef151": fx("PASS", "OAuth 팝업이 gpui 중앙 레이어로 렌더, opener 유지, 닫힘 처리", "auto-gui", "2b5b01e*", "2026-11-20")}),
    C("COMP", "리사이즈/스플리터 드래그 중 깜빡임·잔상", todo=("P1", "gui")),
    C("COMP", "gpui 오버레이 위 입력 차단, 웹뷰 영역 밖으로 나가는 드롭다운", todo=("P1", "gui")),
    C("COMP", "고DPI/다중 모니터 DPI 변경, 텍스트 선명도", todo=("P1", "gui")),
    C("COMP", "다크모드(prefers-color-scheme) 연동", todo=("P2", "headless")),
    # ================= AUTH =================
    C("AUTH", "폼 로그인 → Set-Cookie 세션 유지", {"wry": "L1.form_login_cookie", "cef151": "L1.form_login_cookie"}),
    C("AUTH", "영속 쿠키가 앱 재시작 후에도 유지", {"wry": "L2.persistent_cookie", "cef151": "L2.persistent_cookie"}),
    C("AUTH", "세션 쿠키 재시작 후 정책(정보)", {"wry": "L2.session_cookie_policy", "cef151": "L2.session_cookie_policy"}),
    C("AUTH", "OAuth 팝업(window.open + postMessage + close)", {"wry": "L3.oauth_popup_postmessage", "cef151": "L3.oauth_popup_postmessage"}),
    C("AUTH", "target=_blank 링크 처리", {"wry": "L4.target_blank", "cef151": "L4.target_blank"}),
    C("AUTH", "3rd-party 쿠키(SameSite=None, cross-site iframe)", {"wry": "L5.third_party_cookie", "cef151": "L5.third_party_cookie"}),
    C("AUTH", "UA·자동화 탐지 표면(Chrome UA, window.chrome, userAgentData)", {"wry": "L6.ua_chrome_surface", "cef151": "L6.ua_chrome_surface"}),
    C("AUTH", "Google 로그인 페이지 로드", {"wry": "L7.google_signin_page"}),
    C("AUTH", "Google 로그인: 아이디 입력 → Next 에서 크래시 없음(passkey 단계 도달)", {}, {
        "cef151": fx("FAIL", "10/10 크래시(창 없는 헤드리스). 브라우저 프로세스 UI 스레드 CEF 내부 널 역참조(SIGSEGV, 주소 0x10, 오프셋 0x19330e0). 구글의 조건부 passkey 요청(rpId google.com, allowCredentials [], timeout 600000) 직후. WebAuthn API 제거·feature 비활성·prefs 비활성으로 해결 안 됨", "auto-headless", "f164c33*"),
        "cef156": fx("PASS", "0/13 크래시, 모두 'Complete sign-in using your passkey' 화면 도달(PNG 확인) → 151 버그, 156에서 해결", "auto-headless", "f164c33*"),
    }),
    C("AUTH", "Google 비밀번호 로그인 완주(passkey 화면에서 'Try another way')", {}, {"cef156": fx("PASS", "사용자가 수동으로 로그인 성공", "manual", "cc43ae1*")}),
    C("AUTH", "앱 재시작 후 로그인 상태 유지(주기적 쿠키 flush)", {}, {"cef156": fx("PASS", "사용자가 수동 확인", "manual", "cc43ae1*")}),
    C("AUTH", "passkey(Touch ID/Bluetooth) 인증 완료", {}, {"cef156": fx("UNSUPPORTED", "keychain-access-groups 엔타이틀먼트·Bluetooth Info.plist 권한 없이는 불가. 비밀번호 폴백으로 충분(요구사항 아님). BACKLOG", "auto-headless", "f164c33*")}),
    C("AUTH", "종료 직전 쿠키 flush 필요(없으면 CEF 영속 쿠키 유실)", {}, {"cef151": fx("INFO", "flush 없이 종료 시 psid 유실을 실측 → 주기 flush/종료 시 flush 필수", "auto-gui", "2b5b01e*", "2026-11-20")}),
    C("AUTH", "Google 외 서드파티 로그인(GitHub/Apple/Microsoft/네이버/카카오), 2단계 인증", todo=("P0", "manual")),
    # ================= STORE =================
    C("STORE", "쿠키: JS 가시성 vs 서버 수신(HttpOnly), 세션/영속", {"cef156": "B1a.set_cookie_js_vs_server_view"}),
    C("STORE", "Rust(cookie manager)에서 쿠키 설정 → 서버가 수신", {"cef156": "B1b.set_cookie_from_rust"}),
    C("STORE", "쿠키 개별 삭제", {"cef156": "B1c.delete_one_cookie"}),
    C("STORE", "쿠키 전체 삭제", {"cef156": "B1d.delete_all_cookies"}),
    C("STORE", "프로필/세션 분리(RequestContext 별 캐시 경로, Electron partition 대응), 시크릿(메모리) 모드", todo=("P0", "headless")),
    C("STORE", "localStorage/IndexedDB 영속, 쿠키 SameSite 정책별 전송", todo=("P1", "headless")),
    # ================= INPUT =================
    C("INPUT", "영문 텍스트 입력", {"wry": "I1.latin_text", "cef151": "I1.latin_text"}),
    C("INPUT", "한글 IME 조합 이벤트(NSTextInputClient 호출 모사)", {"wry": "I2.korean_ime_composition", "cef151": "I2.korean_ime_composition"}),
    C("INPUT", "마우스 클릭", {"cef151": "I3.mouse_click"}),
    C("INPUT", "휠 스크롤(gpui 경로)", {"cef151": "U1.wheel_scroll"}),
    C("INPUT", "일반 문자 JS keydown + 텍스트 삽입", {"cef151": "U2.plain_char_keydown_and_text"}),
    C("INPUT", "Cmd+A / Cmd+C(클립보드)", {"cef151": "U2.cmd_a_cmd_c_clipboard"}),
    C("INPUT", "Cmd+V 붙여넣기", {"cef151": "U2.cmd_v_paste"}),
    C("INPUT", "gpui 단축키가 웹뷰 포커스 중에도 gpui 로 감", {"cef151": "I4.gpui_shortcut_while_web_focused"}),
    C("INPUT", "커서 모양(링크 hand / 입력칸 ibeam)", {"cef151": "U4.cursor_shape"}),
    C("INPUT", "드래그 텍스트 선택", {"cef151": "U4.drag_select"}),
    C("INPUT", "키보드 스크롤(↓/PageDown/End/Home/Space)", {"cef156": "K1.keyboard_scroll"}),
    C("INPUT", "편집 키: 입력/Backspace/Delete/←", {"cef156": "K2a.type_backspace_delete"}),
    C("INPUT", "Shift+Home 선택", {"cef156": "K2b.shift_home_selection"}),
    C("INPUT", "Cmd+← 줄 처음(OSR 미전달 → Home 으로 매핑)", {"cef156": "K2c.cmd_left_line_start"}),
    C("INPUT", "Opt+→ 단어 이동", {"cef156": "K2d.opt_right_word_jump"}),
    C("INPUT", "전체 선택·실행 취소(frame API)", {"cef156": "K2e.select_all_and_undo_via_frame_api"}),
    C("INPUT", "Tab / Shift+Tab 포커스 순서", {"cef156": "K3a.tab_focus_order"}),
    C("INPUT", "Enter 로 폼 제출", {"cef156": "K3b.enter_submits_form"}),
    C("INPUT", "Escape keydown 전달", {"cef156": "K3c.escape_keydown_delivered"}),
    C("INPUT", "더블클릭 단어 선택", {"cef156": "M1a.double_click_selects_word"}),
    C("INPUT", "트리플클릭 문단 선택", {"cef156": "M1b.triple_click_selects_paragraph"}),
    C("INPUT", "Shift+클릭 범위 선택", {"cef156": "M1c.shift_click_extends_selection"}),
    C("INPUT", "줌 레벨 변경 시 레이아웃 반영", {"cef156": "S3.zoom_level_changes_layout"}),
    C("INPUT", "줌 상태에서 클릭 좌표 정확도(앱이 줌 배율로 변환해야 함)", {"cef156": "B6.click_accuracy_when_zoomed"}),
    C("INPUT", "줌 레벨이 같은 오리진에서 유지", {"cef156": "S3b.zoom_persists_per_origin"}),
    C("INPUT", "키 입력 인코딩 규칙(RAWKEYDOWN 단독은 2회 동작 → 기능 문자 필요, Cmd+방향키 미전달)", {}, {"cef156": fx("INFO", "실험(keyexp): RAWKEYDOWN 단독=편집키 2회 동작, +macOS 기능 문자=1회, Space는 +CHAR 필요, Cmd+←/→ 는 어떤 인코딩도 무효 → keys.rs 에 규칙·매핑 반영", "auto-headless", "218b7f5")}),
    C("INPUT", "실제 한글 키보드 입력(자모 조합·영한 전환·후보창 위치), 일본어/중국어 IME", todo=("P0", "manual")),
    C("INPUT", "핀치 줌(트랙패드 magnify → ctrl+wheel 또는 zoom level), 한계값, 중심점", todo=("P0", "gui")),
    C("INPUT", "키보드 줌(Cmd +/−/0), 도메인별 기억 연동", todo=("P0", "headless")),
    C("INPUT", "휠 스크롤 정확도(픽셀/라인, 관성, 가로, 중첩 스크롤), 부드러운 스크롤", todo=("P0", "gui")),
    C("INPUT", "키 반복·수식키 조합·keydown/up 순서", todo=("P1", "headless")),
    C("INPUT", "호버/mouseleave, 패널 밖으로 나갈 때 leave 이벤트", todo=("P1", "headless")),
    C("INPUT", "중간 버튼/Cmd+클릭(새 탭), 마우스 뒤로/앞으로 버튼", todo=("P1", "gui")),
    C("INPUT", "드래그&드롭(페이지 내, 링크/이미지, 외부 파일 드롭)", todo=("P1", "gui")),
    C("INPUT", "웹뷰↔gpui 입력칸 간 포커스 이동, 창 비활성/활성 시 캐럿·:focus", todo=("P0", "gui")),
    C("INPUT", "preventDefault 한 wheel/키가 gpui 단축키와 충돌하지 않음", todo=("P1", "gui")),
    C("INPUT", "이모지/특수문자 입력, 서식 붙여넣기", todo=("P2", "manual")),
    # ================= NAV =================
    C("NAV", "뒤로/앞으로 이동", {"cef156": "B2a.back_forward_navigation"}),
    C("NAV", "can_go_back / can_go_forward 상태 콜백", {"cef156": "B2b.can_go_back_forward_state"}),
    C("NAV", "새로고침", {"cef156": "B2c.reload"}),
    C("NAV", "SPA pushState 주소 변경 + 뒤로", {"cef156": "B2d.spa_pushstate_and_back"}),
    C("NAV", "제목·주소·파비콘 이벤트", {"cef156": "B12a.title_address_favicon"}),
    C("NAV", "로드 실패(연결 거부) 오류 콜백", {"cef156": "B12b.load_error_reported"}),
    C("NAV", "페이지 내 찾기(개수, 다음, 대소문자 구분)", {"cef156": "B5.find_in_page"}),
    C("NAV", "사용자 제스처 없는 window.open 은 Chromium 이 기본 차단", {"cef156": "B4a.popup_without_gesture_blocked_by_default"}),
    C("NAV", "window.open(제스처) features(크기)·opener 유지", {"cef156": "B4b.window_open_with_gesture_features_opener"}),
    C("NAV", "noopener 팝업은 opener 없음", {"cef156": "B4c.noopener_popup_has_no_opener"}),
    C("NAV", "팝업 허용/차단 정책 훅(on_before_popup)", {"cef156": "B4d.popup_policy_hook_exists"}),
    C("NAV", "탭: 다중 브라우저 생성·전환·닫기, 백그라운드 탭 렌더 중지(was_hidden), 탭별 크기/포커스", todo=("P0", "headless")),
    C("NAV", "탭 닫기 시 메모리 해제, 탭 복원", todo=("P1", "headless")),
    C("NAV", "외부 프로토콜(mailto:/tel:) → OS 열기 정책", todo=("P2", "headless")),
    C("NAV", "로딩 진행률(on_loading_progress_change)", todo=("P1", "headless")),
    # ================= UI =================
    C("UI", "우클릭 컨텍스트 메뉴를 gpui 가 그림(기본 메뉴 제거)", {"cef151": "U4.context_menu_requested"}),
    C("UI", "gpui 메뉴 항목('Select all') 동작·메뉴 닫힘", {"cef151": "U4.context_menu_action"}),
    C("UI", "JS alert/confirm/prompt 처리", {"cef156": "B10a.alert_confirm_prompt"}),
    C("UI", "beforeunload 다이얼로그", {"cef156": "B10b.beforeunload_dialog"}),
    C("UI", "다운로드(경로 지정, 완료 콜백, 내용 일치)", {"cef151": "U6.download"}),
    C("UI", "파일 선택 <input type=file>(테스트 고정 경로)", {"cef151": "U6.file_chooser"}),
    C("UI", "원격 디버깅 엔드포인트(/json/version, /json/list, WS URL)", {"cef156": "B7a.remote_debugging_endpoints"}),
    C("UI", "DevTools UI: gpui 패널(두 번째 windowless 브라우저)에 렌더. 참고: 응답의 frontend URL 은 외부 호스팅(chrome-devtools-frontend.appspot.com)", todo=("P0", "headless")),
    C("UI", "컨텍스트 메뉴 완성(링크/이미지/선택 텍스트/입력칸/페이지 항목별 동작)", todo=("P1", "gui")),
    C("UI", "이미지 저장/복사, download_image, 진행률·취소·재시도", todo=("P1", "headless")),
    C("UI", "권한 프롬프트(카메라/마이크/위치/알림/클립보드/화면 공유)", todo=("P1", "gui")),
    C("UI", "HTTP 인증 프롬프트, 인증서 오류 경고/예외, 혼합 콘텐츠", todo=("P1", "headless")),
    C("UI", "클립보드 API(navigator.clipboard), 이미지 붙여넣기", todo=("P1", "headless")),
    C("UI", "전체화면 API(비디오/페이지), Esc 해제와 gpui 레이아웃 연동", todo=("P1", "gui")),
    C("UI", "네이티브 파일 열기/저장 대화상자 연동(현재는 테스트용 고정 경로)", todo=("P1", "gui")),
    C("UI", "인쇄/PDF 저장, 내장 PDF 뷰어", todo=("P2", "headless")),
    C("UI", "접근성(스크린 리더)·키보드만으로 조작", todo=("P2", "manual")),
    # ================= WEB =================
    C("WEB", "미디어: <video>/<audio> 재생, 자동재생 정책, 코덱(H.264/AAC 지원 여부), 음소거, PiP", todo=("P0", "headless")),
    C("WEB", "WebSocket, Service Worker, WebGL/WebGPU, WebRTC, 웹폰트", todo=("P1", "headless")),
    C("WEB", "한글/이모지 폰트 폴백, 텍스트 렌더링 품질", todo=("P1", "gui")),
    C("WEB", "User-Agent/언어/프록시 설정, 다운로드 폴더 정책", todo=("P2", "headless")),
    # ================= IPC =================
    C("IPC", "프로세스 메시지 브리지(window.workspace.invoke) 왕복 지연", {}, {"cef156": fx("INFO", "64B 평균 0.79ms, 1MB 평균 7.0ms(p99 39.8ms). debug 빌드, 순차 요청", "auto-headless", "191dd55*")}),
    C("IPC", "커스텀 스킴(workspace://) 핸들러 왕복 지연", {}, {"cef156": fx("INFO", "64B 평균 0.21ms, 1MB 평균 1.55ms", "auto-headless", "191dd55*")}),
    C("IPC", "로컬 HTTP 왕복 지연", {}, {"cef156": fx("INFO", "64B 평균 0.71ms, 1MB 평균 1.55ms", "auto-headless", "191dd55*")}),
    C("IPC", "Rust → 페이지 푸시(execute_java_script)", {}, {"cef156": fx("INFO", "2000회 큐잉 2.7ms, 전부 반영 115ms(호출당 약 58µs)", "auto-headless", "191dd55*")}),
    C("IPC", "브리지는 신뢰 오리진에만 노출(비신뢰 오리진에서는 undefined)", {}, {"cef156": fx("PASS", "127.0.0.1: window.workspace=object, localhost: undefined", "auto-headless", "191dd55*")}),
    C("IPC", "커스텀 스킴/HTTP 는 비신뢰 오리진에서도 접근 가능(오리진 검사 없음) → 핸들러 검사 필요", {}, {"cef156": fx("FAIL", "의도적으로 검사 없이 구현한 상태에서 비신뢰 오리진 fetch 성공을 확인(위험 확인용)", "auto-headless", "191dd55*")}),
    C("IPC", "업로드 방향/SharedMemoryRegion/ArrayBuffer/동시성, release 빌드 재측정", todo=("P1", "headless")),
    C("IPC", "브라우저 프로세스 쪽 프레임/오리진 검증 및 우회 시도 테스트", todo=("P0", "headless")),
    # ================= PERF =================
    C("PERF", "애니메이션 프레임레이트(rAF)", {"cef151": "P1.animation_fps"}),
    C("PERF", "리사이즈 지연(토글 → 올바른 크기 프레임 도착)", {"cef151": "P1.resize_latency"}),
    C("PERF", "프레임 파이프라인 비용(CPU 복사 + RenderImage)", {"cef151": "P2.frame_pipeline_cost"}),
    C("PERF", "탭 다수(10~30개) 메모리/CPU, 숨긴 탭 프레임 전달 중지", todo=("P0", "headless")),
    C("PERF", "프레임 전달: CPU 복사 vs IOSurface 공유 텍스처, 4K/고주사율", todo=("P1", "gui")),
    C("PERF", "영상 재생·고해상도 스크롤 프레임 안정성", todo=("P1", "gui")),
    # ================= STAB =================
    C("STAB", "종료 후 헬퍼 프로세스가 남지 않음", {}, {"cef156": fx("PASS", "스위트 종료 2초 뒤 spike-cef 프로세스 0개", "auto-headless", "218b7f5")}),
    C("STAB", "렌더러 크래시 복구(on_render_process_terminated → 다시 로드), 행 처리", todo=("P0", "headless")),
    C("STAB", "장시간 구동 메모리 누수, 프레임 버퍼 해제", todo=("P1", "headless")),
    C("STAB", "악성/대용량 페이지(무한 루프, 거대 이미지)에서 UI 응답성", todo=("P1", "headless")),
    C("STAB", "번들 덮어쓰기 시 코드서명 캐시 불일치(SIGKILL Code Signature Invalid) 주의", {}, {"cef156": fx("INFO", "번들 삭제 후 재생성 + ad-hoc 재서명으로 해결(스크립트 반영)", "auto-headless", "beace55")}),
    # ================= SEC =================
    C("SEC", "IPC 브리지 오리진 통제(비신뢰 오리진/iframe 호출 불가)", todo=("P0", "headless")),
    C("SEC", "스킴 핸들러·로컬 서버 오리진/토큰 검사", todo=("P0", "headless")),
    # ================= PKG =================
    C("PKG", "번들 크기", {}, {"cef151": fx("INFO", "370MB(CEF 프레임워크 343MB)", "auto-gui", "2b5b01e*", "2026-11-20"), "cef156": fx("INFO", "381MB", "auto-headless", "218b7f5")}),
    C("PKG", "샌드박스 켜기(현재 no_sandbox)", todo=("P2", "backlog")),
    C("PKG", "코드서명·공증, 엔타이틀먼트(passkey 포함)", todo=("P2", "backlog")),
    C("PKG", "Windows/Linux, Intel Mac", todo=("P2", "backlog")),
]


def git(*a):
    return subprocess.run(["git", "-C", ROOT, *a], capture_output=True, text=True).stdout.strip()


def head_commit():
    h = git("rev-parse", "--short", "HEAD")
    dirty = git("status", "--porcelain", "--", "crates", "scripts")
    return h + ("+dirty" if dirty else "")


def load():
    if not os.path.exists(CSV):
        return []
    with open(CSV, newline="") as f:
        return list(csv.DictReader(f))


def save(rows):
    os.makedirs(os.path.dirname(CSV), exist_ok=True)
    with open(CSV, "w", newline="") as f:
        w = csv.DictWriter(f, fieldnames=FIELDS)
        w.writeheader()
        w.writerows(rows)


def cmd_init(_):
    rows, counters = [], {}
    for c in CASES:
        counters[c["cat"]] = counters.get(c["cat"], 0) + 1
        cid = f'{c["cat"]}-{counters[c["cat"]]:03d}'
        engines = list(dict.fromkeys(list(c["map"].keys()) + list(c["fixed"].keys())))
        if c["todo"]:
            rows.append(dict(id=cid, category=c["cat"], **{"case": c["name"]}, engine="CEF", engine_version="", result="TODO",
                             method="plan:" + c["todo"][1], detail="", commit="", date="", priority=c["todo"][0], legacy_id=""))
            continue
        for e in engines:
            name, ver = ENGINES[e]
            f = c["fixed"].get(e)
            rows.append(dict(id=cid, category=c["cat"], **{"case": c["name"]}, engine=name, engine_version=ver,
                             result=(f or {}).get("result", "TODO"), method=(f or {}).get("method", ""), detail=(f or {}).get("detail", ""),
                             commit=(f or {}).get("commit", ""), date=(f or {}).get("date", ""), priority="", legacy_id=c["map"].get(e, "")))
    save(rows)
    print(f"init: {len(rows)} rows, {len(counters)} categories")


# 결과 상세 앞에 붙일 주의 문구 (판정은 PASS 지만 조건이 붙는 경우)
PREFIX = {("wry", "Z4.rounded_corner_native_layer"): "[우회 필요: 네이티브 CALayer.cornerRadius 를 직접 적용해야 함] "}


def cmd_update(a):
    rows = load()
    key = ENGINES[a.engine]
    commit = a.commit or head_commit()
    n = 0
    for path in a.files:
        data = json.load(open(path))
        for r in data:
            for row in rows:
                if row["legacy_id"] == r["id"] and row["engine"] == key[0] and row["engine_version"] == key[1]:
                    st = r["status"]
                    row.update(result=st, detail=PREFIX.get((a.engine, r["id"]), "") + r["detail"][:600], method=a.method, commit=commit, date=a.date or TODAY)
                    n += 1
    save(rows)
    print(f"update: {n} rows from {len(a.files)} file(s) as {a.engine} commit={commit}")


def esc(s):
    return s.replace("|", "\\|").replace("\n", " ")


def cmd_render(_):
    rows = load()
    cat_name = dict(CATEGORIES)
    order = {c: i for i, (c, _) in enumerate(CATEGORIES)}
    rows.sort(key=lambda r: (order[r["category"]], r["id"], r["engine"], r["engine_version"]))
    icon = {"PASS": "✅ PASS", "FAIL": "❌ FAIL", "INFO": "ℹ️ INFO", "TODO": "⬜ TODO", "UNSUPPORTED": "🚫 UNSUPPORTED", "SKIP": "⏭ SKIP", "PARTIAL": "⚠️ PARTIAL"}
    lines = ["| ID | 분류 | 케이스 | 엔진 | 결과 | 방법 | 상세 | 커밋 | 날짜 | 우선 |", "|---|---|---|---|---|---|---|---|---|---|"]
    for r in rows:
        eng = (r["engine"] + (" " + r["engine_version"] if r["engine_version"] else "")).strip()
        lines.append("| {id} | {cat} | {case} | {eng} | {res} | {m} | {d} | {c} | {dt} | {p} |".format(
            id=r["id"], cat=r["category"], case=esc(r["case"]), eng=esc(eng), res=icon.get(r["result"], r["result"]),
            m=esc(r["method"]), d=esc(r["detail"]), c=r["commit"] and f'`{r["commit"]}`', dt=r["date"], p=r["priority"]))
    # 요약
    summ = {}
    for r in rows:
        s = summ.setdefault(r["category"], {})
        s[r["result"]] = s.get(r["result"], 0) + 1
    sl = ["| 분류 | 설명 | PASS | FAIL | INFO | UNSUPPORTED | TODO |", "|---|---|---|---|---|---|---|"]
    tot = {}
    for c, desc in CATEGORIES:
        s = summ.get(c, {})
        sl.append(f"| {c} | {desc} | " + " | ".join(str(s.get(k, 0)) for k in ["PASS", "FAIL", "INFO", "UNSUPPORTED", "TODO"]) + " |")
        for k, v in s.items():
            tot[k] = tot.get(k, 0) + v
    sl.append("| **합계** | | " + " | ".join(f"**{tot.get(k, 0)}**" for k in ["PASS", "FAIL", "INFO", "UNSUPPORTED", "TODO"]) + " |")
    block = "<!-- RESULTS:BEGIN (scripts/results.py render 로 생성, 직접 수정 금지) -->\n### 요약\n\n" + "\n".join(sl) + "\n\n### 전체 결과 (단일 표)\n\n" + "\n".join(lines) + "\n<!-- RESULTS:END -->"
    md = open(MD).read()
    md = re.sub(r"<!-- RESULTS:BEGIN.*?<!-- RESULTS:END -->", lambda m: block, md, flags=re.S)
    open(MD, "w").write(md)
    print(f"render: {len(rows)} rows -> {MD}")


if __name__ == "__main__":
    p = argparse.ArgumentParser()
    sp = p.add_subparsers(dest="cmd", required=True)
    sp.add_parser("init").set_defaults(fn=cmd_init)
    u = sp.add_parser("update")
    u.add_argument("--engine", required=True, choices=list(ENGINES))
    u.add_argument("--method", default="auto-headless")
    u.add_argument("--commit")
    u.add_argument("--date")
    u.add_argument("files", nargs="+")
    u.set_defaults(fn=cmd_update)
    sp.add_parser("render").set_defaults(fn=cmd_render)
    a = p.parse_args()
    a.fn(a)
