# -*- coding: utf-8 -*-
"""
build_municipality_target_thickness.py
=======================================

Phase 3 Step 5 本実装スクリプト (スケルトン版)

ステータス: スケルトン (sensitivity 結果待ち)
- ✅ --dry-run / --validate-only モード実装済 (READ-only)
- ⏳ --build モード未実装 (compute_* 関数は NotImplementedError)

実装ロードマップ: docs/SURVEY_MARKET_INTELLIGENCE_PHASE3_F2_IMPLEMENTATION_PLAN.md
プロト元: scripts/proto_evaluate_occupation_population_models.py (model_f2)
DDL: docs/SURVEY_MARKET_INTELLIGENCE_PHASE3_TABLE_RENAME_DECISION.md (v2_municipality_target_thickness)

CLI:
  python scripts/build_municipality_target_thickness.py --dry-run
  python scripts/build_municipality_target_thickness.py --validate-only
  python scripts/build_municipality_target_thickness.py --build  # NotImplementedError

設計原則 (本タスクで本実装):
- READ-only な入力ロード (load_inputs)
- 入力検証 (validate_inputs): 行数・キーセット・型・カバレッジ・重み合計
- DB 検証 SQL 実行 (run_validation_sql): 既存テーブルへの READ-only 検証

設計原則 (本タスクではスケルトンのみ):
- 計算ロジック (compute_baseline / compute_f3 / ... / compute_f6_v2 / integrate_model_f2)
- 派生指標 (derive_thickness_index / derive_rank_and_priority / derive_scenario_indices)
- 出力 (export_to_csv / import_to_sqlite)
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

# anchor city 閾値 (sensitivity 結果待ち、本書ではプロト値で初期化)
DEFAULT_ANCHOR_THRESHOLDS = {
    "mfg_share": 0.12,
    "mfg_emp_per_establishment": 20.0,
    "day_night_ratio": 150.0,
    "hq_excess_ratio": 5.0,
}

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

    # 値が hypothesis_v1 のみであることを確認
    sources = {row[source_col] for row in rows}
    if sources != {EXPECTED_WEIGHT_SOURCE}:
        raise ValueError(
            f"weight_source must be {{'{EXPECTED_WEIGHT_SOURCE}'}} only, found: {sources}"
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
# 計算ロジック (スケルトン、本タスクでは実装しない)
# ============================================================

_NOT_IMPL_MSG = "Phase 5 sensitivity 結果待ち、本実装ラウンドで埋める"


def compute_baseline(data: dict) -> dict:
    """F1 (総人口) × F2 (生産年齢人口比) → Model B 相当.

    都道府県就業者数 × (muni 生産年齢 / pref 生産年齢) で按分.
    """
    raise NotImplementedError(_NOT_IMPL_MSG)


def compute_f3(data: dict, weight_matrix: dict) -> dict:
    """F3 (産業構成補正、べき乗 1.5).

    F3_per_occ = (Σ ind_share × weight) / (Σ nat_share × weight) ** 1.5
    """
    raise NotImplementedError(_NOT_IMPL_MSG)


def compute_f4_occupation_weighted(data: dict) -> dict:
    """F4 (職業別 OCCUPATION_F4_WEIGHT 適用).

    f4 = 1 + (day/night - 1) × occ_weight,  clamp [0.1, 5.0]
    """
    raise NotImplementedError(_NOT_IMPL_MSG)


def compute_f5(data: dict) -> dict:
    """F5 (通勤流入補正).

    f5 = 1 + (inflow / night) × 0.3,  clamp [0.5, 2.0]
    """
    raise NotImplementedError(_NOT_IMPL_MSG)


def compute_industrial_anchor(data: dict, thresholds: Optional[dict] = None) -> dict:
    """4 条件 AND で industrial_anchor_city を判定.

    閾値は dict 渡し:
      {"mfg_share": 0.12, "mfg_emp_per_establishment": 20,
       "day_night_ratio": 150, "hq_excess_ratio": 5.0}

    sensitivity 分析後に推奨値を本書で固定.
    """
    if thresholds is None:
        thresholds = DEFAULT_ANCHOR_THRESHOLDS
    raise NotImplementedError(_NOT_IMPL_MSG)


def compute_f6_v2(data: dict, anchor_set: set) -> dict:
    """F6 anchor 分岐.

    anchor IN: HQ 減衰スキップ + boost
    anchor OUT: F (HQ 減衰 + 工場ブースト)
    """
    raise NotImplementedError(_NOT_IMPL_MSG)


def integrate_model_f2(
    baseline: dict,
    f3: dict,
    f4: dict,
    f5: dict,
    f6: dict,
    data: dict,
) -> dict:
    """全要素統合 + 都道府県スケーリング.

    raw[(pref,muni)][occ] = baseline × f3 × f4 × f5 × f6
    scaling[pref][occ] = pref_target / raw_sum
    out[(pref,muni)][occ] = raw × scaling
    """
    raise NotImplementedError(_NOT_IMPL_MSG)


# ============================================================
# 派生指標 (スケルトン)
# ============================================================

def derive_thickness_index(model_result: dict) -> dict:
    """0-200 正規化 (100 = 全国平均)."""
    raise NotImplementedError(_NOT_IMPL_MSG)


def derive_rank_and_priority(model_result: dict) -> dict:
    """全国順位 + percentile + A/B/C/D 配信優先度."""
    raise NotImplementedError(_NOT_IMPL_MSG)


def derive_scenario_indices(
    model_result: dict,
    turnover_rates: Optional[dict] = None,
) -> dict:
    """conservative=1×, standard=3×, aggressive=5× の濃淡指数."""
    if turnover_rates is None:
        turnover_rates = DEFAULT_TURNOVER_RATES
    raise NotImplementedError(_NOT_IMPL_MSG)


# ============================================================
# 出力 (スケルトン)
# ============================================================

def export_to_csv(result_df, path: str) -> None:
    """CSV 出力 (UTF-8、quoting=csv.QUOTE_MINIMAL)."""
    raise NotImplementedError(_NOT_IMPL_MSG)


def import_to_sqlite(csv_path: str, db_path: str, table_name: str) -> None:
    """ローカル DB 投入 (DROP TABLE IF EXISTS + CREATE + INSERT batch).

    Claude 単独実行禁止 (MEMORY 事故記録)、`--build` モードでも skip 可能.
    """
    raise NotImplementedError(_NOT_IMPL_MSG)


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
