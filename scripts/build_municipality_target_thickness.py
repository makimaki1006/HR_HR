# -*- coding: utf-8 -*-
"""
build_municipality_target_thickness.py
=======================================

Phase 3 Step 5 F2 estimation script (Plan B - resident/fallback only)

役割 (案 B 並列保管):
  - basis='resident', data_label='estimated_beta' レコード生成 (主役)
  - basis='workplace', data_label='estimated_beta' レコード生成 (15-1 fallback、Phase 5+)
  - basis='workplace', data_label='measured' は触れない (15-1 専有)

ステータス: スケルトン (interface 詳細化済 + 計算本体は Phase 5 で実装)
- ✅ --dry-run / --validate-only モード実装済 (READ-only)
- ✅ 12 関数 interface + docstring + type hints 詳細化済
- ⏳ --build モード未実装 (compute_* 関数は NotImplementedError)

ロードマップ: docs/SURVEY_MARKET_INTELLIGENCE_PHASE3_F2_IMPLEMENTATION_PLAN.md
DDL: docs/survey_market_intelligence_phase0_2_schema.sql (Plan B 反映済)
プロト元: scripts/proto_evaluate_occupation_population_models.py (model_f2)
sensitivity 推奨閾値: docs/SURVEY_MARKET_INTELLIGENCE_PHASE3_F2_SENSITIVITY_ANALYSIS.md

CLI:
  python scripts/build_municipality_target_thickness.py --dry-run
  python scripts/build_municipality_target_thickness.py --validate-only
  python scripts/build_municipality_target_thickness.py --build  # NotImplementedError

設計原則 (本タスクで本実装):
- READ-only な入力ロード (load_inputs)
- 入力検証 (validate_inputs): 行数・キーセット・型・カバレッジ・重み合計
- DB 検証 SQL 実行 (run_validation_sql): 既存テーブルへの READ-only 検証

設計原則 (Phase 5 で本実装):
- 計算ロジック (compute_baseline / compute_f3 / ... / compute_f6_v2 / integrate_model_f2)
- 派生指標 (derive_thickness_index / derive_rank_and_priority / derive_scenario_indices)
- 出力 (export_to_csv / import_to_sqlite)

Plan B 出力制約 (validate_outputs() で assert):
- basis = 'resident' のみ (workplace 'measured' は 15-1 専有)
- data_label = 'estimated_beta' 固定
- age_class = '_total' のみ (年齢別は 15-1 専有)
- gender = 'total' のみ (性別は 15-1 専有)
- source_name = 'model_f2_v1'
- weight_source = 'hypothesis_v1'
"""

from __future__ import annotations

import argparse
import csv
import io
import sqlite3
import sys
from collections import defaultdict
from pathlib import Path
from typing import Optional

# UTF-8 ログ出力 (Windows cp932 対策)
try:
    sys.stdout.reconfigure(encoding="utf-8")
except (AttributeError, ValueError):
    pass

# ============================================================
# 定数
# ============================================================

SCRIPT_DIR = Path(__file__).parent
REPO_ROOT = SCRIPT_DIR.parent

DEFAULT_DB_PATH = REPO_ROOT / "data" / "hellowork.db"
DEFAULT_INDUSTRY_CSV = SCRIPT_DIR / "data" / "industry_structure_by_municipality.csv"
DEFAULT_WEIGHT_CSV = REPO_ROOT / "data" / "generated" / "occupation_industry_weight.csv"
DEFAULT_SALESNOW_CSV = REPO_ROOT / "data" / "generated" / "salesnow_aggregate_for_f6.csv"
DEFAULT_OUTPUT_CSV = REPO_ROOT / "data" / "generated" / "v2_municipality_target_thickness.csv"

OUTPUT_TABLE_NAME = "v2_municipality_target_thickness"

# 重み CSV の期待値
EXPECTED_WEIGHT_SOURCE = "hypothesis_v1"
# CSV カラム名のゆらぎ吸収 (実 CSV は 'source'、計画書は 'weight_source')
WEIGHT_SOURCE_COLUMNS = ("weight_source", "source")

# 期待行数 (検証用)
EXPECTED_INDUSTRY_CSV_ROWS = 36099
EXPECTED_WEIGHT_CSV_ROWS = 231  # 21 産業 × 11 職業
EXPECTED_OCCUPATION_COUNT = 11
EXPECTED_INDUSTRY_COUNT = 21
EXPECTED_SALESNOW_AGG_ROWS_MIN = 10000
EXPECTED_PREFECTURE_COUNT = 47

# ===========================================================
# Sensitivity-confirmed thresholds (Worker A3 result)
# DO NOT CHANGE without re-running sensitivity_anchor_thresholds.py
# ===========================================================
DEFAULT_ANCHOR_THRESHOLDS_v1 = {
    "mfg_share": 0.12,                      # Worker A3 sensitivity 推奨
    "mfg_emp_per_establishment": 20.0,
    "day_night_ratio": 150.0,
    "hq_excess_ratio_E": 5.0,
}

# Backward-compat alias (旧キー hq_excess_ratio を参照する箇所のため)
DEFAULT_ANCHOR_THRESHOLDS = DEFAULT_ANCHOR_THRESHOLDS_v1

# Plan B 出力制約 (validate_outputs で assert される値)
PLAN_B_BASIS = "resident"
PLAN_B_DATA_LABEL = "estimated_beta"
PLAN_B_AGE_CLASS = "_total"
PLAN_B_GENDER = "total"
PLAN_B_SOURCE_NAME = "model_f2_v1"
PLAN_B_WEIGHT_SOURCE = "hypothesis_v1"

# 出力 CSV カラム (Plan B 仕様)
OUTPUT_CSV_COLUMNS = (
    "municipality_code",
    "prefecture",
    "municipality_name",
    "basis",
    "occupation_code",
    "occupation_name",
    "age_class",
    "gender",
    "estimate_index",
    "data_label",
    "source_name",
    "source_year",
    "weight_source",
    "is_industrial_anchor",
    "estimated_at",
)

# turnover シナリオ (定数倍ラベル)
DEFAULT_TURNOVER_RATES = {
    "conservative": 1,
    "standard": 3,
    "aggressive": 5,
}


# ============================================================
# I/O: 入力ロード (本実装)
# ============================================================

def _load_db_table_rows(conn: sqlite3.Connection, query: str) -> list[tuple]:
    """READ-only クエリ実行のヘルパ."""
    return conn.execute(query).fetchall()


