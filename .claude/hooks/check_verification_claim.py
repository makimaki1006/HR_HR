"""Stop hook: 強い検証主張 + 検証 tool 使用なし → block。

発火: 最終応答に「整合性確認」「重複なし」「問題ない」等の強い検証主張。
要求: 直前 30 ターンに Bash / Read / Grep / Glob 等の検証 tool 使用があること。
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
from _lib import (  # noqa: E402
    block,
    get_assistant_last_text,
    get_recent_tool_uses,
    has_bypass_signal,
    is_in_project,
    pass_through,
    read_input_json,
    read_transcript,
)

# 強い検証主張のみ。「修正しました」「変更しました」等の事実報告は除外。
STRONG_VERIFICATION_RE = re.compile(
    r"(整合性\s*確認"
    r"|整合性\s*(あり|OK)"
    r"|データ\s*確認(済|完了)"
    r"|逆証明\s*完了"
    r"|重複\s*(なし|無し|ありません|存在しません)"
    r"|(問題|エラー|バグ|脆弱性|事故|不整合)\s*(は|が)?\s*(ない|無い|ありません|存在しません|確認できません)"
    r"|安全\s*(です|である|と確認))",
)

VERIFICATION_TOOLS = {"Bash", "Read", "Grep", "Glob"}


def main() -> None:
    payload = read_input_json()
    if not is_in_project(payload):
        pass_through()

    transcript = read_transcript(payload.get("transcript_path"))
    if not transcript:
        pass_through()

    last = get_assistant_last_text(transcript)
    if not STRONG_VERIFICATION_RE.search(last):
        pass_through()

    if has_bypass_signal(transcript):
        pass_through()

    tool_uses = get_recent_tool_uses(transcript, n=30)
    has_check = any(tu.get("name") in VERIFICATION_TOOLS for tu in tool_uses)

    if not has_check:
        block(
            "[hook: verification-claim] 「整合性確認」「重複なし」「問題ない」等の強い検証主張がありますが、"
            "直前 30 ターンに Bash / Read / Grep / Glob 等の検証 tool 使用ログが確認できません。"
            "推測ではなく実証データ (SQL 結果 / cargo test 出力 / Grep 結果) を提示してから断言してください。"
        )

    pass_through()


if __name__ == "__main__":
    main()
