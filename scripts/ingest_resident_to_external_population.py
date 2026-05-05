# -*- coding: utf-8 -*-
"""
ingest_resident_to_external_population.py
==========================================

estat_resident_merged.csv の designated_ward 175 件分を
v2_external_population (175 行) と v2_external_population_pyramid
(1,575 行) に追加投入する。

Plan:
  - 既存 1,742 / 15,660 行は触らない
  - master JOIN で area_type='designated_ward' のみ抽出
  - PK 衝突なし (apply 前に既存 0 件を assert)
  - age_data_completeness='15plus_only' を data/generated/resident_ingest_log.json に記録
  - 8 項目検証

CLI:
  python scripts/ingest_resident_to_external_population.py --dry-run
  python scripts/ingest_resident_to_external_population.py --apply
  python scripts/ingest_resident_to_external_population.py --validate

設計書: docs/SURVEY_MARKET_INTELLIGENCE_PHASE3_RESIDENT_INGEST_PLAN_FINAL.md
"""
from __future__ import annotations

import argparse
import json
import sqlite3
import sys
from datetime import datetime, timezone
from pathlib import Path

try:
    sys.stdout.reconfigure(encoding="utf-8")
except (AttributeError, ValueError):
    pass

CSV_PATH = Path("data/generated/estat_resident_merged.csv")
DB_PATH = Path("data/hellowork.db")
LOG_PATH = Path("data/generated/resident_ingest_log.json")
TBL_POP = "v2_external_population"
TBL_PYR = "v2_external_population_pyramid"

REFERENCE_DATE = "2020-10-01"
AGE_COMPLETENESS_TAG = "15plus_only"  # 0-14 欠損、10-14 部分欠損

# 5 歳階級 → 10 歳階級 集約マップ
AGE_BUCKET_MAP: dict[str, list[str]] = {
    "0-9":   [],                                          # CSV 不在、0-fill
    "10-19": ["15-19"],                                   # 10-14 欠損、部分値
    "20-29": ["20-24", "25-29"],
    "30-39": ["30-34", "35-39"],
    "40-49": ["40-44", "45-49"],
    "50-59": ["50-54", "55-59"],
    "60-69": ["60-64", "65-69"],
    "70-79": ["70-74", "75-79"],
    "80+":   ["80-84", "85-89", "90-94", "95-99", "100+"],
}

# wide format 用 (15-64 の細分)
AGE_15_64 = ["15-19", "20-24", "25-29", "30-34", "35-39",
             "40-44", "45-49", "50-54", "55-59", "60-64"]

ROLLBACK_SQL = """
-- v2_external_population designated_ward 175 行削除
DELETE FROM v2_external_population
WHERE municipality IN (
  SELECT municipality_name FROM municipality_code_master
  WHERE area_type='designated_ward'
);

-- v2_external_population_pyramid designated_ward 1,575 行削除
DELETE FROM v2_external_population_pyramid
WHERE municipality IN (
  SELECT municipality_name FROM municipality_code_master
  WHERE area_type='designated_ward'
);
"""


def load_designated_master() -> "list[dict]":
    """master の area_type='designated_ward' 175 件を取得"""
    with sqlite3.connect(f"file:{DB_PATH}?mode=ro", uri=True) as conn:
        rows = conn.execute(
            "SELECT municipality_code, prefecture, municipality_name "
            "FROM municipality_code_master WHERE area_type='designated_ward'"
        ).fetchall()
    return [
        {"municipality_code": str(r[0]).zfill(5),
         "prefecture": r[1], "municipality_name": r[2]}
        for r in rows
    ]


def load_designated_csv():
    """CSV から designated_ward 行のみ抽出"""
    import pandas as pd
    df = pd.read_csv(CSV_PATH, dtype={"municipality_code": str})
    df["municipality_code"] = df["municipality_code"].astype(str).str.zfill(5)
    designated_codes = {m["municipality_code"] for m in load_designated_master()}
    sub = df[df["municipality_code"].isin(designated_codes)].copy()
    return sub


