# -*- coding: utf-8 -*-
"""
municipality_living_cost_proxy ビルダー
==========================================
1,917 市区町村 (master 全件) に対し、参考統計の 'reference' データラベルで
生活コスト代理指標を投入する。

ソース継承ルール:
    cost_index           : v2_external_prefecture_stats.price_index (都道府県値、市区町村に継承)
    min_wage             : v2_external_minimum_wage.hourly_min_wage (都道府県値継承)
    land_price_proxy     : 該当ローカルソースなし -> 全件 NULL
    salary_real_terms_proxy : min_wage / (cost_index/100) を両方 NOT NULL の時のみ算出

NULL 許容: データ不足箇所は NULL のまま (非破壊)。

Hard NG 用語 (target_count / estimated_population / 推定人数 等) は本テーブル
スキーマ・関数名・文字列のいずれにも含まない。本テーブルは workplace measured
でも resident estimated_beta でもない、参考統計 (data_label='reference') 専用。

CLI:
    python scripts/build_municipality_living_cost_proxy.py --dry-run
    python scripts/build_municipality_living_cost_proxy.py --apply
    python scripts/build_municipality_living_cost_proxy.py --verify
"""
from __future__ import annotations

import argparse
import sqlite3
import sys
from pathlib import Path

try:
    sys.stdout.reconfigure(encoding="utf-8")
except (AttributeError, ValueError):
    pass


SCRIPT_DIR = Path(__file__).resolve().parent
DEPLOY_ROOT = SCRIPT_DIR.parent
LOCAL_DB = DEPLOY_ROOT / "data" / "hellowork.db"

TABLE = "municipality_living_cost_proxy"
SOURCE_NAME = "prefecture_price_index+min_wage_v1"
SOURCE_YEAR = 2025  # min_wage 2025-10-01 基準


DDL = f"""
CREATE TABLE IF NOT EXISTS {TABLE} (
    municipality_code TEXT NOT NULL,
    prefecture        TEXT NOT NULL,
    municipality_name TEXT NOT NULL,
    basis             TEXT NOT NULL CHECK (basis IN ('reference')),
    cost_index        REAL,
    min_wage          INTEGER,
    land_price_proxy  REAL,
    salary_real_terms_proxy REAL,
    data_label        TEXT NOT NULL CHECK (data_label IN ('reference')),
    source_name       TEXT NOT NULL,
    source_year       INTEGER NOT NULL,
    weight_source     TEXT,
    estimated_at      TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (municipality_code, basis, source_year)
);
"""

INDEX_SQLS = [
    f"CREATE INDEX IF NOT EXISTS idx_mlcp_pref ON {TABLE} (prefecture, municipality_name);",
    f"CREATE INDEX IF NOT EXISTS idx_mlcp_cost_index ON {TABLE} (cost_index);",
    f"CREATE INDEX IF NOT EXISTS idx_mlcp_source ON {TABLE} (source_name, source_year);",
]


def fetch_pref_price_index(conn: sqlite3.Connection) -> dict[str, float]:
    """都道府県別 price_index (=100 が全国平均) を辞書で返す。"""
    rows = conn.execute(
        "SELECT prefecture, price_index FROM v2_external_prefecture_stats"
    ).fetchall()
    return {p: float(v) for p, v in rows if v is not None}


def fetch_pref_min_wage(conn: sqlite3.Connection) -> dict[str, int]:
    """都道府県別 最低賃金 (円/時)。"""
    rows = conn.execute(
        "SELECT prefecture, hourly_min_wage FROM v2_external_minimum_wage"
    ).fetchall()
    return {p: int(v) for p, v in rows if v is not None}


def fetch_master(conn: sqlite3.Connection) -> list[tuple[str, str, str]]:
    """master 全 1,917 件 (municipality_code, prefecture, municipality_name)。"""
    rows = conn.execute(
        "SELECT municipality_code, prefecture, municipality_name "
        "FROM municipality_code_master ORDER BY municipality_code"
    ).fetchall()
    return [(r[0], r[1], r[2]) for r in rows]


