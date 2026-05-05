# -*- coding: utf-8 -*-
"""
ingest_estat_15_1_to_local.py
==============================

15-1 clean CSV を `municipality_occupation_population` (Plan B DDL) に投入。

JOIN 制約: master_by_code.area_level='unit' のみ投入。
- 投入対象: municipality / designated_ward / special_ward (1,896 unit)
- 除外: aggregate_city (20) + aggregate_special_wards (1) = 21 集約

固定値: basis='workplace', data_label='measured', source_name='census_15_1', source_year=2020

CLI:
  python scripts/ingest_estat_15_1_to_local.py --dry-run
  python scripts/ingest_estat_15_1_to_local.py --apply
  python scripts/ingest_estat_15_1_to_local.py --verify-only
"""
from __future__ import annotations

import argparse
import sqlite3
import sys
from datetime import datetime, timezone
from pathlib import Path

try:
    sys.stdout.reconfigure(encoding="utf-8")
except (AttributeError, ValueError):
    pass

CSV_PATH = Path("data/generated/estat_15_1_merged.csv")
DB_PATH = Path("data/hellowork.db")
TABLE = "municipality_occupation_population"

# Plan B DDL (Worker B5 が schema.sql に反映済み、ここに再掲)
DDL_CREATE = """
CREATE TABLE IF NOT EXISTS municipality_occupation_population (
    municipality_code TEXT NOT NULL,
    prefecture        TEXT NOT NULL,
    municipality_name TEXT NOT NULL,
    basis             TEXT NOT NULL CHECK (basis IN ('workplace','resident')),
    occupation_code   TEXT NOT NULL,
    occupation_name   TEXT NOT NULL,
    age_class         TEXT NOT NULL,
    gender            TEXT NOT NULL CHECK (gender IN ('male','female','total')),
    population        INTEGER,
    estimate_index    REAL,
    data_label        TEXT NOT NULL CHECK (data_label IN ('measured','estimated_beta')),
    source_name       TEXT NOT NULL,
    source_year       INTEGER NOT NULL,
    weight_source     TEXT,
    estimated_at      TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (municipality_code, basis, occupation_code, age_class, gender, source_year, data_label),
    CHECK (
      (data_label = 'measured'        AND population IS NOT NULL AND estimate_index IS NULL) OR
      (data_label = 'estimated_beta'  AND population IS NULL     AND estimate_index IS NOT NULL)
    )
)
"""
DDL_INDEXES = [
    "CREATE INDEX IF NOT EXISTS idx_muni_occ_pop_pref   ON municipality_occupation_population (prefecture, municipality_name)",
    "CREATE INDEX IF NOT EXISTS idx_muni_occ_pop_basis  ON municipality_occupation_population (basis, occupation_code)",
    "CREATE INDEX IF NOT EXISTS idx_muni_occ_pop_label  ON municipality_occupation_population (data_label)",
    "CREATE INDEX IF NOT EXISTS idx_muni_occ_pop_source ON municipality_occupation_population (source_name, source_year)",
    "CREATE INDEX IF NOT EXISTS idx_muni_occ_pop_age    ON municipality_occupation_population (age_class)",
]


def load_master_unit_codes() -> tuple[set[str], set[str]]:
    """master から unit / aggregate コード集合を返す。"""
    with sqlite3.connect(f"file:{DB_PATH}?mode=ro", uri=True) as conn:
        rows = conn.execute(
            "SELECT municipality_code, area_level FROM municipality_code_master"
        ).fetchall()
    unit = {str(r[0]).zfill(5) for r in rows if r[1] == "unit"}
    agg = {str(r[0]).zfill(5) for r in rows if r[1] == "aggregate"}
    return unit, agg


