# -*- coding: utf-8 -*-
"""
ingest_industry_structure_to_local.py
======================================

`scripts/data/industry_structure_by_municipality.csv` を hellowork.db の
`v2_external_industry_structure` テーブルに投入する。

データソース: e-Stat 経済センサス R3 (statsDataId=0003449718)
取得元スクリプト: `scripts/fetch_industry_structure.py`
出力 CSV カラム: prefecture_code, city_code, city_name,
                industry_code, industry_name,
                establishments, employees_total, employees_male, employees_female

DDL: `scripts/upload_new_external_to_turso.py:106-119` (Turso 既存スキーマ) と同一。
PRIMARY KEY: (city_code, industry_code)

CLI:
  python scripts/ingest_industry_structure_to_local.py --dry-run
  python scripts/ingest_industry_structure_to_local.py --apply
  python scripts/ingest_industry_structure_to_local.py --verify-only

Round 8 P0-2 前提作業 (2026-05-10): 詳細は
docs/SURVEY_MARKET_INTELLIGENCE_PHASE3_INDUSTRY_STRUCTURE_INGEST.md 参照。
"""
from __future__ import annotations

import argparse
import csv
import os
import sqlite3
import sys
from datetime import datetime, timezone
from pathlib import Path

try:
    sys.stdout.reconfigure(encoding="utf-8")
except (AttributeError, ValueError):
    pass

SCRIPT_DIR = Path(__file__).resolve().parent
DB_PATH = SCRIPT_DIR.parent / "data" / "hellowork.db"
CSV_PATH = SCRIPT_DIR / "data" / "industry_structure_by_municipality.csv"

TABLE_NAME = "v2_external_industry_structure"

DDL = f"""
CREATE TABLE IF NOT EXISTS {TABLE_NAME} (
    prefecture_code  TEXT NOT NULL,
    city_code        TEXT NOT NULL,
    city_name        TEXT,
    industry_code    TEXT NOT NULL,
    industry_name    TEXT,
    establishments   INTEGER,
    employees_total  INTEGER,
    employees_male   INTEGER,
    employees_female INTEGER,
    PRIMARY KEY (city_code, industry_code)
)
"""

DDL_INDEXES = [
    f"CREATE INDEX IF NOT EXISTS idx_indstruct_pref ON {TABLE_NAME} (prefecture_code)",
    f"CREATE INDEX IF NOT EXISTS idx_indstruct_ind ON {TABLE_NAME} (industry_code)",
]

INSERT_COLS = [
    "prefecture_code",
    "city_code",
    "city_name",
    "industry_code",
    "industry_name",
    "establishments",
    "employees_total",
    "employees_male",
    "employees_female",
]


def parse_int(v: str) -> int | None:
    """e-Stat 値の int 変換。空・"-"・"…" は NULL 扱い。"""
    if v is None:
        return None
    s = str(v).strip().replace(",", "")
    if s == "" or s == "-" or s == "…" or s == "***":
        return None
    try:
        return int(float(s))
    except (ValueError, TypeError):
        return None


def load_csv() -> list[dict]:
    """CSV を読み込んで dict のリストを返す (UTF-8 BOM 対応)。"""
    if not CSV_PATH.exists():
        print(f"[ERROR] CSV not found: {CSV_PATH}")
        print("        scripts/fetch_industry_structure.py を先に実行してください。")
        return []
    rows: list[dict] = []
    with open(CSV_PATH, "r", encoding="utf-8-sig", newline="") as f:
        reader = csv.DictReader(f)
        for r in reader:
            rows.append(
                {
                    "prefecture_code": str(r.get("prefecture_code", "")).zfill(2),
                    "city_code": str(r.get("city_code", "")).zfill(5),
                    "city_name": r.get("city_name", ""),
                    "industry_code": r.get("industry_code", ""),
                    "industry_name": r.get("industry_name", ""),
                    "establishments": parse_int(r.get("establishments")),
                    "employees_total": parse_int(r.get("employees_total")),
                    "employees_male": parse_int(r.get("employees_male")),
                    "employees_female": parse_int(r.get("employees_female")),
                }
            )
    return rows


def dry_run(args) -> int:
    rows = load_csv()
    if not rows:
        return 1

    n_total = len(rows)
    cities = {r["city_code"] for r in rows}
    industries = {r["industry_code"] for r in rows}
    expected = len(cities) * len(industries)

    null_male = sum(1 for r in rows if r["employees_male"] is None)
    null_female = sum(1 for r in rows if r["employees_female"] is None)
    null_total = sum(1 for r in rows if r["employees_total"] is None)
    null_estab = sum(1 for r in rows if r["establishments"] is None)

    print(f"[csv] path        : {CSV_PATH}")
    print(f"[csv] total rows  : {n_total:,}")
    print(f"[csv] unique cities    : {len(cities):,}")
    print(f"[csv] unique industries: {len(industries)} ({sorted(industries)})")
    print(f"[csv] expected (cities x industries) = {expected:,}")
    if expected == n_total:
        print(f"[csv] ✅ row count MATCH")
    else:
        print(f"[csv] ⚠️ row count diff: {n_total - expected:+,}")
    print()
    print(f"[null] establishments  : {null_estab:,} ({100.0 * null_estab / n_total:.1f}%)")
    print(f"[null] employees_total : {null_total:,} ({100.0 * null_total / n_total:.1f}%)")
    print(f"[null] employees_male  : {null_male:,} ({100.0 * null_male / n_total:.1f}%)")
    print(f"[null] employees_female: {null_female:,} ({100.0 * null_female / n_total:.1f}%)")
    print()
    print(f"[sample] first 3 rows:")
    for r in rows[:3]:
        print(f"  {r}")
    print()
    print(f"[ingest] target DB    : {DB_PATH}")
    print(f"[ingest] target table : {TABLE_NAME}")
    print(f"[ingest] would insert : {n_total:,} rows")
    print(f"[ingest] mode         : DRY-RUN (no write)")
    return 0


