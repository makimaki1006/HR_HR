"""PreToolUse(Edit/Write/MultiEdit) hook: DB / 圧縮ファイルへの書き込みを block。"""

from __future__ import annotations

import re
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
from _lib import (  # noqa: E402
    block,
    has_bypass_signal,
    is_in_project,
    pass_through,
    read_input_json,
    read_transcript,
)

PROTECTED_RE = re.compile(r"\.(db|gz|sqlite|sqlite3|tar|zip)$", re.IGNORECASE)


def main() -> None:
    payload = read_input_json()
    if not is_in_project(payload):
        pass_through()

    if payload.get("tool_name") not in ("Edit", "Write", "MultiEdit"):
        pass_through()

    file_path = (payload.get("tool_input") or {}).get("file_path", "")
    if not PROTECTED_RE.search(file_path):
        pass_through()

    transcript = read_transcript(payload.get("transcript_path"))
    if has_bypass_signal(transcript):
        pass_through()

    block(
        f"[hook: db-write] 保護対象ファイル ({file_path}) への書き込みが要求されました。"
        "DB / 圧縮ファイルへの直接書き込みはユーザー明示許可が必要です。"
        "なぜこのファイルを変更する必要があるかをまず説明し、ユーザーの承認を得てから実行してください。"
    )


if __name__ == "__main__":
    main()
