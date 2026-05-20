"""Stop hook: i18n/label 完了主張 + audit-i18n-silent-fallback skill 未使用 → block。

発火: 最終応答に「英語ラベル残 0 件」「全件日本語化」「label_for_column 完了」等。
要求: 過去 60 分以内に .claude/.audit_i18n_done が touch されていること
      (= audit-i18n-silent-fallback skill を実行している証)。

背景:
2026-05-20 表 6-E 労働力統計詳細での英語ラベル残事故。1 件追加で commit
する応急対応を繰り返し、ユーザー指摘「なぜこういった漏らしがあるの？」を
受けて 30+ 件まとめて修正。
1 件追加 commit で済ませず、必ず MECE 監査を skill で実施することを強制する。
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

# 「i18n / label 完了」の強い主張パターン。
COMPLETION_RE = re.compile(
    r"(英語\s*ラベル\s*残\s*0\s*件"
    r"|英語\s*(残|残存)\s*(0|なし)\s*件?"
    r"|label_for_column\s*(完了|済|追加完了)"
    r"|i18n\s*(完了|済|追加完了)"
    r"|翻訳\s*(漏れ|残)\s*(0|なし)\s*件?"
    r"|日本語\s*化\s*(完了|済)"
    r"|silent\s*fallback\s*(根絶|解消|完了)"
    r"|未\s*マップ\s*(列|キー)\s*(0|なし)\s*件?"
    r"|全\s*(件|キー|列)\s*(\s*の?\s*)?(マップ|登録|追加)\s*完了)",
)

# 関連トピック (ユーザー or assistant 過去発言にこれがあれば i18n 文脈)。
I18N_TOPIC_RE = re.compile(
    r"(英語\s*ラベル"
    r"|label_for_column"
    r"|<th>[a-z_]+</th>"
    r"|i18n"
    r"|多言語"
    r"|翻訳"
    r"|silent\s*fallback"
    r"|match\s*arm.*default"
    r"|未\s*マップ"
    r"|snake_?case\s*が\s*そのまま)",
    re.IGNORECASE,
)

DONE_MARKER_REL = ".claude/.audit_i18n_done"
MAX_AGE_SEC = 60 * 60  # 60 分以内


def main() -> None:
    payload = read_input_json()
    if not is_in_project(payload):
        pass_through()

    transcript = read_transcript(payload.get("transcript_path"))
    if not transcript:
        pass_through()

    last = get_assistant_last_text(transcript)

    completion_hit = COMPLETION_RE.search(last)
    if not completion_hit:
        pass_through()

    # 完了主張があっても、文脈が i18n でなければスルー
    recent_prompts = "\n".join(get_user_recent_prompts(transcript, n=10))
    if not (I18N_TOPIC_RE.search(last) or I18N_TOPIC_RE.search(recent_prompts)):
        pass_through()

    if has_bypass_signal(transcript):
        pass_through()

    cwd = payload.get("cwd") or "."

    # audit-i18n-silent-fallback skill marker
    marker = Path(cwd) / DONE_MARKER_REL
    marker_fresh = False
    if marker.exists():
        try:
            age = time.time() - marker.stat().st_mtime
            marker_fresh = age <= MAX_AGE_SEC
        except OSError:
            marker_fresh = False

    if not marker_fresh:
        block(
            "[hook: i18n-label-completeness] i18n / 英語ラベル残の完了主張がありますが、"
            f"`audit-i18n-silent-fallback` skill の完了 marker (`{DONE_MARKER_REL}`) が "
            "60 分以内に touch されていません。\n\n"
            "1 件ずつ後追い追加する応急対応では再発します。**仮説立てる前に** skill を呼んで\n"
            "  - SQL 句から全カラム抽出\n"
            "  - label_for_column の match arm 抽出\n"
            "  - diff で未マップを MECE 抽出\n"
            "  - 一括 commit\n"
            "を実施してください。skill 内で監査完了後、\n"
            f"  echo \"$(date -u +%Y-%m-%dT%H:%M:%SZ) <fn_name> <unmapped_count>\" > {DONE_MARKER_REL}\n"
            "を実行して marker を残してから完了主張してください。\n\n"
            "2026-05-20 表 6-E 労働力統計詳細での英語ラベル残事故 (1 件追加 commit を繰り返した) の再発防止策。"
        )

    pass_through()


if __name__ == "__main__":
    main()
