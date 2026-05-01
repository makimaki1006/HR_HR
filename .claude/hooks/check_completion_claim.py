"""Stop hook: 強い完了主張 + 検証 Bash 実行なし → block。

発火: 最終応答に「✅完了」「機能完了」「品質完了」「ALL PASS」「実装完了」等。
要求: 直前 30 ターンに cargo test / pytest / curl 等の検証 Bash 実行があること。
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
from _lib import (  # noqa: E402
    block,
    get_assistant_last_text,
    get_recent_tool_results,
    get_recent_tool_uses,
    has_bypass_signal,
    is_in_project,
    pass_through,
    read_input_json,
    read_transcript,
)

STRONG_COMPLETION_RE = re.compile(
    r"(✅\s*完了|✅\s*ALL\s*PASS|機能完了|品質完了|実装完了"
    r"|ALL\s+PASS|全\s*パス|全テスト\s*pass|全 ?\d+ ?(件)? ?(test|テスト)?\s*PASS)",
    re.IGNORECASE,
)

VERIFICATION_BASH_RE = re.compile(
    r"(cargo\s+(test|check|build|run)"
    r"|pytest|python\s+-m\s+pytest"
    r"|npm\s+(test|run\s+test)|jest|playwright|vitest"
    r"|curl\s|wget\s"
    r"|psql|sqlite3|turso\s+db|libsql)",
    re.IGNORECASE,
)

VERIFICATION_OUTPUT_RE = re.compile(
    r"(test result:\s*ok|All tests passed|tests? passed|FINISHED|✅\s*PASS)",
    re.IGNORECASE,
)


def main() -> None:
    payload = read_input_json()
    if not is_in_project(payload):
        pass_through()

    transcript = read_transcript(payload.get("transcript_path"))
    if not transcript:
        pass_through()

    last = get_assistant_last_text(transcript)
    if not STRONG_COMPLETION_RE.search(last):
        pass_through()

    if has_bypass_signal(transcript):
        pass_through()

    tool_uses = get_recent_tool_uses(transcript, n=30)
    has_verification_cmd = False
    for tu in tool_uses:
        if tu.get("name") == "Bash":
            cmd = (tu.get("input") or {}).get("command", "")
            if VERIFICATION_BASH_RE.search(cmd):
                has_verification_cmd = True
                break

    if not has_verification_cmd:
        # tool_result 出力にテスト成功らしきものがあれば許容 (補助)
        for tr in get_recent_tool_results(transcript, n=30):
            if VERIFICATION_OUTPUT_RE.search(tr):
                has_verification_cmd = True
                break

    if not has_verification_cmd:
        block(
            "[hook: completion-claim] 「✅完了」「機能完了」等の強い完了主張がありますが、"
            "直前 30 ターンに cargo test / pytest / curl 等の検証 Bash 実行が確認できません。"
            "CLAUDE.md の完了定義 (🟡基盤 / 🟢機能 / ✅品質) に従い、テスト実行か実物確認の"
            "エビデンスを提示してから完了報告してください。"
            "Bypass する場合はユーザーが「テスト不要」と明言する必要があります。"
        )

    pass_through()


if __name__ == "__main__":
    main()