def to_population_wide(df_designated):
    """5 歳階級 → 12 列 wide 形式"""
    import pandas as pd

    # designated_ward の master 名前を信頼 (CSV 名と差異がある可能性のため)
    master = load_designated_master()
    code_to_master = {m["municipality_code"]: (m["prefecture"], m["municipality_name"])
                      for m in master}

    rows = []
    for code, grp in df_designated.groupby("municipality_code"):
        pref, muni = code_to_master[code]
        male_total = int(grp[grp["gender"] == "male"]["population"].sum())
        female_total = int(grp[grp["gender"] == "female"]["population"].sum())
        total = male_total + female_total
        age_15_64 = int(grp[grp["age_class"].isin(AGE_15_64)]["population"].sum())
        age_65_over = total - age_15_64
        rows.append({
            "prefecture": pref,
            "municipality": muni,
            "total_population": total,
            "male_population": male_total,
            "female_population": female_total,
            "age_0_14": None,           # CSV 不在
            "age_15_64": age_15_64,
            "age_65_over": age_65_over,
            "aging_rate": None,         # 0-14 不在で分母不完全
            "working_age_rate": None,
            "youth_rate": None,
            "reference_date": REFERENCE_DATE,
        })
    return pd.DataFrame(rows)


def to_pyramid_long(df_designated):
    """5 歳階級 → 10 歳階級 9 バケット long 形式"""
    import pandas as pd

    master = load_designated_master()
    code_to_master = {m["municipality_code"]: (m["prefecture"], m["municipality_name"])
                      for m in master}

    rows = []
    for code, grp in df_designated.groupby("municipality_code"):
        pref, muni = code_to_master[code]
        for bucket, sources in AGE_BUCKET_MAP.items():
            if not sources:
                # 0-9 は CSV 不在のため 0 fill
                rows.append({
                    "prefecture": pref,
                    "municipality": muni,
                    "age_group": bucket,
                    "male_count": 0,
                    "female_count": 0,
                })
            else:
                sub = grp[grp["age_class"].isin(sources)]
                rows.append({
                    "prefecture": pref,
                    "municipality": muni,
                    "age_group": bucket,
                    "male_count": int(sub[sub["gender"] == "male"]["population"].sum()),
                    "female_count": int(sub[sub["gender"] == "female"]["population"].sum()),
                })
    return pd.DataFrame(rows)


def assert_no_existing_designated(conn) -> tuple[int, int]:
    """apply 前に既存 designated_ward が 0 件であることを確認"""
    n_pop = conn.execute("""
        SELECT COUNT(*) FROM v2_external_population p
        JOIN municipality_code_master mcm
          ON p.prefecture=mcm.prefecture AND p.municipality=mcm.municipality_name
        WHERE mcm.area_type='designated_ward'
    """).fetchone()[0]
    n_pyr = conn.execute("""
        SELECT COUNT(*) FROM v2_external_population_pyramid p
        JOIN municipality_code_master mcm
          ON p.prefecture=mcm.prefecture AND p.municipality=mcm.municipality_name
        WHERE mcm.area_type='designated_ward'
    """).fetchone()[0]
    return n_pop, n_pyr


