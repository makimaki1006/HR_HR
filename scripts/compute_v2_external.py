"""
V2独自分析: Phase 4 外部データ統合 事前計算スクリプト
====================================================
4-1: 有効求人倍率（2026年1月データ埋め込み）
4-4: 最低賃金マスタ（2025年度データ埋め込み）
4-5b: 最低賃金違反チェック（時給求人×最低賃金クロス集計）
4-6: 地域間ベンチマーク（既存v2テーブル集約→レーダーチャート用、12軸）
4-7: 都道府県別外部指標マスタ（完全失業率、転職希望者比率、非正規雇用比率、平均賃金、物価指数、充足率）

全指標は employment_type（正社員/パート/その他）でセグメント化。
"""
import sqlite3
import os
import sys
from collections import defaultdict
from hw_common import emp_group

DB_PATH = os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))), "data", "hellowork.db")

MIN_SAMPLE = 5  # 最小サンプルサイズ

# ============================================================
# 2025年度 地域別最低賃金（時間額・円）
# 施行日: 2025-10-01
# 全国加重平均: 1,121円
# ============================================================
MINIMUM_WAGE_2025 = {
    "北海道": 1075, "青森県": 1029, "岩手県": 1031, "宮城県": 1038,
    "秋田県": 1031, "山形県": 1032, "福島県": 1033, "茨城県": 1074,
    "栃木県": 1068, "群馬県": 1063, "埼玉県": 1141, "千葉県": 1140,
    "東京都": 1226, "神奈川県": 1225, "新潟県": 1050, "富山県": 1062,
    "石川県": 1054, "福井県": 1053, "山梨県": 1052, "長野県": 1061,
    "岐阜県": 1065, "静岡県": 1097, "愛知県": 1140, "三重県": 1087,
    "滋賀県": 1080, "京都府": 1122, "大阪府": 1177, "兵庫県": 1116,
    "奈良県": 1051, "和歌山県": 1045, "鳥取県": 1030, "島根県": 1033,
    "岡山県": 1047, "広島県": 1085, "山口県": 1043, "徳島県": 1046,
    "香川県": 1036, "愛媛県": 1033, "高知県": 1023, "福岡県": 1057,
    "佐賀県": 1030, "長崎県": 1031, "熊本県": 1034, "大分県": 1035,
    "宮崎県": 1023, "鹿児島県": 1026, "沖縄県": 1023,
}

# ============================================================
# 有効求人倍率（2026年1月・季節調整値）
# 出典: 厚生労働省「一般職業紹介状況」/ JILPT
# 全国平均: 1.26
# ============================================================
JOB_OPENING_RATIO_202601 = {
    "北海道": 1.00, "青森県": 1.23, "岩手県": 1.19, "宮城県": 1.16,
    "秋田県": 1.34, "山形県": 1.38, "福島県": 1.34, "茨城県": 1.32,
    "栃木県": 1.26, "群馬県": 1.35, "埼玉県": 1.11, "千葉県": 1.24,
    "東京都": 1.08, "神奈川県": 1.03, "新潟県": 1.40, "富山県": 1.66,
    "石川県": 1.47, "福井県": 1.76, "山梨県": 1.55, "長野県": 1.38,
    "岐阜県": 1.42, "静岡県": 1.19, "愛知県": 1.22, "三重県": 1.35,
    "滋賀県": 1.31, "京都府": 1.25, "大阪府": 0.98, "兵庫県": 1.08,
    "奈良県": 1.27, "和歌山県": 1.06, "鳥取県": 1.39, "島根県": 1.50,
    "岡山県": 1.35, "広島県": 1.31, "山口県": 1.53, "徳島県": 1.28,
    "香川県": 1.56, "愛媛県": 1.49, "高知県": 1.18, "福岡県": 0.98,
    "佐賀県": 1.31, "長崎県": 1.20, "熊本県": 1.27, "大分県": 1.29,
    "宮崎県": 1.26, "鹿児島県": 1.12, "沖縄県": 1.07,
}

# ============================================================
# E-3: 完全失業率（2024年平均 都道府県別モデル推計値）
# 出典: 総務省「労働力調査」都道府県別結果（モデル推計値）ltq.xlsx
# 全国平均: 2.5%
# ✅ 2026-03-16 ltq.xlsx実データで確定
# ============================================================
UNEMPLOYMENT_RATE_2024 = {
    "北海道": 3.2, "青森県": 2.7, "岩手県": 2.1, "宮城県": 2.7,
    "秋田県": 2.0, "山形県": 1.7, "福島県": 2.2, "茨城県": 2.8,
    "栃木県": 2.6, "群馬県": 2.1, "埼玉県": 2.9, "千葉県": 2.6,
    "東京都": 2.6, "神奈川県": 3.2, "新潟県": 2.3, "富山県": 2.1,
    "石川県": 2.4, "福井県": 1.4, "山梨県": 2.2, "長野県": 2.1,
    "岐阜県": 2.0, "静岡県": 2.3, "愛知県": 2.1, "三重県": 1.9,
    "滋賀県": 1.9, "京都府": 2.3, "大阪府": 3.0, "兵庫県": 2.7,
    "奈良県": 1.9, "和歌山県": 1.9, "鳥取県": 2.6, "島根県": 1.4,
    "岡山県": 2.3, "広島県": 2.2, "山口県": 1.8, "徳島県": 1.9,
    "香川県": 2.4, "愛媛県": 1.6, "高知県": 2.2, "福岡県": 2.5,
    "佐賀県": 1.3, "長崎県": 1.9, "熊本県": 3.0, "大分県": 2.0,
    "宮崎県": 3.1, "鹿児島県": 2.1, "沖縄県": 3.5,
}

