# -*- coding: utf-8 -*-
"""
Phase 3 Step 5 前提検証: Model F2 閾値 sensitivity 分析 (Worker A3)
==================================================================

**目的**: Model F2 で 5/5 合格 (Grade A-) を達成した 4 つの閾値が
偶然合格しているのか、それとも頑健に合格するのかを確認する。

**手法**: One-At-a-Time (OAT) sensitivity 分析
  - 4 軸 × 7 値 = 28 ケース + ベース 1 = 29 OAT ケース
  - 1 軸ずつ変更、他 3 軸はベース固定
  - フル grid (7^4 = 2,401 ケース) は冗長 → 合格崩壊点周辺のみ追加 grid 検証

**評価指標** (各ケースで計算):
  1. 港区生産工程順位 ≥ 30
  2. 製造系 TOP 10 工業都市 ≥ 4/9
  3. 物流系 TOP 10 湾岸都市 ≥ 5/7
  4. TOP 10 Jaccard 平均 < 0.65
  5. 全職種同 TOP10 でない

合格 grade:
  - A-:  5/5
  - B :  4/5
  - C :  3/5
  - D :  ≤ 2/5

**設計原則**:
  - DB 書き込み禁止
  - Turso 接続不要
  - .env open 禁止
  - Rust 変更禁止
  - push 禁止
  - proto_evaluate_occupation_population_models.py の関数は無変更で呼び出すのみ
    (module 定数は monkey-patch、ただし呼び出しごとに復元)

**入力**: scripts/proto_evaluate_occupation_population_models.py の関数群
**出力**:
  - data/generated/sensitivity_results.json
  - docs/SURVEY_MARKET_INTELLIGENCE_PHASE3_F2_SENSITIVITY_ANALYSIS.md (別途生成)

**実行時間目安**: 数分 (load_data は 1 回のみ、F2 の再計算は軽量)
"""
from __future__ import annotations

import io
import json
import sqlite3
import sys
import time
from pathlib import Path
from statistics import mean, median

try:
    sys.stdout.reconfigure(encoding="utf-8")
except (AttributeError, ValueError):
    pass

# ============================================================
# proto モジュール import + 監視
# ============================================================
SCRIPT_DIR = Path(__file__).parent
PROJECT_ROOT = SCRIPT_DIR.parent
sys.path.insert(0, str(SCRIPT_DIR))

import proto_evaluate_occupation_population_models as proto  # noqa: E402

OUT_JSON = PROJECT_ROOT / "data" / "generated" / "sensitivity_results.json"

# ============================================================
# Sensitivity 分析の設定
# ============================================================

# ベース閾値 (Model F2 5/5 合格時の値)
BASE_THRESHOLDS = {
    "mfg_share":              0.12,   # ANCHOR_MFG_SHARE_MIN
    "mfg_emp_per_establishment": 20.0, # ANCHOR_EMP_PER_EST_MIN
    "day_night_ratio":        150.0,  # ANCHOR_DN_RATIO_MAX
    "hq_excess_ratio":        5.0,    # ANCHOR_HQ_EXCESS_E_MAX
}

# OAT 振り幅
THRESHOLD_VARIATIONS = {
    "mfg_share":              [0.08, 0.10, 0.12, 0.14, 0.16, 0.18, 0.20],
    "mfg_emp_per_establishment": [10.0, 15.0, 20.0, 25.0, 30.0, 40.0, 50.0],
    "day_night_ratio":        [120.0, 130.0, 140.0, 150.0, 160.0, 180.0, 200.0],
    "hq_excess_ratio":        [3.0, 4.0, 5.0, 6.0, 8.0, 10.0, 15.0],
}

# 順位追跡対象都市
TRACE_CITIES = [
    ("東京都", "港区"),       # 抑制したい (≥ 30)
    ("東京都", "中央区"),
    ("東京都", "千代田区"),
    ("愛知県", "豊田市"),     # 浮上したい (≤ 15)
    ("群馬県", "太田市"),
    ("静岡県", "浜松市"),
    ("大阪府", "堺市"),
    ("神奈川県", "川崎市"),
    ("神奈川県", "相模原市"),
    ("福岡県", "北九州市"),
    ("三重県", "四日市市"),
    ("神奈川県", "厚木市"),
    # 物流湾岸
    ("神奈川県", "横浜市"),
    ("大阪府", "大阪市"),
    ("愛知県", "名古屋市"),
    ("福岡県", "福岡市"),
]


