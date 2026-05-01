"""UserPromptSubmit hook: ユーザーが「逆証明」等を要求したらコンテキスト注入。

block しない。Claude が逆証明モードに入ったことを認識できるよう
hookSpecificOutput.additionalContext で追加メッセージを注入する。
"""

from __future__ import annotations

import json
import re
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
from _lib import is_in_project, read_input_json  # noqa: E402

REVERSE_PROOF_RE = re.compile(
    r"(逆証明|反証|反例|横展開|同種パターン|不変条件|invariant)",
    re.IGNORECASE,
)

CONTEXT_MESSAGE = (
    "[逆証明モード起動: hook injected]\n"
    "ユーザーが逆証明 / 反証 / 横展開 を要求しています。"
    "単なる動作確認 (cargo test 1 件 PASS) で済ませてはなりません。"
    "以下を全て実施してください:\n"
    "  (1) 反証試行: 失敗するはずの input を能動的に探す。\n"
    "  (2) コードベース横展開: Grep で同種パターンを全走査し、修正対象漏れを潰す。\n"
    "  (3) ドメイン不変条件: 失業率<100% / SUM 一致 / 重複なし 等の論理矛盾検査。\n"
    "  (4) 視覚レビュー: スクリーンショット / 出力目視 (該当する場合)。\n"
    "memory 参照: feedback_reverse_proof_tests.md, feedback_llm_visual_review.md。\n"
    "省略は許されません。完了主張時は必ず複数経路 (Grep + Bash + tool_result) のエビデンスを提示してください。"
)


def main() -> None:
    payload = read_input_json()
    if not is_in_project(payload):
        sys.exit(0)

    prompt = payload.get("prompt", "")
    if not REVERSE_PROOF_RE.search(prompt):
        sys.exit(0)

    output = {
        "hookSpecificOutput": {
            "hookEventName": "UserPromptSubmit",
            "additionalContext": CONTEXT_MESSAGE,
        }
    }
    print(json.dumps(output, ensure_ascii=False))
    sys.exit(0)


if __name__ == "__main__":
    main()
