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
import math
import sqlite3
import sys
from collections import defaultdict
from datetime import datetime, timezone
from pathlib import Path
from typing import Optional

try:
    import pandas as pd
except ImportError:  # pragma: no cover
    pd = None  # type: ignore

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
# proto から移植した計算定数
# ============================================================

# 国勢調査 R2 全国就業者の職業大分類構成比 (proto と同一値、再正規化済)
_RAW_NATIONAL_OCCUPATION_RATIO = {
    "01_管理": 0.030,
    "02_専門技術": 0.183,
    "03_事務": 0.197,
    "04_販売": 0.139,
    "05_サービス": 0.127,
    "06_保安": 0.018,
    "07_農林漁業": 0.030,
    "08_生産工程": 0.130,
    "09_輸送機械": 0.061,
    "10_建設採掘": 0.064,
    "11_運搬清掃": 0.073,
}
_TOTAL_OCC_RATIO = sum(_RAW_NATIONAL_OCCUPATION_RATIO.values())
NATIONAL_OCCUPATION_RATIO = {k: v / _TOTAL_OCC_RATIO for k, v in _RAW_NATIONAL_OCCUPATION_RATIO.items()}

# 出力 CSV の occupation_name 列用 (コード = 名前)
OCCUPATION_NAMES = {k: k for k in NATIONAL_OCCUPATION_RATIO}

# Model E2 F4 職業別重み (proto OCCUPATION_F4_WEIGHT と同一)
OCCUPATION_F4_WEIGHT = {
    "01_管理":     1.0,
    "02_専門技術": 1.0,
    "03_事務":     1.0,
    "04_販売":     0.7,
    "05_サービス": 0.5,
    "06_保安":     0.5,
    "07_農林漁業": 0.0,
    "08_生産工程": 0.3,
    "09_輸送機械": 0.3,
    "10_建設採掘": 0.5,
    "11_運搬清掃": 0.5,
}

# 全国就業率 (proto build_pref_occupation_ground_truth と同一)
NATIONAL_EMPLOYMENT_RATE = 0.75

# F3 べき乗指数 (proto F3_POWER_E3 = 1.5)
F3_POWER = 1.5

# Model E 用に採用する産業コード (proto と同一)
TARGET_INDUSTRY_CODES = (
    "AB", "C", "D", "E", "F", "G", "H", "I", "J", "K", "L", "M", "N", "O", "P", "Q", "R"
)

# 政令市の区を除外するための area_type 集合 (proto と同一)
INCLUDE_AREA_TYPES = {"aggregate_city", "municipality", "special_ward",
                      "aggregate_special_wards"}

# F6 関連 (proto と同一値)
F6_ALPHA = 0.6
F6_BETA = 0.6
F6_FACTORY_BOOST_THRESHOLD = 1.5
F6_HQ_EXCESS_THRESHOLD = 2.5
F6_HQ_EXCESS_THRESHOLD_BY_IND = {"D": 3.0, "E": 4.0, "H": 2.5}
F6_BLUE_COLLAR_OCCUPATIONS = ("08_生産工程", "09_輸送機械", "10_建設採掘", "11_運搬清掃")
F6_TARGET_JSIC_INDUSTRIES = ("D", "E", "H")

# Anchor judgment (proto と同一値、DEFAULT_ANCHOR_THRESHOLDS_v1 と整合)
ANCHOR_MFG_SHARE_MIN = DEFAULT_ANCHOR_THRESHOLDS_v1["mfg_share"]
ANCHOR_EMP_PER_EST_MIN = DEFAULT_ANCHOR_THRESHOLDS_v1["mfg_emp_per_establishment"]
ANCHOR_DN_RATIO_MAX = DEFAULT_ANCHOR_THRESHOLDS_v1["day_night_ratio"]
ANCHOR_HQ_EXCESS_E_MAX = DEFAULT_ANCHOR_THRESHOLDS_v1["hq_excess_ratio_E"]
ANCHOR_BOOST_GAMMA = 6.0
ANCHOR_BOOST_CAP = 2.5


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