# ============================================================
# ヘルパー: F2 評価 1 ケース実行
# ============================================================

def run_model_f2_with_thresholds(
    base_inputs: dict,
    mfg_share: float,
    mfg_emp_per_est: float,
    day_night_ratio: float,
    hq_excess_ratio: float,
) -> dict:
    """指定閾値で Model F2 を再実行し、評価指標を返す.

    Args:
        base_inputs: load_data ほか pre-computed inputs (1 回ロードして全ケースで使い回す)
        各 *_threshold パラメータ: 4 軸の閾値

    Returns:
        評価結果 dict
          - grade ('A-' | 'B' | 'C' | 'D')
          - pass_count (0..5)
          - jaccard
          - manufacturing_count, logistics_count
          - port_minato_rank
          - trace_ranks: 各都市の '08_生産工程' 順位
    """
    # 1. proto モジュール定数を一時的に書き換え (monkey-patch)
    saved = {
        "ANCHOR_MFG_SHARE_MIN": proto.ANCHOR_MFG_SHARE_MIN,
        "ANCHOR_EMP_PER_EST_MIN": proto.ANCHOR_EMP_PER_EST_MIN,
        "ANCHOR_DN_RATIO_MAX": proto.ANCHOR_DN_RATIO_MAX,
        "ANCHOR_HQ_EXCESS_E_MAX": proto.ANCHOR_HQ_EXCESS_E_MAX,
    }
    proto.ANCHOR_MFG_SHARE_MIN = mfg_share
    proto.ANCHOR_EMP_PER_EST_MIN = mfg_emp_per_est
    proto.ANCHOR_DN_RATIO_MAX = day_night_ratio
    proto.ANCHOR_HQ_EXCESS_E_MAX = hq_excess_ratio

    try:
        data = base_inputs["data"]
        pref_occ_pop = base_inputs["pref_occ_pop"]
        industry_share = base_inputs["industry_share"]
        national_share = base_inputs["national_share"]
        weights = base_inputs["weights"]
        sn_emp = base_inputs["sn_emp"]
        industry_est = base_inputs["industry_est"]

        # 2. anchor cities 再判定 (新閾値で)
        anchor_cities, anchor_audit = proto.compute_industrial_anchor_cities(
            data,
            data["industry_emp"],
            industry_est,
            sn_emp,
            data["national_emp"],
        )

        # 3. F6 factor v2 を再計算 (新 anchor_cities + 同じ alpha で)
        f6_factor_v2, _ = proto.compute_f6_factor_v2(
            sn_emp,
            data["industry_emp"],
            industry_est,
            data["national_emp"],
            weights,
            anchor_cities,
            alpha=proto.F6_ALPHA,
        )

        # 4. Model F2 実行
        result_f2, _ = proto.model_f2(
            data, pref_occ_pop, industry_share, national_share, weights, f6_factor_v2
        )

        # 5. 評価指標計算
        # Jaccard 平均
        occupations = list(proto.NATIONAL_OCCUPATION_RATIO.keys())
        jaccard, _ = proto.jaccard_matrix_avg(result_f2, occupations, n=10)

        # estimate_grade 用 audit (manufacturing_count, logistics_count)
        _, grade_audit = proto.estimate_grade(jaccard, result_f2)
        manufacturing_count = grade_audit["industrial_cities_in_mfg_top10"]
        logistics_count = grade_audit["logistics_hubs_in_logistics_top10"]

        # 港区順位 (08_生産工程 全市区町村ランキング)
        top_prod = proto.ranking_top_n(result_f2, "08_生産工程", n=2000)
        rank_minato = next(
            (i + 1 for i, (loc, _) in enumerate(top_prod) if loc == "東京都 港区"),
            None,
        )

        # 5 条件チェック (Worker F の数値目標)
        cond_results = {
            "port_minato_ge_30": (rank_minato is not None and rank_minato >= 30),
            "manufacturing_ge_4": (manufacturing_count >= 4),
            "logistics_ge_5": (logistics_count >= 5),
            "jaccard_lt_065": (jaccard < 0.65),
            "not_all_same_top10": (jaccard < 1.0),
        }
        pass_count = sum(1 for v in cond_results.values() if v)

        if pass_count == 5:
            grade = "A-"
        elif pass_count == 4:
            grade = "B"
        elif pass_count == 3:
            grade = "C"
        else:
            grade = "D"

        # トレース都市の生産工程順位
        trace_ranks = {}
        for (pref, muni) in TRACE_CITIES:
            loc_str = f"{pref} {muni}"
            rk = next(
                (i + 1 for i, (loc, _) in enumerate(top_prod) if loc == loc_str),
                None,
            )
            trace_ranks[loc_str] = rk

        return {
            "thresholds": {
                "mfg_share": mfg_share,
                "mfg_emp_per_establishment": mfg_emp_per_est,
                "day_night_ratio": day_night_ratio,
                "hq_excess_ratio": hq_excess_ratio,
            },
            "grade": grade,
            "pass_count": pass_count,
            "jaccard": round(jaccard, 4),
            "manufacturing_count": manufacturing_count,
            "logistics_count": logistics_count,
            "port_minato_rank": rank_minato,
            "anchor_city_count": len(anchor_cities),
            "conditions": cond_results,
            "trace_ranks": trace_ranks,
        }
    finally:
        # 必ず復元
        proto.ANCHOR_MFG_SHARE_MIN = saved["ANCHOR_MFG_SHARE_MIN"]
        proto.ANCHOR_EMP_PER_EST_MIN = saved["ANCHOR_EMP_PER_EST_MIN"]
        proto.ANCHOR_DN_RATIO_MAX = saved["ANCHOR_DN_RATIO_MAX"]
        proto.ANCHOR_HQ_EXCESS_E_MAX = saved["ANCHOR_HQ_EXCESS_E_MAX"]


