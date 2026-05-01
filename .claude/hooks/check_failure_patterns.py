"""Stop hook (warn only): CLAUDE.md 重大事故記録と類似パターンを検出 → 警告のみ。

block しない。stderr に warn を出すことで Claude/ユーザー双方が
過去事故と類似する言い回しに気付けるようにする。
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
from _lib import (  # noqa: E402
    get_assistant_last_text,
    is_in_project,
    read_input_json,
    read_transcript,
)

PATTERNS: list[tuple[re.Pattern, str]] = [
    (
        re.compile(r"再生成\s*(し|→|>|>)\s*(再)?投入"),
        "2026-01-04 データ消失事故: CSV 再生成→上書きで 95 万行喪失。"
        "インポート前に DELETE-or-PRESERVE 戦略をユーザーに確認したか?",
    ),
    (
        re.compile(r"重複\s*(なし|無し|ありません).+(確認|verified)"),
        "2026-01-05 全 12 職種 DB 重複事故: Claude が「確認済み」と虚偽報告して追加課金。"
        "SQL 結果を実際に提示したか?",
    ),
    (
        re.compile(r"(USE_CSV_MODE|無料枠.*リセット|無料枠.*待ち)"),
        "2026-01-06 $195 超過請求: 前提を verify せず提案。"
        "CSV 容量や Turso 制約を実数値で確認したか?",
    ),
    (
        re.compile(r"drop_duplicates\([^)]*subset\s*=\s*\[(?:(?!employment_type)[^\]])*\]"),
        "2026-02-24 雇用形態 dedup 事故: subset に employment_type を含めないと"
        "正社員/パートが消える (大量データ消失)。",
    ),
    (
        re.compile(r"git\s+add\s+(-A|-a|--all|\.)\s|git\s+commit\s+-am?\s"),
        "2026-03-10 本番 geojson 消失事故: git add -A で意図しないファイル巻き込み。"
        "ファイル名を明示的に指定すべき。",
    ),
    (
        re.compile(r"(taskkill|kill\s+-9\s+python|pkill\s+python)"),
        "ユーザー長時間処理破壊リスク: taskkill /F /IM python.exe は禁止 (CLAUDE.md)。",
    ),
]


def main() -> None:
    payload = read_input_json()
    if not is_in_project(payload):
        sys.exit(0)

    transcript = read_transcript(payload.get("transcript_path"))
    if not transcript:
        sys.exit(0)

    last = get_assistant_last_text(transcript)
    if not last:
        sys.exit(0)

    matches: list[str] = []
    for pat, msg in PATTERNS:
        if pat.search(last):
            matches.append(msg)

    if matches:
        sys.stderr.write("[hook: failure-pattern-warn] 過去事故と類似パターンを検出 (warn only):\n")
        for m in matches:
            sys.stderr.write(f"  - {m}\n")

    # warn only - never block
    sys.exit(0)


if __name__ == "__main__":
    main()