def _load_population(conn: sqlite3.Connection) -> dict:
    """v2_external_population をロード."""
    rows = _load_db_table_rows(
        conn,
        """
        SELECT prefecture, municipality, total_population, age_15_64
        FROM v2_external_population
        WHERE prefecture IS NOT NULL AND prefecture <> ''
          AND prefecture <> '都道府県'
          AND municipality <> '市区町村'
          AND total_population IS NOT NULL AND total_population > 0
        """,
    )
    return {(r[0], r[1]): {"total": r[2], "age_15_64": r[3] or 0} for r in rows}


def _load_pyramid(conn: sqlite3.Connection) -> dict:
    """v2_external_population_pyramid をロード."""
    rows = _load_db_table_rows(
        conn,
        """
        SELECT prefecture, municipality, age_group, male_count, female_count
        FROM v2_external_population_pyramid
        WHERE prefecture IS NOT NULL AND prefecture <> ''
          AND prefecture <> '都道府県'
          AND municipality <> '市区町村'
        """,
    )
    pyramid: dict = defaultdict(lambda: defaultdict(lambda: {"male": 0, "female": 0}))
    for pref, muni, age, m, f in rows:
        pyramid[(pref, muni)][age]["male"] = m or 0
        pyramid[(pref, muni)][age]["female"] = f or 0
    # defaultdict → 通常 dict に変換
    return {k: dict(v) for k, v in pyramid.items()}


def _load_daytime(conn: sqlite3.Connection) -> dict:
    """v2_external_daytime_population をロード."""
    rows = _load_db_table_rows(
        conn,
        """
        SELECT prefecture, municipality, nighttime_pop, daytime_pop, day_night_ratio,
               inflow_pop, outflow_pop
        FROM v2_external_daytime_population
        WHERE prefecture IS NOT NULL AND prefecture <> ''
        """,
    )
    return {
        (r[0], r[1]): {
            "night": r[2] or 0,
            "day": r[3] or 0,
            "ratio": r[4] or 1.0,
            "inflow": r[5] or 0,
            "outflow": r[6] or 0,
        }
        for r in rows
    }


def _load_commute_flow(conn: sqlite3.Connection) -> list[dict]:
    """commute_flow_summary をロード (F5 補強用)."""
    cur = conn.execute("SELECT * FROM commute_flow_summary LIMIT 0")
    cols = [d[0] for d in cur.description]
    rows = conn.execute("SELECT * FROM commute_flow_summary").fetchall()
    return [dict(zip(cols, r)) for r in rows]


def _load_muni_master(conn: sqlite3.Connection) -> dict:
    """municipality_code_master をロード."""
    rows = _load_db_table_rows(
        conn,
        """
        SELECT municipality_code, prefecture, municipality_name, area_type, parent_code
        FROM municipality_code_master
        """,
    )
    return {
        "by_name": {(r[1], r[2]): {"code": r[0], "area_type": r[3], "parent": r[4]} for r in rows},
        "by_code": {r[0]: {"prefecture": r[1], "name": r[2], "area_type": r[3], "parent": r[4]} for r in rows},
    }


def _load_industry_csv(industry_csv: Path) -> list[dict]:
    """industry_structure_by_municipality.csv をロード."""
    if not industry_csv.exists():
        raise FileNotFoundError(f"Industry CSV not found: {industry_csv}")
    with open(industry_csv, "r", encoding="utf-8", newline="") as f:
        reader = csv.DictReader(f)
        # BOM 処理 (﻿prefecture_code を prefecture_code に正規化)
        rows = []
        for row in reader:
            normalized = {(k.lstrip("﻿") if k else k): v for k, v in row.items()}
            rows.append(normalized)
    return rows


def _load_weight_csv(weight_csv: Path) -> list[dict]:
    """occupation_industry_weight.csv をロード.

    weight_source 列の存在を assert.
    実 CSV は 'source' カラムを使用、計画書は 'weight_source' を想定 → 両対応.
    """
    if not weight_csv.exists():
        raise FileNotFoundError(f"Weight CSV not found: {weight_csv}")
    with open(weight_csv, "r", encoding="utf-8", newline="") as f:
        reader = csv.DictReader(f)
        rows = list(reader)

    if not rows:
        raise ValueError(f"Weight CSV is empty: {weight_csv}")

    # weight_source 列の検出
    columns = set(rows[0].keys())
    source_col = None
    for cand in WEIGHT_SOURCE_COLUMNS:
        if cand in columns:
            source_col = cand
            break
    if source_col is None:
        raise ValueError(
            f"Weight CSV must contain one of {WEIGHT_SOURCE_COLUMNS} columns. "
            f"Found: {sorted(columns)}"
        )

    # 値が hypothesis_v1 のみであることを確認 (Plan B: 非 hypothesis_v1 は WARN)
    sources = {row[source_col] for row in rows}
    non_hypothesis = sources - {EXPECTED_WEIGHT_SOURCE}
    if non_hypothesis:
        # Plan B 制約緩和: hypothesis_v1 以外がある場合は WARN ログのみ
        # (Phase 5 で他 weight_source を許容する可能性があるため)
        print(
            f"[WARN] weight_csv contains non-'{EXPECTED_WEIGHT_SOURCE}' sources: "
            f"{sorted(non_hypothesis)}. Plan B 推奨: '{EXPECTED_WEIGHT_SOURCE}' のみ"
        )
    if EXPECTED_WEIGHT_SOURCE not in sources:
        raise ValueError(
            f"weight_source must include '{EXPECTED_WEIGHT_SOURCE}', found: {sources}"
        )

    print(f"[INFO] Weight CSV: {len(rows)} rows, source column='{source_col}', "
          f"value='{EXPECTED_WEIGHT_SOURCE}'")
    return rows


def _load_salesnow_csv(salesnow_csv: Optional[Path]) -> list[dict]:
    """salesnow_aggregate_for_f6.csv をロード (省略可)."""
    if salesnow_csv is None or not salesnow_csv.exists():
        print(f"[WARN] SalesNow CSV not provided / not found: {salesnow_csv}")
        return []
    with open(salesnow_csv, "r", encoding="utf-8", newline="") as f:
        reader = csv.DictReader(f)
        return list(reader)


