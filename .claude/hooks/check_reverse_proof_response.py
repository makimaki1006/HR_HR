"""Stop hook: 逆証明要求 + 弱い OK + 横断検査なし → block。

直前 5 ユーザー発言に「逆証明」等があり、Claude の最終応答に
「問題ない」「他に無い」「OK」等の弱い断言がある場合、
直近 30 ターンに Grep / Glob / Bash(grep) 等の横断検査が
無ければ block する。
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
    get_user_recent_prompts,
    has_bypass_signal,
    is_in_project,
    pass_through,
    read_input_json,
    read_transcript,
)

REVERSE_PROOF_RE = re.compile(
    r"(逆証明|反証|反例|横展開|同種パターン|不変条件)",
)

WEAK_OK_RE = re.compile(
    r"(問題\s*(は|が)?\s*(ない|無い|ありません)"
    r"|他に\s*(問題|懸念|バグ|事故|脆弱性).+(無|な)い"
    r"|他に\s*(無|な)い"
    r"|OK[。!\.\s]"
    r"|安全\s*(です|である|と確認)"
    r"|全\s*(パス|PASS)"
    r"|All\s*PASS"
    r"|逆証明\s*(完了|OK|PASS))",
    re.IGNORECASE,
)


def has_cross_check(tool_uses: list[dict]) -> bool:
    """横断検査 tool 使用があるか。"""
    for tu in tool_uses:
        name = tu.get("name", "")
        if name in ("Grep", "Glob"):
            return True
        if name == "Bash":
            cmd = (tu.get("input") or {}).get("command", "")
            if re.search(r"\b(grep|rg|ag|ripgrep)\b", cmd):
                return True
    return False


def main() -> None:
    payload = read_input_json()
    if not is_in_project(payload):
        pass_through()

    transcript = read_transcript(payload.get("transcript_path"))
    if not transcript:
        pass_through()

    user_prompts = get_user_recent_prompts(transcript, n=5)
    if not any(REVERSE_PROOF_RE.search(p) for p in user_prompts):
        pass_through()

    last = get_assistant_last_text(transcript)
    if not WEAK_OK_RE.search(last):
        pass_through()

    if has_bypass_signal(transcript):
        pass_through()

    tool_uses = get_recent_tool_uses(transcript, n=30)
    if has_cross_check(tool_uses):
        pass_through()

    block(
        "[hook: reverse-proof] ユーザーが逆証明 / 反証 / 横展開を要求し、"
        "あなたは「問題ない」「OK」「全パス」等と断言していますが、"
        "直近 30 ターンで Grep / Glob / grep 等の横断検査 tool 使用ログが確認できません。"
        "コードベース横展開検査を実施してから断言してください。"
        "memory: feedback_reverse_proof_tests.md / feedback_llm_visual_review.md 参照。"
    )


if __name__ == "__main__":
    main()