def estimate(args) -> int:
    import pandas as pd

    unit_codes, agg_codes = load_master_unit_codes()
    print(f"[master] unit: {len(unit_codes):,}")
    print(f"[master] aggregate: {len(agg_codes):,}")

    df = pd.read_csv(CSV_PATH, dtype={"municipality_code": str})
    df["municipality_code"] = df["municipality_code"].astype(str).str.zfill(5)
    n_total = len(df)
    csv_codes = set(df["municipality_code"].unique())

    csv_in_aggregate = csv_codes & agg_codes
    csv_in_unit = csv_codes & unit_codes
    csv_orphan = csv_codes - unit_codes - agg_codes

    df_unit = df[df["municipality_code"].isin(unit_codes)]
    n_unit_rows = len(df_unit)
    n_excluded_rows = n_total - n_unit_rows

    print(f"[csv] total rows: {n_total:,}")
    print(f"[csv] codes in CSV: {len(csv_codes):,}")
    print(f"[csv] codes JOIN unit: {len(csv_in_unit):,}")
    print(f"[csv] codes JOIN aggregate (will be excluded): {len(csv_in_aggregate)}")
    print(f"  examples: {sorted(csv_in_aggregate)}")
    print(f"[csv] codes not in master (orphan): {len(csv_orphan)}")
    if csv_orphan:
        print(f"  examples: {sorted(csv_orphan)[:10]}")
    print()
    print(f"[ingest] rows to insert: {n_unit_rows:,}")
    print(f"[ingest] rows to exclude: {n_excluded_rows:,}")
    expected = len(csv_in_unit) * 374
    print(f"[ingest] expected = {len(csv_in_unit)} unit muni × 374 = {expected:,}")
    if expected == n_unit_rows:
        print(f"[ingest] ✅ MATCH")
    else:
        print(f"[ingest] ❌ MISMATCH (diff {n_unit_rows - expected:+,})")
    return 0


def apply(args) -> int:
    import pandas as pd

    unit_codes, agg_codes = load_master_unit_codes()
    df = pd.read_csv(CSV_PATH, dtype={"municipality_code": str})
    df["municipality_code"] = df["municipality_code"].astype(str).str.zfill(5)
    df_unit = df[df["municipality_code"].isin(unit_codes)].copy()

    # 固定値
    fetched_at = datetime.now(timezone.utc).isoformat()
    df_unit["basis"] = "workplace"
    df_unit["data_label"] = "measured"
    df_unit["source_name"] = "census_15_1"
    df_unit["source_year"] = 2020
    df_unit["estimate_index"] = None
    df_unit["weight_source"] = None
    df_unit["estimated_at"] = fetched_at

    insert_cols = [
        "municipality_code", "prefecture", "municipality_name",
        "basis", "occupation_code", "occupation_name", "age_class", "gender",
        "population", "estimate_index", "data_label",
        "source_name", "source_year", "weight_source", "estimated_at",
    ]
    df_insert = df_unit[insert_cols].copy()
    print(f"[apply] preparing {len(df_insert):,} rows")

    with sqlite3.connect(DB_PATH) as conn:
        conn.execute(DDL_CREATE)
        for ddl in DDL_INDEXES:
            conn.execute(ddl)

        # idempotent: 既存の census_15_1 行を削除
        cur = conn.execute(
            f"DELETE FROM {TABLE} WHERE source_name=? AND source_year=?",
            ("census_15_1", 2020),
        )
        deleted = cur.rowcount
        print(f"[apply] DELETE existing census_15_1 rows: {deleted:,}")

        # bulk INSERT
        df_insert.to_sql(TABLE, conn, if_exists="append", index=False, chunksize=10000)
        conn.commit()

        n_after = conn.execute(
            f"SELECT COUNT(*) FROM {TABLE} WHERE source_name=? AND source_year=?",
            ("census_15_1", 2020),
        ).fetchone()[0]
        print(f"[apply] INSERT complete, total: {n_after:,}")

    return verify(args)


