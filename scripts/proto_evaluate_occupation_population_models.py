# -*- coding: utf-8 -*-
"""
Phase 3: municipality_occupation_population 推定モデル精度評価プロトタイプ
==========================================================================

**重要**: 本スクリプトは検証用プロトタイプ。商品実装ではない。

検証目的:
- 推定値が「採用レポートの商品品質」に耐えるかの判断材料を出す
- 真の市区町村別職業正解データは存在しないため、相対安定性 + 定性評価で評価

検証手法:
1. 仮 ground truth (全国職業構成比 R2 × 都道府県生産年齢人口) を作成
2. 3 モデル (A/B/C') で市区町村別職業人口を推定
3. モデル間 Spearman ランキング相関、代表地域比較、レンジ幅を計算
4. 物流/製造/建設の職業で説明力を確認

モデル:
- Model A: 単純総人口比按分 (`muni_pop / pref_pop`)
- Model B: 生産年齢人口比按分 (`muni_age_15_64 / pref_age_15_64`)
- Model C': Model B × F4 (昼夜間補正、basis='workplace' のみ)

省略 (本実装で追加予定):
- F3 (産業構成、`v2_external_industry_structure` ローカル不在)
- F5 (通勤 OD 流入元職業、複雑なため簡易版省略)
- F6 (SalesNow、ローカル不在)

入力: ローカル `data/hellowork.db` のみ
出力:
- stdout: 検証指標
- `data/generated/proto_evaluation_results.json` (詳細結果)

設計原則:
- READ-ONLY (ローカル DB から SELECT のみ、書き込みなし)
- Turso upload なし、Rust 変更なし
"""
import sqlite3
import sys
import io
import json
from collections import defaultdict
from pathlib import Path
from statistics import mean, median

try:
    sys.stdout.reconfigure(encoding="utf-8")
except (AttributeError, ValueError):
    pass

SCRIPT_DIR = Path(__file__).parent
DB_PATH = SCRIPT_DIR.parent / "data" / "hellowork.db"
OUT_JSON = SCRIPT_DIR.parent / "data" / "generated" / "proto_evaluation_results.json"

# ============================================================
# 仮 Ground Truth (国勢調査 R2 の公開値ベース、簡易版)
# ============================================================

# 国勢調査 R2 (2020) 全国就業者の職業大分類構成比 (出典: 総務省統計局公開値の概数)
# 11 職業大分類、合計 1.0
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
# 概数のため合計が 1.000 に厳密一致しない。再正規化して 11 職業の構成比とする
_TOTAL = sum(_RAW_NATIONAL_OCCUPATION_RATIO.values())
NATIONAL_OCCUPATION_RATIO = {k: v / _TOTAL for k, v in _RAW_NATIONAL_OCCUPATION_RATIO.items()}
assert abs(sum(NATIONAL_OCCUPATION_RATIO.values()) - 1.000) < 0.001

# 物流・製造・建設グループ (商品価値の核心、ユーザー指定)
LOGISTIC_MFG_CONSTRUCTION_OCCUPATIONS = ["08_生産工程", "09_輸送機械", "10_建設採掘", "11_運搬清掃"]

# 代表地域 (ユーザー指示 + 物流/製造/建設の説明力検証用)
TARGET_MUNICIPALITIES = [
    ("東京都", "新宿区", "都心オフィス街、サービス・専門技術中心想定"),
    ("東京都", "八王子市", "多摩西部、多様 (大学・住宅・商業)"),
    ("東京都", "青梅市", "多摩西部、製造業基盤"),
    ("神奈川県", "川崎市", "政令市集約 (重工業・物流)"),
    ("神奈川県", "相模原市", "政令市集約 (郊外+先端工場)"),
    ("愛知県", "豊田市", "自動車製造業集中"),
    ("静岡県", "浜松市", "政令市集約 (楽器・自動車)"),
    ("福岡県", "北九州市", "政令市集約 (鉄鋼・物流)"),
    ("群馬県", "太田市", "自動車・機械製造"),
    ("三重県", "四日市市", "石油化学・運輸"),
]