def build_records(conn: sqlite3.Connection) -> list[tuple]:
    pref_cost = fetch_pref_price_index(conn)
    pref_wage = fetch_pref_min_wage(conn)
    master = fetch_master(conn)

    records: list[tuple] = []
    for code, pref, name in master:
        cost_index = pref_cost.get(pref)
        min_wage = pref_wage.get(pref)
        land_price_proxy = None  # local source 無し
        if cost_index is not None and min_wage is not None and cost_index > 0:
            salary_real = round(min_wage / (cost_index / 100.0), 2)
        else:
            salary_real = None
        records.append(
            (
                code,
                pref,
                name,
                "reference",          # basis
                cost_index,
                min_wage,
                land_price_proxy,
                salary_real,
                "reference",          # data_label
                SOURCE_NAME,
                SOURCE_YEAR,
                "official",           # weight_source (実測ソース継承)
            )
        )
    return records


def apply_to_local(conn: sqlite3.Connection, records: list[tuple]) -> int:
    cur = conn.cursor()
    cur.execute(f"DROP TABLE IF EXISTS {TABLE}")
    cur.executescript(DDL)
    for idx_sql in INDEX_SQLS:
        cur.execute(idx_sql)
    insert_sql = (
        f"INSERT INTO {TABLE} ("
        "municipality_code, prefecture, municipality_name, basis, "
        "cost_index, min_wage, land_price_proxy, salary_real_terms_proxy, "
        "data_label, source_name, source_year, weight_source"
        ") VALUES (?,?,?,?,?,?,?,?,?,?,?,?)"
    )
    cur.executemany(insert_sql, records)
    conn.commit()
    return cur.rowcount if cur.rowcount >= 0 else len(records)