def verify(args) -> int:
    print("\n=== Post-ingest verification ===\n")
    overall = True

    with sqlite3.connect(f"file:{DB_PATH}?mode=ro", uri=True) as conn:
        c = conn.cursor()

        # [1] 行数
        n = c.execute(
            f"SELECT COUNT(*) FROM {TABLE} WHERE source_name='census_15_1' AND source_year=2020"
        ).fetchone()[0]
        n_unit = c.execute(
            "SELECT COUNT(*) FROM municipality_code_master WHERE area_level='unit'"
        ).fetchone()[0]
        expected = n_unit * 374
        ok1 = n == expected
        overall &= ok1
        print(f"[1] census_15_1 rows: {n:,} | unit×374 = {expected:,} | {'✅' if ok1 else '❌'}")

        # [2] aggregate=0
        n_agg = c.execute(f"""
            SELECT COUNT(*) FROM {TABLE} mop
            JOIN municipality_code_master mcm ON mop.municipality_code = mcm.municipality_code
            WHERE mop.source_name='census_15_1' AND mcm.area_level='aggregate'
        """).fetchone()[0]
        ok2 = n_agg == 0
        overall &= ok2
        print(f"[2] aggregate rows: {n_agg:,} (期待 0) | {'✅' if ok2 else '❌'}")

        # [3] PK 重複
        dup = c.execute(f"""
            SELECT COUNT(*) FROM (
              SELECT 1 FROM {TABLE}
              WHERE source_name='census_15_1'
              GROUP BY municipality_code, basis, occupation_code, age_class, gender, source_year, data_label
              HAVING COUNT(*) > 1
            )
        """).fetchone()[0]
        ok3 = dup == 0
        overall &= ok3
        print(f"[3] PK duplicates: {dup:,} | {'✅' if ok3 else '❌'}")

        # [4] master 未登録
        orphan = c.execute(f"""
            SELECT COUNT(*) FROM {TABLE} mop
            LEFT JOIN municipality_code_master mcm ON mop.municipality_code = mcm.municipality_code
            WHERE mop.source_name='census_15_1' AND mcm.municipality_code IS NULL
        """).fetchone()[0]
        ok4 = orphan == 0
        overall &= ok4
        print(f"[4] master orphan: {orphan:,} | {'✅' if ok4 else '❌'}")

        # [5] distinct
        n_g = c.execute(f"SELECT COUNT(DISTINCT gender) FROM {TABLE} WHERE source_name='census_15_1'").fetchone()[0]
        n_a = c.execute(f"SELECT COUNT(DISTINCT age_class) FROM {TABLE} WHERE source_name='census_15_1'").fetchone()[0]
        n_o = c.execute(f"SELECT COUNT(DISTINCT occupation_code) FROM {TABLE} WHERE source_name='census_15_1'").fetchone()[0]
        ok5 = n_g == 2 and n_a == 17 and n_o == 11
        overall &= ok5
        print(f"[5] distinct gender={n_g}/2, age={n_a}/17, occ={n_o}/11 | {'✅' if ok5 else '❌'}")

        # [6] サンプル unit (各 374)
        ok6_all = True
        for code, name in [
            ("13103", "港区"), ("13104", "新宿区"), ("13201", "八王子市"),
            ("23211", "豊田市"), ("01101", "札幌市中央区"),
        ]:
            cnt = c.execute(
                f"SELECT COUNT(*) FROM {TABLE} WHERE source_name='census_15_1' AND municipality_code=?",
                (code,)
            ).fetchone()[0]
            ok = cnt == 374
            ok6_all &= ok
            print(f"[6] {code} ({name}): {cnt} (期待 374) | {'✅' if ok else '❌'}")
        overall &= ok6_all

        # [7] aggregate サンプル (各 0)
        ok7_all = True
        for code, name in [
            ("13100", "特別区部"), ("01100", "札幌市"), ("14100", "横浜市"),
            ("27100", "大阪市"), ("22100", "静岡市"),
        ]:
            cnt = c.execute(
                f"SELECT COUNT(*) FROM {TABLE} WHERE source_name='census_15_1' AND municipality_code=?",
                (code,)
            ).fetchone()[0]
            ok = cnt == 0
            ok7_all &= ok
            print(f"[7] {code} ({name}): {cnt} (期待 0) | {'✅' if ok else '❌'}")
        overall &= ok7_all

        # [8] basis/data_label/source/year 分布
        rows = c.execute(f"""
            SELECT basis, data_label, source_name, source_year, COUNT(*)
            FROM {TABLE}
            WHERE source_name='census_15_1'
            GROUP BY basis, data_label, source_name, source_year
        """).fetchall()
        print(f"[8] distribution:")
        ok8 = (len(rows) == 1 and rows[0][:4] == ("workplace", "measured", "census_15_1", 2020))
        for r in rows:
            print(f"    basis={r[0]}, label={r[1]}, source={r[2]}, year={r[3]}, count={r[4]:,}")
        overall &= ok8
        print(f"     | {'✅' if ok8 else '❌'}")

    print()
    print(f"=== Overall: {'✅ PASS' if overall else '❌ FAIL'} ===")
    return 0 if overall else 1


def main() -> int:
    parser = argparse.ArgumentParser()
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--dry-run", action="store_true", help="estimate only, no INSERT")
    mode.add_argument("--apply", action="store_true", help="run CREATE + DELETE + INSERT + verify")
    mode.add_argument("--verify-only", action="store_true", help="run verification only")
    args = parser.parse_args()

    if args.dry_run:
        return estimate(args)
    if args.apply:
        return apply(args)
    if args.verify_only:
        return verify(args)
    return 1


if __name__ == "__main__":
    sys.exit(main())