def load_inputs(
    db_path: str,
    industry_csv: str,
    weight_csv: str,
    salesnow_csv: Optional[str],
) -> dict:
    """入力をロード.

    各データの件数・キーセット・型をログ出力。
    weight CSV の weight_source カラムを assert (hypothesis_v1 が含まれる)。

    Args:
        db_path: ローカル SQLite DB パス
        industry_csv: 産業構造 CSV
        weight_csv: 産業×職業重み CSV
        salesnow_csv: SalesNow 集約 CSV (省略可)

    Returns:
        data: {
            "population": {...},
            "pyramid": {...},
            "daytime": {...},
            "commute_flow": [...],
            "muni_master": {"by_name": {...}, "by_code": {...}},
            "industry_csv_rows": [...],
            "weight_csv_rows": [...],
            "salesnow_csv_rows": [...],
        }
    """
    db_path_p = Path(db_path)
    if not db_path_p.exists():
        raise FileNotFoundError(f"Local SQLite DB not found: {db_path}")

    print(f"[INFO] Loading inputs...")
    print(f"[INFO]   DB: {db_path_p}")
    print(f"[INFO]   Industry CSV: {industry_csv}")
    print(f"[INFO]   Weight CSV: {weight_csv}")
    print(f"[INFO]   SalesNow CSV: {salesnow_csv}")

    conn = sqlite3.connect(f"file:{db_path_p}?mode=ro", uri=True)
    try:
        population = _load_population(conn)
        pyramid = _load_pyramid(conn)
        daytime = _load_daytime(conn)
        commute_flow = _load_commute_flow(conn)
        muni_master = _load_muni_master(conn)
    finally:
        conn.close()

    industry_csv_rows = _load_industry_csv(Path(industry_csv))
    weight_csv_rows = _load_weight_csv(Path(weight_csv))
    salesnow_csv_rows = _load_salesnow_csv(Path(salesnow_csv) if salesnow_csv else None)

    print(f"[INFO] Inputs loaded:")
    print(f"[INFO]   v2_external_population: {len(population)} rows")
    print(f"[INFO]   v2_external_population_pyramid: {len(pyramid)} (pref, muni) keys")
    print(f"[INFO]   v2_external_daytime_population: {len(daytime)} rows")
    print(f"[INFO]   commute_flow_summary: {len(commute_flow)} rows")
    print(f"[INFO]   municipality_code_master: by_name={len(muni_master['by_name'])}, "
          f"by_code={len(muni_master['by_code'])}")
    print(f"[INFO]   industry CSV: {len(industry_csv_rows)} rows")
    print(f"[INFO]   weight CSV: {len(weight_csv_rows)} rows")
    print(f"[INFO]   salesnow agg CSV: {len(salesnow_csv_rows)} rows")

    return {
        "population": population,
        "pyramid": pyramid,
        "daytime": daytime,
        "commute_flow": commute_flow,
        "muni_master": muni_master,
        "industry_csv_rows": industry_csv_rows,
        "weight_csv_rows": weight_csv_rows,
        "salesnow_csv_rows": salesnow_csv_rows,
    }


# ============================================================
# 入力検証 (本実装)
# ============================================================

def validate_inputs(data: dict) -> list[str]:
    """入力検証. エラーメッセージのリストを返却 (空なら OK).

    チェック:
    - 行数 (各テーブル ±5%)
    - 都道府県カバレッジ (47 県完全)
    - JIS コード形式 (5 桁数字)
    - weight CSV: 21 産業 × 11 職業 = 231 行、weight_source = 'hypothesis_v1'
    - 重み合計 = 1.0 ± 0.001
    - municipality_code_master との結合可能性 (orphan rate < 5%)
    """
    errors: list[str] = []

    # ---- 1. population: 47 都道府県カバレッジ ----
    pop_prefs = {pref for (pref, _muni) in data["population"].keys()}
    if len(pop_prefs) != EXPECTED_PREFECTURE_COUNT:
        errors.append(
            f"population: prefecture coverage {len(pop_prefs)} != {EXPECTED_PREFECTURE_COUNT}"
        )
    pop_count = len(data["population"])
    if pop_count < 1500 or pop_count > 2000:
        errors.append(f"population: row count {pop_count} out of expected range [1500, 2000]")

    # ---- 2. pyramid: 行数チェック (1740±20 muni × 8-10 age groups) ----
    pyramid_total = sum(len(v) for v in data["pyramid"].values())
    if pyramid_total < 10000 or pyramid_total > 20000:
        errors.append(
            f"pyramid: total entries {pyramid_total} out of expected range [10000, 20000]"
        )

    # ---- 3. daytime: 47 都道府県カバレッジ ----
    daytime_prefs = {pref for (pref, _muni) in data["daytime"].keys()}
    if len(daytime_prefs) != EXPECTED_PREFECTURE_COUNT:
        errors.append(
            f"daytime: prefecture coverage {len(daytime_prefs)} != {EXPECTED_PREFECTURE_COUNT}"
        )

    # ---- 4. muni_master: JIS コード形式 (5 桁数字) ----
    bad_codes = []
    for code in data["muni_master"]["by_code"].keys():
        if not (isinstance(code, str) and len(code) == 5 and code.isdigit()):
            bad_codes.append(code)
            if len(bad_codes) >= 5:
                break
    if bad_codes:
        errors.append(f"muni_master: invalid JIS codes (sample): {bad_codes}")

    # ---- 5. weight CSV: 行数 + 重み合計 ----
    weight_rows = data["weight_csv_rows"]
    if len(weight_rows) != EXPECTED_WEIGHT_CSV_ROWS:
        errors.append(
            f"weight_csv: row count {len(weight_rows)} != {EXPECTED_WEIGHT_CSV_ROWS} "
            f"(21 industries × 11 occupations)"
        )

    # 産業ごとの重み合計が 1.0 ± 0.001
    industry_sums: dict = defaultdict(float)
    for row in weight_rows:
        try:
            ind = row["industry_code"]
            w = float(row["weight"])
            industry_sums[ind] += w
        except (KeyError, ValueError, TypeError) as exc:
            errors.append(f"weight_csv: malformed row {row}: {exc}")
            break

    for ind, total in industry_sums.items():
        if abs(total - 1.0) >= 0.001:
            errors.append(f"weight_csv: industry '{ind}' weight sum = {total:.6f}, expected 1.0 ± 0.001")

    # 職業数 = 11
    occupation_set = {row.get("occupation_code") for row in weight_rows}
    if len(occupation_set) != EXPECTED_OCCUPATION_COUNT:
        errors.append(
            f"weight_csv: distinct occupations {len(occupation_set)} != {EXPECTED_OCCUPATION_COUNT}"
        )

    # 産業数 = 21
    industry_set = {row.get("industry_code") for row in weight_rows}
    if len(industry_set) != EXPECTED_INDUSTRY_COUNT:
        errors.append(
            f"weight_csv: distinct industries {len(industry_set)} != {EXPECTED_INDUSTRY_COUNT}"
        )

    # ---- 6. industry CSV: 行数 ±5% ----
    industry_rows = data["industry_csv_rows"]
    lower = int(EXPECTED_INDUSTRY_CSV_ROWS * 0.95)
    upper = int(EXPECTED_INDUSTRY_CSV_ROWS * 1.05)
    if not (lower <= len(industry_rows) <= upper):
        errors.append(
            f"industry_csv: row count {len(industry_rows)} out of ±5% of {EXPECTED_INDUSTRY_CSV_ROWS}"
        )

    # ---- 7. industry CSV ↔ muni_master 結合可能性 (orphan rate < 5%) ----
    valid_codes = set(data["muni_master"]["by_code"].keys())
    orphan = 0
    total_rows_with_code = 0
    for row in industry_rows:
        city_code = row.get("city_code")
        if not city_code:
            continue
        try:
            normalized = str(int(str(city_code).strip())).zfill(5)
        except (ValueError, TypeError):
            continue
        total_rows_with_code += 1
        if normalized not in valid_codes:
            orphan += 1
    if total_rows_with_code > 0:
        orphan_rate = orphan / total_rows_with_code
        if orphan_rate >= 0.05:
            errors.append(
                f"industry_csv: orphan rate {orphan_rate:.2%} >= 5% (orphan={orphan}/{total_rows_with_code})"
            )

    # ---- 8. SalesNow CSV: 行数 (任意) ----
    salesnow_rows = data["salesnow_csv_rows"]
    if salesnow_rows and len(salesnow_rows) < EXPECTED_SALESNOW_AGG_ROWS_MIN:
        errors.append(
            f"salesnow_csv: row count {len(salesnow_rows)} < {EXPECTED_SALESNOW_AGG_ROWS_MIN}"
        )

    return errors


