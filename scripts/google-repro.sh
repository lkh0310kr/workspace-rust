#!/usr/bin/env bash
# 구글 로그인(아이디 입력 → Next) 크래시 재현. 창이 뜨고 포커스를 가져간다.
# 사용: scripts/google-repro.sh <google_id> [CEF/Chromium 추가 인자...]
set -uo pipefail
cd "$(dirname "$0")/.."
ID="${1:?google id}"; shift || true
BIN=target/bundle/spike-cef.app/Contents/MacOS/spike-cef
codesign --verify --deep --strict target/bundle/spike-cef.app 2>/dev/null || { echo "번들 서명 불일치 → 재서명"; codesign --force --deep -s - target/bundle/spike-cef.app; }
rm -f ~/Library/Logs/spike-cef/*.log
SPIKE_AUTOTEST=1 SPIKE_GOOGLE_LOGIN="$ID" perl -e 'alarm 90; exec @ARGV' "$BIN" "$@" > target/google-repro.log 2>&1
echo "exit=$?  (0=정상종료, 138/139/134 등=크래시, 142=타임아웃)"
grep -E "^STEP|RESULT|cef initialize|INVALID" target/google-repro.log | tail -8