# ============================================================
# main
# ============================================================

def main():
    print("=" * 75)
    print("Phase 3 Step 5 前提検証: Model F2 閾値 sensitivity 分析 (Worker A3)")
    print("=" * 75)

    if not proto.DB_PATH.exists():
        print(f"ERROR: DB not found: {proto.DB_PATH}", file=sys.stderr)
        return 1

    t_start = time.time()

    # ------------------------------------------------------------
    # データロード (1 回のみ、全ケースで使い回す)
    # ------------------------------------------------------------
    print("\n[1] データロード (全ケース共通)")
    conn = sqlite3.connect(str(proto.DB_PATH))
    data = proto.load_data(conn)
    print(f"  - 市区町村人口: {len(data['population']):,} 件")

    pref_occ_pop = proto.build_pref_occupation_ground_truth(data)
    print(f"  - pref_occ_pop: {len(pref_occ_pop)} 都道府県")

    industry_share, national_share, weights = proto.load_industry_data(data)
    print(f"  - industry_share: {len(industry_share):,} 市区町村")

    sn_emp, sn_companies = proto.load_salesnow_aggregate(use_cache=True)
    print(f"  - SalesNow 集約: {len(sn_emp):,} cells")

    industry_est = proto.load_industry_establishments(data)
    print(f"  - industry_est: {len(industry_est):,} 市区町村")

    base_inputs = {
        "data": data,
        "pref_occ_pop": pref_occ_pop,
        "industry_share": industry_share,
        "national_share": national_share,
        "weights": weights,
        "sn_emp": sn_emp,
        "industry_est": industry_est,
    }
    conn.close()

    t_load = time.time() - t_start
    print(f"  ロード時間: {t_load:.1f} 秒")

    # ------------------------------------------------------------
    # ベース 1 ケース
    # ------------------------------------------------------------
    print("\n[2] ベース閾値での Model F2 検証")
    base_result = run_model_f2_with_thresholds(
        base_inputs,
        BASE_THRESHOLDS["mfg_share"],
        BASE_THRESHOLDS["mfg_emp_per_establishment"],
        BASE_THRESHOLDS["day_night_ratio"],
        BASE_THRESHOLDS["hq_excess_ratio"],
    )
    print(f"  ベース: grade={base_result['grade']}  pass={base_result['pass_count']}/5  "
          f"jaccard={base_result['jaccard']:.3f}  "
          f"製造={base_result['manufacturing_count']}/9  物流={base_result['logistics_count']}/7  "
          f"港区={base_result['port_minato_rank']}")

    if base_result["grade"] != "A-":
        print(f"  ⚠️ ベースが A- になっていない (grade={base_result['grade']})。")
        print(f"     proto 側でベース 5/5 達成が報告済みのため、入力データ整合性を要確認。")

    # ------------------------------------------------------------
    # OAT (One-At-a-Time) 28 ケース
    # ------------------------------------------------------------
    print("\n[3] OAT sensitivity (1 軸 × 7 値、他 3 軸はベース固定)")
    oat_results = {}
    case_id = 0
    total_oat = sum(len(v) for v in THRESHOLD_VARIATIONS.values())

    for axis_name, values in THRESHOLD_VARIATIONS.items():
        oat_results[axis_name] = []
        print(f"\n  --- 軸: {axis_name} (base={BASE_THRESHOLDS[axis_name]}) ---")
        for v in values:
            case_id += 1
            ts = {
                "mfg_share": BASE_THRESHOLDS["mfg_share"],
                "mfg_emp_per_establishment": BASE_THRESHOLDS["mfg_emp_per_establishment"],
                "day_night_ratio": BASE_THRESHOLDS["day_night_ratio"],
                "hq_excess_ratio": BASE_THRESHOLDS["hq_excess_ratio"],
            }
            ts[axis_name] = v
            t0 = time.time()
            r = run_model_f2_with_thresholds(
                base_inputs,
                ts["mfg_share"], ts["mfg_emp_per_establishment"],
                ts["day_night_ratio"], ts["hq_excess_ratio"],
            )
            dt = time.time() - t0
            is_base = (v == BASE_THRESHOLDS[axis_name])
            base_marker = " (BASE)" if is_base else ""
            print(f"    [{case_id:2}/{total_oat}] {axis_name}={v:>6}: "
                  f"grade={r['grade']}  pass={r['pass_count']}/5  "
                  f"jaccard={r['jaccard']:.3f}  製造={r['manufacturing_count']}/9  "
                  f"物流={r['logistics_count']}/7  港区={r['port_minato_rank']}  "
                  f"({dt:.1f}s){base_marker}")
            r["axis"] = axis_name
            r["axis_value"] = v
            r["is_base"] = is_base
            oat_results[axis_name].append(r)

    # ------------------------------------------------------------
    # 安定領域 / 合格崩壊閾値 抽出
    # ------------------------------------------------------------
    print("\n[4] 安定領域 (A- 維持区間) と合格崩壊閾値")
    stable_intervals = {}
    critical_breakpoints = []

    for axis_name, results in oat_results.items():
        # A- 維持の値範囲
        a_minus_values = [r["axis_value"] for r in results if r["grade"] == "A-"]
        if a_minus_values:
            stable_intervals[axis_name] = {
                "min": min(a_minus_values),
                "max": max(a_minus_values),
                "values": a_minus_values,
                "count_a_minus": len(a_minus_values),
                "total": len(results),
            }
            print(f"  {axis_name}: A- 維持区間 [{min(a_minus_values)}, {max(a_minus_values)}] "
                  f"({len(a_minus_values)}/{len(results)} ケース)")
        else:
            stable_intervals[axis_name] = {
                "min": None, "max": None, "values": [],
                "count_a_minus": 0, "total": len(results),
            }
            print(f"  {axis_name}: A- 維持なし")

        # 合格崩壊閾値: 連続する 2 ケースで grade が A- → 非 A- に変化した点
        for i in range(len(results) - 1):
            cur = results[i]
            nxt = results[i + 1]
            if cur["grade"] == "A-" and nxt["grade"] != "A-":
                broken_conds = [k for k, v in nxt["conditions"].items() if not v]
                critical_breakpoints.append({
                    "axis": axis_name,
                    "from_value": cur["axis_value"],
                    "to_value": nxt["axis_value"],
                    "from_grade": cur["grade"],
                    "to_grade": nxt["grade"],
                    "broken_conditions": broken_conds,
                    "to_pass_count": nxt["pass_count"],
                })
                print(f"     ⚠️ breakpoint: {axis_name}={cur['axis_value']}→{nxt['axis_value']}  "
                      f"{cur['grade']}→{nxt['grade']}  失敗条件: {broken_conds}")
            elif cur["grade"] != "A-" and nxt["grade"] == "A-":
                # 逆方向 (戻り) も記録
                critical_breakpoints.append({
                    "axis": axis_name,
                    "from_value": cur["axis_value"],
                    "to_value": nxt["axis_value"],
                    "from_grade": cur["grade"],
                    "to_grade": nxt["grade"],
                    "broken_conditions": [],
                    "direction": "recover",
                    "to_pass_count": nxt["pass_count"],
                })

    # ------------------------------------------------------------
    # 推奨閾値 (頑健性最大化、A- 維持区間の中央値)
    # ------------------------------------------------------------
    print("\n[5] 推奨閾値 (各軸 A- 維持区間の中央値またはベース)")
    recommended_thresholds = {}
    for axis_name, interval in stable_intervals.items():
        if interval["values"]:
            mid = median(interval["values"])
            recommended_thresholds[axis_name] = {
                "value": mid,
                "robust_range": [interval["min"], interval["max"]],
                "rationale": f"A- 維持区間 [{interval['min']}, {interval['max']}] の中央値",
            }
        else:
            # A- なし → ベースを保持 (再調整必要)
            recommended_thresholds[axis_name] = {
                "value": BASE_THRESHOLDS[axis_name],
                "robust_range": None,
                "rationale": "A- 維持区間なし、ベース値を暫定維持 (再調整必要)",
            }
        print(f"  {axis_name}: 推奨={recommended_thresholds[axis_name]['value']}  "
              f"(robust_range={recommended_thresholds[axis_name]['robust_range']})")

    # ------------------------------------------------------------
    # 頑健性スコア
    # ------------------------------------------------------------
    all_results = [r for results in oat_results.values() for r in results]
    n_total = len(all_results)
    n_a_minus = sum(1 for r in all_results if r["grade"] == "A-")
    robustness_score = n_a_minus / n_total if n_total > 0 else 0.0
    print(f"\n[6] 頑健性スコア: {robustness_score:.3f} ({n_a_minus}/{n_total} ケースが A-)")

    # ------------------------------------------------------------
    # Grid 検証 (合格崩壊周辺の組み合わせ)
    # ------------------------------------------------------------
    print("\n[7] Grid 検証 (合格崩壊閾値の組み合わせ、最大 20 ケース)")
    # 各軸で「ベースから 1 step だけ崩壊側に動かした値」を組み合わせる
    # 合格崩壊が見つかった軸のみ対象 (ベースの近傍検証)
    grid_results = []
    grid_combinations = []
    # 各軸の境界値 (A- ラインの最小/最大) を採用、それと安全側 (中央値) の組み合わせ
    safe_values = {}
    edge_values = {}
    for axis_name, interval in stable_intervals.items():
        if interval["values"]:
            safe_values[axis_name] = median(interval["values"])
            edge_values[axis_name] = [interval["min"], interval["max"]]
        else:
            safe_values[axis_name] = BASE_THRESHOLDS[axis_name]
            edge_values[axis_name] = [BASE_THRESHOLDS[axis_name]]

    # 各軸の境界値を 2 つずつ取り、4 軸で 2^4 = 16 ケース
    import itertools
    axis_list = ["mfg_share", "mfg_emp_per_establishment", "day_night_ratio", "hq_excess_ratio"]
    edge_combos = list(itertools.product(*[edge_values[a] for a in axis_list]))
    if len(edge_combos) > 20:
        edge_combos = edge_combos[:20]

    for combo in edge_combos:
        ts = dict(zip(axis_list, combo))
        # ベースと完全に同じ場合スキップ
        if all(ts[a] == BASE_THRESHOLDS[a] for a in axis_list):
            continue
        r = run_model_f2_with_thresholds(
            base_inputs,
            ts["mfg_share"], ts["mfg_emp_per_establishment"],
            ts["day_night_ratio"], ts["hq_excess_ratio"],
        )
        r["combination"] = ts
        grid_results.append(r)
        grid_combinations.append(ts)
        print(f"  combo={combo}: grade={r['grade']}  pass={r['pass_count']}/5  "
              f"jaccard={r['jaccard']:.3f}")

    n_grid_a_minus = sum(1 for r in grid_results if r["grade"] == "A-")
    n_grid = len(grid_results)
    grid_robustness = n_grid_a_minus / n_grid if n_grid > 0 else 0.0
    print(f"\n  Grid 検証 A- 維持率: {grid_robustness:.3f} ({n_grid_a_minus}/{n_grid})")

    # ------------------------------------------------------------
    # 個別都市の順位変動範囲
    # ------------------------------------------------------------
    print("\n[8] トレース都市の '08_生産工程' 順位変動 (OAT 全ケース)")
    trace_summary = {}
    for (pref, muni) in TRACE_CITIES:
        loc_str = f"{pref} {muni}"
        ranks = []
        for r in all_results:
            v = r["trace_ranks"].get(loc_str)
            if v is not None:
                ranks.append(v)
        if ranks:
            trace_summary[loc_str] = {
                "min_rank": min(ranks),
                "max_rank": max(ranks),
                "median_rank": median(ranks),
                "mean_rank": round(mean(ranks), 1),
                "n_observations": len(ranks),
            }
            print(f"  {loc_str:<22} min={min(ranks)}  max={max(ranks)}  "
                  f"median={median(ranks)}  mean={trace_summary[loc_str]['mean_rank']}")

    # ------------------------------------------------------------
    # 本実装着手判定
    # ------------------------------------------------------------
    print("\n[9] 本実装着手判定")
    if robustness_score >= 0.80:
        verdict = "OK"
        verdict_msg = f"本実装着手 OK (頑健性スコア {robustness_score:.3f} ≥ 0.80)"
    elif robustness_score >= 0.50:
        verdict = "MARGINAL"
        verdict_msg = f"頑健性スコア {robustness_score:.3f} (中程度)。閾値選択は推奨値を採用、本実装着手は条件付き OK"
    else:
        verdict = "NG"
        verdict_msg = f"頑健性スコア {robustness_score:.3f} < 0.80。閾値再調整が必要"
    print(f"  >>> {verdict_msg}")

    # ------------------------------------------------------------
    # JSON 出力
    # ------------------------------------------------------------
    OUT_JSON.parent.mkdir(parents=True, exist_ok=True)
    output = {
        "schema_version": "1.0.0",
        "analysis": "Phase 3 Step 5 前提検証: Model F2 閾値 sensitivity 分析",
        "worker": "A3",
        "base_thresholds": BASE_THRESHOLDS,
        "threshold_variations": THRESHOLD_VARIATIONS,
        "trace_cities": [f"{p} {m}" for (p, m) in TRACE_CITIES],
        "base_result": base_result,
        "oat_results": oat_results,
        "stable_intervals": stable_intervals,
        "critical_breakpoints": critical_breakpoints,
        "recommended_thresholds": recommended_thresholds,
        "robustness_score": round(robustness_score, 4),
        "n_a_minus": n_a_minus,
        "n_total_oat": n_total,
        "grid_results": grid_results,
        "grid_combinations": grid_combinations,
        "grid_robustness": round(grid_robustness, 4),
        "n_grid_a_minus": n_grid_a_minus,
        "n_grid": n_grid,
        "trace_summary": trace_summary,
        "verdict": verdict,
        "verdict_message": verdict_msg,
        "elapsed_sec": round(time.time() - t_start, 1),
    }
    with open(OUT_JSON, "w", encoding="utf-8") as f:
        json.dump(output, f, ensure_ascii=False, indent=2, default=str)
    print(f"\n[10] JSON 出力: {OUT_JSON}")
    print(f"   総実行時間: {time.time() - t_start:.1f} 秒")

    return 0


if __name__ == "__main__":
    sys.exit(main())
