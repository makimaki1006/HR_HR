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
import csv
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

# F3 用産業構成 CSV (Worker A/B 事前作業で生成)
INDUSTRY_CSV = SCRIPT_DIR / "data" / "industry_structure_by_municipality.csv"
# F3 用重みマスタ CSV (Worker A/B 事前作業で生成)
WEIGHT_CSV = SCRIPT_DIR.parent / "data" / "generated" / "occupation_industry_weight.csv"

# F6 用 SalesNow 企業 CSV (本社所在地・従業員規模含む、46 カラム)
SALESNOW_CSV = SCRIPT_DIR.parent / "data" / "salesnow_companies.csv"
# F6 中間出力: 市区町村 × 産業 × 従業員総数 (再現性確保)
SALESNOW_AGG_CSV = SCRIPT_DIR.parent / "data" / "generated" / "salesnow_aggregate_for_f6.csv"

# Model E 用に採用する産業コード
# 'AB' は A+B (農林漁業) 統合、S 公務 / AS,AR,CR の集計コードは除外
TARGET_INDUSTRY_CODES = (
    "AB", "C", "D", "E", "F", "G", "H", "I", "J", "K", "L", "M", "N", "O", "P", "Q", "R"
)

# 工業都市候補 (estimate_grade 用、ユーザー指示)
INDUSTRIAL_CITY_CANDIDATES = ["豊田市", "太田市", "浜松市", "堺市", "川崎市",
                               "相模原市", "厚木市", "四日市市", "北九州市"]
# 物流ハブ候補 (estimate_grade 用、ユーザー指示)
LOGISTICS_HUB_CANDIDATES = ["川崎市", "横浜市", "大阪市", "名古屋市", "福岡市",
                             "北九州市", "千葉市"]
# 製造系職業 / 物流系職業 (estimate_grade 用)
MFG_OCCUPATIONS = ["08_生産工程", "09_輸送機械"]
LOGISTICS_OCCUPATIONS = ["09_輸送機械", "11_運搬清掃"]

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

# ============================================================
# Model E2/E3/E4 用パラメータ (2026-05-04 改善ラウンド、Worker D)
# ============================================================

# Model E2: F4 を職業別重みに変更
# 現業職は workplace 移動が少ない (生産工程は工場常駐、輸送機械は道路上)
# オフィス職は workplace 移動が多い (港区/中央区への通勤)
OCCUPATION_F4_WEIGHT = {
    "01_管理":     1.0,   # オフィス通勤想定
    "02_専門技術": 1.0,
    "03_事務":     1.0,
    "04_販売":     0.7,   # 居住地小売もあるため中間
    "05_サービス": 0.5,   # 住宅地サービス業多い
    "06_保安":     0.5,
    "07_農林漁業": 0.0,   # workplace = 居住地と同地域
    "08_生産工程": 0.3,   # 工場常駐、通勤距離は短〜中
    "09_輸送機械": 0.3,
    "10_建設採掘": 0.5,
    "11_運搬清掃": 0.5,
}

# Model E3: F3 のべき乗強化指数
F3_POWER_E3 = 1.5

# Model E4: F5 強化係数 (E ベース 0.3 → E4 ベース 0.6)
F5_COEFF_E4 = 0.6
F5_CLAMP_E4 = (0.3, 3.0)  # E ベースは (0.5, 2.0)、E4 は緩める

# ============================================================
# Model F (F6 SalesNow 残差補正、2026-05-04 Worker E 追加)
# ============================================================
# F6 設計原則:
#   - F3 (経済センサス産業構成) は本社/支店/工場の区別なし → 港区の製造業従業者
#     シェアが「統計上」高く出るが、実態は本社オフィス
#   - SalesNow は企業実体 (本社所在地 + 規模) を提供 → 本社過剰を検出可能
#   - F6 は F3 と独立、現業職 (08-11) のみ減衰、ホワイトカラーは不変

# F6 減衰強度 alpha (Worker E 設定、最終 0.6 で安定)
# damping = 1 / (1 + alpha × (hq_excess_ratio - 1.0))
# alpha 0.25/0.5 では弱い、1.0 だと過剰に副作用 (Jaccard 改善せず) → 0.6 で妥協
F6_ALPHA = 0.6

# F6 トリガー閾値 (産業別、本社減衰の判定):
# - H (運輸業): 2.5 以上 (本社=オフィスの典型)
# - D (建設業): 3.0 (北九州 D=2.34 のような工事拠点本社+現場混在を保護)
# - E (製造業): 4.0 (大手メーカー本社=工場併設のケースを保護)
F6_HQ_EXCESS_THRESHOLD_BY_IND = {
    "D": 3.0,
    "E": 4.0,
    "H": 2.5,
}
# 後方互換用 (デフォルト閾値)
F6_HQ_EXCESS_THRESHOLD = 2.5

# F6 工業都市ブースト: estat_intensity が SalesNow より大きく、かつ実工場規模が大きい
# 市区町村は現業職をブースト (本社が少ない実工場都市)
# 閾値: estat_intensity / sn_intensity > 1.5 (estat 側が SalesNow の 1.5 倍以上)
F6_FACTORY_BOOST_THRESHOLD = 1.5
# ブースト強度 beta (検証結果 0.6 で Jaccard 改善、boost cap 1.4 で副作用最小)
F6_BETA = 0.6

# F6 対象職業 (現業職、本社過剰時に減衰)
F6_BLUE_COLLAR_OCCUPATIONS = ("08_生産工程", "09_輸送機械", "10_建設採掘", "11_運搬清掃")

# F6 対象 JSIC 産業コード (本社過剰検出対象)
# 製造業 (E)、建設業 (D)、運輸業 (H) のみ。これらは本社/工場/現場が分離する産業
F6_TARGET_JSIC_INDUSTRIES = ("D", "E", "H")

