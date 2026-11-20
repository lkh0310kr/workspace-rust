#!/usr/bin/env bash
# 창/포커스 없이(헤드리스 CEF OSR) 구글 로그인(아이디 입력 → Next) 크래시율 측정. 사용자 작업에 영향 없음.
# 사용: scripts/google-headless.sh <google_id> <반복횟수> [CEF 인자...]   (환경변수: SPIKE_DISABLE_WEBAUTHN=1 등)
set -uo pipefail
cd "$(dirname "$0")/.."
ID="${1:?google id}"; N="${2:-1}"; shift 2 || true
BIN=target/bundle/spike-cef.app/Contents/MacOS/spike-cef
codesign --verify --deep --strict target/bundle/spike-cef.app 2>/dev/null || codesign --force --deep -s - target/bundle/spike-cef.app
crash=0; ok=0; inval=0; codes=()
for i in $(seq 1 "$N"); do
  rm -f ~/Library/Logs/spike-cef/*.log
  SPIKE_HEADLESS=1 SPIKE_GOOGLE_LOGIN="$ID" perl -e 'alarm 100; exec @ARGV' "$BIN" "$@" > target/headless-run.log 2>&1
  e=$?
  if grep -q "SURVIVED" target/headless-run.log; then ok=$((ok+1)); r=survive
  elif grep -q "INVALID" target/headless-run.log; then inval=$((inval+1)); r=invalid
  else crash=$((crash+1)); r="crash(exit=$e)"; codes+=("$e"); fi
  echo "run $i/$N: $r"
done
echo "SUMMARY crash=$crash survive=$ok invalid=$inval (n=$N) flags=[$*] env=[${SPIKE_DISABLE_WEBAUTHN:+DISABLE_WEBAUTHN }${SPIKE_RUNTIME:+RUNTIME=$SPIKE_RUNTIME}]"
