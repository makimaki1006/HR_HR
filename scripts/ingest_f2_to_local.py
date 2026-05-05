# -*- coding: utf-8 -*-
"""
ingest_f2_to_local.py
======================

F2 推定 CSV (resident × estimated_beta) を `municipality_occupation_population` に投入。

実測 (workplace × measured × census_15_1) とは混同しないよう、
source_name='model_f2_target_thickness' で明示的に区別する。

人数 (population) は NULL、estimate_index (0-200) のみ書き込む。
派生指標 (rank/priority/scenario) は本テーブルには入れない (v2_municipality_target_thickness 側)。

CLI:
  python scripts/ingest_f2_to_local.py --dry-run
  python scripts/ingest_f2_to_local.py --apply
  python scripts/ingest_f2_to_local.py --verify-only
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
TABLE = "municipality_occupation_population"

# ユーザー指示の固定値 (実測と混同しない命名)
F2_SOURCE_NAME = "model_f2_target_thickness"
F2_SOURCE_YEAR = 2020
F2_WEIGHT_SOURCE = "hypothesis_v1"
F2_BASIS = "resident"
F2_DATA_LABEL = "estimated_beta"
F2_AGE_CLASS = "_total"
F2_GENDER = "total"


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
    csv_orphan = csv_codes - unit_codes - agg_codes

    df_unit = df[df["municipality_code"].isin(unit_codes)]
    n_unit_rows = len(df_unit)

    print(f"[csv] total rows: {n_total:,}")
    print(f"[csv] distinct codes: {len(csv_codes):,}")
    print(f"[csv] codes JOIN unit: {len(csv_in_unit):,}")
    print(f"[csv] codes JOIN aggregate (excluded): {len(csv_in_agg)} -> {sorted(csv_in_agg)[:10]}")
    print(f"[csv] codes orphan: {len(csv_orphan)} -> {sorted(csv_orphan)[:5]}")
    print()
    print(f"[ingest] rows to insert: {n_unit_rows:,}")
    expected = len(csv_in_unit) * 11
    print(f"[ingest] expected = {len(csv_in_unit)} unit muni × 11 occ = {expected:,}")
    print(f"[ingest] match: {'✅' if expected == n_unit_rows else '❌ ' + str(n_unit_rows - expected)}")

    # Plan B 制約
    print()
    print("[csv] Plan B constraint check (input):")
    for col, expected_val in [
        ("basis", F2_BASIS), ("data_label", F2_DATA_LABEL),
        ("age_class", F2_AGE_CLASS), ("gender", F2_GENDER),
    ]:
        ok = (df_unit[col] == expected_val).all()
        print(f"  {col} == {expected_val!r}: {'✅' if ok else '❌'}")

    # estimate_index range
    idx_min = df_unit["estimate_index"].min()
    idx_max = df_unit["estimate_index"].max()
    print(f"  estimate_index range: [{idx_min:.2f}, {idx_max:.2f}] (expect [0, 200])")

    # 既存 mop 行数
    with sqlite3.connect(f"file:{DB_PATH}?mode=ro", uri=True) as conn:
        n_workplace = conn.execute(
            f"SELECT COUNT(*) FROM {TABLE} WHERE basis='workplace' AND data_label='measured'"
        ).fetchone()[0]
        n_existing_f2 = conn.execute(
            f"SELECT COUNT(*) FROM {TABLE} WHERE basis=? AND data_label=? AND source_name=?",
            (F2_BASIS, F2_DATA_LABEL, F2_SOURCE_NAME),
        ).fetchone()[0]
    print()
    print(f"[db] existing workplace/measured rows (must NOT be deleted): {n_workplace:,}")
    print(f"[db] existing F2 rows (will be replaced): {n_existing_f2:,}")
    return 0


def apply(args) -> int:
    import pandas as pd

    unit_codes, _ = load_master_codes()
    df = pd.read_csv(CSV_PATH, dtype={"municipality_code": str})
    df["municipality_code"] = df["municipality_code"].astype(str).str.zfill(5)

    # JOIN unit only
    df_unit = df[df["municipality_code"].isin(unit_codes)].copy()
    print(f"[apply] preparing {len(df_unit):,} rows")

    # ユーザー指示の固定値で上書き
    fetched_at = datetime.now(timezone.utc).isoformat()
    df_unit["basis"] = F2_BASIS
    df_unit["data_label"] = F2_DATA_LABEL
    df_unit["age_class"] = F2_AGE_CLASS
    df_unit["gender"] = F2_GENDER
    df_unit["source_name"] = F2_SOURCE_NAME
    df_unit["source_year"] = F2_SOURCE_YEAR
    df_unit["weight_source"] = F2_WEIGHT_SOURCE
    df_unit["population"] = None  # estimated_beta は人数 NULL
    df_unit["estimated_at"] = fetched_at

    insert_cols = [
        "municipality_code", "prefecture", "municipality_name",
        "basis", "occupation_code", "occupation_name", "age_class", "gender",
        "population", "estimate_index", "data_label",
        "source_name", "source_year", "weight_source", "estimated_at",
    ]
    df_insert = df_unit[insert_cols].copy()

    with sqlite3.connect(DB_PATH) as conn:
        # rollback 条件: basis=resident AND data_label=estimated_beta AND source_name=model_f2_target_thickness
        cur = conn.execute(
            f"DELETE FROM {TABLE} WHERE basis=? AND data_label=? AND source_name=?",
            (F2_BASIS, F2_DATA_LABEL, F2_SOURCE_NAME),
        )
        deleted = cur.rowcount
        print(f"[apply] DELETE existing F2 rows: {deleted:,}")

        # workplace/measured が消えていないことを確認
        n_workplace_before = conn.execute(
            f"SELECT COUNT(*) FROM {TABLE} WHERE basis='workplace' AND data_label='measured'"
        ).fetchone()[0]
        print(f"[apply] workplace/measured before INSERT: {n_workplace_before:,} (must remain)")
        if n_workplace_before != 709_104:
            print(f"[apply] ❌ workplace/measured count changed unexpectedly")
            conn.rollback()
            return 1

        df_insert.to_sql(TABLE, conn, if_exists="append", index=False, chunksize=5000)
        conn.commit()

        n_after = conn.execute(
            f"SELECT COUNT(*) FROM {TABLE} WHERE basis=? AND data_label=? AND source_name=?",
            (F2_BASIS, F2_DATA_LABEL, F2_SOURCE_NAME),
        ).fetchone()[0]
        print(f"[apply] INSERT complete, F2 rows: {n_after:,}")

    return verify(args)


def verify(args) -> int:
    print("\n=== Post-ingest verification (F2) ===\n")
    overall = True

    with sqlite3.connect(f"file:{DB_PATH}?mode=ro", uri=True) as conn:
        c = conn.cursor()

        # [1] workplace/measured 維持
        n_w = c.execute(
            f"SELECT COUNT(*) FROM {TABLE} WHERE basis='workplace' AND data_label='measured'"
        ).fetchone()[0]
        ok1 = n_w == 709_104
        overall &= ok1
        print(f"[1] workplace/measured rows: {n_w:,} (expect 709,104) | {'✅' if ok1 else '❌'}")

        # [2] resident/estimated_beta 行数
        n_r = c.execute(
            f"SELECT COUNT(*) FROM {TABLE} WHERE basis='resident' AND data_label='estimated_beta' AND source_name=?",
            (F2_SOURCE_NAME,),
        ).fetchone()[0]
        # 期待: 1,740 (CSV) ∩ 1,896 (master unit) × 11
        # CSV の muni 1,740 のうち master unit に該当する数を計算
        n_unique_muni = c.execute(
            f"SELECT COUNT(DISTINCT municipality_code) FROM {TABLE} WHERE basis='resident' AND source_name=?",
            (F2_SOURCE_NAME,),
        ).fetchone()[0]
        expected_r = n_unique_muni * 11
        ok2 = n_r == expected_r
        overall &= ok2
        print(f"[2] resident/estimated_beta rows: {n_r:,} = {n_unique_muni} muni × 11 = {expected_r:,} | {'✅' if ok2 else '❌'}")

        # [3] basis/data_label/source_name 分布
        rows = c.execute(f"""
            SELECT basis, data_label, source_name, source_year, COUNT(*)
            FROM {TABLE}
            GROUP BY basis, data_label, source_name, source_year
            ORDER BY basis, data_label
        """).fetchall()
        print(f"[3] full distribution:")
        for r in rows:
            print(f"    basis={r[0]}, label={r[1]}, source={r[2]}, year={r[3]}, count={r[4]:,}")
        # 期待: 2 行のみ (workplace/measured/census_15_1/2020 + resident/estimated_beta/model_f2_target_thickness/2020)
        ok3 = (
            len(rows) == 2
            and ("workplace", "measured", "census_15_1", 2020) in [(r[0], r[1], r[2], r[3]) for r in rows]
            and ("resident", "estimated_beta", F2_SOURCE_NAME, F2_SOURCE_YEAR) in [(r[0], r[1], r[2], r[3]) for r in rows]
        )
        overall &= ok3
        print(f"     | {'✅' if ok3 else '❌'}")

        # [4] aggregate 行数 (F2 の中)
        n_agg = c.execute(f"""
            SELECT COUNT(*) FROM {TABLE} mop
            JOIN municipality_code_master mcm ON mop.municipality_code = mcm.municipality_code
            WHERE mop.source_name=? AND mcm.area_level='aggregate'
        """, (F2_SOURCE_NAME,)).fetchone()[0]
        ok4 = n_agg == 0
        overall &= ok4
        print(f"[4] F2 aggregate rows: {n_agg:,} (expect 0) | {'✅' if ok4 else '❌'}")

        # [5] master orphan
        orphan = c.execute(f"""
            SELECT COUNT(*) FROM {TABLE} mop
            LEFT JOIN municipality_code_master mcm ON mop.municipality_code = mcm.municipality_code
            WHERE mop.source_name=? AND mcm.municipality_code IS NULL
        """, (F2_SOURCE_NAME,)).fetchone()[0]
        ok5 = orphan == 0
        overall &= ok5
        print(f"[5] master orphan: {orphan:,} (expect 0) | {'✅' if ok5 else '❌'}")

        # [6] PK 重複
        dup = c.execute(f"""
            SELECT COUNT(*) FROM (
              SELECT 1 FROM {TABLE}
              GROUP BY municipality_code, basis, occupation_code, age_class, gender, source_year, data_label
              HAVING COUNT(*) > 1
            )
        """).fetchone()[0]
        ok6 = dup == 0
        overall &= ok6
        print(f"[6] PK duplicates (full table): {dup:,} (expect 0) | {'✅' if ok6 else '❌'}")

        # [7] サンプル自治体 11 行 (F2)
        ok7_all = True
        for code, name in [
            ("13103", "港区"), ("13104", "新宿区"),
            ("13201", "八王子市"), ("23211", "豊田市"),
        ]:
            cnt = c.execute(
                f"SELECT COUNT(*) FROM {TABLE} WHERE source_name=? AND municipality_code=?",
                (F2_SOURCE_NAME, code),
            ).fetchone()[0]
            ok = cnt == 11
            ok7_all &= ok
            print(f"[7] F2 {code} ({name}): {cnt} rows (expect 11) | {'✅' if ok else '❌'}")
        overall &= ok7_all

        # [8] 人数表示禁止フラグ: estimated_beta の population は全 NULL
        n_pop_violations = c.execute(
            f"SELECT COUNT(*) FROM {TABLE} WHERE data_label='estimated_beta' AND population IS NOT NULL"
        ).fetchone()[0]
        ok8a = n_pop_violations == 0
        # measured の population NOT NULL
        n_measured_null = c.execute(
            f"SELECT COUNT(*) FROM {TABLE} WHERE data_label='measured' AND population IS NULL"
        ).fetchone()[0]
        ok8b = n_measured_null == 0
        # estimate_index NOT NULL for estimated_beta
        n_idx_null = c.execute(
            f"SELECT COUNT(*) FROM {TABLE} WHERE data_label='estimated_beta' AND estimate_index IS NULL"
        ).fetchone()[0]
        ok8c = n_idx_null == 0
        ok8 = ok8a and ok8b and ok8c
        overall &= ok8
        print(f"[8] human-count display guard:")
        print(f"    estimated_beta with population NOT NULL: {n_pop_violations:,} | {'✅' if ok8a else '❌'}")
        print(f"    measured with population NULL: {n_measured_null:,} | {'✅' if ok8b else '❌'}")
        print(f"    estimated_beta with estimate_index NULL: {n_idx_null:,} | {'✅' if ok8c else '❌'}")

        # [9] estimate_index range
        idx_stats = c.execute(
            f"SELECT MIN(estimate_index), MAX(estimate_index), AVG(estimate_index) FROM {TABLE} "
            f"WHERE source_name=?", (F2_SOURCE_NAME,)
        ).fetchone()
        print(f"[9] estimate_index: min={idx_stats[0]:.2f}, max={idx_stats[1]:.2f}, avg={idx_stats[2]:.2f}")
        ok9 = 0.0 <= idx_stats[0] and idx_stats[1] <= 200.0
        overall &= ok9
        print(f"     | {'✅' if ok9 else '❌'}")

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