def estimate(args) -> int:
    print("=== --dry-run estimate ===")
    sub = load_designated_csv()
    pop_df = to_population_wide(sub)
    pyr_df = to_pyramid_long(sub)
    print(f"[csv] designated_ward source rows: {len(sub):,}")
    print(f"  expected = 175 muni × 2 gender × 18 age = {175*2*18:,}")
    print(f"[derived] population wide rows: {len(pop_df):,} (expect 175)")
    print(f"[derived] pyramid long rows: {len(pyr_df):,} (expect 1,575)")
    print()

    with sqlite3.connect(f"file:{DB_PATH}?mode=ro", uri=True) as conn:
        n_pop_before, n_pyr_before = assert_no_existing_designated(conn)
        n_pop_total = conn.execute(f"SELECT COUNT(*) FROM {TBL_POP}").fetchone()[0]
        n_pyr_total = conn.execute(f"SELECT COUNT(*) FROM {TBL_PYR}").fetchone()[0]
    print(f"[db] {TBL_POP} total: {n_pop_total:,} (existing designated: {n_pop_before})")
    print(f"[db] {TBL_PYR} total: {n_pyr_total:,} (existing designated: {n_pyr_before})")
    print()
    print(f"[plan] After apply: {TBL_POP} {n_pop_total:,} -> {n_pop_total + 175:,}")
    print(f"[plan] After apply: {TBL_PYR} {n_pyr_total:,} -> {n_pyr_total + 1575:,}")
    print()
    print(f"[note] reference_date = {REFERENCE_DATE}")
    print(f"[note] age_data_completeness = {AGE_COMPLETENESS_TAG} (recorded in {LOG_PATH})")
    print(f"[note] age_0_14 / aging_rate / working_age_rate / youth_rate = NULL")
    print(f"[note] pyramid 0-9 = 0 fill, 10-19 = 15-19 only (10-14 missing)")
    print()
    print("[rollback target]:")
    print(ROLLBACK_SQL)
    return 0


def apply(args) -> int:
    import pandas as pd
    print("=== --apply ===")
    sub = load_designated_csv()
    pop_df = to_population_wide(sub)
    pyr_df = to_pyramid_long(sub)
    if len(pop_df) != 175 or len(pyr_df) != 1575:
        print(f"[ERROR] expected (175, 1575), got ({len(pop_df)}, {len(pyr_df)})")
        return 1

    with sqlite3.connect(DB_PATH) as conn:
        # 安全装置 1: 既存 designated_ward が 0 件 (idempotent re-run なら DELETE 後にゼロ化)
        n_pop_before, n_pyr_before = assert_no_existing_designated(conn)
        print(f"[pre] designated_ward existing: pop={n_pop_before}, pyr={n_pyr_before}")

        # 安全装置 2: 既存 1,742 件 (ベース集合) を保存して比較用 sentinel
        n_pop_total_before = conn.execute(f"SELECT COUNT(*) FROM {TBL_POP}").fetchone()[0]
        n_pyr_total_before = conn.execute(f"SELECT COUNT(*) FROM {TBL_PYR}").fetchone()[0]
        n_existing_non_designated_pop = n_pop_total_before - n_pop_before
        n_existing_non_designated_pyr = n_pyr_total_before - n_pyr_before
        print(f"[pre] non-designated to preserve: pop={n_existing_non_designated_pop}, "
              f"pyr={n_existing_non_designated_pyr}")

        # 既存 designated_ward 削除 (idempotent)
        cur1 = conn.execute(f"""
            DELETE FROM {TBL_POP}
            WHERE municipality IN (
              SELECT municipality_name FROM municipality_code_master
              WHERE area_type='designated_ward'
            )
        """)
        cur2 = conn.execute(f"""
            DELETE FROM {TBL_PYR}
            WHERE municipality IN (
              SELECT municipality_name FROM municipality_code_master
              WHERE area_type='designated_ward'
            )
        """)
        print(f"[delete] {TBL_POP}: {cur1.rowcount}")
        print(f"[delete] {TBL_PYR}: {cur2.rowcount}")

        # 削除後 sentinel 確認 (non-designated が破壊されていない)
        n_after_delete_pop = conn.execute(f"SELECT COUNT(*) FROM {TBL_POP}").fetchone()[0]
        n_after_delete_pyr = conn.execute(f"SELECT COUNT(*) FROM {TBL_PYR}").fetchone()[0]
        if n_after_delete_pop != n_existing_non_designated_pop:
            print(f"[ERROR] non-designated pop count changed: "
                  f"{n_existing_non_designated_pop} -> {n_after_delete_pop}, abort")
            conn.rollback()
            return 1
        if n_after_delete_pyr != n_existing_non_designated_pyr:
            print(f"[ERROR] non-designated pyr count changed: "
                  f"{n_existing_non_designated_pyr} -> {n_after_delete_pyr}, abort")
            conn.rollback()
            return 1

        # INSERT
        pop_df.to_sql(TBL_POP, conn, if_exists="append", index=False)
        pyr_df.to_sql(TBL_PYR, conn, if_exists="append", index=False)
        conn.commit()

        n_pop_after = conn.execute(f"SELECT COUNT(*) FROM {TBL_POP}").fetchone()[0]
        n_pyr_after = conn.execute(f"SELECT COUNT(*) FROM {TBL_PYR}").fetchone()[0]
        print(f"[post] {TBL_POP}: {n_pop_after:,} (was {n_pop_total_before:,})")
        print(f"[post] {TBL_PYR}: {n_pyr_after:,} (was {n_pyr_total_before:,})")

    # 完全性ログ
    LOG_PATH.parent.mkdir(parents=True, exist_ok=True)
    log = {
        "ingested_at": datetime.now(timezone.utc).isoformat(),
        "source_csv": str(CSV_PATH),
        "source_name": "census_resident_pop",
        "source_year": 2020,
        "source_sid": "0003445236",
        "target_tables": [TBL_POP, TBL_PYR],
        "target_area_type": "designated_ward",
        "target_count": 175,
        "rows_inserted": {
            TBL_POP: 175,
            TBL_PYR: 1575,
        },
        "age_data_completeness": AGE_COMPLETENESS_TAG,
        "completeness_notes": {
            "age_0_14": "missing (CSV starts at 15+)",
            "age_15_64": "complete",
            "age_65_over": "complete",
            "aging_rate": "NULL (0-14 unknown, denominator incomplete)",
            "working_age_rate": "NULL",
            "youth_rate": "NULL",
            "pyramid_0-9": "zero-filled (not actual zero)",
            "pyramid_10-19": "partial (15-19 only, 10-14 missing)",
            "pyramid_20-79": "complete (5y aggregated to 10y)",
            "pyramid_80+": "complete (80-84+85-89+90-94+95-99+100+ summed)",
        },
        "reference_date": REFERENCE_DATE,
        "rollback_sql": ROLLBACK_SQL.strip(),
    }
    with open(LOG_PATH, "w", encoding="utf-8") as f:
        json.dump(log, f, ensure_ascii=False, indent=2)
    print(f"[log] {LOG_PATH}")
    print()
    return validate(args)


