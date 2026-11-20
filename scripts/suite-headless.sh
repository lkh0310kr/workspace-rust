#!/usr/bin/env bash
# 창/포커스 없이(헤드리스 CEF OSR) 검증 스위트 실행. 사용: scripts/suite-headless.sh <input|browser|all>
set -uo pipefail
cd "$(dirname "$0")/.."
export CEF_PATH="${CEF_PATH:-$HOME/.local/share/cef156}"
W="${1:-all}"
BIN=target/bundle/spike-cef.app/Contents/MacOS/spike-cef
if [[ ! -x "$BIN" || -n "$(find crates -newer "$BIN" -name '*.rs' | head -1)" ]]; then
  rm -rf target/bundle/spike-cef.app
  bundle-cef-app spike-cef -o target/bundle >/dev/null 2>&1 && codesign --force --deep -s - target/bundle/spike-cef.app >/dev/null 2>&1
fi
codesign --verify --deep --strict target/bundle/spike-cef.app 2>/dev/null || codesign --force --deep -s - target/bundle/spike-cef.app
rm -f ~/Library/Logs/spike-cef/*.log
SPIKE_HEADLESS=1 SPIKE_SUITE="$W" perl -e 'alarm 600; exec @ARGV' "$BIN" "${@:2}" > target/suite-$W.log 2>&1
echo "exit=$?"
grep -E "^RESULT|^SUITE" target/suite-$W.log | cut -c1-400
sleep 2
left=$(pgrep -f "spike-cef.app" | wc -l | tr -d ' ')
echo "G7 leftover processes after exit: $left"