def _build_industry_structures(
    industry_csv_rows: list[dict],
    weight_csv_rows: list[dict],
    muni_master: dict,
) -> dict:
    """proto load_industry_data + load_industry_establishments を移植.

    Returns dict with keys:
      industry_share, national_share, weights, industry_emp, national_emp, industry_est
    """
    # ---- 1. weights ----
    weights: dict = {}
    for row in weight_csv_rows:
        try:
            ind = row["industry_code"]
            occ = row["occupation_code"]
            w = float(row["weight"])
        except (KeyError, ValueError, TypeError):
            continue
        weights[(ind, occ)] = w
    # AB = (A + B) / 2 補完 (proto と同一)
    if ("A", "01_管理") in weights and ("B", "01_管理") in weights:
        for occ in NATIONAL_OCCUPATION_RATIO:
            wa = weights.get(("A", occ), 0.0)
            wb = weights.get(("B", occ), 0.0)
            weights[("AB", occ)] = (wa + wb) / 2.0

    # ---- 2. master code -> (pref, muni) ----
    code_to_pref_muni: dict = {}
    for code, info in muni_master["by_code"].items():
        if info["area_type"] in INCLUDE_AREA_TYPES:
            code_to_pref_muni[code] = (info["prefecture"], info["name"])

    # ---- 3. industry_emp / national_emp / industry_est ----
    industry_emp: dict = defaultdict(lambda: defaultdict(float))
    national_emp: dict = defaultdict(float)
    industry_est: dict = defaultdict(lambda: defaultdict(float))

    for row in industry_csv_rows:
        ind = row.get("industry_code")
        if ind not in TARGET_INDUSTRY_CODES:
            continue
        city_code_raw = row.get("city_code")
        if not city_code_raw:
            continue
        try:
            city_code = str(int(str(city_code_raw).strip())).zfill(5)
        except (ValueError, TypeError):
            continue
        pref_muni = code_to_pref_muni.get(city_code)
        if pref_muni is None:
            continue
        try:
            emp = float(row["employees_total"]) if row.get("employees_total") else 0.0
        except (ValueError, TypeError):
            emp = 0.0
        if emp > 0:
            industry_emp[pref_muni][ind] += emp
            national_emp[ind] += emp
        try:
            est = float(row["establishments"]) if row.get("establishments") else 0.0
        except (ValueError, TypeError):
            est = 0.0
        if est > 0:
            industry_est[pref_muni][ind] += est

    # ---- 4. industry_share / national_share ----
    industry_share: dict = {}
    for pref_muni, ind_dict in industry_emp.items():
        total = sum(ind_dict.values())
        if total <= 0:
            continue
        industry_share[pref_muni] = {ind: emp / total for ind, emp in ind_dict.items()}
    national_total = sum(national_emp.values())
    national_share = ({ind: emp / national_total for ind, emp in national_emp.items()}
                      if national_total > 0 else {})

    return {
        "weights": weights,
        "industry_emp": {pm: dict(d) for pm, d in industry_emp.items()},
        "industry_est": {pm: dict(d) for pm, d in industry_est.items()},
        "industry_share": industry_share,
        "national_share": national_share,
        "national_emp": dict(national_emp),
    }