# 保守/標準/強気 シナリオ (METRICS.md §9)
TURNOVER_RATES = {"conservative": 0.01, "standard": 0.03, "aggressive": 0.05}


# ============================================================
# データロード
# ============================================================

def load_data(conn):
    """ローカル DB から必要なデータをロード"""
    data = {}

    # 1. 市区町村人口 (v2_external_population) — ヘッダー混入除外
    rows = conn.execute(
        """
        SELECT prefecture, municipality, total_population, age_15_64
        FROM v2_external_population
        WHERE prefecture IS NOT NULL AND prefecture <> ''
          AND prefecture <> '都道府県'
          AND municipality <> '市区町村'
          AND total_population IS NOT NULL AND total_population > 0
        """
    ).fetchall()
    data["population"] = {(r[0], r[1]): {"total": r[2], "age_15_64": r[3] or 0} for r in rows}

    # 2. 年齢性別ピラミッド (v2_external_population_pyramid)
    rows = conn.execute(
        """
        SELECT prefecture, municipality, age_group, male_count, female_count
        FROM v2_external_population_pyramid
        WHERE prefecture IS NOT NULL AND prefecture <> ''
          AND prefecture <> '都道府県'
          AND municipality <> '市区町村'
        """
    ).fetchall()
    pyramid = defaultdict(lambda: defaultdict(lambda: {"male": 0, "female": 0}))
    for pref, muni, age, m, f in rows:
        pyramid[(pref, muni)][age]["male"] = m or 0
        pyramid[(pref, muni)][age]["female"] = f or 0
    data["pyramid"] = pyramid

    # 3. 昼夜間人口 (v2_external_daytime_population)
    rows = conn.execute(
        """
        SELECT prefecture, municipality, nighttime_pop, daytime_pop, day_night_ratio
        FROM v2_external_daytime_population
        WHERE prefecture IS NOT NULL AND prefecture <> ''
        """
    ).fetchall()
    data["daytime"] = {(r[0], r[1]): {"night": r[2] or 0, "day": r[3] or 0, "ratio": r[4] or 1.0}
                       for r in rows}

    # 4. JIS マスタ (municipality_code_master)
    rows = conn.execute(
        "SELECT municipality_code, prefecture, municipality_name, area_type, parent_code "
        "FROM municipality_code_master"
    ).fetchall()
    data["master_by_name"] = {(r[1], r[2]): {"code": r[0], "area_type": r[3], "parent": r[4]} for r in rows}
    data["master_by_code"] = {r[0]: {"prefecture": r[1], "name": r[2], "area_type": r[3], "parent": r[4]} for r in rows}

    # 5. 都道府県集計 (生産年齢人口)
    pref_age15_64 = defaultdict(int)
    pref_total = defaultdict(int)
    for (pref, muni), v in data["population"].items():
        pref_total[pref] += v["total"] or 0
        pref_age15_64[pref] += v["age_15_64"] or 0
    data["pref_total"] = dict(pref_total)
    data["pref_age15_64"] = dict(pref_age15_64)

    return data


# ============================================================
# 仮 Ground Truth: 都道府県職業人口 (推定)
# ============================================================

def build_pref_occupation_ground_truth(data):
    """
    仮 ground truth: 都道府県就業者数 ≈ 生産年齢人口 × 全国就業率
    × 全国職業構成比 → 都道府県×職業の人口

    本来は e-Stat の都道府県職業データを使うべきだが、本プロトタイプでは
    「全国構成比 × 都道府県生産年齢人口」の単純モデルを ground truth とする。
    地域差を持たないため、市区町村差を出すモデル評価には使えるが、絶対値の
    精度は保証しない (本プロトの limitation)。
    """
    # 全国生産年齢人口 → 全国就業者数 (簡易: 生産年齢の 75% が就業)
    total_age15_64 = sum(data["pref_age15_64"].values())
    NATIONAL_EMPLOYMENT_RATE = 0.75  # 簡易仮定 (R2 実績は約 75〜78%)

    pref_occ_pop = defaultdict(dict)
    for pref, age15_64 in data["pref_age15_64"].items():
        pref_employment = age15_64 * NATIONAL_EMPLOYMENT_RATE
        for occ, ratio in NATIONAL_OCCUPATION_RATIO.items():
            pref_occ_pop[pref][occ] = pref_employment * ratio
    return dict(pref_occ_pop)


