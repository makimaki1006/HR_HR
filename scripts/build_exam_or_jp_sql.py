"""
build_exam_or_jp_sql.py
exam_or_jp_qualifications.json から Turso インポート用 SQL を生成する。

出力: data/license_exam_or_jp_turso_import.sql
"""

import json
import sys
from pathlib import Path

sys.stdout.reconfigure(encoding="utf-8")

# ---------------------------------------------------------------------------
# パス設定
# ---------------------------------------------------------------------------
HERE = Path(__file__).parent.parent  # hellowork-deploy/
JSON_PATH = HERE / "data" / "generated" / "exam_or_jp_qualifications.json"
OUT_SQL = HERE / "data" / "license_exam_or_jp_turso_import.sql"

DDL = """\
CREATE TABLE IF NOT EXISTS v2_external_license_exam_or_jp (
    jilpt_name TEXT NOT NULL,
    exam_url TEXT NOT NULL,
    exam_title TEXT NOT NULL,
    section_order INTEGER NOT NULL,
    section_h2 TEXT NOT NULL,
    section_body TEXT NOT NULL,
    fetched_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (jilpt_name, section_order)
);
CREATE INDEX IF NOT EXISTS idx_license_exam_or_jp_name ON v2_external_license_exam_or_jp(jilpt_name);
"""


def escape_sql_string(s: str) -> str:
    """SQLシングルクォート内で使用するためのエスケープ"""
    return s.replace("'", "''")


def main() -> None:
    print(f"JSON読み込み: {JSON_PATH}")
    with open(JSON_PATH, encoding="utf-8") as f:
        records = json.load(f)
    print(f"  -> {len(records)} 件")

    lines: list[str] = []
    lines.append("-- exam.or.jp 免許試験データ Tursoインポート用SQL")
    lines.append("-- 生成元: scripts/build_exam_or_jp_sql.py")
    lines.append("")
    lines.append(DDL)

    # 既存データのクリア（冪等性確保）
    lines.append("-- 既存データ削除 (冪等性確保)")
    lines.append("DELETE FROM v2_external_license_exam_or_jp;")
    lines.append("")
    lines.append("-- データ挿入")

    row_count = 0
    skipped_count = 0

    for record in records:
        jilpt_name = record.get("jilpt_name", "")
        exam_url = record.get("exam_url", "")
        exam_title = record.get("exam_title", "")
        fetched_at = record.get("fetched_at", "")
        sections = record.get("sections", [])

        # jilpt_name が空の場合は exam_title をキーとして使用
        # (未マッチでもURLとコンテンツを保持するため)
        effective_key = jilpt_name if jilpt_name else exam_title

        if not effective_key:
            skipped_count += 1
            continue

        for order, section in enumerate(sections):
            h2 = section.get("h2", "")
            body = section.get("body", "")

            # 空セクションはスキップ
            if not h2 and not body.strip():
                continue

            vals = (
                f"'{escape_sql_string(effective_key)}'",
                f"'{escape_sql_string(exam_url)}'",
                f"'{escape_sql_string(exam_title)}'",
                str(order),
                f"'{escape_sql_string(h2)}'",
                f"'{escape_sql_string(body)}'",
                f"'{escape_sql_string(fetched_at)}'",
            )
            line = (
                "INSERT INTO v2_external_license_exam_or_jp "
                "(jilpt_name, exam_url, exam_title, section_order, section_h2, section_body, fetched_at) "
                f"VALUES ({', '.join(vals)});"
            )
            lines.append(line)
            row_count += 1

    lines.append("")
    lines.append(f"-- 合計 {row_count} 行挿入")

    OUT_SQL.parent.mkdir(parents=True, exist_ok=True)
    sql_content = "\n".join(lines)
    with open(OUT_SQL, "w", encoding="utf-8") as f:
        f.write(sql_content)

    print(f"SQL生成完了: {OUT_SQL}")
    print(f"  挿入行数: {row_count}")
    print(f"  スキップ: {skipped_count}")

    # サマリー確認
    matched = [r for r in records if r.get("jilpt_name")]
    unmatched = [r for r in records if not r.get("jilpt_name")]
    print(f"  JILPTマッチ済み試験数: {len(matched)}")
    print(f"  JILPTマッチなし試験数: {len(unmatched)} (exam_titleをキーとして保存)")
    print()
    print("=== SQLプレビュー (DDL + 先頭2行) ===")
    preview_lines = [l for l in lines if l.strip()]
    for line in preview_lines[:25]:
        print(line[:200])


if __name__ == "__main__":
    main()