def _build_salesnow_sn_emp(salesnow_csv_rows: list[dict]) -> dict:
    """salesnow_aggregate_for_f6.csv の集約結果を sn_emp[(pref, muni, jsic)] -> emp に変換."""
    sn_emp: dict = {}
    for row in salesnow_csv_rows:
        try:
            key = (row["prefecture"], row["municipality"], row["jsic_code"])
            sn_emp[key] = float(row["total_employees"])
        except (KeyError, ValueError, TypeError):
            continue
    return sn_emp


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

    # 派生構造の構築 (本実装で必要)
    ind_struct = _build_industry_structures(industry_csv_rows, weight_csv_rows, muni_master)
    sn_emp = _build_salesnow_sn_emp(salesnow_csv_rows)

    # 都道府県集計 (生産年齢人口)
    pref_age15_64: dict = defaultdict(int)
    pref_total: dict = defaultdict(int)
    for (pref, muni), v in population.items():
        pref_total[pref] += v.get("total", 0) or 0
        pref_age15_64[pref] += v.get("age_15_64", 0) or 0

    # municipality_code_map: (pref, muni) -> 5桁 JIS
    municipality_code_map: dict = {
        (info["prefecture"], info["name"]): code
        for code, info in muni_master["by_code"].items()
        if info["area_type"] in INCLUDE_AREA_TYPES
    }

    # pref_occ_target (proto build_pref_occupation_ground_truth と同等)
    pref_occ_target: dict = defaultdict(dict)
    for pref, age15_64 in pref_age15_64.items():
        pref_employment = age15_64 * NATIONAL_EMPLOYMENT_RATE
        for occ, ratio in NATIONAL_OCCUPATION_RATIO.items():
            pref_occ_target[pref][occ] = pref_employment * ratio

    print(f"[INFO]   industry_emp: {len(ind_struct['industry_emp'])} (pref, muni) keys")
    print(f"[INFO]   industry_share: {len(ind_struct['industry_share'])} (pref, muni) keys")
    print(f"[INFO]   weights: {len(ind_struct['weights'])} (industry, occupation) keys")
    print(f"[INFO]   sn_emp: {len(sn_emp)} (pref, muni, jsic) keys")
    print(f"[INFO]   municipality_code_map: {len(municipality_code_map)} (pref, muni) keys")

    return {
        "population": population,
        "pyramid": pyramid,
        "daytime": daytime,
        "commute_flow": commute_flow,
        "muni_master": muni_master,
        "industry_csv_rows": industry_csv_rows,
        "weight_csv_rows": weight_csv_rows,
        "salesnow_csv_rows": salesnow_csv_rows,
        # 派生構造
        "weights": ind_struct["weights"],
        "industry_emp": ind_struct["industry_emp"],
        "industry_est": ind_struct["industry_est"],
        "industry_share": ind_struct["industry_share"],
        "national_share": ind_struct["national_share"],
        "national_emp": ind_struct["national_emp"],
        "sn_emp": sn_emp,
        "pref_age15_64": dict(pref_age15_64),
        "pref_total": dict(pref_total),
        "pref_occ_target": {p: dict(d) for p, d in pref_occ_target.items()},
        "municipality_code_map": municipality_code_map,
        "national_occ_ratio": NATIONAL_OCCUPATION_RATIO,
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
    """F1 + F2: 都道府県職業就業者数を生産年齢人口比で市区町村に按分 (proto model_b 移植).

    Plan B: age_class='_total', gender='total' の集約値のみ生成。
    """
    pref_age15_64 = data["pref_age15_64"]
    pref_occ_target = data["pref_occ_target"]
    out: dict = defaultdict(dict)
    for (pref, muni), v in data["population"].items():
        denom = pref_age15_64.get(pref, 0) or 0
        if denom <= 0:
            continue
        ratio = (v.get("age_15_64", 0) or 0) / denom
        for occ, pref_pop in pref_occ_target.get(pref, {}).items():
            out[(pref, muni)][occ] = pref_pop * ratio
    return dict(out)


def compute_f3(data: dict, weight_matrix: dict, power: float = F3_POWER) -> dict:
    """F3 産業構成補正 (べき乗 1.5、proto model_e/model_f2 から移植).

    weight_matrix: dict[(industry_code, occupation_code)] -> weight
    """
    industry_share = data["industry_share"]
    national_share = data["national_share"]

    # 国内ベース denom
    nat_denom: dict = {}
    for occ in NATIONAL_OCCUPATION_RATIO:
        nat_denom[occ] = sum(
            national_share.get(ind, 0.0) * weight_matrix.get((ind, occ), 0.0)
            for ind in TARGET_INDUSTRY_CODES
        )

    out: dict = defaultdict(dict)
    # baseline と同じキー集合で f3 を返す (欠損 muni は f3=1.0)
    keys = set(data["population"].keys())
    for (pref, muni) in keys:
        ind_share = industry_share.get((pref, muni))
        if ind_share is None:
            for occ in NATIONAL_OCCUPATION_RATIO:
                out[(pref, muni)][occ] = 1.0
            continue
        for occ in NATIONAL_OCCUPATION_RATIO:
            num = sum(
                ind_share.get(ind, 0.0) * weight_matrix.get((ind, occ), 0.0)
                for ind in TARGET_INDUSTRY_CODES
            )
            denom = nat_denom.get(occ, 0.0)
            ratio = (num / denom) if denom > 0 else 1.0
            if ratio > 0 and power != 1.0:
                ratio = ratio ** power
            out[(pref, muni)][occ] = ratio
    return dict(out)


def compute_f4_occupation_weighted(data: dict, basis: str = "resident") -> dict:
    """F4 昼夜間人口比 × 職業別重み (Model E2).

    Plan B (basis='resident'): 居住地ベースなので workplace 補正は不要 → 全職業 1.0 固定。
    workplace fallback では proto Model E2 と同じ計算。
    """
    out: dict = defaultdict(dict)
    if basis == "resident":
        for key in data["population"]:
            for occ in NATIONAL_OCCUPATION_RATIO:
                out[key][occ] = 1.0
        return dict(out)

    # workplace fallback (将来用)
    for (pref, muni) in data["population"]:
        d = data["daytime"].get((pref, muni))
        f4_raw = 1.0
        if d and d.get("night", 0) > 0:
            f4_raw = (d.get("day", 0) or 0) / d["night"]
            f4_raw = max(0.1, min(f4_raw, 5.0))
        for occ in NATIONAL_OCCUPATION_RATIO:
            w_occ = OCCUPATION_F4_WEIGHT.get(occ, 1.0)
            f4 = 1.0 + (f4_raw - 1.0) * w_occ
            f4 = max(0.1, min(f4, 5.0))
            out[(pref, muni)][occ] = f4
    return dict(out)


def compute_f5(data: dict, basis: str = "resident") -> dict:
    """F5 通勤流入補正 (職業非依存 scalar).

    Plan B (basis='resident'): 居住者は流入元と関係ない → f5=1.0 固定。
    workplace fallback では proto F5 と同じ計算。
    """
    out: dict = {}
    if basis == "resident":
        for key in data["population"]:
            out[key] = 1.0
        return out

    for (pref, muni) in data["population"]:
        d = data["daytime"].get((pref, muni))
        f5 = 1.0
        if d and d.get("night", 0) > 0:
            inflow = d.get("inflow", 0) or 0
            inflow_rate = inflow / d["night"]
            f5 = 1.0 + (inflow_rate * 0.3)
            f5 = max(0.5, min(f5, 2.0))
        out[(pref, muni)] = f5
    return out


def compute_industrial_anchor(
    data: dict,
    thresholds: Optional[dict] = None,
) -> set:
    """4 条件 AND で industrial_anchor_city を判定 (proto compute_industrial_anchor_cities 移植).
    """
    if thresholds is None:
        thresholds = DEFAULT_ANCHOR_THRESHOLDS_v1

    industry_emp = data["industry_emp"]
    industry_est = data["industry_est"]
    sn_emp = data["sn_emp"]
    national_emp = data["national_emp"]
    daytime = data["daytime"]

    # 全国 SalesNow 製造業 E 総数
    national_sn_E = 0.0
    for (pref, muni, ind), emp in sn_emp.items():
        if ind == "E":
            national_sn_E += emp
    national_estat_E = national_emp.get("E", 0.0)

    anchor_set: set = set()
    all_munis = set(industry_emp.keys()) | set(data["population"].keys())

    mfg_share_min = thresholds["mfg_share"]
    emp_per_est_min = thresholds["mfg_emp_per_establishment"]
    dn_ratio_max = thresholds["day_night_ratio"]
    hq_excess_e_max = thresholds["hq_excess_ratio_E"]

    for (pref, muni) in all_munis:
        ind_emp = industry_emp.get((pref, muni), {})
        ind_est = industry_est.get((pref, muni), {})

        e_emp = ind_emp.get("E", 0.0)
        total_emp = sum(v for v in ind_emp.values())
        mfg_share = (e_emp / total_emp) if total_emp > 0 else 0.0
        cond_a = mfg_share > mfg_share_min

        e_est = ind_est.get("E", 0.0)
        emp_per_est = (e_emp / e_est) if e_est > 0 else 0.0
        cond_b = emp_per_est > emp_per_est_min

        d = daytime.get((pref, muni))
        dn_ratio = (d["ratio"] if d else 100.0)
        cond_c = dn_ratio < dn_ratio_max

        sn_E = sn_emp.get((pref, muni, "E"), 0.0)
        if national_sn_E > 0 and e_emp > 0 and national_estat_E > 0:
            sn_intensity = sn_E / national_sn_E
            estat_intensity = e_emp / national_estat_E
            hq_excess = (sn_intensity / estat_intensity) if estat_intensity > 0 else 999.0
        elif national_sn_E > 0 and e_emp == 0 and sn_E > 0:
            hq_excess = 999.0
        else:
            hq_excess = 0.0
        cond_d = hq_excess < hq_excess_e_max

        if cond_a and cond_b and cond_c and cond_d:
            anchor_set.add((pref, muni))
    return anchor_set


def compute_f6_v2(data: dict, anchor_set: set, basis: str = "resident") -> dict:
    """F6 anchor 分岐 (本社減衰 + 工場ブースト、proto compute_f6_factor_v2 移植).

    Plan B (basis='resident'): 本社過剰は workplace 概念 → f6=1.0 固定。
    workplace fallback では proto と同じ計算。
    """
    out: dict = defaultdict(dict)
    if basis == "resident":
        for key in data["population"]:
            for occ in NATIONAL_OCCUPATION_RATIO:
                out[key][occ] = 1.0
        return dict(out)

    sn_emp = data["sn_emp"]
    industry_emp = data["industry_emp"]
    industry_est = data["industry_est"]
    national_emp = data["national_emp"]
    weights = data["weights"]
    alpha = F6_ALPHA

    national_sn_emp: dict = defaultdict(float)
    for (pref, muni, ind), emp in sn_emp.items():
        national_sn_emp[ind] += emp

    hq_excess_ratio: dict = defaultdict(dict)
    factory_excess_ratio: dict = defaultdict(dict)
    all_munis: set = set()
    for (pref, muni, ind) in sn_emp.keys():
        all_munis.add((pref, muni))
    for pref_muni in industry_emp.keys():
        all_munis.add(pref_muni)
    for pref_muni in data["population"]:
        all_munis.add(pref_muni)

    for (pref, muni) in all_munis:
        for ind in F6_TARGET_JSIC_INDUSTRIES:
            sn = sn_emp.get((pref, muni, ind), 0.0)
            nat_sn = national_sn_emp.get(ind, 0.0)
            estat_v = industry_emp.get((pref, muni), {}).get(ind, 0.0)
            estat_total = national_emp.get(ind, 0.0)
            if nat_sn <= 0:
                hq_excess_ratio[(pref, muni)][ind] = 1.0
                factory_excess_ratio[(pref, muni)][ind] = 1.0
                continue
            sn_intensity = sn / nat_sn if sn > 0 else 0.0
            if estat_v > 0 and estat_total > 0:
                estat_intensity = estat_v / estat_total
                if sn > 0 and sn_intensity > 0:
                    hq_ratio = sn_intensity / estat_intensity
                    factory_ratio = estat_intensity / sn_intensity
                else:
                    hq_ratio = 1.0
                    factory_ratio = 5.0
                hq_excess_ratio[(pref, muni)][ind] = min(hq_ratio, 10.0)
                factory_excess_ratio[(pref, muni)][ind] = min(factory_ratio, 5.0)
            else:
                if sn > 0 and len(all_munis) > 0:
                    hq_ratio = sn_intensity * len(all_munis)
                else:
                    hq_ratio = 1.0
                hq_excess_ratio[(pref, muni)][ind] = min(hq_ratio, 10.0)
                factory_excess_ratio[(pref, muni)][ind] = 1.0

    f6: dict = {}
    for (pref, muni) in all_munis:
        per_occ_factor = {occ: 1.0 for occ in NATIONAL_OCCUPATION_RATIO}
        is_anchor = (pref, muni) in anchor_set
        ratio_dict = hq_excess_ratio.get((pref, muni), {})

        if not is_anchor:
            for ind, ratio in ratio_dict.items():
                ind_threshold = F6_HQ_EXCESS_THRESHOLD_BY_IND.get(ind, F6_HQ_EXCESS_THRESHOLD)
                if ratio > ind_threshold:
                    damping = 1.0 / (1.0 + alpha * (ratio - 1.0))
                    for occ in F6_BLUE_COLLAR_OCCUPATIONS:
                        bcs = weights.get((ind, occ), 0.0)
                        damp_amount = bcs * (1.0 - damping)
                        per_occ_factor[occ] *= (1.0 - damp_amount)

        if F6_BETA > 0:
            factory_dict = factory_excess_ratio.get((pref, muni), {})
            for ind, f_ratio in factory_dict.items():
                if f_ratio > F6_FACTORY_BOOST_THRESHOLD:
                    boost = 1.0 + F6_BETA * (f_ratio - 1.0)
                    boost = min(boost, 1.4)
                    for occ in F6_BLUE_COLLAR_OCCUPATIONS:
                        bcs = weights.get((ind, occ), 0.0)
                        boost_amount = bcs * (boost - 1.0)
                        per_occ_factor[occ] *= (1.0 + boost_amount)

        if is_anchor:
            ind_emp_local = industry_emp.get((pref, muni), {})
            ind_est_local = industry_est.get((pref, muni), {})
            e_emp = ind_emp_local.get("E", 0.0)
            total_ind_emp = sum(v for v in ind_emp_local.values())
            mfg_share_v = (e_emp / total_ind_emp) if total_ind_emp > 0 else 0.0
            e_est = ind_est_local.get("E", 0.0)
            emp_per_est_v = (e_emp / e_est) if e_est > 0 else 0.0
            if mfg_share_v > ANCHOR_MFG_SHARE_MIN and emp_per_est_v > 0:
                share_excess = max(0.0, mfg_share_v - ANCHOR_MFG_SHARE_MIN)
                size_factor = math.sqrt(emp_per_est_v / ANCHOR_EMP_PER_EST_MIN)
                anchor_boost = 1.0 + ANCHOR_BOOST_GAMMA * share_excess * size_factor
                anchor_boost = min(anchor_boost, ANCHOR_BOOST_CAP)
                for occ in F6_BLUE_COLLAR_OCCUPATIONS:
                    bcs = weights.get(("E", occ), 0.0)
                    boost_amount = bcs * (anchor_boost - 1.0)
                    per_occ_factor[occ] *= (1.0 + boost_amount)

        f6[(pref, muni)] = per_occ_factor

    return f6


def integrate_model_f2(
    baseline: dict,
    f3: dict,
    f4: dict,
    f5: dict,
    f6: dict,
    pref_occ_target: dict,
) -> dict:
    """全要素統合 + 都道府県スケーリング (proto model_f2 末尾と同等)."""
    raw: dict = defaultdict(dict)
    for (pref, muni), occ_dict in baseline.items():
        f3_pm = f3.get((pref, muni), {})
        f4_pm = f4.get((pref, muni), {})
        f5_pm = f5.get((pref, muni), 1.0)
        f6_pm = f6.get((pref, muni), {})
        for occ, base_pop in occ_dict.items():
            v = (base_pop
                 * f3_pm.get(occ, 1.0)
                 * f4_pm.get(occ, 1.0)
                 * f5_pm
                 * f6_pm.get(occ, 1.0))
            raw[(pref, muni)][occ] = v

    pref_raw_sum: dict = defaultdict(lambda: defaultdict(float))
    for (pref, muni), occ_dict in raw.items():
        for occ, v in occ_dict.items():
            pref_raw_sum[pref][occ] += v

    out: dict = defaultdict(dict)
    for (pref, muni), occ_dict in raw.items():
        for occ, v in occ_dict.items():
            target = pref_occ_target.get(pref, {}).get(occ, 0)
            denom = pref_raw_sum[pref].get(occ, 0)
            scaling = (target / denom) if denom > 0 else 1.0
            out[(pref, muni)][occ] = v * scaling
    return dict(out)


# ============================================================
# 派生指標
# ============================================================

def derive_thickness_index(model_result: dict) -> dict:
    """0-200 正規化 (100 = 全国平均、cap 200)."""
    occ_vals: dict = defaultdict(list)
    for (_pref, _muni), occ_dict in model_result.items():
        for occ, v in occ_dict.items():
            occ_vals[occ].append(v)
    nat_avg = {occ: (sum(vs) / len(vs)) if vs else 1.0 for occ, vs in occ_vals.items()}

    out: dict = defaultdict(dict)
    for (pref, muni), occ_dict in model_result.items():
        for occ, v in occ_dict.items():
            avg = nat_avg.get(occ, 1.0) or 1.0
            idx = (v / avg) * 100 if avg > 0 else 100.0
            out[(pref, muni)][occ] = max(0.0, min(idx, 200.0))
    return dict(out)


def derive_rank_and_priority(model_result: dict) -> dict:
    """全国順位 + percentile + A/B/C/D priority (大きい順、1-indexed)."""
    out: dict = defaultdict(dict)
    for occ in NATIONAL_OCCUPATION_RATIO:
        items = []
        for (pref, muni), occ_dict in model_result.items():
            v = occ_dict.get(occ, 0.0)
            items.append(((pref, muni), v))
        items.sort(key=lambda x: -x[1])
        n = len(items) or 1
        for rank, ((pref, muni), _v) in enumerate(items, start=1):
            percentile = rank / n
            if percentile <= 0.05:
                priority = "A"
            elif percentile <= 0.15:
                priority = "B"
            elif percentile <= 0.50:
                priority = "C"
            else:
                priority = "D"
            out[(pref, muni)][occ] = {
                "rank": rank,
                "percentile": round(percentile, 4),
                "priority": priority,
            }
    return dict(out)


def derive_scenario_indices(
    model_result: dict,
    turnover_rates: Optional[dict] = None,
) -> dict:
    """1× / 3× / 5× の指数倍数 (人数ではない)."""
    if turnover_rates is None:
        turnover_rates = DEFAULT_TURNOVER_RATES

    thickness = derive_thickness_index(model_result)
    out: dict = defaultdict(dict)
    for (pref, muni), occ_dict in thickness.items():
        for occ, idx in occ_dict.items():
            out[(pref, muni)][occ] = {
                "conservative_index": int(idx * turnover_rates["conservative"]),
                "standard_index": int(idx * turnover_rates["standard"]),
                "aggressive_index": int(idx * turnover_rates["aggressive"]),
            }
    return dict(out)


# ============================================================
# 出力検証
# ============================================================

def validate_outputs(result_df) -> list:
    """Plan B 制約 + 拡張 4 件の assert."""
    errors: list = []
    if pd is None:
        errors.append("pandas not installed; cannot validate DataFrame")
        return errors

    if not (result_df["basis"] == PLAN_B_BASIS).all():
        errors.append(f"basis must be '{PLAN_B_BASIS}' only")
    if not (result_df["data_label"] == PLAN_B_DATA_LABEL).all():
        errors.append(f"data_label must be '{PLAN_B_DATA_LABEL}' only")
    if not (result_df["age_class"] == PLAN_B_AGE_CLASS).all():
        errors.append(f"age_class must be '{PLAN_B_AGE_CLASS}' only")
    if not (result_df["gender"] == PLAN_B_GENDER).all():
        errors.append(f"gender must be '{PLAN_B_GENDER}' only")
    if not (result_df["source_name"] == PLAN_B_SOURCE_NAME).all():
        errors.append(f"source_name must be '{PLAN_B_SOURCE_NAME}' only")
    if not (result_df["weight_source"] == PLAN_B_WEIGHT_SOURCE).all():
        errors.append(f"weight_source must be '{PLAN_B_WEIGHT_SOURCE}' only")
    if result_df["estimate_index"].isna().any():
        errors.append("estimate_index has NaN")
    bad_idx = (result_df["estimate_index"] < 0) | (result_df["estimate_index"] > 200)
    if bad_idx.any():
        errors.append(f"estimate_index out of [0, 200] range: {int(bad_idx.sum())} rows")
    code_re = result_df["municipality_code"].astype(str).str.match(r"^\d{5}$")
    if not code_re.all():
        errors.append(f"municipality_code is not all 5-digit JIS: {int((~code_re).sum())} rows")
    return errors


# ============================================================
# 出力
# ============================================================

def export_to_csv(
    thickness: dict,
    rank_priority: dict,
    scenario: dict,
    anchor_set: set,
    municipality_code_map: dict,
    output_path,
) -> int:
    """Plan B 制約準拠の CSV 出力. Returns row count."""
    if pd is None:
        raise RuntimeError("pandas is required for export_to_csv")

    output_path = Path(output_path)
    output_path.parent.mkdir(parents=True, exist_ok=True)

    estimated_at = datetime.now(timezone.utc).isoformat()
    rows = []
    skipped_no_code = 0
    for (pref, muni), occ_dict in thickness.items():
        muni_code = municipality_code_map.get((pref, muni))
        if not muni_code:
            skipped_no_code += 1
            continue
        is_anchor = 1 if (pref, muni) in anchor_set else 0
        for occ_code in NATIONAL_OCCUPATION_RATIO:
            idx = occ_dict.get(occ_code, 0.0)
            rp = rank_priority.get((pref, muni), {}).get(occ_code, {})
            sc = scenario.get((pref, muni), {}).get(occ_code, {})
            rows.append({
                "municipality_code": muni_code,
                "prefecture": pref,
                "municipality_name": muni,
                "basis": PLAN_B_BASIS,
                "occupation_code": occ_code,
                "occupation_name": OCCUPATION_NAMES.get(occ_code, occ_code),
                "age_class": PLAN_B_AGE_CLASS,
                "gender": PLAN_B_GENDER,
                "estimate_index": round(float(idx), 2),
                "data_label": PLAN_B_DATA_LABEL,
                "source_name": PLAN_B_SOURCE_NAME,
                "source_year": 2026,
                "weight_source": PLAN_B_WEIGHT_SOURCE,
                "is_industrial_anchor": is_anchor,
                "estimated_at": estimated_at,
                "rank_in_occupation": rp.get("rank"),
                "rank_percentile": rp.get("percentile"),
                "distribution_priority": rp.get("priority"),
                "scenario_conservative_index": sc.get("conservative_index"),
                "scenario_standard_index": sc.get("standard_index"),
                "scenario_aggressive_index": sc.get("aggressive_index"),
            })

    df = pd.DataFrame(rows)
    df.to_csv(output_path, index=False, encoding="utf-8")
    print(f"[exported] {output_path} ({len(df):,} rows, skipped_no_code={skipped_no_code})")
    return len(df)


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
    """--build [--csv-only]: フル計算 + CSV 出力 + 出力検証.

    Plan B (basis='resident', estimated_beta) のみ生成。DB 投入はユーザー手動。
    """
    print("=" * 70)
    print("[MODE] --build")
    print(f"[INFO] csv_only={args.csv_only}")
    print("=" * 70)

    print("[build] loading inputs...")
    data = load_inputs(
        db_path=args.db_path,
        industry_csv=args.industry_csv,
        weight_csv=args.weight_csv,
        salesnow_csv=args.salesnow_csv,
    )

    print()
    print("[build] validating inputs...")
    in_errors = validate_inputs(data)
    if in_errors:
        print(f"[FAIL] {len(in_errors)} input validation error(s):")
        for i, e in enumerate(in_errors, start=1):
            print(f"       {i}. {e}")
        return 2
    print("[OK] inputs valid.")

    basis = PLAN_B_BASIS  # 'resident'

    print()
    print("[build] computing baseline (F1+F2)...")
    baseline = compute_baseline(data)
    print(f"[INFO]   baseline: {len(baseline)} muni keys")

    print("[build] computing F3 (industry correction, power=1.5)...")
    f3 = compute_f3(data, data["weights"])

    print(f"[build] computing F4 (basis='{basis}': resident=no-op)...")
    f4 = compute_f4_occupation_weighted(data, basis=basis)

    print(f"[build] computing F5 (basis='{basis}': resident=no-op)...")
    f5 = compute_f5(data, basis=basis)

    print("[build] computing industrial_anchor (4-cond AND)...")
    anchor_set = compute_industrial_anchor(data, thresholds=DEFAULT_ANCHOR_THRESHOLDS_v1)
    print(f"[INFO]   anchor cities: {len(anchor_set)}")

    print(f"[build] computing F6 v2 (basis='{basis}': resident=no-op)...")
    f6 = compute_f6_v2(data, anchor_set, basis=basis)

    print("[build] integrating Model F2 (raw × scaling)...")
    model_result = integrate_model_f2(
        baseline, f3, f4, f5, f6, data["pref_occ_target"]
    )
    print(f"[INFO]   model_result: {len(model_result)} muni keys")

    print("[build] deriving thickness_index (0-200)...")
    thickness = derive_thickness_index(model_result)

    print("[build] deriving rank/percentile/priority (A/B/C/D)...")
    rank_priority = derive_rank_and_priority(model_result)

    print("[build] deriving scenario indices (1×/3×/5×)...")
    scenario = derive_scenario_indices(model_result)

    print("[build] exporting CSV...")
    output_path = Path(args.output_csv)
    n_rows = export_to_csv(
        thickness, rank_priority, scenario, anchor_set,
        data["municipality_code_map"], output_path,
    )

    print()
    print("[build] validating outputs...")
    if pd is None:
        print("[ERROR] pandas not installed; cannot validate output CSV")
        return 3
    df = pd.read_csv(output_path, dtype={"municipality_code": str})
    out_errors = validate_outputs(df)
    if out_errors:
        print(f"[FAIL] {len(out_errors)} output validation error(s):")
        for i, e in enumerate(out_errors, start=1):
            print(f"       {i}. {e}")
        return 4
    print(f"[OK] CSV exported and validated. rows={len(df):,}")

    if args.csv_only:
        print()
        print("[INFO] --csv-only 指定のため DB 投入はスキップ。")
        print(f"[INFO] 次のステップ: ユーザー手動で {output_path} を Plan B 制約に従って")
        print("       INSERT (DROP TABLE 禁止、basis='resident' 領域のみ更新)。")
        return 0

    # DB 投入は NotImplementedError 維持 (ユーザー手動実行が前提)
    print()
    print("[ERROR] DB 投入は Claude 実行禁止 (MEMORY 事故記録 2026-01-06)。")
    print("[ERROR] --csv-only を指定して CSV 出力までで停止してください。")
    return 5


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