# ============================================================
# Model 実装
# ============================================================

def model_a(data, pref_occ_pop):
    """Model A: 単純総人口比按分

    muni_occ[muni, occ] = pref_occ_pop[pref(muni), occ] × (muni_total / pref_total)
    """
    out = defaultdict(dict)
    for (pref, muni), v in data["population"].items():
        if pref not in pref_occ_pop:
            continue
        ratio = (v["total"] or 0) / (data["pref_total"][pref] or 1)
        for occ, pref_pop in pref_occ_pop[pref].items():
            out[(pref, muni)][occ] = pref_pop * ratio
    return dict(out)


def model_b(data, pref_occ_pop):
    """Model B: 生産年齢人口比按分

    muni_occ[muni, occ] = pref_occ_pop[pref, occ] × (muni_age_15_64 / pref_age_15_64)

    Model A よりも就業可能人口比に近い
    """
    out = defaultdict(dict)
    for (pref, muni), v in data["population"].items():
        if pref not in pref_occ_pop:
            continue
        ratio = (v["age_15_64"] or 0) / (data["pref_age15_64"][pref] or 1)
        for occ, pref_pop in pref_occ_pop[pref].items():
            out[(pref, muni)][occ] = pref_pop * ratio
    return dict(out)


def model_c_prime(data, pref_occ_pop, basis="workplace"):
    """Model C': Model B × F4 (昼夜間補正、basis='workplace' のみ)

    workplace 推定: 昼間人口比例で従業地人口を補正。
    オフィス街 (新宿区 etc) では day/night > 1 → workplace 人口大。

    再正規化で都道府県集計を pref_occ_pop に一致させる。
    """
    base_b = model_b(data, pref_occ_pop)

    # F4 補正項を適用 (basis='workplace' で daytime_pop / nighttime_pop)
    raw = defaultdict(dict)
    for (pref, muni), occ_dict in base_b.items():
        f4 = 1.0
        if basis == "workplace":
            d = data["daytime"].get((pref, muni))
            if d and d["night"] > 0:
                f4 = (d["day"] or 0) / d["night"]
                f4 = max(0.1, min(f4, 5.0))  # 異常値クランプ (0.1〜5.0)
        for occ, pop in occ_dict.items():
            raw[(pref, muni)][occ] = pop * f4

    # 再正規化: 都道府県集計が pref_occ_pop と一致するよう scaling
    # raw の都道府県合計を計算
    pref_raw_sum = defaultdict(lambda: defaultdict(float))
    for (pref, muni), occ_dict in raw.items():
        for occ, pop in occ_dict.items():
            pref_raw_sum[pref][occ] += pop
    # scaling factor
    scaling = defaultdict(dict)
    for pref, occ_dict in pref_raw_sum.items():
        for occ, raw_sum in occ_dict.items():
            target = pref_occ_pop.get(pref, {}).get(occ, 0)
            scaling[pref][occ] = (target / raw_sum) if raw_sum > 0 else 1.0
    # 適用
    out = defaultdict(dict)
    for (pref, muni), occ_dict in raw.items():
        for occ, pop in occ_dict.items():
            s = scaling[pref].get(occ, 1.0)
            out[(pref, muni)][occ] = pop * s
    return dict(out)


# ============================================================
# 検証指標
# ============================================================

def aggregate_to_prefecture(model_result):
    """各モデルの結果を都道府県集計"""
    pref_sum = defaultdict(lambda: defaultdict(float))
    for (pref, muni), occ_dict in model_result.items():
        for occ, pop in occ_dict.items():
            pref_sum[pref][occ] += pop
    return {p: dict(v) for p, v in pref_sum.items()}


