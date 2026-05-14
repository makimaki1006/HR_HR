"""Stop hook: 数値/視覚レビュー完了主張 + audit-numeric-anomaly skill 未使用 → block。

発火: 最終応答に「視覚レビュー完了」「数値確認OK」「PDF確認 ✅」等。
要求: 過去 60 分以内に .claude/.audit_numeric_done が touch されていること
      (= audit-numeric-anomaly skill を実行している証)。

背景:
2026-05-14 表示層 ×100 バグで 30 分浪費。データ層仮説に固執して
3 層 grep を省いたのが原因。メモリだけでは行動が変わらないので
hook で「skill 使った?」をチェックボックス的に検証する。
"""

from __future__ import annotations

import re
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
from _lib import (  # noqa: E402
    block,
    get_assistant_last_text,
    get_user_recent_prompts,
    has_bypass_signal,
    is_in_project,
    pass_through,
    read_input_json,
    read_transcript,
)

# 「数値/視覚レビュー完了」の強い主張パターン。
COMPLETION_RE = re.compile(
    r"(視覚\s*レビュー\s*(完了|済|完)"
    r"|PDF\s*(視覚|確認)\s*(完了|済|OK)"
    r"|数値\s*(確認|レビュー|検証)\s*(完了|済|OK)"
    r"|12\s*/\s*12\s*(完了|済|OK)"
    r"|\d+\s*項目\s*(全|すべて)\s*(解決|完了|クリア)"
    r"|表\s*\d+-[A-Z]\s*(✅|OK|解決|完了))",
)

# 関連トピック (ユーザー or assistant 過去発言にこれがあれば数値関連の review 文脈)。
NUMERIC_TOPIC_RE = re.compile(
    r"(視覚\s*レビュー"
    r"|数値\s*(が|の)?\s*(異常|おかしい|ずれ|桁違い)"
    r"|100\s*倍"
    r"|PDF.*?(表|グラフ|数値|値)"
    r"|\+\d{3,}\s*%"
    r"|delta|employee_delta"
    r"|表\s*\d+-[A-Z])",
)

DONE_MARKER_REL = ".claude/.audit_numeric_done"
MAX_AGE_SEC = 60 * 60  # 60 分以内


def main() -> None:
    payload = read_input_json()
    if not is_in_project(payload):
        pass_through()

    transcript = read_transcript(payload.get("transcript_path"))
    if not transcript:
        pass_through()

    last = get_assistant_last_text(transcript)
    if not COMPLETION_RE.search(last):
        pass_through()

    # 完了主張があっても、文脈が数値レビューでなければスルー
    # (機能完了の主張等を誤検知しないため)。
    recent_prompts = "\n".join(get_user_recent_prompts(transcript, n=10))
    if not (NUMERIC_TOPIC_RE.search(last) or NUMERIC_TOPIC_RE.search(recent_prompts)):
        pass_through()

    if has_bypass_signal(transcript):
        pass_through()

    cwd = payload.get("cwd") or "."
    marker = Path(cwd) / DONE_MARKER_REL
    fresh = False
    if marker.exists():
        try:
            age = time.time() - marker.stat().st_mtime
            fresh = age <= MAX_AGE_SEC
        except OSError:
            fresh = False

    if not fresh:
        block(
            "[hook: numeric-review-skill] 数値/視覚レビューの完了主張がありますが、"
            f"`audit-numeric-anomaly` skill の完了 marker (`{DONE_MARKER_REL}`) が "
            "60 分以内に touch されていません。\n\n"
            "数値の表示異常を扱う時は **仮説立てる前に** skill を呼んで 3 層 "
            "(データ/計算/表示) を全 grep してください。skill 内で監査完了後、\n"
            "  echo \"$(date -u +%Y-%m-%dT%H:%M:%SZ) <var> <layer>\" > "
            f"{DONE_MARKER_REL}\n"
            "を実行して marker を残してから完了主張してください。\n\n"
            "2026-05-14 表示層 ×100 バグ (30 分浪費) の再発防止策。"
        )

    pass_through()


if __name__ == "__main__":
    main()