def verify(conn: sqlite3.Connection) -> None:
    print("=" * 70)
    print(f"[verify] {TABLE}")
    print("=" * 70)
    cur = conn.cursor()
    fails = 0

    # 1. 行数
    n = cur.execute(f"SELECT COUNT(*) FROM {TABLE}").fetchone()[0]
    ok1 = abs(n - 1917) <= 100
    print(f"  1. row count       : {n:,}  (expect ~1917) {'OK' if ok1 else 'FAIL'}")
    if not ok1:
        fails += 1

    # 2. PK 重複
    dup = cur.execute(
        f"SELECT COUNT(*) FROM (SELECT municipality_code, basis, source_year, "
        f"COUNT(*) c FROM {TABLE} GROUP BY 1,2,3 HAVING c>1)"
    ).fetchone()[0]
    print(f"  2. PK duplicates   : {dup}  {'OK' if dup == 0 else 'FAIL'}")
    if dup:
        fails += 1

    # 3. 都道府県数
    pref_n = cur.execute(
        f"SELECT COUNT(DISTINCT prefecture) FROM {TABLE}"
    ).fetchone()[0]
    print(f"  3. prefectures     : {pref_n}  (expect 47) {'OK' if pref_n == 47 else 'FAIL'}")
    if pref_n != 47:
        fails += 1

    # 4. master orphan
    orphan = cur.execute(
        f"SELECT COUNT(*) FROM {TABLE} t "
        f"LEFT JOIN municipality_code_master m ON t.municipality_code = m.municipality_code "
        f"WHERE m.municipality_code IS NULL"
    ).fetchone()[0]
    print(f"  4. master orphan   : {orphan}  {'OK' if orphan == 0 else 'WARN'}")
    if orphan > 5:
        fails += 1

    # 5. cost_index 妥当性 (NULL 除外、50 < val < 200)
    bad_cost = cur.execute(
        f"SELECT COUNT(*) FROM {TABLE} "
        f"WHERE cost_index IS NOT NULL AND (cost_index < 50 OR cost_index > 200)"
    ).fetchone()[0]
    print(f"  5. cost_index range: bad={bad_cost}  {'OK' if bad_cost == 0 else 'FAIL'}")
    if bad_cost:
        fails += 1

    # 6. min_wage 妥当性 (700-1500 円)
    bad_wage = cur.execute(
        f"SELECT COUNT(*) FROM {TABLE} "
        f"WHERE min_wage IS NOT NULL AND (min_wage < 700 OR min_wage > 1500)"
    ).fetchone()[0]
    print(f"  6. min_wage range  : bad={bad_wage}  {'OK' if bad_wage == 0 else 'FAIL'}")
    if bad_wage:
        fails += 1

    # 7. land_price_proxy 妥当性 (NULL のみ許容、もし値があれば 0-10000)
    bad_land = cur.execute(
        f"SELECT COUNT(*) FROM {TABLE} "
        f"WHERE land_price_proxy IS NOT NULL AND (land_price_proxy < 0 OR land_price_proxy > 10000)"
    ).fetchone()[0]
    null_land = cur.execute(
        f"SELECT COUNT(*) FROM {TABLE} WHERE land_price_proxy IS NULL"
    ).fetchone()[0]
    print(f"  7. land_price_proxy: bad={bad_land} null={null_land}  {'OK' if bad_land == 0 else 'FAIL'}")
    if bad_land:
        fails += 1

    # 8. data_label と source_name 統一
    bad_label = cur.execute(
        f"SELECT COUNT(*) FROM {TABLE} WHERE data_label != 'reference'"
    ).fetchone()[0]
    src_n = cur.execute(
        f"SELECT COUNT(DISTINCT source_name) FROM {TABLE}"
    ).fetchone()[0]
    label_ok = (bad_label == 0) and (src_n == 1)
    print(f"  8. label/source    : non_ref={bad_label} distinct_source={src_n}  {'OK' if label_ok else 'FAIL'}")
    if not label_ok:
        fails += 1

    # 補足: NULL 件数サマリ
    null_cost = cur.execute(f"SELECT COUNT(*) FROM {TABLE} WHERE cost_index IS NULL").fetchone()[0]
    null_wage = cur.execute(f"SELECT COUNT(*) FROM {TABLE} WHERE min_wage IS NULL").fetchone()[0]
    null_real = cur.execute(f"SELECT COUNT(*) FROM {TABLE} WHERE salary_real_terms_proxy IS NULL").fetchone()[0]
    print("-" * 70)
    print(f"  NULL summary: cost_index={null_cost}  min_wage={null_wage}  "
          f"land_price_proxy={null_land}  salary_real_terms_proxy={null_real}")

    if fails:
        raise SystemExit(f"[abort] verify failed: {fails} check(s)")
    print("  ALL 8 CHECKS PASS")


def parse_args() -> argparse.Namespace:
    p = argparse.ArgumentParser(description="Build municipality_living_cost_proxy")
    mode = p.add_mutually_exclusive_group(required=True)
    mode.add_argument("--dry-run", action="store_true")
    mode.add_argument("--apply", action="store_true")
    mode.add_argument("--verify", action="store_true")
    return p.parse_args()


def main() -> None:
    args = parse_args()
    if not LOCAL_DB.exists():
        raise SystemExit(f"[abort] local DB not found: {LOCAL_DB}")

    if args.verify:
        conn = sqlite3.connect(f"file:{LOCAL_DB}?mode=ro", uri=True)
        verify(conn)
        return

    conn = sqlite3.connect(LOCAL_DB)
    records = build_records(conn)
    print(f"  built {len(records):,} records from master + pref sources")

    if args.dry_run:
        # サンプル 3 行
        for r in records[:3]:
            print("   sample:", r)
        non_null_cost = sum(1 for r in records if r[4] is not None)
        non_null_wage = sum(1 for r in records if r[5] is not None)
        non_null_real = sum(1 for r in records if r[7] is not None)
        print(f"  cost_index NOT NULL: {non_null_cost:,}")
        print(f"  min_wage   NOT NULL: {non_null_wage:,}")
        print(f"  salary_real NOT NULL: {non_null_real:,}")
        print("  [dry-run] no DB write")
        return

    if args.apply:
        n = apply_to_local(conn, records)
        print(f"  applied {n:,} rows to local DB")
        verify(sqlite3.connect(f"file:{LOCAL_DB}?mode=ro", uri=True))


if __name__ == "__main__":
    main()