def validate(args) -> int:
    print("=== --validate ===")
    overall = True

    with sqlite3.connect(f"file:{DB_PATH}?mode=ro", uri=True) as conn:
        # [1] designated_ward population 175
        n1 = conn.execute("""
            SELECT COUNT(*) FROM v2_external_population p
            JOIN municipality_code_master mcm
              ON p.prefecture=mcm.prefecture AND p.municipality=mcm.municipality_name
            WHERE mcm.area_type='designated_ward'
        """).fetchone()[0]
        ok1 = n1 == 175
        overall &= ok1
        print(f"[1] population designated_ward: {n1} (expect 175) | {'✅' if ok1 else '❌'}")

        # [2] non-designated 維持
        n2 = conn.execute("""
            SELECT COUNT(*) FROM v2_external_population p
            JOIN municipality_code_master mcm
              ON p.prefecture=mcm.prefecture AND p.municipality=mcm.municipality_name
            WHERE mcm.area_type IN ('aggregate_city','municipality','special_ward')
        """).fetchone()[0]
        ok2 = n2 == 1741
        overall &= ok2
        print(f"[2] population non-designated: {n2} (expect 1,741) | {'✅' if ok2 else '❌'}")

        # [3] 完全カバレッジ (LEFT JOIN orphan)
        orphan = conn.execute("""
            SELECT COUNT(*) FROM municipality_code_master mcm
            LEFT JOIN v2_external_population p
              ON p.prefecture=mcm.prefecture AND p.municipality=mcm.municipality_name
            WHERE mcm.area_type='designated_ward' AND p.municipality IS NULL
        """).fetchone()[0]
        ok3 = orphan == 0
        overall &= ok3
        print(f"[3] designated coverage orphan: {orphan} (expect 0) | {'✅' if ok3 else '❌'}")

        # [4] pyramid designated 1,575
        n4 = conn.execute("""
            SELECT COUNT(*) FROM v2_external_population_pyramid p
            JOIN municipality_code_master mcm
              ON p.prefecture=mcm.prefecture AND p.municipality=mcm.municipality_name
            WHERE mcm.area_type='designated_ward'
        """).fetchone()[0]
        ok4 = n4 == 1575
        overall &= ok4
        print(f"[4] pyramid designated: {n4} (expect 1,575) | {'✅' if ok4 else '❌'}")

        # [5] 9 age_group 揃い
        bad5 = conn.execute("""
            SELECT COUNT(*) FROM (
              SELECT mcm.prefecture, mcm.municipality_name, COUNT(DISTINCT p.age_group) AS n
              FROM v2_external_population_pyramid p
              JOIN municipality_code_master mcm
                ON p.prefecture=mcm.prefecture AND p.municipality=mcm.municipality_name
              WHERE mcm.area_type='designated_ward'
              GROUP BY mcm.prefecture, mcm.municipality_name
              HAVING n != 9
            )
        """).fetchone()[0]
        ok5 = bad5 == 0
        overall &= ok5
        print(f"[5] designated age_group complete (=9): {bad5} bad (expect 0) | {'✅' if ok5 else '❌'}")

        # [6] サンプル横浜市鶴見区
        sample = conn.execute("""
            SELECT age_group, male_count, female_count
            FROM v2_external_population_pyramid
            WHERE prefecture='神奈川県' AND municipality='横浜市鶴見区'
            ORDER BY age_group
        """).fetchall()
        ok6 = len(sample) == 9
        overall &= ok6
        print(f"[6] sample 横浜市鶴見区: {len(sample)} rows (expect 9) | {'✅' if ok6 else '❌'}")
        if sample:
            for s in sample:
                print(f"      {s[0]:<6}: male={s[1]:>7,}  female={s[2]:>7,}")

        # [7] population 数値妥当性 (15_64 + 65_over = total)
        bad7 = conn.execute("""
            SELECT COUNT(*) FROM v2_external_population p
            JOIN municipality_code_master mcm
              ON p.prefecture=mcm.prefecture AND p.municipality=mcm.municipality_name
            WHERE mcm.area_type='designated_ward'
              AND (p.age_15_64 + p.age_65_over) != p.total_population
        """).fetchone()[0]
        ok7 = bad7 == 0
        overall &= ok7
        print(f"[7] designated 15_64+65_over=total: {bad7} bad (expect 0) | {'✅' if ok7 else '❌'}")

        # [8] PK 重複なし
        dup_pop = conn.execute("""
            SELECT COUNT(*) FROM (
              SELECT 1 FROM v2_external_population
              GROUP BY prefecture, municipality HAVING COUNT(*) > 1
            )
        """).fetchone()[0]
        dup_pyr = conn.execute("""
            SELECT COUNT(*) FROM (
              SELECT 1 FROM v2_external_population_pyramid
              GROUP BY prefecture, municipality, age_group HAVING COUNT(*) > 1
            )
        """).fetchone()[0]
        ok8 = dup_pop == 0 and dup_pyr == 0
        overall &= ok8
        print(f"[8] PK duplicates pop={dup_pop} pyr={dup_pyr} (expect 0/0) | {'✅' if ok8 else '❌'}")

    print()
    print(f"=== Overall: {'✅ PASS' if overall else '❌ FAIL'} ===")
    print()
    print("[rollback if needed]:")
    print(ROLLBACK_SQL)
    return 0 if overall else 1


def main() -> int:
    parser = argparse.ArgumentParser()
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--dry-run", action="store_true")
    mode.add_argument("--apply", action="store_true")
    mode.add_argument("--validate", action="store_true")
    args = parser.parse_args()
    if args.dry_run:
        return estimate(args)
    if args.apply:
        return apply(args)
    if args.validate:
        return validate(args)
    return 1


if __name__ == "__main__":
    sys.exit(main())
