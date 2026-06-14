"""
sikaku7_qualifications.json を読み込み、
v2_external_license_sikaku7 テーブルへの INSERT SQL を生成する。

出力: data/license_sikaku7_turso_import.sql
"""

from __future__ import annotations

import json
import sys
from pathlib import Path

# ---------------------------------------------------------------------------
# パス定義
# ---------------------------------------------------------------------------
BASE_DIR = Path(__file__).parent.parent  # hellowork-deploy/
DATA_DIR = BASE_DIR / "data" / "generated"
INPUT_PATH = DATA_DIR / "sikaku7_qualifications.json"
OUTPUT_PATH = BASE_DIR / "data" / "license_sikaku7_turso_import.sql"

DDL = """\
CREATE TABLE IF NOT EXISTS v2_external_license_sikaku7 (
    jilpt_name   TEXT    NOT NULL,
    sikaku7_url  TEXT    NOT NULL,
    sikaku7_title TEXT   NOT NULL,
    field_order  INTEGER NOT NULL,
    field_key    TEXT    NOT NULL,
    field_value  TEXT    NOT NULL,
    fetched_at   TEXT    NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (jilpt_name, field_order)
);
CREATE INDEX IF NOT EXISTS idx_license_sikaku7_name
    ON v2_external_license_sikaku7(jilpt_name);
"""


def escape_sql_string(s: str) -> str:
    """SQLite/Turso 用のシングルクォートエスケープ。"""
    return s.replace("'", "''")


def build_sql(records: list[dict]) -> str:
    lines: list[str] = [DDL]

    total_rows = 0
    for rec in records:
        jilpt_name = escape_sql_string(rec["jilpt_name"])
        sikaku7_url = escape_sql_string(rec["sikaku7_url"])
        sikaku7_title = escape_sql_string(rec["sikaku7_title"])
        fetched_at = escape_sql_string(rec.get("fetched_at", ""))
        fields: dict[str, str] = rec.get("fields", {})

        for order, (key, value) in enumerate(fields.items()):
            key_esc = escape_sql_string(key)
            value_esc = escape_sql_string(str(value))
            lines.append(
                f"INSERT OR REPLACE INTO v2_external_license_sikaku7 "
                f"(jilpt_name, sikaku7_url, sikaku7_title, field_order, field_key, field_value, fetched_at) VALUES ("
                f"'{jilpt_name}', '{sikaku7_url}', '{sikaku7_title}', "
                f"{order}, '{key_esc}', '{value_esc}', '{fetched_at}');"
            )
            total_rows += 1

    return "\n".join(lines), total_rows


def main() -> None:
    print("=== build_sikaku7_sql.py 開始 ===")

    if not INPUT_PATH.exists():
        print(f"[ERROR] 入力ファイルが存在しない: {INPUT_PATH}", file=sys.stderr)
        print("  先に fetch_sikaku7.py を実行してください。")
        sys.exit(1)

    with open(INPUT_PATH, encoding="utf-8") as f:
        records = json.load(f)

    print(f"[入力] レコード数: {len(records)}")

    sql_content, total_rows = build_sql(records)

    OUTPUT_PATH.parent.mkdir(parents=True, exist_ok=True)
    with open(OUTPUT_PATH, "w", encoding="utf-8") as f:
        f.write(sql_content)

    print(f"[出力] {OUTPUT_PATH}")
    print(f"  INSERT 行数: {total_rows}")
    print(f"  資格件数: {len(records)}")

    # フィールド数の統計
    if records:
        field_counts = [len(r.get("fields", {})) for r in records]
        avg_fields = sum(field_counts) / len(field_counts)
        print(f"  平均フィールド数/資格: {avg_fields:.1f}")
        print(f"  最大フィールド数: {max(field_counts)}")
        print(f"  最小フィールド数: {min(field_counts)}")

    # サンプル
    print()
    print("=== サンプル確認 (先頭 3 件) ===")
    for rec in records[:3]:
        print(f"  [{rec['jilpt_name']}] {rec['sikaku7_url']}")
        for k, v in list(rec.get("fields", {}).items())[:4]:
            print(f"    {k}: {v[:60]}")
        print()


if __name__ == "__main__":
    main()