def apply_(args) -> int:
    rows = load_csv()
    if not rows:
        return 1

    if not DB_PATH.exists():
        print(f"[ERROR] DB not found: {DB_PATH}")
        return 1

    conn = sqlite3.connect(DB_PATH)
    cur = conn.cursor()

    # DDL (idempotent)
    cur.execute(DDL)
    for ddl in DDL_INDEXES:
        cur.execute(ddl)

    # 既存件数
    cur.execute(f"SELECT COUNT(*) FROM {TABLE_NAME}")
    n_before = cur.fetchone()[0]

    # 一括 INSERT OR REPLACE (PRIMARY KEY: city_code + industry_code で upsert)
    placeholders = ", ".join(["?"] * len(INSERT_COLS))
    sql = f"INSERT OR REPLACE INTO {TABLE_NAME} ({', '.join(INSERT_COLS)}) VALUES ({placeholders})"
    payload = [tuple(r[c] for c in INSERT_COLS) for r in rows]
    cur.executemany(sql, payload)
    conn.commit()

    cur.execute(f"SELECT COUNT(*) FROM {TABLE_NAME}")
    n_after = cur.fetchone()[0]
    conn.close()

    print(f"[apply] db        : {DB_PATH}")
    print(f"[apply] table     : {TABLE_NAME}")
    print(f"[apply] before    : {n_before:,}")
    print(f"[apply] inserted  : {len(rows):,}")
    print(f"[apply] after     : {n_after:,}")
    print(f"[apply] delta     : {n_after - n_before:+,}")
    print(f"[apply] timestamp : {datetime.now(timezone.utc).isoformat()}")
    return 0


def verify(args) -> int:
    if not DB_PATH.exists():
        print(f"[ERROR] DB not found: {DB_PATH}")
        return 1

    conn = sqlite3.connect(DB_PATH)
    cur = conn.cursor()

    # テーブル存在確認
    cur.execute(
        "SELECT name FROM sqlite_master WHERE type='table' AND name = ?",
        (TABLE_NAME,),
    )
    if not cur.fetchone():
        print(f"[verify] ❌ table {TABLE_NAME} does not exist")
        return 1

    # スキーマ
    cur.execute(f"PRAGMA table_info({TABLE_NAME})")
    cols = cur.fetchall()
    print(f"[schema] columns ({len(cols)}):")
    for c in cols:
        print(f"  {c}")
    print()

    # 行数 + 一意キー
    cur.execute(f"SELECT COUNT(*) FROM {TABLE_NAME}")
    n_total = cur.fetchone()[0]
    cur.execute(f"SELECT COUNT(DISTINCT city_code) FROM {TABLE_NAME}")
    n_cities = cur.fetchone()[0]
    cur.execute(f"SELECT COUNT(DISTINCT industry_code) FROM {TABLE_NAME}")
    n_industries = cur.fetchone()[0]
    print(f"[count] total rows           : {n_total:,}")
    print(f"[count] unique city_code     : {n_cities:,}")
    print(f"[count] unique industry_code : {n_industries}")
    print()

    # NULL 率
    for col in ("establishments", "employees_total", "employees_male", "employees_female"):
        cur.execute(f"SELECT SUM(CASE WHEN {col} IS NULL THEN 1 ELSE 0 END) FROM {TABLE_NAME}")
        n_null = cur.fetchone()[0]
        pct = 100.0 * n_null / n_total if n_total else 0.0
        print(f"[null] {col:<18}: {n_null:,} ({pct:.1f}%)")
    print()

    # サンプル 5 行 (新宿区)
    cur.execute(
        f"SELECT prefecture_code, city_code, city_name, industry_code, industry_name, "
        f"       establishments, employees_total, employees_male, employees_female "
        f"FROM {TABLE_NAME} WHERE city_code='13104' ORDER BY industry_code LIMIT 5"
    )
    print(f"[sample] 新宿区 (city_code=13104) 5 行:")
    for r in cur.fetchall():
        print(f"  {r}")
    print()

    # サンプル 5 行 (千代田区)
    cur.execute(
        f"SELECT prefecture_code, city_code, city_name, industry_code, industry_name, "
        f"       establishments, employees_total, employees_male, employees_female "
        f"FROM {TABLE_NAME} WHERE city_code='13101' ORDER BY industry_code LIMIT 5"
    )
    print(f"[sample] 千代田区 (city_code=13101) 5 行:")
    for r in cur.fetchall():
        print(f"  {r}")

    conn.close()
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(
        description="industry_structure_by_municipality.csv → v2_external_industry_structure (Local)"
    )
    sub = parser.add_subparsers(dest="cmd", required=True)
    sub.add_parser("--dry-run", help="投入計画のみ (CSV 解析、DB 書込なし)").set_defaults(func=dry_run)
    sub.add_parser("--apply", help="hellowork.db に投入 (INSERT OR REPLACE)").set_defaults(func=apply_)
    sub.add_parser("--verify-only", help="投入後の検証のみ (COUNT, schema, NULL 率, sample)").set_defaults(func=verify)

    # argparse subparsers は "--" prefix を直接受けないので別パース
    raw_args = sys.argv[1:]
    if not raw_args:
        parser.print_help()
        return 1
    cmd = raw_args[0]
    if cmd == "--dry-run":
        return dry_run(None)
    if cmd == "--apply":
        return apply_(None)
    if cmd == "--verify-only":
        return verify(None)
    parser.print_help()
    return 1


if __name__ == "__main__":
    sys.exit(main())