# ============================================================
# 検証 SQL 実行 (--validate-only、本実装)
# ============================================================

def run_validation_sql(
    db_path: str,
    table_name: str = OUTPUT_TABLE_NAME,
) -> dict:
    """本実装計画 §5 の 8 件の検証 SQL を READ-only で実行.

    テーブル不在なら 'テーブルなし' を報告して exit 0 (errors 空、status='missing').
    """
    db_path_p = Path(db_path)
    if not db_path_p.exists():
        return {
            "status": "db_missing",
            "message": f"Local DB not found: {db_path}",
            "results": {},
        }

    conn = sqlite3.connect(f"file:{db_path_p}?mode=ro", uri=True)
    try:
        # テーブル存在確認
        cur = conn.execute(
            "SELECT name FROM sqlite_master WHERE type='table' AND name=?",
            (table_name,),
        )
        if cur.fetchone() is None:
            print(f"[INFO] テーブル {table_name} が存在しません (Build モード未実行)")
            return {
                "status": "table_missing",
                "table_name": table_name,
                "message": f"Table {table_name} not yet created",
                "results": {},
            }

        results: dict = {}

        # (1) 行数チェック (期待 ~38,324)
        row_count = conn.execute(f"SELECT COUNT(*) FROM {table_name}").fetchone()[0]
        results["row_count"] = row_count
        print(f"[CHECK 1/8] row_count: {row_count} (expected ~38,324, ±5%)")

        # (2) thickness_index レンジ
        rng = conn.execute(
            f"SELECT MIN(thickness_index), MAX(thickness_index), AVG(thickness_index) "
            f"FROM {table_name}"
        ).fetchone()
        results["thickness_range"] = {"min": rng[0], "max": rng[1], "avg": rng[2]}
        print(f"[CHECK 2/8] thickness_index: min={rng[0]}, max={rng[1]}, avg={rng[2]} "
              f"(expected min>=0, max<=200, avg≈100)")

        # (3) distribution_priority 分布
        priority_dist = conn.execute(
            f"SELECT distribution_priority, COUNT(*) FROM {table_name} "
            f"GROUP BY distribution_priority ORDER BY distribution_priority"
        ).fetchall()
        results["priority_distribution"] = {p: c for p, c in priority_dist}
        print(f"[CHECK 3/8] priority distribution: {dict(priority_dist)}")

        # (4) 港区生産工程順位
        minato = conn.execute(
            f"SELECT rank_in_occupation, thickness_index, distribution_priority "
            f"FROM {table_name} "
            f"WHERE prefecture='東京都' AND municipality_name='港区' "
            f"  AND occupation_code='08_生産工程' AND basis='workplace'"
        ).fetchone()
        results["minato_seisan"] = minato
        print(f"[CHECK 4/8] 港区 生産工程 (workplace): {minato} (expected rank>=30)")

        # (5) 製造系工業都市 TOP 10
        mfg_top = conn.execute(
            f"""
            SELECT prefecture, municipality_name, AVG(thickness_index) AS avg_idx
            FROM {table_name}
            WHERE basis='workplace'
              AND occupation_code IN ('08_生産工程', '09_輸送機械')
            GROUP BY prefecture, municipality_name
            ORDER BY avg_idx DESC
            LIMIT 10
            """
        ).fetchall()
        results["mfg_top10"] = mfg_top
        print(f"[CHECK 5/8] mfg TOP 10:")
        for row in mfg_top:
            print(f"           {row[0]} {row[1]}: {row[2]:.1f}")

        # (6) weight_source 確認
        ws = conn.execute(f"SELECT DISTINCT weight_source FROM {table_name}").fetchall()
        results["weight_source"] = [r[0] for r in ws]
        print(f"[CHECK 6/8] weight_source: {[r[0] for r in ws]} (expected ['hypothesis_v1'])")

        # (7) is_industrial_anchor 件数
        anchor = conn.execute(
            f"SELECT is_industrial_anchor, COUNT(DISTINCT prefecture || '/' || municipality_name) "
            f"FROM {table_name} GROUP BY is_industrial_anchor"
        ).fetchall()
        results["anchor_distribution"] = {a: c for a, c in anchor}
        print(f"[CHECK 7/8] is_industrial_anchor: {dict(anchor)}")

        # (8) estimate_grade
        grades = conn.execute(f"SELECT DISTINCT estimate_grade FROM {table_name}").fetchall()
        results["estimate_grade"] = [g[0] for g in grades]
        print(f"[CHECK 8/8] estimate_grade: {[g[0] for g in grades]} (expected ['A-'])")

        return {
            "status": "ok",
            "table_name": table_name,
            "results": results,
        }
    finally:
        conn.close()


