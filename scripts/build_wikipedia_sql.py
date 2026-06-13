"""
Wikipedia 資格情報 JSON から Turso 投入用 SQL を生成する。

入力: data/generated/wikipedia_qualifications.json
出力: data/license_wikipedia_turso_import.sql

テーブル:
  v2_external_license_wikipedia (
      jilpt_name TEXT NOT NULL PRIMARY KEY,
      wikipedia_title TEXT NOT NULL,
      wikipedia_url TEXT NOT NULL,
      extract TEXT NOT NULL,
      fetched_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
  )
"""

import sys
import json
from pathlib import Path
from datetime import datetime, timezone

sys.stdout.reconfigure(encoding="utf-8")

BASE_DIR = Path(__file__).parent.parent
INPUT_PATH = BASE_DIR / "data" / "generated" / "wikipedia_qualifications.json"
OUTPUT_PATH = BASE_DIR / "data" / "license_wikipedia_turso_import.sql"

DDL = """\
-- ============================================================
-- v2_external_license_wikipedia Turso 投入用 SQL
-- 生成日時: {generated_at}
-- 件数: {count} 件
-- ライセンス: Wikipedia CC BY-SA 4.0
-- ============================================================

CREATE TABLE IF NOT EXISTS v2_external_license_wikipedia (
    jilpt_name TEXT NOT NULL PRIMARY KEY,
    wikipedia_title TEXT NOT NULL,
    wikipedia_url TEXT NOT NULL,
    extract TEXT NOT NULL,
    fetched_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_license_wikipedia_name
    ON v2_external_license_wikipedia(jilpt_name);

"""

INSERT_TEMPLATE = (
    "INSERT OR REPLACE INTO v2_external_license_wikipedia "
    "(jilpt_name, wikipedia_title, wikipedia_url, extract, fetched_at) VALUES\n"
    "  ({jilpt_name}, {wikipedia_title}, {wikipedia_url}, {extract}, {fetched_at});\n"
)


def sql_escape(s: str) -> str:
    """SQLite 文字列エスケープ (シングルクォート二重化)"""
    return "'" + s.replace("'", "''") + "'"


def main():
    if not INPUT_PATH.exists():
        print(f"ERROR: 入力ファイルが存在しません: {INPUT_PATH}")
        print("先に fetch_wikipedia_qualifications.py を実行してください。")
        raise SystemExit(1)

    records: list[dict] = json.loads(INPUT_PATH.read_text(encoding="utf-8"))
    print(f"入力: {len(records)} 件")

    generated_at = datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")
    fetched_at_val = sql_escape(generated_at)

    lines: list[str] = [DDL.format(generated_at=generated_at, count=len(records))]

    for rec in records:
        lines.append(
            INSERT_TEMPLATE.format(
                jilpt_name=sql_escape(rec["jilpt_name"]),
                wikipedia_title=sql_escape(rec["wikipedia_title"]),
                wikipedia_url=sql_escape(rec["wikipedia_url"]),
                extract=sql_escape(rec["extract"]),
                fetched_at=fetched_at_val,
            )
        )

    sql_content = "".join(lines)
    OUTPUT_PATH.write_text(sql_content, encoding="utf-8")

    print(f"出力: {OUTPUT_PATH}")
    print(f"INSERT 文: {len(records)} 件")
    print("完了")


if __name__ == "__main__":
    main()
