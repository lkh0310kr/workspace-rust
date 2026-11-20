#!/usr/bin/env bash
# 원본 index.html 을 창 없이(CEF 헤드리스) 그려 PNG 로 저장. 사용: tools_shot_orig.sh <out.png> [WxH] [theme:light|dark] [JS]
set -uo pipefail
cd "$(dirname "$0")/../.."
export CEF_PATH="${CEF_PATH:-$HOME/.local/share/cef156}"
OUT="${1:?out.png}"; VIEW="${2:-1100x800}"; THEME="${3:-light}"; JS="${4:-}"
cargo build -q -p spike-cef --bins 2>&1 | grep -E "^error" -A6
BIN=target/bundle/spike-cef.app/Contents/MacOS/spike-cef
if [[ ! -x "$BIN" || -n "$(find crates -newer "$BIN" -name '*.rs' | head -1)" ]]; then
  rm -rf target/bundle/spike-cef.app; bundle-cef-app spike-cef -o target/bundle >/dev/null 2>&1 && codesign --force --deep -s - target/bundle/spike-cef.app >/dev/null 2>&1
fi
pgrep -f "spike-cef.app" >/dev/null && { echo "spike-cef 실행 중(포트 충돌) — 종료 후 재시도" >&2; exit 2; }
THEMEJS="document.documentElement.dataset.theme='$THEME';"
rm -rf target/shot-cache
SPIKE_CACHE="$PWD/target/shot-cache" SPIKE_HEADLESS=1 SPIKE_VIEW="$VIEW" SPIKE_SHOT_URL="http://127.0.0.1:18080/rte-orig" SPIKE_SHOT_JS="$THEMEJS$JS" SPIKE_SHOT_OUT="$OUT" perl -e 'alarm 120; exec @ARGV' "$BIN" 2>&1 | grep -E "SHOT"