# ============================================================
# 計算ロジック (Plan B - interface 詳細化済 / 本体は Phase 5 実装)
# ============================================================

# 共通型エイリアス (本実装時の参照用)
# MuniKey = tuple[str, str]   # (prefecture, municipality_name)
# OccCode = str               # 例: '08_生産工程'
# FactorMap = dict[MuniKey, dict[OccCode, float]]
# ScalarMap = dict[MuniKey, float]


def compute_baseline(data: dict) -> dict:
    """F1 (総人口) × F2 (生産年齢人口比) baseline を計算.

    Plan B 制約:
      - 出力は age_class='_total', gender='total' のレコードに対応する集約値のみ。
        年齢別 / 性別の細分は 15-1 (basis='workplace', data_label='measured') の
        専有領域であり、本関数は触れない。

    アルゴリズム (proto 移植予定):
      muni_baseline[(pref, muni)][occ]
        = pref_employment[pref][occ]
          × (muni_age_15_64[(pref, muni)] / pref_age_15_64[pref])
      ここで pref_employment は都道府県 × 11 職業の就業者数 (基準年)。

    Args:
        data: load_inputs() の戻り値 dict.
              - data['population']: {(pref, muni): {'total': int, 'age_15_64': int}}
              - data['pyramid']:   {(pref, muni): {age_group: {'male': int, 'female': int}}}
              - data['muni_master']: 結合用マスタ
              - 都道府県別就業者数: 別途 data 内に格納される予定
                (Phase 5 で _load_pref_employment ヘルパ追加)

    Returns:
        baseline: dict[tuple[str, str], dict[str, float]]
                  {(pref, muni): {occ_code: baseline_value}}
                  age 統合・gender 統合後の値 (Plan B: _total / total のみ)。

    前提:
      - data['population'] が 47 都道府県カバー
      - 全 muni_key について age_15_64 > 0 (0 のものは pyramid から再計算)

    副作用: なし (純関数)

    実装プロト元:
      scripts/proto_evaluate_occupation_population_models.py
        compute_model_b_baseline() (model_f2 の F1×F2 部分)

    本実装: Phase 5 (sensitivity 結果反映後)
    """
    raise NotImplementedError(
        "Phase 5 で実装。proto compute_model_b_baseline を移植 + Plan B 制約 "
        "(age_class='_total', gender='total' の集約値のみ生成) を適用。"
    )


def compute_f3(data: dict, weight_matrix: dict) -> dict:
    """F3 産業構成補正 (べき乗 1.5).

    アルゴリズム:
      ind_share[muni][industry] = muni 産業別就業者数 / muni 総就業者数
      nat_share[industry]      = 全国産業別就業者数 / 全国総就業者数
      F3_raw[muni][occ]        = Σ_industry (ind_share × weight[industry][occ])
                               / Σ_industry (nat_share × weight[industry][occ])
      F3[muni][occ]            = F3_raw ** 1.5

    Args:
        data: load_inputs() の戻り値。data['industry_csv_rows'] を主に参照。
        weight_matrix: dict[str, dict[str, float]]
                       {industry_code: {occupation_code: weight}}
                       weight_csv_rows から構築 (本関数の caller 責務)。
                       weight_source = 'hypothesis_v1' を満たすこと。

    Returns:
        f3: dict[tuple[str, str], dict[str, float]]
            {(pref, muni): {occ: factor}}

    前提・assert:
      - weight_matrix の全 industry の重み合計 ≒ 1.0 ± 0.001
      - weight_source == PLAN_B_WEIGHT_SOURCE ('hypothesis_v1')
      - factor は [0.1, 10.0] にクリップ (極端値除外)

    実装プロト元:
      scripts/proto_evaluate_occupation_population_models.py: compute_f3() 相当

    本実装: Phase 5
    """
    raise NotImplementedError(
        "Phase 5 で実装。proto compute_f3 を移植 + weight_matrix の "
        "weight_source='hypothesis_v1' を assert。"
    )


def compute_f4_occupation_weighted(data: dict) -> dict:
    """F4 昼夜間人口比 × 職業別重み (Worker D Model E2).

    アルゴリズム:
      day_night = data['daytime'][(pref, muni)]['ratio'] / 100.0  # %→比率
      f4[(pref, muni)][occ] = clamp(
          1 + (day_night - 1) × OCCUPATION_F4_WEIGHT[occ],
          min=0.1, max=5.0
      )

    OCCUPATION_F4_WEIGHT (Worker D Model E2 確定値):
      事務系・販売系 = 1.0 (フル昼夜効果)
      生産工程・建設等 = 0.3〜0.5 (限定的)
      農林漁業 = 0.0 (effect なし)
      → Phase 5 で proto から移植

    Args:
        data: load_inputs() の戻り値。data['daytime'] を参照。

    Returns:
        f4: dict[tuple[str, str], dict[str, float]]
            {(pref, muni): {occ: factor in [0.1, 5.0]}}

    前提:
      - data['daytime'] が 47 都道府県カバー
      - day_night_ratio が NULL の場合は 100.0 (= 1.0 比率) で fallback

    実装プロト元:
      scripts/proto_evaluate_occupation_population_models.py: model_e2 関連

    本実装: Phase 5
    """
    raise NotImplementedError(
        "Phase 5 で実装。proto Model E2 OCCUPATION_F4_WEIGHT を移植。"
    )


def compute_f5(data: dict) -> dict:
    """F5 通勤流入補正 (職業非依存).

    アルゴリズム:
      f5[(pref, muni)] = clamp(
          1 + (inflow / night) × 0.3,
          min=0.5, max=2.0
      )

    Args:
        data: load_inputs() の戻り値。data['daytime'] / data['commute_flow'] を参照。

    Returns:
        f5: dict[tuple[str, str], float]
            {(pref, muni): scalar_factor}
            (職業非依存、職業ごとに同じ値を適用)

    前提:
      - night > 0 (= nighttime_pop) でゼロ除算回避
      - inflow / outflow が NULL の muni は f5 = 1.0 (中立)

    実装プロト元:
      scripts/proto_evaluate_occupation_population_models.py: compute_f5() 相当

    本実装: Phase 5
    """
    raise NotImplementedError(
        "Phase 5 で実装。proto compute_f5 を移植。職業非依存 scalar を返す。"
    )


