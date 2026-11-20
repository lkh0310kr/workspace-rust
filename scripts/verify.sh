#!/usr/bin/env bash
# 자동 검증 (CEF OSR). 이 하니스는 gpui 창을 실제로 띄운다.
#  - 창이 전면에 뜨고 포커스를 가져간다(약 1~2분). 실행 중 다른 앱 조작 금지(합성 입력과 충돌, 결과가 간헐 실패).
#  - 시스템 클립보드는 백업 후 복원한다.
#  - 백그라운드/투명/화면 밖 창은 gpui가 프레임을 그리지 않아 검증 불가(실측, docs/VERIFICATION.md 참고).
# 사용자 작업을 방해하므로 명시적으로 --yes 를 줘야 실행된다.
set -euo pipefail
cd "$(dirname "$0")/.."
COLOR=(); [[ " $* " == *" --colorful "* ]] && COLOR=(SPIKE_COLORFUL=1)
[[ "${1:-}" == "--yes" ]] || { echo "창이 뜨고 포커스를 가져갑니다. 실행하려면: scripts/verify.sh --yes [--with-wry] [--colorful]"; echo "  기본은 흰색 UI(픽셀 검증 SKIP). 빨강/파랑/초록으로 z-index 등 픽셀 검증까지 하려면 --colorful"; exit 1; }
export CEF_PATH="${CEF_PATH:-$HOME/.local/share/cef}"   # CEF 151 minimal 바이너리 위치
run() { perl -e 'alarm 180; exec @ARGV' "$@"; }

echo "== B. CEF OSR =="
command -v bundle-cef-app >/dev/null || cargo install --git https://github.com/tauri-apps/cef-rs bundle-cef-app
rm -rf target/bundle/spike-cef.app   # 덮어쓰기 시 macOS 서명 캐시 불일치(SIGKILL Code Signature Invalid) 방지
bundle-cef-app spike-cef -o target/bundle >/dev/null
codesign --force --deep -s - target/bundle/spike-cef.app
CACHE="$PWD/target/cef-cache"; rm -rf "$CACHE"
run env "${COLOR[@]}" SPIKE_AUTOTEST=1 SPIKE_CACHE="$CACHE" target/bundle/spike-cef.app/Contents/MacOS/spike-cef 2>&1 | grep -E "RESULT|DONE" || true
run env "${COLOR[@]}" SPIKE_PHASE=2 SPIKE_AUTOTEST=1 SPIKE_CACHE="$CACHE" target/bundle/spike-cef.app/Contents/MacOS/spike-cef 2>&1 | grep -E "RESULT|DONE" || true

if [[ " $* " == *" --with-wry "* ]]; then
  echo "== A. wry =="
  cargo build -p spike-wry
  run env SPIKE_AUTOTEST=1 ./target/debug/spike-wry 2>&1 | grep -E "RESULT|DONE" || true
  run env SPIKE_PHASE=2 SPIKE_AUTOTEST=1 ./target/debug/spike-wry 2>&1 | grep -E "RESULT|DONE" || true
fi
echo "스크린샷: target/shots/  결과 JSON: target/results/"
