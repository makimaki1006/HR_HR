#!/bin/bash
# =============================================================================
# E2E Regression Suite Runner
# =============================================================================
# 本番デプロイ (https://hr-hw.onrender.com) に対する全E2Eテストを順次実行する。
#
# 重要: ブラウザ並列実行は禁止 (Chromium リソース競合で FAIL する実証済み)。
#       すべて sequential に実行すること。
#
# 実行環境:
#   - Windows: Git Bash (MSYS2)
#   - Linux/macOS: 標準 bash
#
# 使い方:
#   bash scripts/run_all_e2e.sh
#
# 終了コード:
#   0: 全スイート合格
#   1: 1つ以上のスイートに FAIL または VULNERABLE あり
# =============================================================================

set -u  # 未定義変数は即エラー (set -e は使わない: 1 件の FAIL で中断させず最後まで流す)

EXIT=0
ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT_DIR"

# Windows (Git Bash) では /tmp が %TEMP% に解決される
LOG_DIR="${TMPDIR:-/tmp}"
mkdir -p "$LOG_DIR"

echo "=== E2E Regression Suite ==="
echo "Root      : $ROOT_DIR"
echo "Log dir   : $LOG_DIR"
echo "Started   : $(date '+%Y-%m-%d %H:%M:%S')"
echo ""

# -----------------------------------------------------------------------------
# 1. ユニットテスト (cargo test)
# -----------------------------------------------------------------------------
echo "--- cargo test ---"
if cargo test 2>&1 | tee "$LOG_DIR/cargo_test.log"; then
    if grep -qE "FAILED|test result: FAILED" "$LOG_DIR/cargo_test.log"; then
        echo "FAIL: cargo test"
        EXIT=1
    else
        echo "PASS: cargo test"
    fi
else
    echo "FAIL: cargo test (non-zero exit)"
    EXIT=1
fi
echo ""

# -----------------------------------------------------------------------------
# 2. E2E スクリプト群 (順次実行 - 並列禁止)
# -----------------------------------------------------------------------------
#   ブラウザ競合回避のため、必ず1本ずつ順番に流す。
#   スイートごとに最大 10 分の実行を想定。
# -----------------------------------------------------------------------------
E2E_SCRIPTS=(
    "e2e_security"
    "e2e_report_survey"
    "e2e_report_jobbox"
    "e2e_report_insight"
    "e2e_other_tabs"
    "e2e_api_excel"
    "e2e_print_verify"
)

for script in "${E2E_SCRIPTS[@]}"; do
    echo "--- $script ---"
    if [ ! -f "${script}.py" ]; then
        echo "SKIP: ${script}.py not found"
        continue
    fi
    START_TS=$(date +%s)
    if python "${script}.py" 2>&1 | tee "$LOG_DIR/${script}.log"; then
        ELAPSED=$(( $(date +%s) - START_TS ))
        echo "DONE: $script (${ELAPSED}s)"
    else
        ELAPSED=$(( $(date +%s) - START_TS ))
        echo "FAIL: $script (${ELAPSED}s, non-zero exit)"
        EXIT=1
    fi
    echo ""
done

# -----------------------------------------------------------------------------
# 3. 最終集計
# -----------------------------------------------------------------------------
echo "=== Summary ==="
echo "Finished  : $(date '+%Y-%m-%d %H:%M:%S')"

FAIL_COUNT=0
for script in "${E2E_SCRIPTS[@]}"; do
    LOGFILE="$LOG_DIR/${script}.log"
    [ -f "$LOGFILE" ] || continue
    # FAIL / VULNERABLE の件数を抽出 (大文字小文字区別)
    CNT=$(grep -cE "\b(FAIL|VULNERABLE)\b" "$LOGFILE" 2>/dev/null || echo 0)
    CNT=${CNT:-0}
    printf "  %-25s FAIL/VULN lines: %s\n" "$script" "$CNT"
    FAIL_COUNT=$(( FAIL_COUNT + CNT ))
done

echo ""
echo "Total FAIL/VULN lines : $FAIL_COUNT"
echo "Suite exit code       : $EXIT"

exit "$EXIT"