def compute_industrial_anchor(
    data: dict,
    thresholds: Optional[dict] = None,
) -> set:
    """industrial_anchor_city 判定 (4 条件 AND, Worker A3 sensitivity 推奨閾値で固定).

    判定条件 (全て満たす市区町村が anchor):
      1. mfg_share         >= thresholds['mfg_share']
                           (= 製造業就業者比率 12%)
      2. mfg_emp_per_establishment >= thresholds['mfg_emp_per_establishment']
                           (= 1 事業所あたり製造業従業者 20 人)
      3. day_night_ratio   >= thresholds['day_night_ratio']
                           (= 昼夜間比率 150%、流入超過)
      4. hq_excess_ratio_E >= thresholds['hq_excess_ratio_E']
                           (= SalesNow 本社過剰指標 5.0)

    Args:
        data: load_inputs() の戻り値。
              - data['industry_csv_rows']: mfg_share / mfg_emp_per_establishment 計算用
              - data['daytime']:           day_night_ratio
              - data['salesnow_csv_rows']: hq_excess_ratio_E
        thresholds: 4 条件の閾値 dict (None で DEFAULT_ANCHOR_THRESHOLDS_v1)

    Returns:
        anchor_set: set[tuple[str, str]]
                    { (prefecture, municipality_name), ... }

    前提:
      - data['industry_csv_rows'] に 21 産業の就業者数あり
      - data['daytime'] が 47 都道府県カバー
      - SalesNow CSV が空でも fallback (hq_excess_ratio_E=0 → 該当条件 false)

    実装プロト元:
      scripts/proto_evaluate_occupation_population_models.py:
      compute_industrial_anchor_cities()

    sensitivity 推奨値:
      docs/SURVEY_MARKET_INTELLIGENCE_PHASE3_F2_SENSITIVITY_ANALYSIS.md (Worker A3)

    本実装: Phase 5
    """
    if thresholds is None:
        thresholds = DEFAULT_ANCHOR_THRESHOLDS_v1
    raise NotImplementedError(
        "Phase 5 で実装。proto compute_industrial_anchor_cities を移植 + "
        "DEFAULT_ANCHOR_THRESHOLDS_v1 を引数として受け取る形に。"
    )


def compute_f6_v2(data: dict, anchor_set: set) -> dict:
    """F6 anchor 分岐 (本社減衰 + 工場ブースト).

    アルゴリズム:
      anchor IN  ((pref, muni) in anchor_set):
        → F6 = 1.0 (HQ 減衰スキップ、工場ブースト効果は baseline に既に反映)
      anchor OUT:
        → F6 = HQ_decay_factor × factory_boost
        HQ_decay_factor = 1 / (1 + hq_excess × decay_rate)
        factory_boost   = 1 + factory_density × boost_rate

    Args:
        data: load_inputs() の戻り値。data['salesnow_csv_rows'] を参照。
        anchor_set: compute_industrial_anchor() の戻り値
                    { (pref, muni), ... }

    Returns:
        f6: dict[tuple[str, str], dict[str, float]]
            {(pref, muni): {occ: factor}}

    前提:
      - anchor 内市区町村は f6 = 1.0 (全職業)
      - anchor 外市区町村は職業ごとに減衰係数を計算
      - factor は [0.1, 5.0] にクリップ

    実装プロト元:
      scripts/proto_evaluate_occupation_population_models.py: compute_f6_v2()

    本実装: Phase 5
    """
    raise NotImplementedError(
        "Phase 5 で実装。proto compute_f6_v2 を移植。anchor IN は f6=1.0、"
        "anchor OUT は HQ 減衰×工場ブースト。"
    )


def integrate_model_f2(
    baseline: dict,
    f3: dict,
    f4: dict,
    f5: dict,
    f6: dict,
    data: dict,
) -> dict:
    """全要素統合 + 都道府県スケーリング.

    アルゴリズム:
      raw[(pref, muni)][occ] = baseline × f3 × f4 × f5 × f6
      pref_raw_sum[pref][occ] = Σ_muni raw[(pref, muni)][occ]
      scaling[pref][occ]     = pref_target[pref][occ] / pref_raw_sum[pref][occ]
      out[(pref, muni)][occ] = raw × scaling[pref][occ]

    都道府県スケーリングの目的:
      F3〜F6 の補正で総量がずれるため、都道府県単位で
      公式統計の就業者数に整合するようにスケール。

    Args:
        baseline: compute_baseline() 結果
        f3:       compute_f3() 結果
        f4:       compute_f4_occupation_weighted() 結果
        f5:       compute_f5() 結果 (scalar、職業非依存)
        f6:       compute_f6_v2() 結果
        data:     load_inputs() の戻り値 (pref_target 取得用)

    Returns:
        out: dict[tuple[str, str], dict[str, float]]
             {(pref, muni): {occ: estimated_value}}
             estimated_value は **指数の元値** (人数換算ではない)。
             最終的に derive_thickness_index() で 0-200 に正規化される。

    前提:
      - 全引数のキーセットが概ね一致 (欠損は 1.0 で fallback)
      - pref_target は data 内に格納される予定 (Phase 5)

    実装プロト元:
      scripts/proto_evaluate_occupation_population_models.py: integrate_model_f2()

    本実装: Phase 5
    """
    raise NotImplementedError(
        "Phase 5 で実装。proto integrate_model_f2 を移植 + 都道府県 scaling。"
    )


# ============================================================
# 派生指標 (Phase 5 で本実装)
# ============================================================

def derive_thickness_index(model_result: dict) -> dict:
    """0-200 正規化 (100 = 全国平均).

    アルゴリズム:
      nat_avg[occ] = Σ model_result[muni][occ] / N_muni
      thickness_index[muni][occ] = clamp(
          model_result[muni][occ] / nat_avg[occ] × 100,
          min=0, max=200
      )

    Args:
        model_result: integrate_model_f2() の戻り値
                      {(pref, muni): {occ: value}}

    Returns:
        thickness: dict[tuple[str, str], dict[str, float]]
                   {(pref, muni): {occ: index in [0, 200]}}
                   100 = 全国平均、200 = 全国平均の 2 倍以上 (cap)。

    前提:
      - nat_avg[occ] > 0 (ゼロ除算回避)
      - 出力は REAL (整数化しない)

    本実装: Phase 5
    """
    raise NotImplementedError(
        "Phase 5 で実装。全国平均を 100 として 0-200 にクリップ。"
    )


