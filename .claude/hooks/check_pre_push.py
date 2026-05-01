"""PreToolUse(Bash) hook: git push 前にテスト成功ログがなければ block。"""

from __future__ import annotations

import re
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
from _lib import (  # noqa: E402
    block,
    get_recent_tool_results,
    has_bypass_signal,
    is_in_project,
    pass_through,
    read_input_json,
    read_transcript,
)

GIT_PUSH_RE = re.compile(r"\bgit\s+push\b", re.IGNORECASE)
TEST_OK_RE = re.compile(
    r"(test result:\s*ok|All tests passed|tests? passed|"
    r"\d+\s+passed[\s,;]|cargo:.*Finished|✅\s*ALL\s*PASS)",
    re.IGNORECASE,
)


def main() -> None:
    payload = read_input_json()
    if not is_in_project(payload):
        pass_through()

    if payload.get("tool_name") != "Bash":
        pass_through()

    cmd = (payload.get("tool_input") or {}).get("command", "")
    if not GIT_PUSH_RE.search(cmd):
        pass_through()

    transcript = read_transcript(payload.get("transcript_path"))
    if has_bypass_signal(transcript):
        pass_through()

    results = get_recent_tool_results(transcript, n=50)
    has_test_ok = any(TEST_OK_RE.search(r) for r in results)

    if not has_test_ok:
        block(
            "[hook: pre-push] git push が要求されましたが、直近 50 ターンで cargo test 等の"
            "成功ログが見つかりません。テスト未実行の可能性があります。"
            "テストを実行してから push してください。"
            "Bypass する場合はユーザーが「テスト不要」「強制 push」を明言してください。"
        )

    pass_through()


if __name__ == "__main__":
    main()