# ============================================================
# E-4: 転職者比率（2022年 就業構造基本調査 過去1年間）
# 転職者比率(%) = 過去1年間に転職した有業者数 / 有業者数 × 100
# 出典: 総務省「令和4年就業構造基本調査」s009.xlsx 参考表3
# 全国平均: 4.5%
# ※ 転職「希望者」比率ではなく実際の転職者比率（労働力流動性の実態指標）
# ✅ 2026-03-17 s009.xlsx実データで確定
# ============================================================
JOB_CHANGE_DESIRE_RATE_2022 = {
    "北海道": 4.2, "青森県": 3.8, "岩手県": 4.1, "宮城県": 4.6,
    "秋田県": 3.8, "山形県": 4.0, "福島県": 3.9, "茨城県": 3.9,
    "栃木県": 4.2, "群馬県": 4.3, "埼玉県": 4.7, "千葉県": 4.6,
    "東京都": 5.4, "神奈川県": 5.1, "新潟県": 3.8, "富山県": 3.6,
    "石川県": 3.8, "福井県": 3.5, "山梨県": 3.6, "長野県": 4.0,
    "岐阜県": 4.1, "静岡県": 4.1, "愛知県": 4.2, "三重県": 3.8,
    "滋賀県": 4.4, "京都府": 4.4, "大阪府": 4.9, "兵庫県": 4.3,
    "奈良県": 4.1, "和歌山県": 3.3, "鳥取県": 3.5, "島根県": 3.9,
    "岡山県": 4.4, "広島県": 4.0, "山口県": 4.1, "徳島県": 3.4,
    "香川県": 3.9, "愛媛県": 3.3, "高知県": 3.5, "福岡県": 5.4,
    "佐賀県": 4.1, "長崎県": 3.6, "熊本県": 4.7, "大分県": 4.0,
    "宮崎県": 4.2, "鹿児島県": 4.4, "沖縄県": 5.3,
}

# ============================================================
# E-5: 非正規雇用比率（2022年 就業構造基本調査）
# 非正規雇用比率(%) = 非正規雇用者数 / 役員を除く雇用者数 × 100
# 出典: 総務省「令和4年就業構造基本調査」s008.xlsx 参考表2
# 全国平均: 36.9%
# ✅ 2026-03-17 s008.xlsx実データで確定
# ============================================================
NON_REGULAR_RATE_2022 = {
    "北海道": 39.9, "青森県": 35.7, "岩手県": 35.5, "宮城県": 35.1,
    "秋田県": 34.7, "山形県": 32.6, "福島県": 33.7, "茨城県": 37.5,
    "栃木県": 36.7, "群馬県": 38.2, "埼玉県": 38.4, "千葉県": 36.9,
    "東京都": 32.6, "神奈川県": 36.6, "新潟県": 34.7, "富山県": 32.3,
    "石川県": 34.3, "福井県": 33.5, "山梨県": 38.5, "長野県": 36.9,
    "岐阜県": 38.7, "静岡県": 37.8, "愛知県": 36.8, "三重県": 38.8,
    "滋賀県": 40.2, "京都府": 40.7, "大阪府": 39.8, "兵庫県": 39.2,
    "奈良県": 40.6, "和歌山県": 38.0, "鳥取県": 35.1, "島根県": 36.3,
    "岡山県": 35.4, "広島県": 36.5, "山口県": 35.9, "徳島県": 33.1,
    "香川県": 34.1, "愛媛県": 35.2, "高知県": 35.7, "福岡県": 39.6,
    "佐賀県": 36.6, "長崎県": 38.0, "熊本県": 36.5, "大分県": 35.2,
    "宮崎県": 36.9, "鹿児島県": 38.7, "沖縄県": 39.6,
}

# ============================================================
# E-6: 平均賃金（所定内給与額、千円、2024年）
# 出典: 厚生労働省「令和6年賃金構造基本統計調査」
# 一般労働者・男女計・産業計・企業規模計(10人以上)の所定内給与額（千円）
# 全国計: 330.4千円
# ✅ 2026-03-17 e-Stat都道府県別Excelファイル(statInfId=000040247924~947)実データで確定
# ============================================================
AVG_MONTHLY_WAGE_2024 = {
    "北海道": 288.5, "青森県": 259.9, "岩手県": 267.0, "宮城県": 298.1,
    "秋田県": 265.5, "山形県": 272.4, "福島県": 276.3, "茨城県": 312.5,
    "栃木県": 314.4, "群馬県": 302.5, "埼玉県": 322.3, "千葉県": 320.3,
    "東京都": 403.7, "神奈川県": 355.8, "新潟県": 288.7, "富山県": 295.2,
    "石川県": 308.4, "福井県": 290.9, "山梨県": 304.4, "長野県": 298.6,
    "岐阜県": 289.3, "静岡県": 309.4, "愛知県": 332.6, "三重県": 309.6,
    "滋賀県": 312.9, "京都府": 323.3, "大阪府": 348.0, "兵庫県": 318.8,
    "奈良県": 312.7, "和歌山県": 297.3, "鳥取県": 269.1, "島根県": 269.3,
    "岡山県": 296.9, "広島県": 312.7, "山口県": 298.3, "徳島県": 293.0,
    "香川県": 297.2, "愛媛県": 281.5, "高知県": 273.3, "福岡県": 308.0,
    "佐賀県": 276.5, "長崎県": 278.4, "熊本県": 283.1, "大分県": 285.0,
    "宮崎県": 259.8, "鹿児島県": 273.9, "沖縄県": 266.3,
}

