# -*- coding: utf-8 -*-
"""
ingest_v2_thickness_to_local.py
================================

F2 thickness CSV の派生指標 (rank/priority/scenario) を
`v2_municipality_target_thickness` に投入。

入力 CSV カラム → DDL 列マッピング:
  estimate_index               -> thickness_index
  rank_in_occupation           -> 同
  rank_percentile              -> 同
  distribution_priority        -> 同
  scenario_conservative_index  -> 同
  scenario_standard_index      -> 同
  scenario_aggressive_index    -> 同
  is_industrial_anchor         -> 同
  (CSV) basis='resident'       -> basis (固定)
  (固定) estimate_grade='A-'
  (固定) weight_source='hypothesis_v1'
  (固定) source_year=2020

注: DDL `v2_municipality_target_thickness` には source_name 列がない。
F2 派生集約専用テーブルなので、識別は basis + weight_source + estimate_grade で行う。

CLI:
  --dry-run / --apply / --verify-only

rollback:
  WHERE basis='resident' AND weight_source='hypothesis_v1' AND source_year=2020
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

CSV_PATH = Path("data/generated/v2_municipality_target_thickness.csv")
DB_PATH = Path("data/hellowork.db")
TABLE = "v2_municipality_target_thickness"
MOP_TABLE = "municipality_occupation_population"

# Plan B DDL (Worker B5 schema.sql)
DDL_CREATE = """
CREATE TABLE IF NOT EXISTS v2_municipality_target_thickness (
    municipality_code TEXT NOT NULL,
    prefecture        TEXT NOT NULL,
    municipality_name TEXT NOT NULL,
    basis             TEXT NOT NULL,
    occupation_code   TEXT NOT NULL,
    occupation_name   TEXT NOT NULL,
    thickness_index   REAL NOT NULL,
    rank_in_occupation INTEGER,
    rank_percentile   REAL,
    distribution_priority TEXT,
    scenario_conservative_index INTEGER,
    scenario_standard_index INTEGER,
    scenario_aggressive_index INTEGER,
    estimate_grade    TEXT,
    weight_source     TEXT NOT NULL,
    is_industrial_anchor INTEGER NOT NULL DEFAULT 0,
    source_year       INTEGER NOT NULL,
    estimated_at      TEXT NOT NULL,
    PRIMARY KEY (municipality_code, basis, occupation_code, source_year)
)
"""
DDL_INDEXES = [
    "CREATE INDEX IF NOT EXISTS idx_v2_muni_target_thick_idx ON v2_municipality_target_thickness (occupation_code, thickness_index DESC)",
    "CREATE INDEX IF NOT EXISTS idx_v2_muni_target_rank ON v2_municipality_target_thickness (occupation_code, rank_in_occupation)",
    "CREATE INDEX IF NOT EXISTS idx_v2_muni_target_pref ON v2_municipality_target_thickness (prefecture, occupation_code)",
]

# 固定値
FIXED_BASIS = "resident"
FIXED_WEIGHT_SOURCE = "hypothesis_v1"
FIXED_SOURCE_YEAR = 2020
FIXED_ESTIMATE_GRADE = "A-"


def load_master_codes() -> tuple[set[str], set[str]]:
    with sqlite3.connect(f"file:{DB_PATH}?mode=ro", uri=True) as conn:
        rows = conn.execute(
            "SELECT municipality_code, area_level FROM municipality_code_master"
        ).fetchall()
    unit = {str(r[0]).zfill(5) for r in rows if r[1] == "unit"}
    agg = {str(r[0]).zfill(5) for r in rows if r[1] == "aggregate"}
    return unit, agg


def estimate(args) -> int:
    import pandas as pd

    unit_codes, agg_codes = load_master_codes()
    print(f"[master] unit: {len(unit_codes):,} / aggregate: {len(agg_codes):,}")

    df = pd.read_csv(CSV_PATH, dtype={"municipality_code": str})
    df["municipality_code"] = df["municipality_code"].astype(str).str.zfill(5)
    n_total = len(df)
    csv_codes = set(df["municipality_code"].unique())

    csv_in_unit = csv_codes & unit_codes
    csv_in_agg = csv_codes & agg_codes

    df_unit = df[df["municipality_code"].isin(unit_codes)]
    n_unit_rows = len(df_unit)

    print(f"[csv] total rows: {n_total:,}")
    print(f"[csv] codes JOIN unit: {len(csv_in_unit):,}")
    print(f"[csv] codes JOIN aggregate (excluded): {len(csv_in_agg)}")
    print()
    print(f"[ingest] rows to insert: {n_unit_rows:,}")
    expected = len(csv_in_unit) * 11
    print(f"[ingest] expected = {len(csv_in_unit)} unit muni × 11 occ = {expected:,}")
    print(f"[ingest] match: {'✅' if expected == n_unit_rows else '❌ ' + str(n_unit_rows - expected)}")

    # 必須 CSV 列
    print()
    print("[csv] required columns check:")
    required = [
        "municipality_code", "occupation_code", "estimate_index",
        "rank_in_occupation", "rank_percentile", "distribution_priority",
        "scenario_conservative_index", "scenario_standard_index", "scenario_aggressive_index",
        "is_industrial_anchor",
    ]
    for col in required:
        ok = col in df.columns
        print(f"  {col}: {'✅' if ok else '❌ MISSING'}")

    # mop 既存件数 (touch しないこと確認用)
    with sqlite3.connect(f"file:{DB_PATH}?mode=ro", uri=True) as conn:
        n_mop = conn.execute(f"SELECT COUNT(*) FROM {MOP_TABLE}").fetchone()[0]
        n_existing = conn.execute(
            f"SELECT COUNT(*) FROM sqlite_master WHERE name=?", (TABLE,)
        ).fetchone()[0]
        if n_existing:
            n_v2 = conn.execute(
                f"SELECT COUNT(*) FROM {TABLE} WHERE basis=? AND weight_source=? AND source_year=?",
                (FIXED_BASIS, FIXED_WEIGHT_SOURCE, FIXED_SOURCE_YEAR),
            ).fetchone()[0]
        else:
            n_v2 = "(table not yet created)"
    print()
    print(f"[db] mop rows (workplace/measured must remain at 709,104): {n_mop:,}")
    print(f"[db] existing v2_thickness F2 rows (will be replaced): {n_v2}")
    return 0


def apply(args) -> int:
    import pandas as pd

    unit_codes, _ = load_master_codes()
    df = pd.read_csv(CSV_PATH, dtype={"municipality_code": str})
    df["municipality_code"] = df["municipality_code"].astype(str).str.zfill(5)
    df_unit = df[df["municipality_code"].isin(unit_codes)].copy()
    print(f"[apply] preparing {len(df_unit):,} rows")

    # CSV → DDL マッピング (estimate_index -> thickness_index リネーム)
    df_unit = df_unit.rename(columns={"estimate_index": "thickness_index"})

    # 固定値
    fetched_at = datetime.now(timezone.utc).isoformat()
    df_unit["basis"] = FIXED_BASIS
    df_unit["weight_source"] = FIXED_WEIGHT_SOURCE
    df_unit["source_year"] = FIXED_SOURCE_YEAR
    df_unit["estimate_grade"] = FIXED_ESTIMATE_GRADE
    df_unit["estimated_at"] = fetched_at

    insert_cols = [
        "municipality_code", "prefecture", "municipality_name",
        "basis", "occupation_code", "occupation_name",
        "thickness_index", "rank_in_occupation", "rank_percentile", "distribution_priority",
        "scenario_conservative_index", "scenario_standard_index", "scenario_aggressive_index",
        "estimate_grade", "weight_source", "is_industrial_anchor",
        "source_year", "estimated_at",
    ]
    df_insert = df_unit[insert_cols].copy()

    with sqlite3.connect(DB_PATH) as conn:
        conn.execute(DDL_CREATE)
        for ddl in DDL_INDEXES:
            conn.execute(ddl)

        # mop touch しないこと確認 (動的に投入前後で比較、ハードコード値廃止)
        n_mop_before = conn.execute(f"SELECT COUNT(*) FROM {MOP_TABLE}").fetchone()[0]
        # workplace/measured の維持を最低限ガード
        n_workplace_before = conn.execute(
            f"SELECT COUNT(*) FROM {MOP_TABLE} WHERE basis='workplace' AND data_label='measured'"
        ).fetchone()[0]
        if n_workplace_before != 709_104:
            print(f"[apply] ❌ workplace/measured = {n_workplace_before:,} != 709,104, abort")
            return 1
        print(f"[apply] mop rows before: {n_mop_before:,} "
              f"(workplace/measured sentinel: {n_workplace_before:,} OK)")

        # rollback target
        cur = conn.execute(
            f"DELETE FROM {TABLE} WHERE basis=? AND weight_source=? AND source_year=?",
            (FIXED_BASIS, FIXED_WEIGHT_SOURCE, FIXED_SOURCE_YEAR),
        )
        deleted = cur.rowcount
        print(f"[apply] DELETE existing F2 rows: {deleted:,}")

        df_insert.to_sql(TABLE, conn, if_exists="append", index=False, chunksize=5000)
        conn.commit()

        n_after = conn.execute(f"SELECT COUNT(*) FROM {TABLE}").fetchone()[0]
        print(f"[apply] INSERT complete, total: {n_after:,}")

        # mop touch しないこと再確認 (投入前後の不変を確認、+ workplace 維持)
        n_mop_after = conn.execute(f"SELECT COUNT(*) FROM {MOP_TABLE}").fetchone()[0]
        n_workplace_after = conn.execute(
            f"SELECT COUNT(*) FROM {MOP_TABLE} WHERE basis='workplace' AND data_label='measured'"
        ).fetchone()[0]
        if n_mop_after != n_mop_before:
            print(f"[apply] ❌ mop total changed: {n_mop_before:,} -> {n_mop_after:,}")
            return 1
        if n_workplace_after != 709_104:
            print(f"[apply] ❌ workplace/measured changed to {n_workplace_after:,}")
            return 1

    return verify(args)


def verify(args) -> int:
    print("\n=== Post-ingest verification (v2_thickness) ===\n")
    overall = True

    with sqlite3.connect(f"file:{DB_PATH}?mode=ro", uri=True) as conn:
        c = conn.cursor()

        # [1] 行数
        n = c.execute(f"SELECT COUNT(*) FROM {TABLE}").fetchone()[0]
        n_unique_muni = c.execute(f"SELECT COUNT(DISTINCT municipality_code) FROM {TABLE}").fetchone()[0]
        expected = n_unique_muni * 11
        ok1 = n == expected
        overall &= ok1
        print(f"[1] rows: {n:,} = {n_unique_muni} muni × 11 = {expected:,} | {'✅' if ok1 else '❌'}")

        # [2] aggregate=0
        n_agg = c.execute(f"""
            SELECT COUNT(*) FROM {TABLE} v
            JOIN municipality_code_master mcm ON v.municipality_code = mcm.municipality_code
            WHERE mcm.area_level='aggregate'
        """).fetchone()[0]
        ok2 = n_agg == 0
        overall &= ok2
        print(f"[2] aggregate rows: {n_agg:,} | {'✅' if ok2 else '❌'}")

        # [3] master orphan
        n_orphan = c.execute(f"""
            SELECT COUNT(*) FROM {TABLE} v
            LEFT JOIN municipality_code_master mcm ON v.municipality_code = mcm.municipality_code
            WHERE mcm.municipality_code IS NULL
        """).fetchone()[0]
        ok3 = n_orphan == 0
        overall &= ok3
        print(f"[3] master orphan: {n_orphan:,} | {'✅' if ok3 else '❌'}")

        # [4] occupation count
        n_occ = c.execute(f"SELECT COUNT(DISTINCT occupation_code) FROM {TABLE}").fetchone()[0]
        ok4 = n_occ == 11
        overall &= ok4
        print(f"[4] distinct occupations: {n_occ} (expect 11) | {'✅' if ok4 else '❌'}")

        # [5] municipality count
        ok5 = n_unique_muni == 1895  # 2026-05-06: 1720 + 175 designated_ward
        overall &= ok5
        print(f"[5] distinct municipalities: {n_unique_muni:,} (expect 1,895 = 1,720 + 175 designated_ward) | {'✅' if ok5 else '❌'}")

        # [6] rank uniqueness per occupation (occ + rank の組合せ重複なし)
        dup_rank = c.execute(f"""
            SELECT COUNT(*) FROM (
              SELECT 1 FROM {TABLE}
              GROUP BY occupation_code, rank_in_occupation
              HAVING COUNT(*) > 1
            )
        """).fetchone()[0]
        ok6 = dup_rank == 0
        overall &= ok6
        print(f"[6] rank duplicates per (occ, rank): {dup_rank:,} | {'✅' if ok6 else '❌'}")

        # [7] rank_percentile [0, 1]
        rp = c.execute(f"SELECT MIN(rank_percentile), MAX(rank_percentile) FROM {TABLE}").fetchone()
        ok7 = 0.0 <= rp[0] and rp[1] <= 1.0
        overall &= ok7
        print(f"[7] rank_percentile: [{rp[0]:.4f}, {rp[1]:.4f}] | {'✅' if ok7 else '❌'}")

        # [8] priority + scenario range
        priorities = {r[0] for r in c.execute(f"SELECT DISTINCT distribution_priority FROM {TABLE}").fetchall()}
        ok8a = priorities <= {"S", "A", "B", "C", "D"} and priorities >= {"A", "B", "C", "D"}
        scn = c.execute(f"""
            SELECT MIN(scenario_conservative_index), MAX(scenario_conservative_index),
                   MIN(scenario_standard_index), MAX(scenario_standard_index),
                   MIN(scenario_aggressive_index), MAX(scenario_aggressive_index)
            FROM {TABLE}
        """).fetchone()
        ok8b = all(v >= 0 for v in scn)
        ok8 = ok8a and ok8b
        overall &= ok8
        print(f"[8] priority distinct: {sorted(priorities)} | scenario range cons[{scn[0]}-{scn[1]}] std[{scn[2]}-{scn[3]}] agg[{scn[4]}-{scn[5]}] | {'✅' if ok8 else '❌'}")

        # [9] cons <= std <= agg
        bad = c.execute(f"""
            SELECT COUNT(*) FROM {TABLE}
            WHERE scenario_conservative_index > scenario_standard_index
               OR scenario_standard_index > scenario_aggressive_index
        """).fetchone()[0]
        ok9 = bad == 0
        overall &= ok9
        print(f"[9] cons<=std<=agg violations: {bad:,} | {'✅' if ok9 else '❌'}")

        # [10a] unit + F2 CSV 収録自治体 → 11 行期待
        ok10a_all = True
        print("[10a] sample units present in F2 CSV (expect 11 rows each):")
        for code, name in [
            ("13103", "港区 (special_ward)"),
            ("13104", "新宿区 (special_ward)"),
            ("13201", "八王子市 (municipality)"),
            ("23211", "豊田市 (municipality)"),
            ("01202", "函館市 (municipality)"),
            ("12203", "船橋市 (municipality)"),
        ]:
            cnt = c.execute(
                f"SELECT COUNT(*) FROM {TABLE} WHERE municipality_code=?", (code,)
            ).fetchone()[0]
            ok = cnt == 11
            ok10a_all &= ok
            print(f"      {code} ({name}): {cnt} rows | {'✅' if ok else '❌'}")
        overall &= ok10a_all

        # [10b] 政令市本体 (aggregate_city) → Plan B 設計通り 0 行
        ok10b_all = True
        print("[10b] designated city aggregates (expect 0, excluded by Plan B):")
        for code, name in [
            ("14130", "川崎市本体"),
            ("27140", "堺市本体"),
            ("14100", "横浜市本体"),
            ("01100", "札幌市本体"),
        ]:
            cnt = c.execute(
                f"SELECT COUNT(*) FROM {TABLE} WHERE municipality_code=?", (code,)
            ).fetchone()[0]
            ok = cnt == 0
            ok10b_all &= ok
            print(f"      {code} ({name}): {cnt} rows | {'✅' if ok else '❌'}")
        overall &= ok10b_all

        # [10c] 政令市の区 (designated_ward unit) → F2 CSV 入力粒度制約により 0 行
        # NOTE: build_municipality_target_thickness.py の入力 v2_external_population に
        # 政令市の区別データがないため、F2 推定 CSV にも区別行が存在しない。
        # これは Plan B 投入の問題ではなく、F2 推定スクリプト側の入力データ制約。
        # 別タスクで解決予定 (例: 政令市の区別人口データを v2_external_population に追加)。
        print("[10c] designated wards: F2 input granularity gap (informational only)")
        for code, name in [
            ("14131", "川崎市川崎区"),
            ("27141", "堺市堺区"),
            ("14101", "横浜市鶴見区"),
        ]:
            cnt = c.execute(
                f"SELECT COUNT(*) FROM {TABLE} WHERE municipality_code=?", (code,)
            ).fetchone()[0]
            note = "F2 CSV gap" if cnt == 0 else "covered"
            print(f"      {code} ({name}): {cnt} rows ({note})")

        # [11] mop workplace/measured 維持 (resident 件数は別途 §2 で確認済)
        n_mop = c.execute(f"SELECT COUNT(*) FROM {MOP_TABLE}").fetchone()[0]
        n_workplace = c.execute(
            f"SELECT COUNT(*) FROM {MOP_TABLE} WHERE basis='workplace' AND data_label='measured'"
        ).fetchone()[0]
        ok11 = n_workplace == 709_104
        overall &= ok11
        print(f"[11] mop total: {n_mop:,} | workplace/measured: {n_workplace:,} "
              f"(expect 709,104) | {'✅' if ok11 else '❌'}")

    print()
    print(f"=== Overall: {'✅ PASS' if overall else '❌ FAIL'} ===")
    return 0 if overall else 1


def main() -> int:
    parser = argparse.ArgumentParser()
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--dry-run", action="store_true")
    mode.add_argument("--apply", action="store_true")
    mode.add_argument("--verify-only", action="store_true")
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
