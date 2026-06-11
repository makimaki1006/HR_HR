"""
build_occupation_middle_sql.py
==============================
estat_occupation_middle_pref.csv → Turso 投入用 SQL を生成。
性別 'total' のみ抽出して行数を 1/3 に絞る。
"""
from __future__ import annotations
import csv
import sys
from pathlib import Path

try:
    sys.stdout.reconfigure(encoding="utf-8")
except Exception:
    pass

CSV_IN = Path("data/generated/estat_occupation_middle_pref.csv")
SQL_OUT = Path("data/occupation_middle_pref_turso_import.sql")

DDL = """
CREATE TABLE IF NOT EXISTS v2_external_occupation_middle_pref (
    prefecture TEXT NOT NULL,
    pref_code TEXT NOT NULL,
    occupation_code TEXT NOT NULL,
    occupation_middle TEXT NOT NULL,
    occupation_major_code TEXT,
    age_class TEXT NOT NULL,
    gender TEXT NOT NULL,
    population INTEGER,
    source_year INTEGER NOT NULL DEFAULT 2020,
    PRIMARY KEY (prefecture, occupation_code, age_class, gender)
);
CREATE INDEX IF NOT EXISTS idx_occ_middle_pref_code ON v2_external_occupation_middle_pref(occupation_code, prefecture);
CREATE INDEX IF NOT EXISTS idx_occ_middle_pref_pref ON v2_external_occupation_middle_pref(prefecture, occupation_code);
""".strip()


def esc(s: str) -> str:
    return s.replace("'", "''")


def main() -> int:
    with open(CSV_IN, "r", encoding="utf-8") as f:
        rows = list(csv.DictReader(f))
    # gender = total のみ
    rows = [r for r in rows if r["gender"] == "total"]
    print(f"rows (gender=total only): {len(rows)}")

    sql_lines = [
        "BEGIN;",
        DDL,
        "DELETE FROM v2_external_occupation_middle_pref;",
    ]
    for r in rows:
        pop_str = r["population"]
        pop_val = pop_str if pop_str.strip() else "NULL"
        sql_lines.append(
            "INSERT INTO v2_external_occupation_middle_pref "
            "(prefecture, pref_code, occupation_code, occupation_middle, occupation_major_code, age_class, gender, population) VALUES "
            f"('{esc(r['prefecture'])}', '{r['pref_code']}', '{r['occupation_code']}', "
            f"'{esc(r['occupation_middle'])}', '{esc(r['occupation_major_code'])}', "
            f"'{esc(r['age_class'])}', '{r['gender']}', {pop_val});"
        )
    sql_lines.append("COMMIT;")
    SQL_OUT.write_text("\n".join(sql_lines), encoding="utf-8")
    sz = SQL_OUT.stat().st_size
    print(f"SQL written: {SQL_OUT} ({sz:,} bytes, {len(sql_lines)} statements)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