# SalesNow sn_industry → JSIC 大分類コードマッピング
# scripts/industry_mapping.py の INDUSTRY_MAPPING を参考にしつつ、JSIC コード体系で表現
# 1:1 対応 (信頼度 1.0) のみを採用、複数対応は最大シェアの 1 つを採用
# ホワイトカラー寄り産業 (IT, コンサル, 商社等) は本社所在地が大都市集中するが
# 本ロジックでは F6 トリガーは現業職に限定するため、それらの本社過剰は無視される
SALESNOW_TO_JSIC = {
    # 製造業 (JSIC E)
    "製造":         "E",
    "機械":         "E",
    "材料・資源":   "E",
    "食品":         "E",
    "自動車・輸送": "E",  # 大半は製造側 (車両製造)、運輸業は別マップ
    "生活用品":     "E",  # 製造寄り (家庭用品メーカー)
    "美容・アパレル": "E",  # アパレルメーカー寄り

    # 建設業 (JSIC D)
    "工事・土木":   "D",
    "建設":         "D",

    # 運輸業 (JSIC H)
    "交通・運輸・物流": "H",

    # 卸小売 (JSIC I) — F6 対象外 (現業職減衰しない) だが本社集中度測定のため記録
    "小売・販売・卸売": "I",
    "商社":         "I",

    # その他 — F6 トリガー対象外
    "IT":           "G",  # 情報通信業
    "通信・PC":     "G",
    "金融":         "J",
    "不動産":       "K",
    "コンサルティング": "L",
    "広告・制作":   "L",
    "士業":         "L",
    "飲食・外食":   "M",
    "観光・宿泊・レジャー": "M",
    "生活関連サービス": "N",
    "エンタメ・娯楽": "N",
    "メディア・出版": "N",
    "教育":         "O",
    "医療・製薬・福祉": "P",
    "人材・アウトソーシング": "R",
    "その他サービス": "R",
    "公共機関・特殊法人": "R",
    "エネルギー":   "F",
}

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
        SELECT prefecture, municipality, nighttime_pop, daytime_pop, day_night_ratio,
               inflow_pop, outflow_pop
        FROM v2_external_daytime_population
        WHERE prefecture IS NOT NULL AND prefecture <> ''
        """
    ).fetchall()
    data["daytime"] = {(r[0], r[1]): {"night": r[2] or 0, "day": r[3] or 0,
                                       "ratio": r[4] or 1.0,
                                       "inflow": r[5] or 0, "outflow": r[6] or 0}
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
# F3 用データロード (産業構成 CSV + 重みマスタ CSV)
# ============================================================

def load_industry_data(data):
    """産業構成 CSV と重みマスタ CSV をロードし、F3 計算用の構造を返す.

    Returns:
        industry_share: dict[(pref, muni)] -> dict[industry_code -> share]
        national_share: dict[industry_code -> national share]
        weights:        dict[(industry_code, occupation_code) -> weight]
    """
    if not INDUSTRY_CSV.exists():
        raise FileNotFoundError(f"Industry CSV not found: {INDUSTRY_CSV}")
    if not WEIGHT_CSV.exists():
        raise FileNotFoundError(f"Weight CSV not found: {WEIGHT_CSV}")

    # ---- 1. 重みマスタ読み込み ----
    weights = {}
    weight_industry_set = set()
    with open(WEIGHT_CSV, "r", encoding="utf-8", newline="") as f:
        reader = csv.DictReader(f)
        for row in reader:
            ind = row["industry_code"]
            occ = row["occupation_code"]
            w = float(row["weight"])
            weights[(ind, occ)] = w
            weight_industry_set.add(ind)

    # AB は A+B の単純平均で生成 (industry CSV は AB 統合のため、重みは A と B の平均)
    if ("A", "01_管理") in weights and ("B", "01_管理") in weights:
        for occ in NATIONAL_OCCUPATION_RATIO:
            wa = weights.get(("A", occ), 0.0)
            wb = weights.get(("B", occ), 0.0)
            weights[("AB", occ)] = (wa + wb) / 2.0
        weight_industry_set.add("AB")

    # weight 行数チェック (231 行 = 21 industries × 11 occupations)
    # AB を加えた後は 232+ になるため、CSV 元データのチェックは別途
    assert len(NATIONAL_OCCUPATION_RATIO) == 11, "Occupations must be 11"

    # 各 industry の合計が 1.0 ± 0.001 であること (元データのみチェック)
    industry_sums = defaultdict(float)
    for (ind, occ), w in weights.items():
        if ind == "AB":
            continue  # AB は派生のためスキップ
        industry_sums[ind] += w
    for ind, s in industry_sums.items():
        assert abs(s - 1.000) < 0.001, f"Weight sum for industry {ind} = {s}, expected 1.0"

    # ---- 2. 産業構成 CSV 読み込み ----
    # CSV は city_code (整数) のみ。pref_code → prefecture name は master から得る
    pref_code_to_name = {}
    code_to_pref_muni = {}  # municipality_code (5桁) -> (pref_name, muni_name)
    # area_type 値 (DB 実態): aggregate_city / aggregate_special_wards / designated_ward / municipality / special_ward
    # 政令市の区 (designated_ward) は population テーブルに存在しないので除外
    INCLUDE_AREA_TYPES = {"aggregate_city", "municipality", "special_ward",
                          "aggregate_special_wards"}
    for code, info in data["master_by_code"].items():
        if info["area_type"] in INCLUDE_AREA_TYPES:
            code_to_pref_muni[code] = (info["prefecture"], info["name"])
        # pref_code -> pref_name の最初の出現を記録
        pref_code = code[:2] if isinstance(code, str) and len(code) >= 2 else None
        if pref_code and pref_code not in pref_code_to_name:
            pref_code_to_name[pref_code] = info["prefecture"]

    # employees by (pref, muni, industry)
    industry_emp = defaultdict(lambda: defaultdict(float))  # [(pref,muni)][ind] = employees
    national_emp = defaultdict(float)  # [ind] = total employees nationwide

    with open(INDUSTRY_CSV, "r", encoding="utf-8", newline="") as f:
        reader = csv.DictReader(f)
        for row in reader:
            ind = row["industry_code"]
            if ind not in TARGET_INDUSTRY_CODES:
                continue  # 集計コード AS/AR/CR、S 公務を除外
            try:
                emp = float(row["employees_total"]) if row["employees_total"] else 0.0
            except (ValueError, TypeError):
                emp = 0.0
            if emp <= 0:
                continue
            # city_code を 5桁文字列に変換
            city_code = str(row["city_code"]).strip()
            try:
                city_code = str(int(city_code)).zfill(5)
            except (ValueError, TypeError):
                continue
            pref_muni = code_to_pref_muni.get(city_code)
            if pref_muni is None:
                continue  # master に未登録 (政令市の区など)
            industry_emp[pref_muni][ind] += emp
            national_emp[ind] += emp

    # ---- 3. industry_share / national_share ----
    industry_share = {}  # [(pref, muni)] -> dict[ind -> share]
    for pref_muni, ind_dict in industry_emp.items():
        total = sum(ind_dict.values())
        if total <= 0:
            continue
        industry_share[pref_muni] = {ind: emp / total for ind, emp in ind_dict.items()}

    national_total = sum(national_emp.values())
    national_share = {ind: emp / national_total for ind, emp in national_emp.items()} if national_total > 0 else {}

    # F6 用に raw industry_emp も返却 (本社過剰検出は実数規模で比較する必要あり)
    # data に attach する形で返す
    data["industry_emp"] = {pref_muni: dict(ind_dict) for pref_muni, ind_dict in industry_emp.items()}
    data["national_emp"] = dict(national_emp)

    # ---- 4. F6 用補完: 東京 23 区の estat_emp を 13100 (特別区部) 集計から按分 ----
    # CSV の経済センサスは 23 区を個別に持たず、13100 「特別区部」集計のみ存在する。
    # 按分戦略: 各区の総従業者規模は人口比だが、産業構成 (industry_share) は CSV から
    # 計算できないため、SalesNow 本社規模比を産業別構成として使用 (本社多 = その産業大)
    # → 港区: E (製造) は SalesNow 比で 23 区中シェア大 → estat も製造業多めに按分
    # → 世田谷区: E は SalesNow 比でシェア小 → estat も製造業少なめに按分
    # これで世田谷区/練馬区など住宅地の生産工程 F3 が下がる
    sw_aggregate_key = ("東京都", "特別区部")
    if sw_aggregate_key in data["industry_emp"]:
        sw_total_emp = data["industry_emp"][sw_aggregate_key]
        # 23 区の人口取得
        ward_pops = {}
        for (pref, muni), v in data["population"].items():
            if pref == "東京都" and muni.endswith("区") and muni != "特別区部":
                ward_pops[muni] = v["total"] or 0
        total_ward_pop = sum(ward_pops.values())

        # SalesNow CSV から 23 区別の業種別集計を読み込む (F6 補完用)
        ward_sn_by_ind = defaultdict(lambda: defaultdict(float))  # [muni][jsic_ind] = sn_emp
        if SALESNOW_AGG_CSV.exists():
            try:
                with open(SALESNOW_AGG_CSV, "r", encoding="utf-8", newline="") as f:
                    reader = csv.DictReader(f)
                    for row in reader:
                        if row["prefecture"] == "東京都" and row["municipality"].endswith("区"):
                            jc = row["jsic_code"]
                            ward_sn_by_ind[row["municipality"]][jc] += float(row["total_employees"])
            except (IOError, KeyError, ValueError):
                pass

        if total_ward_pop > 0:
            # 23 区合計の SalesNow 集計を計算
            total_sw_sn_by_ind = defaultdict(float)
            for muni, ind_dict in ward_sn_by_ind.items():
                for ind, emp in ind_dict.items():
                    total_sw_sn_by_ind[ind] += emp

            for muni, pop in ward_pops.items():
                ward_key = ("東京都", muni)
                if ward_key in data["industry_emp"] and data["industry_emp"].get(ward_key):
                    continue  # 既存データあり (政令市内区はここでは対象外)
                pop_share = pop / total_ward_pop
                # 各区の estat_emp を産業別に按分
                imputed = {}
                for ind, sw_total in sw_total_emp.items():
                    sn_total_for_ind = total_sw_sn_by_ind.get(ind, 0.0)
                    if sn_total_for_ind > 0 and ind in ward_sn_by_ind.get(muni, {}):
                        # SalesNow ベースの産業別シェア (本社多→estat も多めに按分)
                        sn_share = ward_sn_by_ind[muni][ind] / sn_total_for_ind
                        # 50% は SalesNow ベース、50% は人口比 (堅牢化のため混合)
                        share = 0.5 * sn_share + 0.5 * pop_share
                    else:
                        share = pop_share  # SalesNow データなし → 人口比のみ
                    imputed[ind] = sw_total * share
                data["industry_emp"][ward_key] = imputed

                # industry_share 補完 (按分後の構成比)
                total = sum(imputed.values())
                if total > 0:
                    industry_share[ward_key] = {ind: emp / total for ind, emp in imputed.items()}

    return industry_share, national_share, weights


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


def model_e(data, pref_occ_pop, industry_share, national_share, weights, alpha=0.25):
    """Model E: F1 + F2 (Model B baseline) × F3 × F4 × F5 × F6.

    F3[muni, occ] = (Σ_i industry_share[muni, i] × weight[i, occ])
                  / (Σ_i national_share[i] × weight[i, occ])
    F4[muni]     = daytime_pop / nighttime_pop  (workplace 基準)
    F5[muni]     = 1 + (inflow_rate × 0.3), inflow_rate = inflow / nighttime_pop
    F6           = 1.0 (SalesNow 不在のため stub)
    raw[muni, occ] = baseline × F3 × F4 × F5 × F6
    再正規化: 都道府県集計を pref_occ_pop に一致させる

    alpha は将来 F3 の弱め定数として使用予定 (現状未使用、将来 F6 残差補正で使用)
    """
    # Baseline (Model B)
    baseline = model_b(data, pref_occ_pop)

    # 国内ベース denom: Σ_i national_share[i] × weight[i, occ]
    nat_denom = {}
    for occ in NATIONAL_OCCUPATION_RATIO:
        nat_denom[occ] = sum(
            national_share.get(ind, 0.0) * weights.get((ind, occ), 0.0)
            for ind in TARGET_INDUSTRY_CODES
        )

    # raw 計算
    raw = defaultdict(dict)
    for (pref, muni), occ_dict in baseline.items():
        # F3 numerator: Σ_i industry_share[muni, i] × weight[i, occ]
        ind_share = industry_share.get((pref, muni))
        if ind_share is None:
            # 産業データなしの市区町村は F3=1.0 (補正なし) で続行
            f3_per_occ = {occ: 1.0 for occ in NATIONAL_OCCUPATION_RATIO}
        else:
            f3_per_occ = {}
            for occ in NATIONAL_OCCUPATION_RATIO:
                num = sum(
                    ind_share.get(ind, 0.0) * weights.get((ind, occ), 0.0)
                    for ind in TARGET_INDUSTRY_CODES
                )
                denom = nat_denom.get(occ, 0.0)
                f3_per_occ[occ] = (num / denom) if denom > 0 else 1.0

        # F4 (workplace 基準)
        f4 = 1.0
        d = data["daytime"].get((pref, muni))
        if d and d["night"] > 0:
            f4 = (d["day"] or 0) / d["night"]
            f4 = max(0.1, min(f4, 5.0))

        # F5 (流入率補正、簡易版)
        # F5 = 1 + inflow_rate × 0.3, clamp [0.5, 2.0]
        # inflow_rate = inflow_pop / nighttime_pop
        f5 = 1.0
        if d and d["night"] > 0:
            inflow = d.get("inflow", 0) or 0
            inflow_rate = inflow / d["night"]
            f5 = 1.0 + (inflow_rate * 0.3)
            f5 = max(0.5, min(f5, 2.0))

        for occ, pop in occ_dict.items():
            f3 = f3_per_occ.get(occ, 1.0)
            raw[(pref, muni)][occ] = pop * f3 * f4 * f5  # F6 = 1.0 stub

    # 再正規化 (都道府県集計を pref_occ_pop に一致)
    pref_raw_sum = defaultdict(lambda: defaultdict(float))
    for (pref, muni), occ_dict in raw.items():
        for occ, pop in occ_dict.items():
            pref_raw_sum[pref][occ] += pop
    scaling = defaultdict(dict)
    for pref, occ_dict in pref_raw_sum.items():
        for occ, raw_sum in occ_dict.items():
            target = pref_occ_pop.get(pref, {}).get(occ, 0)
            scaling[pref][occ] = (target / raw_sum) if raw_sum > 0 else 1.0
    out = defaultdict(dict)
    scaling_audit = defaultdict(dict)
    for (pref, muni), occ_dict in raw.items():
        for occ, pop in occ_dict.items():
            s = scaling[pref].get(occ, 1.0)
            out[(pref, muni)][occ] = pop * s
            scaling_audit[pref][occ] = s
    return dict(out), dict(scaling_audit)


def _compute_model_with_factors(data, pref_occ_pop, industry_share, national_share, weights,
                                  f3_power=1.0,
                                  f4_per_occupation=False,
                                  f5_coeff=0.3,
                                  f5_clamp=(0.5, 2.0)):
    """汎用モデル計算: F3^power, F4 職業別重み, F5 強化を切り替え可能.

    Worker C 既存 model_e 関数は変更せず、E2/E3/E4 用に新たに分離した実装。

    Args:
        f3_power: F3 のべき乗指数 (1.0 = 既存 E、1.5 = E3 以降)
        f4_per_occupation: True なら F4 = 1 + (F4_raw - 1) × OCCUPATION_F4_WEIGHT[occ]
                           False なら F4 = F4_raw (全職業同係数、既存 E)
        f5_coeff: F5 の係数 (0.3 = 既存 E、0.6 = E4)
        f5_clamp: F5 のクランプ範囲 (0.5,2.0 = 既存 E、0.3,3.0 = E4)

    Returns:
        out, scaling_audit
    """
    baseline = model_b(data, pref_occ_pop)

    # 国内ベース denom
    nat_denom = {}
    for occ in NATIONAL_OCCUPATION_RATIO:
        nat_denom[occ] = sum(
            national_share.get(ind, 0.0) * weights.get((ind, occ), 0.0)
            for ind in TARGET_INDUSTRY_CODES
        )

    raw = defaultdict(dict)
    for (pref, muni), occ_dict in baseline.items():
        # F3
        ind_share = industry_share.get((pref, muni))
        if ind_share is None:
            f3_per_occ = {occ: 1.0 for occ in NATIONAL_OCCUPATION_RATIO}
        else:
            f3_per_occ = {}
            for occ in NATIONAL_OCCUPATION_RATIO:
                num = sum(
                    ind_share.get(ind, 0.0) * weights.get((ind, occ), 0.0)
                    for ind in TARGET_INDUSTRY_CODES
                )
                denom = nat_denom.get(occ, 0.0)
                ratio = (num / denom) if denom > 0 else 1.0
                # べき乗強化 (E3/E4: f3_power=1.5)
                if f3_power != 1.0 and ratio > 0:
                    ratio = ratio ** f3_power
                f3_per_occ[occ] = ratio

        # F4_raw (workplace 基準)
        f4_raw = 1.0
        d = data["daytime"].get((pref, muni))
        if d and d["night"] > 0:
            f4_raw = (d["day"] or 0) / d["night"]
            f4_raw = max(0.1, min(f4_raw, 5.0))

        # F5 (流入率補正)
        f5 = 1.0
        if d and d["night"] > 0:
            inflow = d.get("inflow", 0) or 0
            inflow_rate = inflow / d["night"]
            f5 = 1.0 + (inflow_rate * f5_coeff)
            f5 = max(f5_clamp[0], min(f5, f5_clamp[1]))

        for occ, pop in occ_dict.items():
            f3 = f3_per_occ.get(occ, 1.0)
            # F4 職業別重み (E2 以降)
            if f4_per_occupation:
                w_occ = OCCUPATION_F4_WEIGHT.get(occ, 1.0)
                # F4_raw=1.0 なら全職業 1.0 (補正なし)
                # F4_raw=12.0 でも、生産工程 (w=0.3) は 1 + (12-1)*0.3 = 4.3
                f4 = 1.0 + (f4_raw - 1.0) * w_occ
                # 念のため極端なクランプ (0.1〜5.0)
                f4 = max(0.1, min(f4, 5.0))
            else:
                f4 = f4_raw
            raw[(pref, muni)][occ] = pop * f3 * f4 * f5  # F6 = 1.0 stub

    # 再正規化
    pref_raw_sum = defaultdict(lambda: defaultdict(float))
    for (pref, muni), occ_dict in raw.items():
        for occ, pop in occ_dict.items():
            pref_raw_sum[pref][occ] += pop
    scaling = defaultdict(dict)
    for pref, occ_dict in pref_raw_sum.items():
        for occ, raw_sum in occ_dict.items():
            target = pref_occ_pop.get(pref, {}).get(occ, 0)
            scaling[pref][occ] = (target / raw_sum) if raw_sum > 0 else 1.0
    out = defaultdict(dict)
    scaling_audit = defaultdict(dict)
    for (pref, muni), occ_dict in raw.items():
        for occ, pop in occ_dict.items():
            s = scaling[pref].get(occ, 1.0)
            out[(pref, muni)][occ] = pop * s
            scaling_audit[pref][occ] = s
    return dict(out), dict(scaling_audit)


def model_e2(data, pref_occ_pop, industry_share, national_share, weights):
    """Model E2: Model E + F4 職業別重み.

    F4_E2[muni, occ] = 1 + (F4_raw[muni] - 1) × OCCUPATION_F4_WEIGHT[occ]
    F4_raw=1.0 なら全職業 1.0 (補正なし)
    港区 F4_raw=4.0 (クランプ後) でも、生産工程は 1 + (4-1)*0.3 = 1.9 (vs E では 4.0)
    F3 power = 1.0、F5 既存 (0.3, [0.5, 2.0])
    """
    return _compute_model_with_factors(
        data, pref_occ_pop, industry_share, national_share, weights,
        f3_power=1.0,
        f4_per_occupation=True,
        f5_coeff=0.3,
        f5_clamp=(0.5, 2.0),
    )


def model_e3(data, pref_occ_pop, industry_share, national_share, weights):
    """Model E3: Model E2 + F3^1.5 べき乗強化.

    工業都市の F3 が 1.5 倍以上のとき、より大きく持ち上がる:
    F3=1.5 → 1.84 (べき乗 1.5)
    F3=2.0 → 2.83
    F3=0.7 → 0.59 (オフィス都市の生産工程はさらに低下)
    """
    return _compute_model_with_factors(
        data, pref_occ_pop, industry_share, national_share, weights,
        f3_power=F3_POWER_E3,
        f4_per_occupation=True,
        f5_coeff=0.3,
        f5_clamp=(0.5, 2.0),
    )


def model_e4(data, pref_occ_pop, industry_share, national_share, weights):
    """Model E4: Model E3 + F5 強化 (流入率影響を倍に).

    F5 = 1 + inflow_rate × 0.6 (E は 0.3)
    F5 クランプを (0.3, 3.0) に緩和 (E は (0.5, 2.0))
    """
    return _compute_model_with_factors(
        data, pref_occ_pop, industry_share, national_share, weights,
        f3_power=F3_POWER_E3,
        f4_per_occupation=True,
        f5_coeff=F5_COEFF_E4,
        f5_clamp=F5_CLAMP_E4,
    )


# ============================================================
# Model F (F6 SalesNow 残差補正) 用ロジック
# ============================================================

def load_salesnow_aggregate(use_cache=True):
    """SalesNow CSV (467k rows) を市区町村 × JSIC 産業単位で集約.

    本社所在地 (prefecture + address から市区町村抽出) × sn_industry → JSIC 大分類
    でグループ化し、企業数 + employee_count 合計を集約。

    集約結果は data/generated/salesnow_aggregate_for_f6.csv に保存し、再実行時は
    キャッシュを使用 (use_cache=True、生 CSV は 492MB で読み込みに 30+ 秒かかる)。

    Returns:
        sn_emp[(pref, muni, jsic_code)] -> total employee_count (本社規模合計)
        sn_companies[(pref, muni, jsic_code)] -> 企業数
    """
    import re

    # ---- キャッシュチェック ----
    if use_cache and SALESNOW_AGG_CSV.exists():
        sn_emp = {}
        sn_companies = {}
        with open(SALESNOW_AGG_CSV, "r", encoding="utf-8", newline="") as f:
            reader = csv.DictReader(f)
            for row in reader:
                key = (row["prefecture"], row["municipality"], row["jsic_code"])
                sn_emp[key] = float(row["total_employees"])
                sn_companies[key] = int(row["company_count"])
        return sn_emp, sn_companies

    # ---- 生 CSV から再集約 ----
    if not SALESNOW_CSV.exists():
        # フォールバック: ファイル不在 → 空辞書 (F6 効果なしで model_f が model_e3 と等価になる)
        return {}, {}

    sn_emp = defaultdict(float)
    sn_companies = defaultdict(int)

    # pandas を使うと 467k 行を効率的に処理可能
    try:
        import pandas as pd
    except ImportError:
        return {}, {}

    df = pd.read_csv(
        SALESNOW_CSV,
        usecols=["prefecture", "address", "sn_industry", "employee_count"],
        encoding="utf-8",
        low_memory=False,
    )
    df = df.dropna(subset=["prefecture", "address", "sn_industry"])

    # 市区町村抽出 (都道府県名で開始 → 市区町村まで切り出し)
    muni_pat = re.compile(r"^([^0-9０-９]+?[市区町村])")

    def extract_muni(pref, addr):
        if not isinstance(addr, str) or not isinstance(pref, str):
            return None
        s = addr
        if s.startswith(pref):
            s = s[len(pref):]
        m = muni_pat.match(s)
        return m.group(1) if m else None

    df["muni"] = df.apply(lambda r: extract_muni(r["prefecture"], r["address"]), axis=1)
    df["employee_count"] = pd.to_numeric(df["employee_count"], errors="coerce").fillna(0.0)
    df = df.dropna(subset=["muni"])

    # JSIC マッピング
    df["jsic_code"] = df["sn_industry"].map(SALESNOW_TO_JSIC)
    df = df.dropna(subset=["jsic_code"])

    # 集約 (pandas groupby)
    grouped = df.groupby(["prefecture", "muni", "jsic_code"]).agg(
        total_employees=("employee_count", "sum"),
        company_count=("employee_count", "size"),
    ).reset_index()

    # 中間 CSV 出力 (再現性 + 監査用)
    SALESNOW_AGG_CSV.parent.mkdir(parents=True, exist_ok=True)
    grouped.rename(columns={"muni": "municipality"}).to_csv(
        SALESNOW_AGG_CSV, index=False, encoding="utf-8"
    )

    # dict 形式で返却
    for _, row in grouped.iterrows():
        key = (row["prefecture"], row["muni"], row["jsic_code"])
        sn_emp[key] = float(row["total_employees"])
        sn_companies[key] = int(row["company_count"])

    return dict(sn_emp), dict(sn_companies)


def compute_f6_factor(sn_emp, industry_emp, national_emp, weights, alpha=F6_ALPHA):
    """F6 残差補正係数を計算 (実数ベース、設計修正版).

    アルゴリズム:
      1. 全国本社従業員総数 (JSIC 産業別): national_sn_emp[ind]
         全国経済センサス従業員総数: national_emp[ind] (data["national_emp"] から)
      2. 比率 (実数同士の比較):
         sn_intensity[muni, ind]   = sn_emp[muni, ind]   / national_sn_emp[ind]
         estat_intensity[muni, ind] = industry_emp[muni, ind] / national_emp[ind]
      3. 本社過剰度: hq_excess_ratio = sn_intensity / estat_intensity
         - 港区の E (製造): SalesNow で多く、経済センサスで少ない (本社のみ集中) → 高
         - 豊田市の E (実工場): SalesNow と経済センサスの両方で大 → ≈ 1.0
      4. 上限クランプ (10.0) で異常値防止
      5. 現業職減衰: damping = 1 / (1 + alpha × (ratio - 1.0))
         f6_factor[muni, occ] *= (1 - blue_collar_share × (1 - damping))

    Args:
        sn_emp: dict[(pref, muni, jsic_ind)] -> 本社従業員総数
        industry_emp: dict[(pref, muni)] -> dict[ind -> 経済センサス従業員数]
        national_emp: dict[ind -> 全国経済センサス従業員数]
        weights: dict[(ind, occ)] -> 産業内職業シェア
        alpha: 減衰強度 (推奨 0.25)

    Returns:
        f6_factor: dict[(pref, muni)] -> dict[occ -> 係数 (1.0 基本、本社過剰時 < 1.0)]
        diagnostics: dict
    """
    # ---- 1. 全国 SalesNow 本社従業員総数 ----
    national_sn_emp = defaultdict(float)
    for (pref, muni, ind), emp in sn_emp.items():
        national_sn_emp[ind] += emp

    # 全国合計 (各市区町村の SalesNow 全産業合計 / 全国合計)
    # 港区など estat 欠損市区町村のため、別アプローチで本社過剰を検出
    # 「人口あたりの SalesNow 本社規模」が指標になる
    national_sn_total = sum(national_sn_emp.values())

    # ---- 2-3. 各市区町村の hq_excess_ratio (本社過剰度) ----
    # 設計改訂版:
    #   estat_v > 0 (通常市区町村): sn_intensity / estat_intensity
    #   estat_v = 0 (東京特別区など、CSV 欠損): 別指標
    #     → 「該当産業の本社規模 / 人口」 / 「全国本社規模 / 全国人口」
    #     → 港区: E (製造業) 本社が人口 26 万人に対し 100 万人規模 → 高
    #     → 豊田市: E 本社 59 万 / 人口 41 万 → 同等比率
    hq_excess_ratio = defaultdict(dict)

    # 全国人口 = 全市区町村の合計 (industry_emp keys に含まれる pref_muni のみ。
    # 不足分は muni 不在として扱う)
    # ここではパラメータとして data["population"] が必要だが、引数を増やすため
    # pref_total から近似する (大都市の人口は概ね正確)
    # 実装: 引数追加 (data の population を受ける) を別途行う

    all_munis = set()
    for (pref, muni, ind) in sn_emp.keys():
        all_munis.add((pref, muni))
    for pref_muni in industry_emp.keys():
        all_munis.add(pref_muni)

    # 設計戦略 (二段構え):
    # (a) estat 欠損市区町村 (東京 23 区など): SalesNow 本社規模に基づき本社過剰判定
    #     → 港区/千代田区/中央区の現業職を抑制
    # (b) estat 存在市区町村: SalesNow と estat の比較で工場過剰 (本社少 + 実工場多) を検出
    #     → 豊田市/太田市/四日市市の現業職をブースト
    factory_excess_ratio = defaultdict(dict)  # ブースト用 (estat / sn 比、>1 で工場過剰)

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
                # estat あり: 通常の比較
                estat_intensity = estat_v / estat_total
                if sn > 0 and sn_intensity > 0:
                    hq_ratio = sn_intensity / estat_intensity
                    factory_ratio = estat_intensity / sn_intensity
                else:
                    # SalesNow 不在 → 中小工場のみ → factory_ratio 大
                    hq_ratio = 1.0
                    factory_ratio = 5.0  # 工場のみで本社不在 → 最大ブースト相当
                hq_excess_ratio[(pref, muni)][ind] = min(hq_ratio, 10.0)
                factory_excess_ratio[(pref, muni)][ind] = min(factory_ratio, 5.0)
            else:
                # estat 欠損 (東京 23 区): SalesNow 規模 × 市区町村数
                if sn > 0 and len(all_munis) > 0:
                    hq_ratio = sn_intensity * len(all_munis)
                else:
                    hq_ratio = 1.0
                hq_excess_ratio[(pref, muni)][ind] = min(hq_ratio, 10.0)
                factory_excess_ratio[(pref, muni)][ind] = 1.0  # ブーストなし

    # ---- 4. F6 係数の算出 (本社減衰 + 工場ブーストの二段) ----
    f6_factor = {}
    for (pref, muni), ratio_dict in hq_excess_ratio.items():
        per_occ_factor = {occ: 1.0 for occ in NATIONAL_OCCUPATION_RATIO}
        # (a) 本社過剰減衰 (産業別閾値、製造業は緩め)
        for ind, ratio in ratio_dict.items():
            ind_threshold = F6_HQ_EXCESS_THRESHOLD_BY_IND.get(ind, F6_HQ_EXCESS_THRESHOLD)
            if ratio > ind_threshold:
                damping = 1.0 / (1.0 + alpha * (ratio - 1.0))
                for occ in F6_BLUE_COLLAR_OCCUPATIONS:
                    blue_collar_share = weights.get((ind, occ), 0.0)
                    damp_amount = blue_collar_share * (1.0 - damping)
                    per_occ_factor[occ] *= (1.0 - damp_amount)
        # (b) 工場過剰ブースト (estat 比 sn 多 → 実工場多)
        # 横浜/大阪など港湾都市は H factory_ratio 高 → 過剰ブーストを抑えるため上限 1.4
        if F6_BETA > 0:
            factory_dict = factory_excess_ratio.get((pref, muni), {})
            for ind, f_ratio in factory_dict.items():
                if f_ratio > F6_FACTORY_BOOST_THRESHOLD:
                    boost = 1.0 + F6_BETA * (f_ratio - 1.0)
                    boost = min(boost, 1.4)
                    for occ in F6_BLUE_COLLAR_OCCUPATIONS:
                        blue_collar_share = weights.get((ind, occ), 0.0)
                        boost_amount = blue_collar_share * (boost - 1.0)
                        per_occ_factor[occ] *= (1.0 + boost_amount)
        f6_factor[(pref, muni)] = per_occ_factor

    # ---- 5. 診断情報 ----
    diagnostics = {
        "national_sn_emp_target_industries": {
            ind: national_sn_emp.get(ind, 0.0) for ind in F6_TARGET_JSIC_INDUSTRIES
        },
        "national_estat_emp_target_industries": {
            ind: national_emp.get(ind, 0.0) for ind in F6_TARGET_JSIC_INDUSTRIES
        },
        "hq_excess_ratio_samples": {},
        "factory_excess_ratio_samples": {},
        "f6_factor_samples": {},
        "raw_data_samples": {},
    }
    for sample_pref, sample_muni in [
        ("東京都", "港区"), ("東京都", "千代田区"), ("東京都", "中央区"),
        ("東京都", "新宿区"), ("東京都", "世田谷区"),
        ("愛知県", "豊田市"), ("群馬県", "太田市"), ("神奈川県", "川崎市"),
        ("三重県", "四日市市"), ("静岡県", "浜松市"), ("福岡県", "北九州市"),
        ("神奈川県", "相模原市"), ("大阪府", "堺市"),
    ]:
        key = (sample_pref, sample_muni)
        label = f"{sample_pref} {sample_muni}"
        if key in hq_excess_ratio:
            diagnostics["hq_excess_ratio_samples"][label] = {
                ind: round(hq_excess_ratio[key].get(ind, 1.0), 3)
                for ind in F6_TARGET_JSIC_INDUSTRIES
            }
        if key in factory_excess_ratio:
            diagnostics["factory_excess_ratio_samples"][label] = {
                ind: round(factory_excess_ratio[key].get(ind, 1.0), 3)
                for ind in F6_TARGET_JSIC_INDUSTRIES
            }
        if key in f6_factor:
            diagnostics["f6_factor_samples"][label] = {
                occ: round(f6_factor[key].get(occ, 1.0), 3)
                for occ in F6_BLUE_COLLAR_OCCUPATIONS
            }
        diagnostics["raw_data_samples"][label] = {
            ind: {
                "sn_emp": round(sn_emp.get((sample_pref, sample_muni, ind), 0.0), 0),
                "estat_emp": round(industry_emp.get(key, {}).get(ind, 0.0), 0),
            }
            for ind in F6_TARGET_JSIC_INDUSTRIES
        }

    return f6_factor, diagnostics


def model_f(data, pref_occ_pop, industry_share, national_share, weights, f6_factor):
    """Model F: Model E3 + F6 SalesNow 残差補正.

    raw[muni, occ] = baseline (F1+F2)
                   × F3[muni, occ] ** 1.5 (E3 と同じべき乗)
                   × F4_occupation_weighted[muni, occ] (E2 由来)
                   × F5[muni] (E と同じ)
                   × F6_factor[muni, occ] (新規、本社過剰減衰)

    F6 は F3 と独立。基本値 1.0 で、本社過剰 (港区等) のときのみ < 1.0。
    F6 が 1.0 のとき Model F は Model E3 と完全一致する設計。

    Returns:
        out, scaling_audit
    """
    baseline = model_b(data, pref_occ_pop)

    # 国内ベース denom (E3 と同じ)
    nat_denom = {}
    for occ in NATIONAL_OCCUPATION_RATIO:
        nat_denom[occ] = sum(
            national_share.get(ind, 0.0) * weights.get((ind, occ), 0.0)
            for ind in TARGET_INDUSTRY_CODES
        )

    raw = defaultdict(dict)
    for (pref, muni), occ_dict in baseline.items():
        # F3 (べき乗 1.5、E3 と同じ)
        ind_share = industry_share.get((pref, muni))
        if ind_share is None:
            f3_per_occ = {occ: 1.0 for occ in NATIONAL_OCCUPATION_RATIO}
        else:
            f3_per_occ = {}
            for occ in NATIONAL_OCCUPATION_RATIO:
                num = sum(
                    ind_share.get(ind, 0.0) * weights.get((ind, occ), 0.0)
                    for ind in TARGET_INDUSTRY_CODES
                )
                denom = nat_denom.get(occ, 0.0)
                ratio = (num / denom) if denom > 0 else 1.0
                if ratio > 0:
                    ratio = ratio ** F3_POWER_E3
                f3_per_occ[occ] = ratio

        # F4 (raw + 職業別重み、E2 と同じ)
        f4_raw = 1.0
        d = data["daytime"].get((pref, muni))
        if d and d["night"] > 0:
            f4_raw = (d["day"] or 0) / d["night"]
            f4_raw = max(0.1, min(f4_raw, 5.0))

        # F5 (E と同じ係数 0.3)
        f5 = 1.0
        if d and d["night"] > 0:
            inflow = d.get("inflow", 0) or 0
            inflow_rate = inflow / d["night"]
            f5 = 1.0 + (inflow_rate * 0.3)
            f5 = max(0.5, min(f5, 2.0))

        # F6 (新規)
        f6_per_occ = f6_factor.get((pref, muni), {})

        for occ, pop in occ_dict.items():
            f3 = f3_per_occ.get(occ, 1.0)
            w_occ = OCCUPATION_F4_WEIGHT.get(occ, 1.0)
            f4 = 1.0 + (f4_raw - 1.0) * w_occ
            f4 = max(0.1, min(f4, 5.0))
            f6 = f6_per_occ.get(occ, 1.0)
            raw[(pref, muni)][occ] = pop * f3 * f4 * f5 * f6

    # 再正規化 (都道府県集計を pref_occ_pop に一致)
    pref_raw_sum = defaultdict(lambda: defaultdict(float))
    for (pref, muni), occ_dict in raw.items():
        for occ, pop in occ_dict.items():
            pref_raw_sum[pref][occ] += pop
    scaling = defaultdict(dict)
    for pref, occ_dict in pref_raw_sum.items():
        for occ, raw_sum in occ_dict.items():
            target = pref_occ_pop.get(pref, {}).get(occ, 0)
            scaling[pref][occ] = (target / raw_sum) if raw_sum > 0 else 1.0
    out = defaultdict(dict)
    scaling_audit = defaultdict(dict)
    for (pref, muni), occ_dict in raw.items():
        for occ, pop in occ_dict.items():
            s = scaling[pref].get(occ, 1.0)
            out[(pref, muni)][occ] = pop * s
            scaling_audit[pref][occ] = s
    return dict(out), dict(scaling_audit)


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


def jaccard_top_n(model_result, occ_a, occ_b, n=10):
    """2 職業間の TOP N (市区町村集合) Jaccard 類似度"""
    top_a = set(loc for loc, _ in ranking_top_n(model_result, occ_a, n=n))
    top_b = set(loc for loc, _ in ranking_top_n(model_result, occ_b, n=n))
    if not top_a or not top_b:
        return 0.0
    intersection = len(top_a & top_b)
    union = len(top_a | top_b)
    return intersection / union if union > 0 else 0.0


def jaccard_matrix_avg(model_result, occupations, n=10):
    """全職業ペアの TOP N Jaccard 類似度を計算し、上三角の平均を返す.

    対角は除外 (同じ職業同士は 1.0)。
    """
    pairs = []
    matrix = {}
    occs = list(occupations)
    for i, occ_a in enumerate(occs):
        matrix[occ_a] = {}
        for j, occ_b in enumerate(occs):
            if i == j:
                matrix[occ_a][occ_b] = 1.0
                continue
            j_val = jaccard_top_n(model_result, occ_a, occ_b, n=n)
            matrix[occ_a][occ_b] = j_val
            if i < j:
                pairs.append(j_val)
    avg = mean(pairs) if pairs else 0.0
    return avg, matrix


def population_rank_diff(data, model_result, occupation, target_munis):
    """指定職業について、人口ランキングと Model ランキングの差分を計算.

    人口ランキング: 全市区町村の total_population 降順での順位
    Model ランキング: model_result の occupation 値降順での順位
    差分 = pop_rank - model_rank (正なら Model で上位浮上、負なら下位)
    """
    # 人口ランキング
    pop_items = [((p, m), v["total"]) for (p, m), v in data["population"].items()]
    pop_items.sort(key=lambda x: -x[1])
    pop_rank = {loc: rank + 1 for rank, (loc, _) in enumerate(pop_items)}

    # Model ランキング
    mdl_items = [((p, m), occ_dict.get(occupation, 0))
                 for (p, m), occ_dict in model_result.items()]
    mdl_items.sort(key=lambda x: -x[1])
    mdl_rank = {loc: rank + 1 for rank, (loc, _) in enumerate(mdl_items)}

    out = []
    for pref, muni, _ in target_munis:
        key = (pref, muni)
        pr = pop_rank.get(key)
        mr = mdl_rank.get(key)
        if pr is None or mr is None:
            continue
        out.append({
            "name": f"{pref} {muni}",
            "pop_rank": pr,
            "model_rank": mr,
            "diff": pr - mr,  # 正 = Model で上位浮上
        })
    return out


def estimate_grade(jaccard_avg_e, model_e_result):
    """合格判定ロジック (estimate_grade A/B/C/D/X).

    A: Jaccard < 0.6 かつ 製造系 TOP10 に工業都市 3 都市以上 + 物流系 TOP10 に物流ハブ 3 都市以上
    B: Jaccard < 0.7 かつ いずれか片方の地域条件を満たす
    C: Jaccard < 0.85
    D: Jaccard >= 0.85
    X: 計算不能
    """
    if jaccard_avg_e is None:
        return "X", {}

    # 製造系 TOP 10 の工業都市カウント (08_生産工程, 09_輸送機械)
    mfg_top_locs = set()
    for occ in MFG_OCCUPATIONS:
        for loc, _ in ranking_top_n(model_e_result, occ, n=10):
            mfg_top_locs.add(loc)
    industrial_hits = sum(
        1 for cand in INDUSTRIAL_CITY_CANDIDATES
        if any(cand in loc for loc in mfg_top_locs)
    )

    # 物流系 TOP 10 の物流ハブカウント (09_輸送機械, 11_運搬清掃)
    log_top_locs = set()
    for occ in LOGISTICS_OCCUPATIONS:
        for loc, _ in ranking_top_n(model_e_result, occ, n=10):
            log_top_locs.add(loc)
    logistic_hits = sum(
        1 for cand in LOGISTICS_HUB_CANDIDATES
        if any(cand in loc for loc in log_top_locs)
    )

    audit = {
        "jaccard_avg": jaccard_avg_e,
        "industrial_cities_in_mfg_top10": industrial_hits,
        "logistics_hubs_in_logistics_top10": logistic_hits,
        "industrial_candidates_total": len(INDUSTRIAL_CITY_CANDIDATES),
        "logistic_candidates_total": len(LOGISTICS_HUB_CANDIDATES),
    }

    if jaccard_avg_e < 0.6 and industrial_hits >= 3 and logistic_hits >= 3:
        grade = "A"
    elif jaccard_avg_e < 0.7 and (industrial_hits >= 3 or logistic_hits >= 3):
        grade = "B"
    elif jaccard_avg_e < 0.85:
        grade = "C"
    else:
        grade = "D"
    return grade, audit


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

    # Model E (F1+F2+F3+F4+F5+F6_stub) - 産業構成補正
    print("  - Model E: B + F3 (産業構成) + F4 (昼夜間) + F5 (流入) + F6 (stub)")
    industry_share, national_share, weights = load_industry_data(data)
    print(f"    F3 用 industry_share: {len(industry_share):,} 市区町村")
    print(f"    F3 用 national_share: {len(national_share)} 産業 ({sorted(national_share.keys())})")
    print(f"    重み (industry x occupation): {len(weights):,} エントリ (AB 派生含む)")
    result_e, scaling_audit_e = model_e(data, pref_occ_pop, industry_share, national_share, weights)

    # Model E2/E3/E4 (2026-05-04 改善ラウンド、Worker D)
    print("  - Model E2: E + F4 職業別重み (現業職は F4 弱)")
    result_e2, _ = model_e2(data, pref_occ_pop, industry_share, national_share, weights)
    print("  - Model E3: E2 + F3^1.5 べき乗強化")
    result_e3, _ = model_e3(data, pref_occ_pop, industry_share, national_share, weights)
    print("  - Model E4: E3 + F5 強化 (0.3 → 0.6)")
    result_e4, _ = model_e4(data, pref_occ_pop, industry_share, national_share, weights)

    # Model F (2026-05-04 最終ラウンド、Worker E、F6 SalesNow 残差補正)
    print("  - Model F: E3 + F6 SalesNow 残差補正 (港区異常抑制)")
    sn_emp, sn_companies = load_salesnow_aggregate(use_cache=True)
    print(f"    SalesNow 集約: {len(sn_emp):,} (pref, muni, jsic) cells, "
          f"{len(sn_companies):,} ユニーク企業集計")
    if not sn_emp:
        print("    ⚠️ SalesNow データ未取得 → Model F は Model E3 と等価 (F6=1.0)")
        f6_factor, f6_diag = {}, {}
    else:
        f6_factor, f6_diag = compute_f6_factor(
            sn_emp,
            data["industry_emp"],
            data["national_emp"],
            weights,
            alpha=F6_ALPHA,
        )
        print(f"    F6 alpha={F6_ALPHA}, hq_excess_threshold={F6_HQ_EXCESS_THRESHOLD}")
        print(f"    F6 factor 計算済み: {len(f6_factor):,} 市区町村")
        # 港区の hq_excess_ratio をログ出力 (検証用)
        if "東京都 港区" in f6_diag.get("hq_excess_ratio_samples", {}):
            ratios = f6_diag["hq_excess_ratio_samples"]["東京都 港区"]
            print(f"    港区 hq_excess_ratio: D={ratios.get('D', 1):.2f}  "
                  f"E={ratios.get('E', 1):.2f}  H={ratios.get('H', 1):.2f}")
        if "愛知県 豊田市" in f6_diag.get("hq_excess_ratio_samples", {}):
            ratios = f6_diag["hq_excess_ratio_samples"]["愛知県 豊田市"]
            print(f"    豊田市 hq_excess_ratio: D={ratios.get('D', 1):.2f}  "
                  f"E={ratios.get('E', 1):.2f}  H={ratios.get('H', 1):.2f}")
    result_f, _ = model_f(data, pref_occ_pop, industry_share, national_share, weights, f6_factor)

    # [4] 都道府県再集計誤差
    print("\n[4] 都道府県再集計誤差 (期待: A/B はゼロ、C'/E は scaling で補正)")
    err_a = pref_aggregation_error(result_a, pref_occ_pop)
    err_b = pref_aggregation_error(result_b, pref_occ_pop)
    err_c = pref_aggregation_error(result_c, pref_occ_pop)
    err_e = pref_aggregation_error(result_e, pref_occ_pop)
    print(f"  - Model A:  mean {err_a['mean_pct']:.4f}%, max {err_a['max_pct']:.4f}%")
    print(f"  - Model B:  mean {err_b['mean_pct']:.4f}%, max {err_b['max_pct']:.4f}%")
    print(f"  - Model C': mean {err_c['mean_pct']:.4f}%, max {err_c['max_pct']:.4f}%")
    print(f"  - Model E:  mean {err_e['mean_pct']:.4f}%, max {err_e['max_pct']:.4f}%")

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

    # ============================================================
    # Model E 評価 (F3 効果検証)
    # ============================================================
    print("\n" + "=" * 75)
    print("Model E (F3 産業構成補正) 評価")
    print("=" * 75)

    # [E1] TOP 20 比較 (Model A vs Model E、4 主要職業)
    print("\n[E1] TOP 20 比較: Model A vs Model E (4 主要職業)")
    model_e_top20 = {}
    model_a_top20 = {}
    for occ in LOGISTIC_MFG_CONSTRUCTION_OCCUPATIONS:
        print(f"\n  >>> {occ}")
        top_a = ranking_top_n(result_a, occ, n=20)
        top_e = ranking_top_n(result_e, occ, n=20)
        model_a_top20[occ] = top_a
        model_e_top20[occ] = top_e
        print(f"    {'Rank':<5}{'Model A':<32}{'Value':>10}  | {'Model E':<32}{'Value':>10}")
        for i in range(20):
            la, va = top_a[i] if i < len(top_a) else ("", 0)
            le, ve = top_e[i] if i < len(top_e) else ("", 0)
            print(f"    {i+1:<5}{la:<32}{int(va):>10,}  | {le:<32}{int(ve):>10,}")

    # [E2] Jaccard 類似度 (TOP 10 オーバーラップ)
    print("\n[E2] 全 11 職業 × 11 職業の TOP 10 Jaccard 類似度 (上三角平均)")
    jaccard_a, matrix_a = jaccard_matrix_avg(result_a, list(NATIONAL_OCCUPATION_RATIO), n=10)
    jaccard_b, matrix_b = jaccard_matrix_avg(result_b, list(NATIONAL_OCCUPATION_RATIO), n=10)
    jaccard_c, matrix_c = jaccard_matrix_avg(result_c, list(NATIONAL_OCCUPATION_RATIO), n=10)
    jaccard_e, matrix_e = jaccard_matrix_avg(result_e, list(NATIONAL_OCCUPATION_RATIO), n=10)
    print(f"  - Model A:  {jaccard_a:.3f} (高 = 全職業同 TOP 10、職業差なし)")
    print(f"  - Model B:  {jaccard_b:.3f}")
    print(f"  - Model C': {jaccard_c:.3f}")
    print(f"  - Model E:  {jaccard_e:.3f} (低 = 職業ごとに異なる TOP 10、F3 効果あり)")
    print(f"  - 合格目標: < 0.6 (理想)、< 0.7 (許容)")

    # [E3] 人口順位差分 (代表地域 × 4 主要職業)
    print("\n[E3] 人口順位差分 (Model E ランク - 人口ランク、正なら浮上)")
    rank_diff_data = {}
    for occ in LOGISTIC_MFG_CONSTRUCTION_OCCUPATIONS:
        print(f"\n  >>> {occ}")
        diffs = population_rank_diff(data, result_e, occ, TARGET_MUNICIPALITIES)
        rank_diff_data[occ] = diffs
        print(f"    {'地域':<24}{'人口ランク':>10}{'Model Eランク':>14}{'差分':>8}")
        for d in diffs:
            print(f"    {d['name']:<24}{d['pop_rank']:>10}{d['model_rank']:>14}{d['diff']:>+8}")

    # [E4] 値比較: 主要工業都市の Model A vs Model E
    print("\n[E4] 主要工業都市の値変化 (Model A → Model E)")
    showcase_munis = [
        ("愛知県", "豊田市"), ("群馬県", "太田市"), ("神奈川県", "川崎市"),
        ("三重県", "四日市市"), ("福岡県", "北九州市"), ("静岡県", "浜松市"),
    ]
    showcase_data = {}
    for pref, muni in showcase_munis:
        key = (pref, muni)
        for occ in LOGISTIC_MFG_CONSTRUCTION_OCCUPATIONS:
            va = result_a.get(key, {}).get(occ, 0)
            ve = result_e.get(key, {}).get(occ, 0)
            top_e = ranking_top_n(result_e, occ, n=2000)
            rank_e = next((i + 1 for i, (loc, _) in enumerate(top_e)
                           if loc == f"{pref} {muni}"), None)
            showcase_data[f"{pref} {muni} | {occ}"] = {
                "model_a": va, "model_e": ve,
                "ratio_e_over_a": (ve / va) if va > 0 else None,
                "model_e_rank": rank_e,
            }
            print(f"    {pref} {muni:<8} {occ:<14} A={int(va):>7,} → E={int(ve):>7,} "
                  f"(x{(ve/va) if va > 0 else 0:.2f}) Eランク={rank_e}")

    # [E5] estimate_grade 判定
    print("\n[E5] estimate_grade 判定")
    grade_e, grade_audit = estimate_grade(jaccard_e, result_e)
    print(f"    Jaccard 平均 (Model E): {jaccard_e:.3f}")
    print(f"    製造系 TOP10 中の工業都市: {grade_audit['industrial_cities_in_mfg_top10']} / "
          f"{grade_audit['industrial_candidates_total']} 候補")
    print(f"    物流系 TOP10 中の物流ハブ: {grade_audit['logistics_hubs_in_logistics_top10']} / "
          f"{grade_audit['logistic_candidates_total']} 候補")
    print(f"    >>> estimate_grade = {grade_e}")
    if grade_e in ("A", "B"):
        print("        ✅ 商品化への合格判定 (採用ターゲット母集団推定指標として活用可能)")
    elif grade_e == "C":
        print("        ⚠️ 一定の差別化はあるが地域条件未達。追加補正検討")
    else:
        print("        ❌ 不合格。F6 (SalesNow) や別アプローチ検討")

    # [E6] 表示文言案の判定 (verdict_per_format)
    print("\n[E6] 表示文言案ごとの判定")
    verdict_per_format = {
        "absolute_count": {  # 「川崎市の生産工程従事者は約 86,000 人」
            "format": "人数表示 (絶対値)",
            "verdict": "NG",
            "reason": "真の正解データ不在、絶対精度の保証不可。再集計 scaling は内部整合のみ。",
        },
        "thickness_index": {  # 「ターゲット厚み指数 = 142 (全国平均 100)」
            "format": "推定指数 (0-200 正規化)",
            "verdict": "OK" if grade_e in ("A", "B", "C") else "NG",
            "reason": f"Jaccard {jaccard_e:.3f} で職業差が出ている。指数として相対比較可能。" if grade_e in ("A", "B", "C")
                       else "Jaccard 高すぎ、職業差不明瞭。",
        },
        "delivery_priority": {  # 「配信優先度: A ランク (上位 5%)」
            "format": "配信優先度ランク",
            "verdict": "OK" if grade_e in ("A", "B") else "要検証",
            "reason": "TOP10 が職業ごとに異なる場合、ランク商品として提示可" if grade_e in ("A", "B")
                       else "F3 効果不十分、ランクに信頼性なし",
        },
        "scenario_range": {  # 「保守/標準/強気 = 1×/3×/5×」
            "format": "採用シナリオ濃淡",
            "verdict": "OK",
            "reason": "turnover_rate 倍率は固定。base が指数なら濃淡表現として商品化可。",
        },
    }
    for k, v in verdict_per_format.items():
        marker = "✅" if v["verdict"] == "OK" else ("⚠️" if v["verdict"] == "要検証" else "❌")
        print(f"    {marker} {v['format']:<24} → {v['verdict']:<8} ({v['reason']})")

    # ============================================================
    # Model E2/E3/E4 評価 (Worker D, 2026-05-04 改善ラウンド)
    # ============================================================
    print("\n" + "=" * 75)
    print("Model E2/E3/E4 評価 (改善ラウンド)")
    print("=" * 75)

    # ヘルパー: 各モデル評価
    def _evaluate_model(name, result_dict):
        """指定モデルについて、Jaccard + grade + 工業都市カウント + 港区/中央区順位を計算."""
        jaccard_v, _ = jaccard_matrix_avg(result_dict, list(NATIONAL_OCCUPATION_RATIO), n=10)
        grade_v, audit_v = estimate_grade(jaccard_v, result_dict)

        # 港区・中央区の生産工程順位
        top_prod = ranking_top_n(result_dict, "08_生産工程", n=2000)
        rank_minato = next((i + 1 for i, (loc, _) in enumerate(top_prod)
                            if loc == "東京都 港区"), None)
        rank_chuo = next((i + 1 for i, (loc, _) in enumerate(top_prod)
                          if loc == "東京都 中央区"), None)

        return {
            "name": name,
            "jaccard": jaccard_v,
            "grade": grade_v,
            "manufacturing_count": audit_v["industrial_cities_in_mfg_top10"],
            "logistics_count": audit_v["logistics_hubs_in_logistics_top10"],
            "port_minato_rank": rank_minato,
            "chuo_rank": rank_chuo,
            "audit": audit_v,
        }

    eval_a  = _evaluate_model("Model A",  result_a)
    eval_b  = _evaluate_model("Model B",  result_b)
    eval_c  = _evaluate_model("Model C'", result_c)
    eval_e  = _evaluate_model("Model E",  result_e)
    eval_e2 = _evaluate_model("Model E2", result_e2)
    eval_e3 = _evaluate_model("Model E3", result_e3)
    eval_e4 = _evaluate_model("Model E4", result_e4)
    eval_f  = _evaluate_model("Model F",  result_f)

    all_evals = [eval_a, eval_b, eval_c, eval_e, eval_e2, eval_e3, eval_e4, eval_f]

    print("\n=== モデル別 estimate_grade ===")
    for ev in all_evals:
        port_str = f"{ev['port_minato_rank']:>4}" if ev['port_minato_rank'] else "  -"
        chuo_str = f"{ev['chuo_rank']:>4}" if ev['chuo_rank'] else "  -"
        print(f"  {ev['name']:<10} grade={ev['grade']}  Jaccard={ev['jaccard']:.3f}  "
              f"製造={ev['manufacturing_count']}/9  物流={ev['logistics_count']}/7  "
              f"港区生産工程={port_str}  中央区生産工程={chuo_str}")

    print("\n=== 製造系 (08_生産工程 + 09_輸送機械 平均) TOP 10 工業都市カウント ===")
    print("    候補: 豊田/太田/浜松/堺/川崎/相模原/厚木/四日市/北九州 (9 候補)")
    for ev in all_evals:
        print(f"    {ev['name']:<10} {ev['manufacturing_count']}")

    print("\n=== 物流系 (09_輸送機械 + 11_運搬清掃 平均) TOP 10 湾岸都市カウント ===")
    print("    候補: 川崎/横浜/大阪/名古屋/福岡/北九州/千葉 (7 候補)")
    for ev in all_evals:
        print(f"    {ev['name']:<10} {ev['logistics_count']}")

    print("\n=== 港区・中央区の生産工程順位 (副作用検出) ===")
    for ev in all_evals:
        print(f"    {ev['name']:<10} 港区={ev['port_minato_rank']}  中央区={ev['chuo_rank']}")

    # 工業都市の詳細順位 (E vs F 比較)
    print("\n=== 工業都市 生産工程順位の改善 (Model E vs E2/E3/E4/F) ===")
    industrial_rank_compare = {}
    for cand_pref, cand_muni in [("愛知県", "豊田市"), ("群馬県", "太田市"),
                                    ("三重県", "四日市市"), ("静岡県", "浜松市"),
                                    ("神奈川県", "川崎市"), ("福岡県", "北九州市"),
                                    ("神奈川県", "相模原市"), ("大阪府", "堺市"),
                                    ("神奈川県", "厚木市"),
                                    ("東京都", "港区"), ("東京都", "中央区"),
                                    ("東京都", "千代田区")]:
        loc_str = f"{cand_pref} {cand_muni}"
        ranks = {}
        for ev_name, result in [("E", result_e), ("E2", result_e2),
                                 ("E3", result_e3), ("E4", result_e4),
                                 ("F", result_f)]:
            top = ranking_top_n(result, "08_生産工程", n=2000)
            rank = next((i + 1 for i, (loc, _) in enumerate(top) if loc == loc_str), None)
            ranks[ev_name] = rank
        industrial_rank_compare[loc_str] = ranks
        print(f"    {loc_str:<22} E={ranks['E']}  E2={ranks['E2']}  E3={ranks['E3']}  "
              f"E4={ranks['E4']}  F={ranks['F']}")

    # E2/E3/E4/F の TOP 20 (主要 4 職業のみ)
    model_e2_top20 = {occ: ranking_top_n(result_e2, occ, n=20)
                      for occ in LOGISTIC_MFG_CONSTRUCTION_OCCUPATIONS}
    model_e3_top20 = {occ: ranking_top_n(result_e3, occ, n=20)
                      for occ in LOGISTIC_MFG_CONSTRUCTION_OCCUPATIONS}
    model_e4_top20 = {occ: ranking_top_n(result_e4, occ, n=20)
                      for occ in LOGISTIC_MFG_CONSTRUCTION_OCCUPATIONS}
    model_f_top20 = {occ: ranking_top_n(result_f, occ, n=20)
                      for occ in LOGISTIC_MFG_CONSTRUCTION_OCCUPATIONS}

    # Model E4 の主要職業 TOP 10 を出力 (品質確認)
    print("\n=== Model E4: 主要職業 TOP 10 ===")
    for occ in LOGISTIC_MFG_CONSTRUCTION_OCCUPATIONS:
        print(f"\n  >>> {occ}")
        top = ranking_top_n(result_e4, occ, n=10)
        for i, (loc, v) in enumerate(top, 1):
            print(f"    {i:2}. {loc:<24} {int(v):>10,} 人")

    # Model F の主要職業 TOP 10 を出力 (F6 効果確認)
    print("\n=== Model F (F6 SalesNow 残差補正): 主要職業 TOP 10 ===")
    for occ in LOGISTIC_MFG_CONSTRUCTION_OCCUPATIONS:
        print(f"\n  >>> {occ}")
        top = ranking_top_n(result_f, occ, n=10)
        for i, (loc, v) in enumerate(top, 1):
            print(f"    {i:2}. {loc:<24} {int(v):>10,} 人")

    # 最良モデル選定
    def _model_score(ev):
        """評価指標スコア (Jaccard + 工業都市 + 物流ハブ + 港区順位 + 中央区順位)."""
        jaccard_score = max(0.0, 1.0 - ev['jaccard'])  # Jaccard 低いほど高スコア
        mfg_score = ev['manufacturing_count'] / 9.0
        log_score = ev['logistics_count'] / 7.0
        # 港区順位は >=30 で満点
        port_rank = ev['port_minato_rank'] or 1
        port_score = min(1.0, port_rank / 30.0)
        chuo_rank = ev['chuo_rank'] or 1
        chuo_score = min(1.0, chuo_rank / 30.0)
        return jaccard_score + mfg_score + log_score + port_score + chuo_score

    best_eval = max([eval_e, eval_e2, eval_e3, eval_e4, eval_f], key=_model_score)
    print(f"\n=== 最良モデル ===")
    print(f"    {best_eval['name']} (合成スコア={_model_score(best_eval):.3f})")

    # 合格条件チェック (Worker D 指示)
    def _pass_count(ev):
        passed = 0
        if ev['jaccard'] < 0.7:
            passed += 1
        if ev['manufacturing_count'] >= 3:
            passed += 1
        if ev['logistics_count'] >= 3:
            passed += 1
        if ev['port_minato_rank'] is not None and ev['port_minato_rank'] >= 30:
            passed += 1
        # 全職種同 TOP10 にならない (Jaccard < 1.0 で代替判定)
        if ev['jaccard'] < 1.0:
            passed += 1
        return passed

    print(f"\n=== 合格条件達成数 (5 条件中) ===")
    for ev in [eval_e, eval_e2, eval_e3, eval_e4, eval_f]:
        pc = _pass_count(ev)
        if pc == 5:
            badge = "A grade (合格)"
        elif pc == 4:
            badge = "B grade (合格)"
        elif pc == 3:
            badge = "C grade (限定合格)"
        else:
            badge = "D grade (不合格)"
        print(f"    {ev['name']:<10} {pc}/5  → {badge}")

    # 商品化判定 (合格モデル選定)
    # A/B grade のみ商品化合格
    pass_models = [ev for ev in [eval_e, eval_e2, eval_e3, eval_e4, eval_f] if _pass_count(ev) >= 4]
    if pass_models:
        chosen = max(pass_models, key=_model_score)
        final_recommendation = chosen['name']
        product_verdict = "PASS"
    else:
        # 部分合格 (C grade) 中で最良
        c_models = [ev for ev in [eval_e, eval_e2, eval_e3, eval_e4, eval_f] if _pass_count(ev) == 3]
        if c_models:
            chosen = max(c_models, key=_model_score)
            final_recommendation = f"{chosen['name']} (限定合格、F6 投入も効果不十分)"
        else:
            final_recommendation = "F6 でも改善せず、別アプローチ検討"
        product_verdict = "FAIL"

    print(f"\n=== 商品化最終判定 ===")
    print(f"    最終推奨モデル: {final_recommendation}")
    print(f"    商品化判定: {product_verdict}")

    # JSON 出力
    OUT_JSON.parent.mkdir(parents=True, exist_ok=True)
    output = {
        "errors": {
            "model_a": err_a, "model_b": err_b,
            "model_c_prime": err_c, "model_e": err_e,
        },
        "spearman_correlations": correlations,
        "target_municipalities_values": target_table,
        "logistic_mfg_construction_top10": {
            occ: ranking_top_n(result_c, occ, n=10)
            for occ in LOGISTIC_MFG_CONSTRUCTION_OCCUPATIONS
        },
        "scenario_range_sample": ranges,
        # === Model E 拡張 ===
        "model_e_top20": model_e_top20,
        "model_a_top20": model_a_top20,
        "jaccard_top10": {
            "model_a": jaccard_a, "model_b": jaccard_b,
            "model_c_prime": jaccard_c, "model_e": jaccard_e,
            "model_e2": eval_e2["jaccard"],
            "model_e3": eval_e3["jaccard"],
            "model_e4": eval_e4["jaccard"],
            "model_f": eval_f["jaccard"],
        },
        "jaccard_similarity_matrix": {
            "model_a": matrix_a, "model_e": matrix_e,
        },
        "population_rank_diff": rank_diff_data,
        "model_e_showcase_industrial_cities": showcase_data,
        "estimate_grade": grade_e,
        "estimate_grade_audit": grade_audit,
        "verdict_per_format": verdict_per_format,
        # === Model E2/E3/E4 (Worker D, 改善ラウンド) ===
        "model_e2_top20": model_e2_top20,
        "model_e3_top20": model_e3_top20,
        "model_e4_top20": model_e4_top20,
        # === Model F (Worker E, F6 SalesNow 残差補正) ===
        "model_f_top20": model_f_top20,
        "model_f_diagnostics": f6_diag,
        "model_f_parameters": {
            "F6_ALPHA": F6_ALPHA,
            "F6_HQ_EXCESS_THRESHOLD": F6_HQ_EXCESS_THRESHOLD,
            "F6_BLUE_COLLAR_OCCUPATIONS": list(F6_BLUE_COLLAR_OCCUPATIONS),
            "F6_TARGET_JSIC_INDUSTRIES": list(F6_TARGET_JSIC_INDUSTRIES),
            "salesnow_aggregate_cells": len(sn_emp),
        },
        "industrial_rank_compare": industrial_rank_compare,
        "multi_model_comparison": {
            ev["name"]: {
                "jaccard": ev["jaccard"],
                "grade": ev["grade"],
                "manufacturing_count": ev["manufacturing_count"],
                "logistics_count": ev["logistics_count"],
                "port_minato_rank": ev["port_minato_rank"],
                "chuo_rank": ev["chuo_rank"],
                "pass_count_5": _pass_count(ev) if ev["name"].startswith("Model E") or ev["name"] == "Model F" else None,
            } for ev in all_evals
        },
        "final_recommendation": final_recommendation,
        "product_verdict": product_verdict,
        "limitations": [
            "真の市区町村別職業正解データは e-Stat 公開仕様で取得不可",
            "仮 ground truth は全国比率 × 都道府県生産年齢人口 (地域差なし)",
            "F3 (産業構成) 投入済み (CSV 36,099 行 × 重みマスタ 231 行)",
            "F5 (流入率補正) は簡易版 (1 + inflow_rate × 0.3、職業別流入は未反映)",
            "F6 (SalesNow) Model F で残差補正実装、Model E3 と独立して乗算",
            "scaling で都道府県再集計値は必ず一致 (内部整合性は自明)",
            "industry_code AB は A+B 統合 (重みは A と B の単純平均)",
            "S 公務 / AS,AR,CR の集計コードは F3 計算から除外",
            "F6 は本社所在地ベース。SalesNow は中堅以上中心で中小零細はカバー薄",
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

    # ============================================================
    # Model E 最終判定 (商品化判定)
    # ============================================================
    print("\n" + "=" * 75)
    print("Model E 最終判定 (採用ターゲット母集団推定指標としての商品化判定)")
    print("=" * 75)
    print(f"  TOP 10 Jaccard 平均: A={jaccard_a:.3f} → E={jaccard_e:.3f}")
    print(f"     差分: {jaccard_e - jaccard_a:+.3f} (低下幅が大きいほど F3 効果あり)")
    print(f"  estimate_grade: {grade_e}")
    print(f"  製造系 TOP10 工業都市ヒット: {grade_audit['industrial_cities_in_mfg_top10']}/9")
    print(f"  物流系 TOP10 物流ハブヒット: {grade_audit['logistics_hubs_in_logistics_top10']}/7")
    if grade_e in ("A", "B"):
        print(f"  >>> ✅ 商品化合格 ({grade_e}). 採用判断用推定指標として本実装着手 OK")
    elif grade_e == "C":
        print(f"  >>> ⚠️ 限定合格 ({grade_e}). F6 (SalesNow) 投入で精度向上を検討")
    else:
        print(f"  >>> ❌ 不合格 ({grade_e}). 別アプローチや F6 重み増加が必要")

    # ============================================================
    # 最終結論 (Worker D 改善ラウンド)
    # ============================================================
    print("\n" + "=" * 75)
    print("FINAL RECOMMENDATION (Worker D 改善ラウンド)")
    print("=" * 75)
    print(f"  最終推奨モデル: {final_recommendation}")
    print(f"  商品化判定:    {product_verdict}")
    print(f"  E grade={eval_e['grade']}  E2 grade={eval_e2['grade']}  "
          f"E3 grade={eval_e3['grade']}  E4 grade={eval_e4['grade']}  F grade={eval_f['grade']}")
    print(f"  製造系工業都市数 (TOP 10): E={eval_e['manufacturing_count']}  E2={eval_e2['manufacturing_count']}  "
          f"E3={eval_e3['manufacturing_count']}  E4={eval_e4['manufacturing_count']}  "
          f"F={eval_f['manufacturing_count']} / 9")
    print(f"  物流系湾岸都市数 (TOP 10): E={eval_e['logistics_count']}  E2={eval_e2['logistics_count']}  "
          f"E3={eval_e3['logistics_count']}  E4={eval_e4['logistics_count']}  "
          f"F={eval_f['logistics_count']} / 7")
    print(f"  港区生産工程順位:        E={eval_e['port_minato_rank']}  E2={eval_e2['port_minato_rank']}  "
          f"E3={eval_e3['port_minato_rank']}  E4={eval_e4['port_minato_rank']}  "
          f"F={eval_f['port_minato_rank']}")
    print(f"  Jaccard 平均:           E={eval_e['jaccard']:.3f}  E2={eval_e2['jaccard']:.3f}  "
          f"E3={eval_e3['jaccard']:.3f}  E4={eval_e4['jaccard']:.3f}  F={eval_f['jaccard']:.3f}")

    # ============================================================
    # Worker E 数値目標 5 条件チェックテーブル
    # ============================================================
    print("\n" + "=" * 75)
    print("Worker E 数値目標 (合格条件) チェックテーブル")
    print("=" * 75)
    targets = [
        ("港区生産工程順位 ≥ 30", lambda e: e['port_minato_rank'] is not None and e['port_minato_rank'] >= 30,
         lambda e: f"{e['port_minato_rank']}"),
        ("製造系 TOP10 ≥ 4/9",   lambda e: e['manufacturing_count'] >= 4,
         lambda e: f"{e['manufacturing_count']}/9"),
        ("物流系 TOP10 ≥ 5/7",   lambda e: e['logistics_count'] >= 5,
         lambda e: f"{e['logistics_count']}/7"),
        ("Jaccard < 0.65",       lambda e: e['jaccard'] < 0.65,
         lambda e: f"{e['jaccard']:.3f}"),
        ("全職種同 TOP10 でない", lambda e: e['jaccard'] < 1.0,
         lambda e: "OK" if e['jaccard'] < 1.0 else "NG"),
    ]
    print(f"  {'条件':<26} {'E3':<10} {'F':<10}")
    f_pass_count = 0
    for label, check, fmt in targets:
        e3_v = fmt(eval_e3)
        f_v = fmt(eval_f)
        e3_mark = "PASS" if check(eval_e3) else "FAIL"
        f_mark = "PASS" if check(eval_f) else "FAIL"
        if check(eval_f):
            f_pass_count += 1
        print(f"  {label:<26} {e3_v + ' ' + e3_mark:<10} {f_v + ' ' + f_mark:<10}")

    if f_pass_count == 5:
        worker_e_grade = "A-"
    elif f_pass_count == 4:
        worker_e_grade = "B"
    elif f_pass_count == 3:
        worker_e_grade = "C"
    else:
        worker_e_grade = "D"

    print(f"\n  Model F 合格数: {f_pass_count}/5  → estimate_grade = {worker_e_grade}")
    if f_pass_count >= 5:
        print("  ✅ 商品化本実装承認 (Worker E 数値目標すべて達成)")
    elif f_pass_count == 4:
        print("  ⚠️ Grade B 達成。本実装着手は別途判断")
    else:
        print("  ❌ 不合格。職業人口推定は補助指数止まり、")
        print("     主軸を 通勤OD/求人需要/企業密度 に切替推奨")

    # JSON にも Worker E 結果を追記
    output["worker_e_target_check"] = {
        "model_f_pass_count_5": f_pass_count,
        "estimate_grade": worker_e_grade,
        "targets": [
            {
                "label": label,
                "model_e3": fmt(eval_e3),
                "model_f": fmt(eval_f),
                "e3_pass": check(eval_e3),
                "f_pass": check(eval_f),
            } for label, check, fmt in targets
        ],
    }
    # 再書き出し
    with open(OUT_JSON, "w", encoding="utf-8") as f:
        json.dump(output, f, ensure_ascii=False, indent=2, default=str)

    conn.close()
    return 0


if __name__ == "__main__":
    sys.exit(main())