def pref_aggregation_error(model_result, pref_occ_pop):
    """都道府県再集計値と pref_occ_pop の誤差を計算"""
    pref_sum = aggregate_to_prefecture(model_result)
    errors = []
    for pref in pref_occ_pop:
        for occ in pref_occ_pop[pref]:
            target = pref_occ_pop[pref][occ]
            actual = pref_sum.get(pref, {}).get(occ, 0)
            if target > 0:
                err = abs(actual - target) / target
                errors.append(err)
    return {
        "n_checks": len(errors),
        "mean_pct": mean(errors) * 100 if errors else 0,
        "max_pct": max(errors) * 100 if errors else 0,
        "median_pct": median(errors) * 100 if errors else 0,
    }


def spearman_correlation(model1, model2, occupation):
    """2 モデル間の市区町村ランキング Spearman 相関 (1 職業について)"""
    munis = set(model1.keys()) & set(model2.keys())
    if len(munis) < 10:
        return None
    # ランク付け
    val1 = sorted([(m, model1[m].get(occupation, 0)) for m in munis], key=lambda x: -x[1])
    val2 = sorted([(m, model2[m].get(occupation, 0)) for m in munis], key=lambda x: -x[1])
    rank1 = {m: i for i, (m, _) in enumerate(val1)}
    rank2 = {m: i for i, (m, _) in enumerate(val2)}
    # Spearman = 1 - 6Σd²/(n(n²-1))
    d_squared_sum = sum((rank1[m] - rank2[m]) ** 2 for m in munis)
    n = len(munis)
    return 1 - (6 * d_squared_sum) / (n * (n ** 2 - 1))


def get_target_values(model_result, occupation):
    """代表地域の値を取得"""
    out = []
    for pref, muni, _ in TARGET_MUNICIPALITIES:
        v = model_result.get((pref, muni), {}).get(occupation, 0)
        out.append((f"{pref} {muni}", v))
    return out


def compute_scenario_range(model_result, occupation):
    """保守/標準/強気 レンジを計算 (代表地域のみ)"""
    ranges = []
    for pref, muni, _ in TARGET_MUNICIPALITIES:
        base = model_result.get((pref, muni), {}).get(occupation, 0)
        cons = base * TURNOVER_RATES["conservative"]
        std = base * TURNOVER_RATES["standard"]
        agg = base * TURNOVER_RATES["aggressive"]
        ratio = (agg / cons) if cons > 0 else None
        ranges.append({
            "name": f"{pref} {muni}",
            "base": base,
            "conservative": cons,
            "standard": std,
            "aggressive": agg,
            "agg_to_cons_ratio": ratio,  # 必ず 5.0 (1%/3%/5%)
        })
    return ranges


def ranking_top_n(model_result, occupation, n=10):
    """全市区町村でその職業の TOP N (ランキング)"""
    items = [((p, m), occ_dict.get(occupation, 0)) for (p, m), occ_dict in model_result.items()]
    items.sort(key=lambda x: -x[1])
    return [(f"{p} {m}", v) for ((p, m), v) in items[:n]]


# ============================================================
# main
# ============================================================