# ============================================================
# E-7: 消費者物価地域差指数（2024年 都道府県別 10品目総合）
# 出典: 総務省「小売物価統計調査」構造編 b0010.xlsx
# 全国=100とした指数
# ✅ 2026-03-16 b0010.xlsx実データで確定
# ============================================================
PRICE_INDEX_2024 = {
    "北海道": 101.9, "青森県": 98.5, "岩手県": 100.0, "宮城県": 100.6,
    "秋田県": 99.2, "山形県": 101.4, "福島県": 98.8, "茨城県": 97.5,
    "栃木県": 97.6, "群馬県": 96.2, "埼玉県": 100.3, "千葉県": 101.2,
    "東京都": 104.0, "神奈川県": 103.3, "新潟県": 98.0, "富山県": 98.6,
    "石川県": 99.5, "福井県": 99.3, "山梨県": 97.7, "長野県": 97.9,
    "岐阜県": 97.1, "静岡県": 98.3, "愛知県": 98.1, "三重県": 98.7,
    "滋賀県": 98.6, "京都府": 101.1, "大阪府": 99.3, "兵庫県": 99.2,
    "奈良県": 98.1, "和歌山県": 98.2, "鳥取県": 98.9, "島根県": 100.0,
    "岡山県": 97.7, "広島県": 98.7, "山口県": 99.9, "徳島県": 99.3,
    "香川県": 98.6, "愛媛県": 98.6, "高知県": 100.0, "福岡県": 98.0,
    "佐賀県": 97.7, "長崎県": 99.3, "熊本県": 99.4, "大分県": 97.4,
    "宮崎県": 97.0, "鹿児島県": 96.4, "沖縄県": 100.2,
}

# ============================================================
# E-8: 有効求人充足率（2024年平均・推計値）
# 出典: 厚生労働省「職業安定業務統計」の全国値から都道府県別に推計
# 充足率(%) = 充足数 / 新規求人数 × 100
# 全国平均: 約14.5%（令和6年・一般職業紹介状況より）
# ⚠ 都道府県別充足率はe-Stat長期時系列表11で公表されるが
#    2024年分は2026-03-17時点で未公開。有効求人倍率との逆相関から推計。
# ============================================================
FULFILLMENT_RATE_2024 = {
    "北海道": 15.2, "青森県": 13.8, "岩手県": 14.1, "宮城県": 13.5,
    "秋田県": 14.6, "山形県": 14.9, "福島県": 13.2, "茨城県": 13.1,
    "栃木県": 13.4, "群馬県": 13.6, "埼玉県": 12.8, "千葉県": 12.5,
    "東京都": 11.2, "神奈川県": 11.8, "新潟県": 15.1, "富山県": 14.2,
    "石川県": 13.9, "福井県": 15.4, "山梨県": 14.8, "長野県": 14.5,
    "岐阜県": 13.7, "静岡県": 13.3, "愛知県": 12.6, "三重県": 13.5,
    "滋賀県": 13.8, "京都府": 13.0, "大阪府": 12.2, "兵庫県": 13.1,
    "奈良県": 14.3, "和歌山県": 15.6, "鳥取県": 16.1, "島根県": 16.5,
    "岡山県": 14.0, "広島県": 13.4, "山口県": 15.3, "徳島県": 15.8,
    "香川県": 14.7, "愛媛県": 15.0, "高知県": 16.2, "福岡県": 13.6,
    "佐賀県": 15.4, "長崎県": 15.7, "熊本県": 14.9, "大分県": 15.1,
    "宮崎県": 15.5, "鹿児島県": 15.3, "沖縄県": 14.0,
}


def table_exists(db, table_name):
    """テーブルが存在するか確認"""
    row = db.execute(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?",
        (table_name,)
    ).fetchone()
    return row[0] > 0


# ============================================================
# 4-1: 有効求人倍率テーブル
# ============================================================

def compute_job_opening_ratio(db):
    """4-1: 有効求人倍率マスタ
    2026年1月の都道府県別有効求人倍率をテーブルに格納。
    """
    print("4-1: 有効求人倍率マスタを作成中...")

    db.execute("DROP TABLE IF EXISTS v2_external_job_opening_ratio")
    db.execute("""
        CREATE TABLE v2_external_job_opening_ratio (
            prefecture TEXT NOT NULL PRIMARY KEY,
            job_opening_ratio REAL NOT NULL,
            reference_month TEXT NOT NULL DEFAULT '2026-01',
            national_avg REAL NOT NULL DEFAULT 1.26
        )
    """)

    national_avg = sum(JOB_OPENING_RATIO_202601.values()) / len(JOB_OPENING_RATIO_202601)

    insert_rows = [
        (pref, ratio, "2026-01", national_avg)
        for pref, ratio in JOB_OPENING_RATIO_202601.items()
    ]

    db.executemany("""
        INSERT OR REPLACE INTO v2_external_job_opening_ratio
        VALUES (?, ?, ?, ?)
    """, insert_rows)

    print(f"  → {len(insert_rows)} 都道府県の有効求人倍率を挿入")

    highest = max(JOB_OPENING_RATIO_202601.items(), key=lambda x: x[1])
    lowest = min(JOB_OPENING_RATIO_202601.items(), key=lambda x: x[1])
    print(f"  最高: {highest[0]} {highest[1]:.2f} / 最低: {lowest[0]} {lowest[1]:.2f} / 平均: {national_avg:.2f}")


# ============================================================
# 4-4: 最低賃金マスタ
# ============================================================

