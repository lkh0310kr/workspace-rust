#!/usr/bin/env bash
# Rust 엔진 vs 원본 JS(CEF 헤드리스, 가짜 시계) 수치 동일성 비교. 창/포커스 없음.
# 사용: crates/rte-engine/tools/run_compare.sh [WxH] [시나리오이름...]
set -uo pipefail
cd "$(dirname "$0")/../../.."
export CEF_PATH="${CEF_PATH:-$HOME/.local/share/cef156}"
VIEW="${1:-1100x800}"; shift || true
W="${VIEW%x*}"; H="${VIEW#*x}"
DIR=crates/rte-engine/tools/scenarios
NAMES=("$@"); [[ ${#NAMES[@]} -eq 0 ]] && NAMES=($(ls $DIR | sed 's/\.json$//'))
python3 crates/rte-engine/tools/gen_scenarios.py >/dev/null
cargo build -q -p rte-engine 2>&1 | tail -3
cargo build -q -p spike-cef --bins 2>&1 | grep -E "^error" -A6
if pgrep -f "spike-cef.app" >/dev/null; then echo "spike-cef 가 이미 실행 중입니다(포트 18080 충돌). 종료 후 다시 실행하세요." >&2; exit 2; fi
OUT=target/rte/$VIEW; mkdir -p "$OUT"; rm -f $OUT/*.json
BIN=target/bundle/spike-cef.app/Contents/MacOS/spike-cef
if [[ ! -x "$BIN" || -n "$(find crates -newer "$BIN" -name '*.rs' | head -1)" ]]; then
  rm -rf target/bundle/spike-cef.app; bundle-cef-app spike-cef -o target/bundle >/dev/null 2>&1 && codesign --force --deep -s - target/bundle/spike-cef.app >/dev/null 2>&1
fi
LIST=$(for n in "${NAMES[@]}"; do printf "%s/%s.json," "$DIR" "$n"; done)
rm -rf target/rte-cache; SPIKE_CACHE="$PWD/target/rte-cache" SPIKE_HEADLESS=1 SPIKE_SUITE=rte SPIKE_VIEW="$VIEW" SPIKE_RTE_SCENARIOS="$LIST" SPIKE_RTE_OUTDIR="$OUT" perl -e 'alarm 900; exec @ARGV' "$BIN" > target/rte-js.log 2>&1
fail=0
for n in "${NAMES[@]}"; do
  ./target/debug/rte-dump "$DIR/$n.json" "$W" "$H" > "$OUT/$n-rust.json"
  if [[ -f "$OUT/$n-js.json" ]]; then
    printf "%-10s " "$n"; python3 crates/rte-engine/tools/compare.py "$OUT/$n-rust.json" "$OUT/$n-js.json" --max 3 || fail=1
  else echo "$n: JS 덤프 없음"; fail=1; fi
done
exit $fail