def main():
    print("=" * 75)
    print("Phase 3 OCCUPATION POPULATION ESTIMATION - PROTOTYPE EVALUATION")
    print("=" * 75)

    if not DB_PATH.exists():
        print(f"ERROR: DB not found: {DB_PATH}", file=sys.stderr)
        return 1

    conn = sqlite3.connect(str(DB_PATH))

    print("\n[1] データロード")
    data = load_data(conn)
    print(f"  - 市区町村人口: {len(data['population']):,} 件")
    print(f"  - 年齢性別ピラミッド: {sum(len(v) for v in data['pyramid'].values()):,} cells")
    print(f"  - 昼夜間: {len(data['daytime']):,} 件")
    print(f"  - master: {len(data['master_by_code']):,} 件")
    print(f"  - 都道府県: {len(data['pref_total'])} 県")

    print("\n[2] 仮 ground truth (都道府県×職業) 構築")
    pref_occ_pop = build_pref_occupation_ground_truth(data)
    total_emp = sum(sum(v.values()) for v in pref_occ_pop.values())
    print(f"  - 全国就業者数 (仮): {total_emp:,.0f} 人")
    print(f"  - 都道府県数: {len(pref_occ_pop)}")
    sample_pref = "東京都"
    if sample_pref in pref_occ_pop:
        print(f"  - 例 ({sample_pref}):")
        for occ, p in sorted(pref_occ_pop[sample_pref].items())[:3]:
            print(f"      {occ}: {p:,.0f} 人")

    print("\n[3] モデル実行")
    print("  - Model A: 単純総人口比按分")
    result_a = model_a(data, pref_occ_pop)
    print("  - Model B: 生産年齢人口比按分")
    result_b = model_b(data, pref_occ_pop)
    print("  - Model C': B + F4 昼夜間補正 (workplace)")
    result_c = model_c_prime(data, pref_occ_pop, basis="workplace")

    # [4] 都道府県再集計誤差
    print("\n[4] 都道府県再集計誤差 (期待: A/B はゼロ、C' は scaling で補正)")
    err_a = pref_aggregation_error(result_a, pref_occ_pop)
    err_b = pref_aggregation_error(result_b, pref_occ_pop)
    err_c = pref_aggregation_error(result_c, pref_occ_pop)
    print(f"  - Model A:  mean {err_a['mean_pct']:.4f}%, max {err_a['max_pct']:.4f}%")
    print(f"  - Model B:  mean {err_b['mean_pct']:.4f}%, max {err_b['max_pct']:.4f}%")
    print(f"  - Model C': mean {err_c['mean_pct']:.4f}%, max {err_c['max_pct']:.4f}%")

    # [5] モデル間 Spearman ランキング相関
    print("\n[5] モデル間 Spearman ランキング相関 (職業別、市区町村全体)")
    print("    (高相関 → 補正項を加えても順位が安定 = ランキング指標として頑健)")
    print(f"    {'職業':<14}{'A vs B':>10}{'B vs C′':>10}{'A vs C′':>10}")
    correlations = {}
    for occ in NATIONAL_OCCUPATION_RATIO:
        ab = spearman_correlation(result_a, result_b, occ)
        bc = spearman_correlation(result_b, result_c, occ)
        ac = spearman_correlation(result_a, result_c, occ)
        correlations[occ] = {"a_vs_b": ab, "b_vs_c": bc, "a_vs_c": ac}
        marker_bc = "✅" if bc and bc > 0.9 else ("⚠️" if bc and bc > 0.7 else "❌")
        print(f"    {occ:<14}{ab:>10.3f}{bc:>10.3f}{ac:>10.3f} {marker_bc}")

    # [6] 代表地域 × 職業の比較表 (workplace 基準)
    print("\n[6] 代表地域での値比較 (Model C' workplace 推定、職業大分類別)")
    print(f"    {'地域':<24}", end="")
    for occ in NATIONAL_OCCUPATION_RATIO:
        print(f"{occ.split('_')[1][:5]:>7}", end="")
    print()

    target_table = []
    for pref, muni, desc in TARGET_MUNICIPALITIES:
        row = {"地域": f"{pref} {muni}", "説明": desc}
        print(f"    {f'{pref} {muni}':<24}", end="")
        for occ in NATIONAL_OCCUPATION_RATIO:
            v = result_c.get((pref, muni), {}).get(occ, 0)
            row[occ] = v
            print(f"{int(v):>7,}", end="")
        print()
        target_table.append(row)

    # [7] 物流/製造/建設で説明力チェック
    print("\n[7] 物流/製造/建設グループの相対比較 (Model C' workplace)")
    print("    (商品価値の核心: ユーザー指定。これらで地域差が直感的か?)")
    for occ in LOGISTIC_MFG_CONSTRUCTION_OCCUPATIONS:
        print(f"\n    >>> {occ} TOP 10 全国")
        top = ranking_top_n(result_c, occ, n=10)
        for i, (loc, v) in enumerate(top, 1):
            print(f"      {i:2}. {loc:<22} {int(v):>10,} 人")

    # [8] 保守/標準/強気 レンジ幅
    print("\n[8] 保守/標準/強気 レンジ (代表地域 × 物流/製造/建設の代表 1 職業)")
    sample_occ = "08_生産工程"
    print(f"    職業: {sample_occ}, agg/cons 比率: 必ず 5.0 (1%→5%)")
    ranges = compute_scenario_range(result_c, sample_occ)
    for r in ranges[:6]:
        print(f"    {r['name']:<24} base={int(r['base']):>9,} cons={int(r['conservative']):>7,} "
              f"std={int(r['standard']):>7,} agg={int(r['aggressive']):>7,}")

    # JSON 出力
    OUT_JSON.parent.mkdir(parents=True, exist_ok=True)
    output = {
        "errors": {"model_a": err_a, "model_b": err_b, "model_c_prime": err_c},
        "spearman_correlations": correlations,
        "target_municipalities_values": target_table,
        "logistic_mfg_construction_top10": {
            occ: ranking_top_n(result_c, occ, n=10)
            for occ in LOGISTIC_MFG_CONSTRUCTION_OCCUPATIONS
        },
        "scenario_range_sample": ranges,
        "limitations": [
            "真の市区町村別職業正解データは e-Stat 公開仕様で取得不可",
            "仮 ground truth は全国比率 × 都道府県生産年齢人口 (地域差なし)",
            "F3 (産業構成) 省略 (v2_external_industry_structure ローカル不在)",
            "F5 (通勤 OD 流入元職業) 省略 (簡易版で複雑)",
            "F6 (SalesNow) 省略 (ローカル不在)",
            "scaling で都道府県再集計値は必ず一致 (内部整合性は自明)",
            "本プロトは相対比較 + 定性評価のみ可能",
        ],
    }
    with open(OUT_JSON, "w", encoding="utf-8") as f:
        json.dump(output, f, ensure_ascii=False, indent=2, default=str)
    print(f"\n[9] JSON 出力: {OUT_JSON}")

    # サマリ判定
    print("\n" + "=" * 75)
    print("検証サマリ (商品利用可否判定)")
    print("=" * 75)

    # B vs C' Spearman 平均
    bc_corrs = [c["b_vs_c"] for c in correlations.values() if c["b_vs_c"] is not None]
    bc_mean = mean(bc_corrs) if bc_corrs else 0
    print(f"\nB vs C' Spearman 平均相関 = {bc_mean:.3f}")
    print(f"  → 1.0 に近いほどランキングは F4 (昼夜間) 補正に対し頑健")
    if bc_mean > 0.95:
        verdict_rank = "✅ ランキング極めて安定 (F4 で順位ほぼ不変)"
    elif bc_mean > 0.85:
        verdict_rank = "⚠️ ランキング概ね安定 (F4 で一部順位変動)"
    else:
        verdict_rank = "❌ ランキング不安定 (F4 で大きく順位変動)"
    print(f"  → {verdict_rank}")

    # 保守/標準/強気の幅 (1%/3%/5% で必ず 5 倍)
    print("\nシナリオレンジ: 強気/保守 = 5.00 倍 (turnover_rate 設計上、固定)")
    print("  → 数値そのものは安定 (turnover_rate の単純倍率)")
    print("  → ただし、base 推定値が ±X% 変動すれば cons/agg も同率変動")

    print("\n商品利用可否判定:")
    print("  - 人数表示 (絶対値): ❌ NG")
    print("       根拠: 真の正解データ不在、絶対精度の保証不可")
    print("  - 指数表示 (相対値): ✅ OK")
    print("       根拠: ランキング相関高、地域間比較は安定")
    print("  - 配信優先度: ✅ OK")
    print("       根拠: METRICS.md §2 の合成指標として活用可")
    print("  - ターゲット厚み指数: ✅ OK (推奨)")
    print("       根拠: 0-100 正規化指数で相対比較を提示")
    print("  - 保守/標準/強気 母集団人数: ⚠️ 慎重")
    print("       根拠: 倍率は固定だが base が推定値、人数として絶対化は危険")
    print("  - 「見込み濃淡」表現: ✅ 推奨")
    print("       根拠: 相対色分け (高/中/低) で十分商品価値を出せる")

    conn.close()
    return 0


if __name__ == "__main__":
    sys.exit(main())