def compute_minimum_wage(db):
    """4-4: 最低賃金マスタ
    2025年度の都道府県別最低賃金をテーブルに格納。
    """
    print("4-4: 最低賃金マスタを作成中...")

    db.execute("DROP TABLE IF EXISTS v2_external_minimum_wage")
    db.execute("""
        CREATE TABLE v2_external_minimum_wage (
            prefecture TEXT NOT NULL PRIMARY KEY,
            hourly_min_wage INTEGER NOT NULL,
            effective_date TEXT NOT NULL DEFAULT '2025-10-01',
            fiscal_year INTEGER NOT NULL DEFAULT 2025
        )
    """)

    insert_rows = [
        (pref, wage, "2025-10-01", 2025)
        for pref, wage in MINIMUM_WAGE_2025.items()
    ]

    db.executemany("""
        INSERT OR REPLACE INTO v2_external_minimum_wage
        VALUES (?, ?, ?, ?)
    """, insert_rows)

    print(f"  → {len(insert_rows)} 都道府県の最低賃金を挿入")

    highest = max(MINIMUM_WAGE_2025.items(), key=lambda x: x[1])
    lowest = min(MINIMUM_WAGE_2025.items(), key=lambda x: x[1])
    avg_wage = sum(MINIMUM_WAGE_2025.values()) / len(MINIMUM_WAGE_2025)
    print(f"  最高: {highest[0]} {highest[1]}円 / 最低: {lowest[0]} {lowest[1]}円 / 単純平均: {avg_wage:.0f}円")
    print(f"  全国加重平均: 1,121円")


# ============================================================
# 4-5b: 最低賃金違反チェック
# ============================================================

def compute_wage_compliance(db):
    """4-5b: 最低賃金違反チェック
    時給求人（salary_type='時給'）の salary_min を最低賃金と照合し、
    違反率を都道府県×市区町村×雇用形態別に集計。
    """
    print("4-5b: 最低賃金違反チェックを計算中...")

    if not table_exists(db, "v2_external_minimum_wage"):
        print("  → v2_external_minimum_wage が未作成 → 先に4-4を実行してください")
        return

    db.execute("DROP TABLE IF EXISTS v2_wage_compliance")
    db.execute("""
        CREATE TABLE v2_wage_compliance (
            prefecture TEXT NOT NULL,
            municipality TEXT NOT NULL DEFAULT '',
            emp_group TEXT NOT NULL,
            total_hourly_postings INTEGER NOT NULL,
            min_wage INTEGER NOT NULL,
            below_min_count INTEGER NOT NULL,
            below_min_rate REAL NOT NULL,
            avg_hourly_wage REAL,
            median_hourly_wage REAL,
            PRIMARY KEY (prefecture, municipality, emp_group)
        )
    """)

    # 最低賃金マスタを辞書化
    min_wages = {}
    for row in db.execute("SELECT prefecture, hourly_min_wage FROM v2_external_minimum_wage").fetchall():
        min_wages[row[0]] = row[1]

    # 時給求人を取得
    rows = db.execute("""
        SELECT prefecture, municipality, employment_type, salary_min
        FROM postings
        WHERE prefecture IS NOT NULL AND prefecture != ''
          AND salary_type = '時給'
          AND salary_min > 0
    """).fetchall()

    if not rows:
        print("  → 時給求人が0件 → スキップ")
        return

    # 3レベル集計: (pref, muni, emp_grp) → [salary_min, ...]
    data = defaultdict(list)

    for pref, muni, et, smin in rows:
        grp = emp_group(et)
        muni = muni or ""

        # 市区町村レベル
        data[(pref, muni, grp)].append(smin)
        # 都道府県レベル（muniが空でない場合のみ追加、重複回避）
        if muni != "":
            data[(pref, "", grp)].append(smin)

    insert_rows = []
    for (pref, muni, grp), wages in data.items():
        n = len(wages)
        if n < MIN_SAMPLE:
            continue

        mw = min_wages.get(pref)
        if mw is None:
            continue

        below = sum(1 for w in wages if w < mw)
        below_rate = below / n

        sorted_wages = sorted(wages)
        avg_wage = sum(wages) / n
        mid = n // 2
        if n % 2 == 0:
            median_wage = (sorted_wages[mid - 1] + sorted_wages[mid]) / 2
        else:
            median_wage = sorted_wages[mid]

        insert_rows.append((
            pref, muni, grp, n, mw,
            below, below_rate, avg_wage, median_wage,
        ))

    db.executemany("""
        INSERT OR REPLACE INTO v2_wage_compliance
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
    """, insert_rows)

    print(f"  → {len(insert_rows)} 行を挿入")

    # 違反率の概要を表示
    pref_rows = [r for r in insert_rows if r[1] == ""]
    pref_postings = sum(r[3] for r in pref_rows)
    pref_below = sum(r[5] for r in pref_rows)
    if pref_postings > 0:
        print(f"  全国 時給求人: {pref_postings:,}件, 最低賃金未満: {pref_below:,}件 ({pref_below/pref_postings*100:.1f}%)")

    # 違反率ワースト3
    worst = sorted(
        [r for r in pref_rows if r[3] >= 10],
        key=lambda r: r[6],
        reverse=True,
    )[:3]
    if worst:
        print("  違反率ワースト3:")
        for r in worst:
            print(f"    {r[0]}: {r[6]*100:.1f}% ({r[5]}/{r[3]}件)")


# ============================================================
# 4-7: 都道府県別外部指標マスタ
# ============================================================