def derive_rank_and_priority(model_result: dict) -> dict:
    """全国順位 + percentile + 配信優先度 (A/B/C/D).

    アルゴリズム:
      rank[occ][(pref, muni)] = 全国 N_muni 中の順位 (大きい順、1-indexed)
      percentile[occ][(pref, muni)] = (N_muni - rank + 1) / N_muni × 100
      priority:
        percentile >= 90  → 'A' (最重点)
        70 <= < 90        → 'B'
        40 <= < 70        → 'C'
        < 40              → 'D' (低優先)

    Args:
        model_result: integrate_model_f2() の戻り値

    Returns:
        result: dict[tuple[str, str], dict[str, dict]]
                {(pref, muni): {occ: {'rank': int, 'percentile': float, 'priority': str}}}

    前提:
      - 同点処理は dense ranking (同値は同順位、次は +1)
      - N_muni ≒ 1740

    本実装: Phase 5
    """
    raise NotImplementedError(
        "Phase 5 で実装。全国順位 + percentile + A/B/C/D 配信優先度。"
    )


def derive_scenario_indices(
    model_result: dict,
    turnover_rates: Optional[dict] = None,
) -> dict:
    """シナリオ別索引 (1× / 3× / 5× は **指数の倍数**、人数ではない).

    アルゴリズム:
      scenario[muni][occ] = {
          'cons': index × turnover_rates['conservative'],
          'std':  index × turnover_rates['standard'],
          'agg':  index × turnover_rates['aggressive'],
      }
      ここで index は thickness_index ではなく integrated value。

    Args:
        model_result:   integrate_model_f2() の戻り値
        turnover_rates: シナリオ倍率 dict
                        (None で DEFAULT_TURNOVER_RATES = {1, 3, 5})

    Returns:
        scenarios: dict[tuple[str, str], dict[str, dict[str, float]]]
                   {(pref, muni): {occ: {'cons': float, 'std': float, 'agg': float}}}

    注意: 本値は **指数の倍数** であり、推定人数ではない。
          採用施策の感度分析用に「離職率 1× / 3× / 5× での濃淡変化」を見るための指標。

    本実装: Phase 5
    """
    if turnover_rates is None:
        turnover_rates = DEFAULT_TURNOVER_RATES
    raise NotImplementedError(
        "Phase 5 で実装。指数の倍数 (1×/3×/5×) を計算、人数ではない。"
    )


# ============================================================
# 出力検証 (Phase 5 で本実装、Plan B 制約)
# ============================================================

def validate_outputs(result_df) -> list[str]:
    """Plan B 出力制約の assert (Phase 5 で本実装).

    Plan B Workspace constraint:
      - basis must be 'resident' only (workplace は 15-1 専有)
      - data_label must be 'estimated_beta' only
      - age_class must be '_total' only (年齢別は 15-1 専有)
      - gender must be 'total' only (性別は 15-1 専有)

    追加 assert (Phase 5):
      - source_name == 'model_f2_v1'
      - weight_source == 'hypothesis_v1'
      - estimate_index in [0, 200]
      - municipality_code が 5 桁数字

    Args:
        result_df: pandas.DataFrame (export_to_csv 直前の出力)

    Returns:
        errors: list[str] (空なら制約満たす)

    本実装: Phase 5。Plan B 制約 4 件 + 拡張 4 件を assert する。
    """
    raise NotImplementedError(
        "Phase 5 で実装。Plan B 制約 (basis/data_label/age_class/gender) と "
        "拡張制約 (source_name/weight_source/estimate_index/code) を assert。"
    )


# ============================================================
# 出力 (Phase 5 で本実装)
# ============================================================

def export_to_csv(result_df, path: str) -> None:
    """CSV 出力 (UTF-8、quoting=csv.QUOTE_MINIMAL).

    Plan B 出力仕様:
      - カラム: OUTPUT_CSV_COLUMNS (15 列、定数参照)
      - basis = 'resident' のみ (workplace fallback は別タスク)
      - data_label = 'estimated_beta' 固定
      - age_class = '_total', gender = 'total' のみ
      - source_name = 'model_f2_v1'
      - weight_source = 'hypothesis_v1'
      - estimated_at = ISO8601 (UTC)

    出力前に validate_outputs() を実行し、制約違反があれば ValueError raise。

    Args:
        result_df: pandas.DataFrame
                   integrate_model_f2 + derive_thickness_index 後の結果を
                   long format に展開したもの。
        path:      出力 CSV パス

    Returns:
        None (副作用: ファイル書き込み)

    前提:
      - result_df.columns ⊇ OUTPUT_CSV_COLUMNS
      - validate_outputs(result_df) が空リストを返すこと

    本実装: Phase 5
    """
    raise NotImplementedError(
        "Phase 5 で実装。validate_outputs() で Plan B 制約確認後、"
        "OUTPUT_CSV_COLUMNS の順で CSV 出力。"
    )


def import_to_sqlite(
    csv_path: str,
    db_path: str,
    table_name: str,
) -> None:
    """ローカル DB 投入 (Plan B: INSERT OR REPLACE で resident 領域のみ更新).

    Plan B 重要制約:
      - DROP TABLE IF EXISTS は禁止 (15-1 投入と衝突するため)
      - 代わりに `DELETE FROM table WHERE basis='resident' AND data_label='estimated_beta'`
        → `INSERT` で resident/estimated_beta 領域のみ更新
      - basis='workplace', data_label='measured' のレコードは保持 (15-1 専有)

    Claude による実行禁止 (MEMORY 事故記録 2026-01-06):
      - 本関数はユーザーが手動で `python ... --build` 実行する想定
      - スケルトン状態でも Phase 5 でも、Claude が直接呼び出さない

    Args:
        csv_path:   export_to_csv() で出力した CSV パス
        db_path:    ローカル SQLite DB パス
        table_name: 出力先テーブル名 (= OUTPUT_TABLE_NAME)

    Returns:
        None

    本実装: Phase 5 (DB 書き込みはユーザー手動実行)
    """
    raise NotImplementedError(
        "DB 書き込みはユーザー手動実行。Plan B では INSERT OR REPLACE で "
        "basis='resident' AND data_label='estimated_beta' 領域のみ更新する "
        "(DROP TABLE 禁止、15-1 と衝突するため)。"
    )


# ============================================================
# メイン関数
# ============================================================