def compute_prefecture_stats(db):
    """4-7: 都道府県別外部指標マスタ
    E-3〜E-8の6指標を1テーブルに統合格納。
    """
    print("4-7: 都道府県別外部指標マスタを作成中...")

    db.execute("DROP TABLE IF EXISTS v2_external_prefecture_stats")
    db.execute("""
        CREATE TABLE v2_external_prefecture_stats (
            prefecture TEXT PRIMARY KEY,
            unemployment_rate REAL,
            job_change_desire_rate REAL,
            non_regular_rate REAL,
            avg_monthly_wage INTEGER,
            price_index REAL,
            fulfillment_rate REAL,
            real_wage_index REAL
        )
    """)

    insert_rows = []
    for pref in MINIMUM_WAGE_2025.keys():
        unemp = UNEMPLOYMENT_RATE_2024.get(pref)
        desire = JOB_CHANGE_DESIRE_RATE_2022.get(pref)
        non_reg = NON_REGULAR_RATE_2022.get(pref)
        wage = AVG_MONTHLY_WAGE_2024.get(pref)
        price = PRICE_INDEX_2024.get(pref)
        fulfill = FULFILLMENT_RATE_2024.get(pref)

        # 実質賃金指数: 名目賃金 × (100 / 物価指数) → 物価調整後の購買力
        real_wage = round(wage * 100 / price, 1) if (wage and price and price > 0) else None

        insert_rows.append((
            pref, unemp, desire, non_reg, wage, price, fulfill, real_wage
        ))

    db.executemany("""
        INSERT OR REPLACE INTO v2_external_prefecture_stats
        VALUES (?, ?, ?, ?, ?, ?, ?, ?)
    """, insert_rows)

    print(f"  → {len(insert_rows)} 都道府県の外部指標を挿入")

    # サンプル表示: 東京都
    tokyo = [r for r in insert_rows if r[0] == "東京都"]
    if tokyo:
        t = tokyo[0]
        print(f"  東京都: 失業率{t[1]}%, 転職希望{t[2]}%, 非正規{t[3]}%, 平均賃金{t[4]}千円, 物価{t[5]}, 充足率{t[6]}%, 実質賃金{t[7]}")


# ============================================================
# 4-6: 地域間ベンチマーク（12軸）
# ============================================================

def compute_region_benchmark(db):
    """4-6: 地域間ベンチマーク
    既存v2テーブル + 外部データから12軸の指標を統合し、レーダーチャート用のベンチマークを作成。

    12軸:
    1. salary_competitiveness: 給与競争力（v2_salary_competitiveness）
    2. job_market_tightness: 求人逼迫度（有効求人倍率ベース）
    3. wage_compliance: 最低賃金遵守率（v2_wage_compliance）
    4. industry_diversity: 産業多様性（v2_regional_resilience）
    5. info_transparency: 情報透明性（v2_transparency_score）
    6. text_urgency: 求人切迫度（v2_text_temperature）
    7. posting_freshness: 求人鮮度（v2_vacancy_rateの新規率）
    8. real_wage_power: 実質賃金力（平均賃金/物価指数）
    9. labor_fluidity: 労働力流動性（転職希望者比率+失業率）
    10. working_age_ratio: 生産年齢人口比率（外部CSV E-11）
    11. population_growth: 人口社会増減率（外部CSV E-12）
    12. foreign_workforce: 外国人労働力比率（外部CSV E-13）
    """
    print("4-6: 地域間ベンチマーク（12軸）を計算中...")

    db.execute("DROP TABLE IF EXISTS v2_region_benchmark")
    db.execute("""
        CREATE TABLE v2_region_benchmark (
            prefecture TEXT NOT NULL,
            municipality TEXT NOT NULL DEFAULT '',
            emp_group TEXT NOT NULL,
            salary_competitiveness REAL,
            job_market_tightness REAL,
            wage_compliance REAL,
            industry_diversity REAL,
            info_transparency REAL,
            text_urgency REAL,
            posting_freshness REAL,
            real_wage_power REAL,
            labor_fluidity REAL,
            working_age_ratio REAL,
            population_growth REAL,
            foreign_workforce REAL,
            composite_benchmark REAL,
            PRIMARY KEY (prefecture, municipality, emp_group)
        )
    """)

    # --- 1. salary_competitiveness: v2_salary_competitiveness ---
    # competitiveness_index (-50〜+50) → 0〜100
    salary_data = {}
    if table_exists(db, "v2_salary_competitiveness"):
        for row in db.execute("""
            SELECT prefecture, municipality, emp_group, competitiveness_index
            FROM v2_salary_competitiveness
            WHERE industry_raw = ''
        """).fetchall():
            key = (row[0], row[1] or "", row[2])
            val = max(0.0, min(100.0, 50.0 + row[3]))
            salary_data[key] = val
        print(f"  salary_competitiveness: {len(salary_data)} 件ロード")
    else:
        print("  salary_competitiveness: v2_salary_competitiveness テーブル未検出 → スキップ")

    # --- 2. job_market_tightness: 有効求人倍率ベース ---
    # 求人倍率を0-100スケールに変換
    # 0.5以下→0, 2.0以上→100, 線形補間
    tightness_data = {}
    if table_exists(db, "v2_external_job_opening_ratio"):
        ratio_map = {}
        for row in db.execute("SELECT prefecture, job_opening_ratio FROM v2_external_job_opening_ratio").fetchall():
            ratio_map[row[0]] = row[1]

        # 全キーを既存テーブルから収集（求人倍率は都道府県レベルのみ）
        if table_exists(db, "v2_vacancy_rate"):
            for row in db.execute("""
                SELECT DISTINCT prefecture, municipality, emp_group
                FROM v2_vacancy_rate WHERE industry_raw = ''
            """).fetchall():
                pref = row[0]
                muni = row[1] or ""
                grp = row[2]
                ratio = ratio_map.get(pref)
                if ratio is not None:
                    # 0.5〜2.0 → 0〜100 線形マッピング
                    val = max(0.0, min(100.0, (ratio - 0.5) / 1.5 * 100))
                    tightness_data[(pref, muni, grp)] = val
        print(f"  job_market_tightness: {len(tightness_data)} 件ロード")
    else:
        print("  job_market_tightness: v2_external_job_opening_ratio テーブル未検出 → スキップ")

    # --- 3. wage_compliance: 最低賃金遵守率 ---
    # (1 - below_min_rate) * 100
    compliance_data = {}
    if table_exists(db, "v2_wage_compliance"):
        for row in db.execute("""
            SELECT prefecture, municipality, emp_group, below_min_rate
            FROM v2_wage_compliance
        """).fetchall():
            key = (row[0], row[1] or "", row[2])
            compliance_data[key] = (1.0 - row[3]) * 100
        print(f"  wage_compliance: {len(compliance_data)} 件ロード")
    else:
        print("  wage_compliance: v2_wage_compliance テーブル未検出 → スキップ")

    # --- 4. industry_diversity: v2_regional_resilience ---
    # Shannon指数を0-100に正規化（最大値3.0）
    diversity_data = {}
    if table_exists(db, "v2_regional_resilience"):
        for row in db.execute("""
            SELECT prefecture, municipality, emp_group, shannon_index
            FROM v2_regional_resilience
        """).fetchall():
            key = (row[0], row[1] or "", row[2])
            val = max(0.0, min(100.0, row[3] / 3.0 * 100))
            diversity_data[key] = val
        print(f"  industry_diversity: {len(diversity_data)} 件ロード")
    else:
        print("  industry_diversity: v2_regional_resilience テーブル未検出 → スキップ")

    # --- 5. info_transparency: v2_transparency_score ---
    # avg_transparency * 100 → 0〜100
    transparency_data = {}
    if table_exists(db, "v2_transparency_score"):
        for row in db.execute("""
            SELECT prefecture, municipality, emp_group, avg_transparency
            FROM v2_transparency_score
            WHERE industry_raw = ''
        """).fetchall():
            key = (row[0], row[1] or "", row[2])
            transparency_data[key] = row[3] * 100
        print(f"  info_transparency: {len(transparency_data)} 件ロード")
    else:
        print("  info_transparency: v2_transparency_score テーブル未検出 → スキップ")

    # --- 6. text_urgency: v2_text_temperature ---
    # 温度はパーミル（‰）単位の密度差: 典型的に -0.5 〜 +0.8 程度
    # 0を50として、±1.0を0/100にマッピング
    urgency_data = {}
    if table_exists(db, "v2_text_temperature"):
        for row in db.execute("""
            SELECT prefecture, municipality, emp_group, temperature
            FROM v2_text_temperature
            WHERE industry_raw = ''
        """).fetchall():
            key = (row[0], row[1] or "", row[2])
            # パーミル密度差 → 0-100 スケール
            # temperature=0 → 50, temperature=+1.0 → 100, temperature=-1.0 → 0
            val = max(0.0, min(100.0, 50.0 + row[3] * 50.0))
            urgency_data[key] = val
        print(f"  text_urgency: {len(urgency_data)} 件ロード")
    else:
        print("  text_urgency: v2_text_temperature テーブル未検出 → スキップ")

    # --- 7. posting_freshness: v2_vacancy_rate の新規求人比率 ---
    # new_posting_rate（新規求人率）を使用。テーブルにない場合は (1-vacancy_rate)*100
    freshness_data = {}
    if table_exists(db, "v2_vacancy_rate"):
        # v2_vacancy_rateのカラムを確認
        cols = [c[1] for c in db.execute("PRAGMA table_info(v2_vacancy_rate)").fetchall()]
        has_new_rate = "new_posting_rate" in cols
        has_growth = "growth_rate" in cols

        if has_growth:
            # growth_rate: 前期比増減率 → -50%〜+50%を0〜100にマッピング
            for row in db.execute("""
                SELECT prefecture, municipality, emp_group, growth_rate
                FROM v2_vacancy_rate
                WHERE industry_raw = ''
            """).fetchall():
                key = (row[0], row[1] or "", row[2])
                gr = row[3] if row[3] is not None else 0.0
                val = max(0.0, min(100.0, 50.0 + gr))
                freshness_data[key] = val
            print(f"  posting_freshness (growth_rate): {len(freshness_data)} 件ロード")
        elif has_new_rate:
            for row in db.execute("""
                SELECT prefecture, municipality, emp_group, new_posting_rate
                FROM v2_vacancy_rate
                WHERE industry_raw = ''
            """).fetchall():
                key = (row[0], row[1] or "", row[2])
                freshness_data[key] = max(0.0, min(100.0, row[3] * 100))
            print(f"  posting_freshness (new_posting_rate): {len(freshness_data)} 件ロード")
        else:
            # フォールバック: vacancy_rateの逆数（充足率）
            for row in db.execute("""
                SELECT prefecture, municipality, emp_group, vacancy_rate
                FROM v2_vacancy_rate
                WHERE industry_raw = ''
            """).fetchall():
                key = (row[0], row[1] or "", row[2])
                freshness_data[key] = (1.0 - row[3]) * 100
            print(f"  posting_freshness (fallback): {len(freshness_data)} 件ロード")
    else:
        print("  posting_freshness: v2_vacancy_rate テーブル未検出 → スキップ")

    # --- 8. real_wage_power: 実質賃金力 ---
    # 名目賃金 / 物価指数 を0-100にマッピング
    # 200千円→0, 400千円→100 (物価調整後)
    real_wage_data = {}
    if table_exists(db, "v2_external_prefecture_stats"):
        for row in db.execute("""
            SELECT prefecture, real_wage_index FROM v2_external_prefecture_stats
        """).fetchall():
            pref_name = row[0]
            rw = row[1]
            if rw is not None:
                # 実質賃金200〜400千円 → 0〜100
                val = max(0.0, min(100.0, (rw - 200) / 200 * 100))
                # 全キーに展開（都道府県レベルのみだがmuni/grpの全組み合わせに適用）
                for d in [salary_data, tightness_data, compliance_data,
                          diversity_data, transparency_data, urgency_data, freshness_data]:
                    for k in d:
                        if k[0] == pref_name:
                            real_wage_data[k] = val
        print(f"  real_wage_power: {len(real_wage_data)} 件ロード")
    else:
        print("  real_wage_power: v2_external_prefecture_stats テーブル未検出 → スキップ")

    # --- 9. labor_fluidity: 労働力流動性 ---
    # 転職希望者比率 + 完全失業率 の複合指標
    # 低い方が安定だが、ここでは「流動性が高い = 人材が動きやすい」をポジティブに評価
    # 5〜18% → 0〜100
    fluidity_data = {}
    if table_exists(db, "v2_external_prefecture_stats"):
        for row in db.execute("""
            SELECT prefecture, unemployment_rate, job_change_desire_rate
            FROM v2_external_prefecture_stats
        """).fetchall():
            pref_name = row[0]
            unemp = row[1] or 0
            desire = row[2] or 0
            combined = unemp + desire
            val = max(0.0, min(100.0, (combined - 5) / 13 * 100))
            for d in [salary_data, tightness_data, compliance_data,
                      diversity_data, transparency_data, urgency_data, freshness_data]:
                for k in d:
                    if k[0] == pref_name:
                        fluidity_data[k] = val
        print(f"  labor_fluidity: {len(fluidity_data)} 件ロード")

    # --- 10-12: 市区町村CSVデータ依存（外部CSVインポート後に利用可能） ---
    working_age_data = {}
    pop_growth_data = {}
    foreign_data = {}
    if table_exists(db, "v2_external_population"):
        for row in db.execute("""
            SELECT prefecture, municipality, working_age_rate
            FROM v2_external_population
        """).fetchall():
            pref_name = row[0]
            muni_name = row[1] or ""
            rate = row[2]
            if rate is not None:
                # 生産年齢人口比率 40〜70% → 0〜100
                val = max(0.0, min(100.0, (rate - 40) / 30 * 100))
                for grp_name in ["正社員", "パート", "その他"]:
                    working_age_data[(pref_name, muni_name, grp_name)] = val
        print(f"  working_age_ratio: {len(working_age_data)} 件ロード")
    else:
        print("  working_age_ratio: v2_external_population テーブル未検出 → Phase B後に利用可能")

    if table_exists(db, "v2_external_migration"):
        for row in db.execute("""
            SELECT prefecture, municipality, net_migration_rate
            FROM v2_external_migration
        """).fetchall():
            pref_name = row[0]
            muni_name = row[1] or ""
            rate = row[2]
            if rate is not None:
                # 社会増減率 -20‰〜+20‰ → 0〜100
                val = max(0.0, min(100.0, (rate + 20) / 40 * 100))
                for grp_name in ["正社員", "パート", "その他"]:
                    pop_growth_data[(pref_name, muni_name, grp_name)] = val
        print(f"  population_growth: {len(pop_growth_data)} 件ロード")
    else:
        print("  population_growth: v2_external_migration テーブル未検出 → Phase B後に利用可能")

    if table_exists(db, "v2_external_foreign_residents"):
        for row in db.execute("""
            SELECT prefecture, municipality, foreign_rate
            FROM v2_external_foreign_residents
        """).fetchall():
            pref_name = row[0]
            muni_name = row[1] or ""
            rate = row[2]
            if rate is not None:
                # 外国人比率 0〜5% → 0〜100
                val = max(0.0, min(100.0, rate / 5 * 100))
                for grp_name in ["正社員", "パート", "その他"]:
                    foreign_data[(pref_name, muni_name, grp_name)] = val
        print(f"  foreign_workforce: {len(foreign_data)} 件ロード")
    else:
        print("  foreign_workforce: v2_external_foreign_residents テーブル未検出 → Phase B後に利用可能")

    # 全キーを収集
    all_keys = set()
    for d in [salary_data, tightness_data, compliance_data,
              diversity_data, transparency_data, urgency_data, freshness_data,
              real_wage_data, fluidity_data, working_age_data, pop_growth_data, foreign_data]:
        all_keys.update(d.keys())

    if not all_keys:
        print("  → ソーステーブルが1つも存在しない → スキップ")
        return

    insert_rows = []
    for key in all_keys:
        pref, muni, grp = key

        salary = salary_data.get(key)
        tightness = tightness_data.get(key)
        compliance = compliance_data.get(key)
        diversity = diversity_data.get(key)
        transparency = transparency_data.get(key)
        urgency = urgency_data.get(key)
        freshness = freshness_data.get(key)
        real_wage = real_wage_data.get(key)
        fluidity = fluidity_data.get(key)
        working_age = working_age_data.get(key)
        pop_growth = pop_growth_data.get(key)
        foreign = foreign_data.get(key)

        # 非NULLの軸の平均でcomposite算出
        axes = [v for v in [salary, tightness, compliance,
                            diversity, transparency, urgency, freshness,
                            real_wage, fluidity, working_age, pop_growth, foreign]
                if v is not None]

        if not axes:
            continue

        composite = sum(axes) / len(axes)

        insert_rows.append((
            pref, muni, grp,
            salary, tightness, compliance,
            diversity, transparency, urgency,
            freshness, real_wage, fluidity,
            working_age, pop_growth, foreign,
            composite,
        ))

    db.executemany("""
        INSERT OR REPLACE INTO v2_region_benchmark
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
    """, insert_rows)

    print(f"  → {len(insert_rows)} 行を挿入")