def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description=(
            "Build v2_municipality_target_thickness (Phase 3 Step 5, skeleton). "
            "Skeleton supports --dry-run / --validate-only only; --build is NotImplementedError."
        ),
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )

    mode_group = parser.add_mutually_exclusive_group(required=True)
    mode_group.add_argument(
        "--dry-run",
        action="store_true",
        help="Load all inputs, validate, and report counts. No computation, no writes.",
    )
    mode_group.add_argument(
        "--validate-only",
        action="store_true",
        help=(
            f"Run validation SQL on existing local DB table '{OUTPUT_TABLE_NAME}' (READ-only). "
            "Reports table-missing if not yet built."
        ),
    )
    mode_group.add_argument(
        "--build",
        action="store_true",
        help="Compute thickness, export CSV, optionally import to SQLite. NotImplementedError until sensitivity result is fixed.",
    )

    parser.add_argument(
        "--csv-only",
        action="store_true",
        help="With --build: emit CSV only, skip SQLite import.",
    )

    parser.add_argument("--db-path", default=str(DEFAULT_DB_PATH))
    parser.add_argument("--industry-csv", default=str(DEFAULT_INDUSTRY_CSV))
    parser.add_argument("--weight-csv", default=str(DEFAULT_WEIGHT_CSV))
    parser.add_argument("--salesnow-csv", default=str(DEFAULT_SALESNOW_CSV))
    parser.add_argument("--output-csv", default=str(DEFAULT_OUTPUT_CSV))
    parser.add_argument(
        "--table-name",
        default=OUTPUT_TABLE_NAME,
        help=f"Output table name (default: {OUTPUT_TABLE_NAME}).",
    )

    return parser.parse_args()


def _run_dry_run(args: argparse.Namespace) -> int:
    """--dry-run: 入力ロード + 検証. 計算しない、書き込まない."""
    print("=" * 70)
    print("[MODE] --dry-run")
    print("=" * 70)
    data = load_inputs(
        db_path=args.db_path,
        industry_csv=args.industry_csv,
        weight_csv=args.weight_csv,
        salesnow_csv=args.salesnow_csv,
    )

    print()
    print("[STEP] Validating inputs...")
    errors = validate_inputs(data)

    if errors:
        print(f"[FAIL] {len(errors)} validation error(s):")
        for i, err in enumerate(errors, start=1):
            print(f"       {i}. {err}")
    else:
        print("[OK] All input validations passed.")

    print()
    print("[INFO] Plan B output constraints (validate_outputs で assert 予定):")
    print(f"[INFO]   basis        = '{PLAN_B_BASIS}' のみ")
    print(f"[INFO]   data_label   = '{PLAN_B_DATA_LABEL}' 固定")
    print(f"[INFO]   age_class    = '{PLAN_B_AGE_CLASS}' のみ (年齢別は 15-1 専有)")
    print(f"[INFO]   gender       = '{PLAN_B_GENDER}' のみ (性別は 15-1 専有)")
    print(f"[INFO]   source_name  = '{PLAN_B_SOURCE_NAME}'")
    print(f"[INFO]   weight_source = '{PLAN_B_WEIGHT_SOURCE}'")

    print()
    print("[INFO] DEFAULT_ANCHOR_THRESHOLDS_v1 (Worker A3 sensitivity 推奨):")
    for k, v in DEFAULT_ANCHOR_THRESHOLDS_v1.items():
        print(f"[INFO]   {k} = {v}")

    print()
    print("=" * 70)
    print("[INFO] Build モードは未実装 (sensitivity 結果待ち)")
    print("[INFO] 次のステップ: scripts/proto_sensitivity_anchor_thresholds.py の結果を反映後、")
    print("[INFO]                compute_* 関数を本実装する。")
    print("=" * 70)

    # dry-run は検証エラー有無に関わらず exit 0 (情報提供モード)
    return 0


def _run_validate_only(args: argparse.Namespace) -> int:
    """--validate-only: 既存 DB の検証 SQL を実行 (READ-only)."""
    print("=" * 70)
    print("[MODE] --validate-only")
    print("=" * 70)
    print(f"[INFO] DB: {args.db_path}")
    print(f"[INFO] Table: {args.table_name}")
    print()

    result = run_validation_sql(db_path=args.db_path, table_name=args.table_name)

    print()
    print("=" * 70)
    if result["status"] == "ok":
        print(f"[OK] Validation SQL completed for table '{args.table_name}'")
    elif result["status"] == "table_missing":
        print(f"[INFO] Table '{args.table_name}' does not exist yet. ")
        print("[INFO] Run --build (after Phase 5 implementation) to create it.")
    elif result["status"] == "db_missing":
        print(f"[ERROR] {result['message']}")
        return 2
    print("=" * 70)
    return 0


def _run_build(args: argparse.Namespace) -> int:
    """--build: NotImplementedError until sensitivity 完了."""
    print("=" * 70)
    print("[MODE] --build")
    print("=" * 70)
    print()
    print("[ERROR] --build モードは未実装です (Phase 5 sensitivity 結果待ち)")
    print()
    print("実装予定の処理フロー:")
    print("  1. load_inputs()")
    print("  2. compute_baseline() → F1 × F2 (生産年齢人口比按分)")
    print("  3. compute_f3() → 産業構成補正")
    print("  4. compute_f4_occupation_weighted() → 昼夜間 × 職業重み")
    print("  5. compute_f5() → 通勤流入補正")
    print("  6. compute_industrial_anchor() → 4 条件 AND")
    print("  7. compute_f6_v2() → anchor 分岐 + HQ-with-factory boost")
    print("  8. integrate_model_f2() → 統合 + 都道府県スケーリング")
    print("  9. derive_thickness_index() → 0-200 正規化")
    print(" 10. derive_rank_and_priority() → 全国順位 + A/B/C/D")
    print(" 11. derive_scenario_indices() → 1× / 3× / 5×")
    print(" 12. export_to_csv()")
    if not args.csv_only:
        print(" 13. import_to_sqlite() (Claude 単独実行禁止、--csv-only 推奨)")
    print()
    print("次のステップ:")
    print("  - scripts/proto_sensitivity_anchor_thresholds.py の sensitivity 分析結果を確認")
    print("  - DEFAULT_ANCHOR_THRESHOLDS を更新")
    print("  - compute_* 関数を proto_evaluate_occupation_population_models.py から移植")
    return 1


def main() -> int:
    args = parse_args()

    if args.dry_run:
        return _run_dry_run(args)
    elif args.validate_only:
        return _run_validate_only(args)
    elif args.build:
        return _run_build(args)
    else:
        # argparse mutually_exclusive_group(required=True) で未到達
        print("[ERROR] No mode specified. Use --dry-run / --validate-only / --build", file=sys.stderr)
        return 2


if __name__ == "__main__":
    sys.exit(main())