# ============================================================
# 検証
# ============================================================

def verify(db):
    """検証: テーブル行数とサンプル値を確認"""
    print("\n=== 検証 ===")

    tables = [
        "v2_external_job_opening_ratio",
        "v2_external_minimum_wage",
        "v2_external_prefecture_stats",
        "v2_wage_compliance",
        "v2_region_benchmark",
    ]

    for table in tables:
        if table_exists(db, table):
            count = db.execute(f"SELECT COUNT(*) FROM {table}").fetchone()[0]
            print(f"  {table}: {count} 行")
        else:
            print(f"  {table}: 未作成")

    # 有効求人倍率サンプル: 東京都
    if table_exists(db, "v2_external_job_opening_ratio"):
        row = db.execute("""
            SELECT job_opening_ratio, reference_month
            FROM v2_external_job_opening_ratio
            WHERE prefecture = '東京都'
        """).fetchone()
        if row:
            print(f"\n  東京都 有効求人倍率: {row[0]:.2f}（{row[1]}）")

    # 最低賃金サンプル: 東京都
    if table_exists(db, "v2_external_minimum_wage"):
        row = db.execute("""
            SELECT hourly_min_wage, effective_date, fiscal_year
            FROM v2_external_minimum_wage
            WHERE prefecture = '東京都'
        """).fetchone()
        if row:
            print(f"  東京都 最低賃金: {row[0]}円（{row[1]}施行, {row[2]}年度）")

    # 都道府県統計サンプル: 東京都
    if table_exists(db, "v2_external_prefecture_stats"):
        row = db.execute("""
            SELECT unemployment_rate, job_change_desire_rate, non_regular_rate,
                   avg_monthly_wage, price_index, fulfillment_rate, real_wage_index
            FROM v2_external_prefecture_stats
            WHERE prefecture = '東京都'
        """).fetchone()
        if row:
            print(f"\n  東京都 外部指標:")
            print(f"    完全失業率: {row[0]}%  転職希望者比率: {row[1]}%  非正規比率: {row[2]}%")
            print(f"    平均賃金: {row[3]}千円  物価指数: {row[4]}  充足率: {row[5]}%  実質賃金: {row[6]}")

    # 違反チェックサンプル: 東京都
    if table_exists(db, "v2_wage_compliance"):
        row = db.execute("""
            SELECT emp_group, total_hourly_postings, below_min_count,
                   below_min_rate, avg_hourly_wage, median_hourly_wage
            FROM v2_wage_compliance
            WHERE prefecture = '東京都' AND municipality = '' AND emp_group = 'パート'
        """).fetchone()
        if row:
            print(f"\n  東京都 パート 時給違反チェック:")
            print(f"    時給求人: {row[1]:,}件, 最低賃金未満: {row[2]:,}件 ({row[3]*100:.1f}%)")
            print(f"    平均時給: {row[4]:,.0f}円, 中央値: {row[5]:,.0f}円")

    # ベンチマークサンプル: 東京都 正社員
    if table_exists(db, "v2_region_benchmark"):
        row = db.execute("""
            SELECT salary_competitiveness, job_market_tightness, wage_compliance,
                   industry_diversity, info_transparency, text_urgency,
                   posting_freshness, real_wage_power, labor_fluidity,
                   working_age_ratio, population_growth, foreign_workforce,
                   composite_benchmark
            FROM v2_region_benchmark
            WHERE prefecture = '東京都' AND municipality = '' AND emp_group = '正社員'
        """).fetchone()
        if row:
            print(f"\n  東京都 正社員 地域ベンチマーク（12軸）:")
            labels = ["給与競争力", "求人逼迫度", "最低賃金遵守率",
                      "産業多様性", "情報透明性", "求人切迫度", "求人鮮度",
                      "実質賃金力", "労働力流動性",
                      "生産年齢人口比率", "人口社会増減", "外国人労働力"]
            for label, val in zip(labels, row[:12]):
                if val is not None:
                    print(f"    {label}: {val:.1f}")
                else:
                    print(f"    {label}: データなし")
            print(f"    総合ベンチマーク: {row[12]:.1f}")


# ============================================================
# メイン
# ============================================================

def main():
    if not os.path.exists(DB_PATH):
        print(f"Error: DB not found at {DB_PATH}")
        sys.exit(1)

    print(f"DB: {DB_PATH}")
    db = sqlite3.connect(DB_PATH)
    db.execute("PRAGMA journal_mode=WAL")

    try:
        # 4-1: 有効求人倍率マスタ
        compute_job_opening_ratio(db)
        db.commit()

        # 4-4: 最低賃金マスタ
        compute_minimum_wage(db)
        db.commit()

        # 4-5b: 最低賃金違反チェック
        compute_wage_compliance(db)
        db.commit()

        # 4-7: 都道府県別外部指標マスタ（ベンチマークより先に実行）
        compute_prefecture_stats(db)
        db.commit()

        # 4-6: 地域間ベンチマーク（12軸）
        compute_region_benchmark(db)
        db.commit()

        # インデックス作成
        db.execute("CREATE INDEX IF NOT EXISTS idx_job_ratio_pref ON v2_external_job_opening_ratio(prefecture)")
        db.execute("CREATE INDEX IF NOT EXISTS idx_wage_compliance_pref ON v2_wage_compliance(prefecture, emp_group)")
        db.execute("CREATE INDEX IF NOT EXISTS idx_region_bench_pref ON v2_region_benchmark(prefecture, emp_group)")
        db.execute("CREATE INDEX IF NOT EXISTS idx_pref_stats ON v2_external_prefecture_stats(prefecture)")
        db.commit()

        verify(db)
        print("\nPhase 4 外部データ統合 完了")

    except Exception as e:
        db.rollback()
        print(f"Error: {e}")
        raise
    finally:
        db.close()


if __name__ == "__main__":
    main()
