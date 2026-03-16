"""
V2独自分析: Phase 1 事前計算スクリプト
======================================
C-4: 欠員補充率 (vacancy_rate) - 求人理由から地域の人材定着度を推定
S-2: 地域レジリエンス (regional_resilience) - Shannon多様性指数で産業集中リスクを評価
C-1: 透明性スコア (transparency_score) - 情報開示率から企業の透明性を数値化

全指標は employment_type（正社員/パート/その他）でセグメント化
"""
import sqlite3
import math
import sys
import os
from collections import defaultdict
from hw_common import emp_group

DB_PATH = os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))), "data", "hellowork.db")


def compute_c4_vacancy_rate(db):
    """C-4: 欠員補充率
    recruitment_reason_code=1(欠員補充) の比率を地域×雇用形態別に計算。
    欠員補充率が高い = 人材が定着しにくい地域/業種。
    """
    print("C-4: 欠員補充率を計算中...")

    db.execute("DROP TABLE IF EXISTS v2_vacancy_rate")
    db.execute("""
        CREATE TABLE v2_vacancy_rate (
            prefecture TEXT NOT NULL,
            municipality TEXT NOT NULL DEFAULT '',
            industry_raw TEXT NOT NULL DEFAULT '',
            emp_group TEXT NOT NULL,
            total_count INTEGER NOT NULL,
            vacancy_count INTEGER NOT NULL,
            growth_count INTEGER NOT NULL,
            new_facility_count INTEGER NOT NULL,
            vacancy_rate REAL NOT NULL,
            growth_rate REAL NOT NULL,
            PRIMARY KEY (prefecture, municipality, industry_raw, emp_group)
        )
    """)

    # 全データを取得してPythonで集計
    rows = db.execute("""
        SELECT prefecture, municipality, industry_raw, employment_type,
               recruitment_reason_code
        FROM postings
        WHERE prefecture IS NOT NULL AND prefecture != ''
    """).fetchall()

    # 集計用辞書: key = (pref, muni, industry, emp_grp)
    # val = [total, vacancy, growth, new_facility]
    stats = defaultdict(lambda: [0, 0, 0, 0])

    for pref, muni, industry, et, reason_code in rows:
        grp = emp_group(et)
        muni = muni or ""
        industry = industry or ""

        # 詳細レベル: pref × muni × industry × emp_grp
        key = (pref, muni, industry, grp)
        stats[key][0] += 1
        if reason_code == 1:
            stats[key][1] += 1
        elif reason_code == 2:
            stats[key][2] += 1
        elif reason_code == 3:
            stats[key][3] += 1

        # 都道府県集計: pref × "" × "" × emp_grp
        pkey = (pref, "", "", grp)
        stats[pkey][0] += 1
        if reason_code == 1:
            stats[pkey][1] += 1
        elif reason_code == 2:
            stats[pkey][2] += 1
        elif reason_code == 3:
            stats[pkey][3] += 1

        # 都道府県×市区町村集計: pref × muni × "" × emp_grp
        if muni:
            mkey = (pref, muni, "", grp)
            stats[mkey][0] += 1
            if reason_code == 1:
                stats[mkey][1] += 1
            elif reason_code == 2:
                stats[mkey][2] += 1
            elif reason_code == 3:
                stats[mkey][3] += 1

    # INSERT
    insert_data = []
    for (pref, muni, industry, grp), (total, vac, grow, new_f) in stats.items():
        if total < 3:  # 最小サンプルサイズ
            continue
        vr = vac / total if total > 0 else 0.0
        gr = grow / total if total > 0 else 0.0
        insert_data.append((pref, muni, industry, grp, total, vac, grow, new_f, vr, gr))

    db.executemany("""
        INSERT OR REPLACE INTO v2_vacancy_rate
        (prefecture, municipality, industry_raw, emp_group, total_count,
         vacancy_count, growth_count, new_facility_count, vacancy_rate, growth_rate)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
    """, insert_data)

    print(f"  → {len(insert_data)}行挿入")
    return len(insert_data)


def compute_s2_resilience(db):
    """S-2: 地域レジリエンス（産業多様性）
    Shannon多様性指数 H = -Σ(p_i × ln(p_i)) を地域×雇用形態別に計算。
    H値が高い = 産業が分散 = 地域の雇用がレジリエント。
    H値が低い = 特定産業に依存 = リスク集中。
    """
    print("S-2: 地域レジリエンスを計算中...")

    db.execute("DROP TABLE IF EXISTS v2_regional_resilience")
    db.execute("""
        CREATE TABLE v2_regional_resilience (
            prefecture TEXT NOT NULL,
            municipality TEXT NOT NULL DEFAULT '',
            emp_group TEXT NOT NULL,
            total_count INTEGER NOT NULL,
            industry_count INTEGER NOT NULL,
            shannon_index REAL NOT NULL,
            max_shannon REAL NOT NULL,
            evenness REAL NOT NULL,
            top_industry TEXT NOT NULL DEFAULT '',
            top_industry_share REAL NOT NULL DEFAULT 0,
            hhi REAL NOT NULL DEFAULT 0,
            PRIMARY KEY (prefecture, municipality, emp_group)
        )
    """)

    rows = db.execute("""
        SELECT prefecture, municipality, employment_type, industry_raw
        FROM postings
        WHERE prefecture IS NOT NULL AND prefecture != ''
          AND industry_raw IS NOT NULL AND industry_raw != ''
    """).fetchall()

    # 集計: (pref, muni, emp_grp) → {industry: count}
    region_industries = defaultdict(lambda: defaultdict(int))

    for pref, muni, et, industry in rows:
        grp = emp_group(et)
        muni = muni or ""

        # 詳細レベル
        region_industries[(pref, muni, grp)][industry] += 1
        # 都道府県集計
        region_industries[(pref, "", grp)][industry] += 1

    insert_data = []
    for (pref, muni, grp), industry_counts in region_industries.items():
        total = sum(industry_counts.values())
        if total < 10:  # 最小サンプルサイズ
            continue

        n_industries = len(industry_counts)

        # Shannon diversity index
        shannon = 0.0
        hhi = 0.0
        top_industry = ""
        top_count = 0

        for ind, cnt in industry_counts.items():
            p = cnt / total
            if p > 0:
                shannon -= p * math.log(p)
            hhi += p * p
            if cnt > top_count:
                top_count = cnt
                top_industry = ind

        max_shannon = math.log(n_industries) if n_industries > 1 else 1.0
        evenness = shannon / max_shannon if max_shannon > 0 else 0.0
        top_share = top_count / total if total > 0 else 0.0

        insert_data.append((
            pref, muni, grp, total, n_industries,
            round(shannon, 4), round(max_shannon, 4), round(evenness, 4),
            top_industry, round(top_share, 4), round(hhi, 6)
        ))

    db.executemany("""
        INSERT OR REPLACE INTO v2_regional_resilience
        (prefecture, municipality, emp_group, total_count, industry_count,
         shannon_index, max_shannon, evenness, top_industry, top_industry_share, hhi)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
    """, insert_data)

    print(f"  → {len(insert_data)}行挿入")
    return len(insert_data)


def compute_c1_transparency(db):
    """C-1: 透明性スコア（情報開示率）
    企業が任意項目をどこまで開示しているかをスコア化。
    開示率が低い求人 = 「沈黙のカラム」= 隠す理由がある可能性。
    地域×雇用形態別に平均透明性スコアを計算。
    """
    print("C-1: 透明性スコアを計算中...")

    db.execute("DROP TABLE IF EXISTS v2_transparency_score")
    db.execute("""
        CREATE TABLE v2_transparency_score (
            prefecture TEXT NOT NULL,
            municipality TEXT NOT NULL DEFAULT '',
            industry_raw TEXT NOT NULL DEFAULT '',
            emp_group TEXT NOT NULL,
            total_count INTEGER NOT NULL,
            avg_transparency REAL NOT NULL,
            median_transparency REAL NOT NULL,
            disclosure_annual_holidays REAL NOT NULL DEFAULT 0,
            disclosure_bonus_months REAL NOT NULL DEFAULT 0,
            disclosure_employee_count REAL NOT NULL DEFAULT 0,
            disclosure_capital REAL NOT NULL DEFAULT 0,
            disclosure_overtime REAL NOT NULL DEFAULT 0,
            disclosure_female_ratio REAL NOT NULL DEFAULT 0,
            disclosure_parttime_ratio REAL NOT NULL DEFAULT 0,
            disclosure_founding_year REAL NOT NULL DEFAULT 0,
            PRIMARY KEY (prefecture, municipality, industry_raw, emp_group)
        )
    """)

    # 透明性チェック対象カラム（任意開示項目）
    check_columns = [
        ("annual_holidays", "annual_holidays IS NOT NULL AND annual_holidays > 0"),
        ("bonus_months", "bonus_months IS NOT NULL AND bonus_months > 0"),
        ("employee_count", "employee_count IS NOT NULL AND employee_count > 0"),
        ("capital", "capital IS NOT NULL AND capital > 0"),
        ("overtime_monthly", "overtime_monthly IS NOT NULL AND overtime_monthly > 0"),
        ("employee_count_female", "employee_count_female IS NOT NULL"),
        ("employee_count_parttime", "employee_count_parttime IS NOT NULL"),
        ("founding_year", "founding_year IS NOT NULL AND founding_year > 0"),
    ]

    rows = db.execute("""
        SELECT prefecture, municipality, industry_raw, employment_type,
               annual_holidays, bonus_months, employee_count, capital,
               overtime_monthly, employee_count_female, employee_count_parttime,
               founding_year
        FROM postings
        WHERE prefecture IS NOT NULL AND prefecture != ''
    """).fetchall()

    # 集計: (pref, muni, industry, emp_grp) → {scores: [], disclosures: [8 counters]}
    region_data = defaultdict(lambda: {"scores": [], "disc": [0]*8, "total": 0})

    for row in rows:
        pref, muni, industry, et = row[0], row[1] or "", row[2] or "", row[3]
        grp = emp_group(et)

        # 個別求人の透明性スコア (0-8)
        score = 0
        disc_flags = []
        # annual_holidays
        if row[4] is not None and row[4] > 0: score += 1; disc_flags.append(0)
        # bonus_months
        if row[5] is not None and row[5] > 0: score += 1; disc_flags.append(1)
        # employee_count
        if row[6] is not None and row[6] > 0: score += 1; disc_flags.append(2)
        # capital
        if row[7] is not None and row[7] > 0: score += 1; disc_flags.append(3)
        # overtime_monthly
        if row[8] is not None and row[8] > 0: score += 1; disc_flags.append(4)
        # employee_count_female
        if row[9] is not None: score += 1; disc_flags.append(5)
        # employee_count_parttime
        if row[10] is not None: score += 1; disc_flags.append(6)
        # founding_year
        if row[11] is not None and row[11] > 0: score += 1; disc_flags.append(7)

        for key in [(pref, muni, industry, grp), (pref, "", "", grp), (pref, muni, "", grp)]:
            if key == (pref, muni, "", grp) and not muni:
                continue
            region_data[key]["scores"].append(score)
            region_data[key]["total"] += 1
            for f in disc_flags:
                region_data[key]["disc"][f] += 1

    insert_data = []
    for (pref, muni, industry, grp), data in region_data.items():
        total = data["total"]
        if total < 3:
            continue

        scores = sorted(data["scores"])
        avg_t = sum(scores) / len(scores) / 8.0  # 0-1スケール
        mid = len(scores) // 2
        median_t = (scores[mid] if len(scores) % 2 else (scores[mid-1] + scores[mid]) / 2) / 8.0

        disc_rates = [data["disc"][i] / total for i in range(8)]

        insert_data.append((
            pref, muni, industry, grp, total,
            round(avg_t, 4), round(median_t, 4),
            *[round(r, 4) for r in disc_rates]
        ))

    db.executemany("""
        INSERT OR REPLACE INTO v2_transparency_score
        (prefecture, municipality, industry_raw, emp_group, total_count,
         avg_transparency, median_transparency,
         disclosure_annual_holidays, disclosure_bonus_months,
         disclosure_employee_count, disclosure_capital,
         disclosure_overtime, disclosure_female_ratio,
         disclosure_parttime_ratio, disclosure_founding_year)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
    """, insert_data)

    print(f"  → {len(insert_data)}行挿入")
    return len(insert_data)


def verify(db):
    """計算結果の検証"""
    print("\n=== 検証 ===")
    for table in ["v2_vacancy_rate", "v2_regional_resilience", "v2_transparency_score"]:
        cnt = db.execute(f"SELECT COUNT(*) FROM {table}").fetchone()[0]
        print(f"  {table}: {cnt}行")

    # 欠員補充率サンプル（東京都、正社員）
    print("\n  C-4 サンプル（東京都集計、正社員）:")
    r = db.execute("""
        SELECT total_count, vacancy_count, vacancy_rate, growth_rate
        FROM v2_vacancy_rate
        WHERE prefecture='東京都' AND municipality='' AND industry_raw='' AND emp_group='正社員'
    """).fetchone()
    if r:
        print(f"    件数={r[0]}, 欠員補充={r[1]}, 欠員率={r[2]:.3f}, 増員率={r[3]:.3f}")

    print("  C-4 サンプル（東京都集計、パート）:")
    r = db.execute("""
        SELECT total_count, vacancy_count, vacancy_rate, growth_rate
        FROM v2_vacancy_rate
        WHERE prefecture='東京都' AND municipality='' AND industry_raw='' AND emp_group='パート'
    """).fetchone()
    if r:
        print(f"    件数={r[0]}, 欠員補充={r[1]}, 欠員率={r[2]:.3f}, 増員率={r[3]:.3f}")

    # レジリエンスサンプル
    print("\n  S-2 サンプル（東京都集計）:")
    for r in db.execute("""
        SELECT emp_group, total_count, industry_count, shannon_index, evenness, top_industry, top_industry_share
        FROM v2_regional_resilience
        WHERE prefecture='東京都' AND municipality=''
        ORDER BY emp_group
    """):
        print(f"    {r[0]}: 件数={r[1]}, 産業数={r[2]}, Shannon={r[3]:.3f}, 均等度={r[4]:.3f}, トップ={r[5]}({r[6]:.1%})")

    # 透明性サンプル
    print("\n  C-1 サンプル（東京都集計）:")
    for r in db.execute("""
        SELECT emp_group, total_count, avg_transparency,
               disclosure_annual_holidays, disclosure_bonus_months, disclosure_overtime
        FROM v2_transparency_score
        WHERE prefecture='東京都' AND municipality='' AND industry_raw=''
        ORDER BY emp_group
    """):
        print(f"    {r[0]}: 件数={r[1]}, 透明度={r[2]:.3f}, 年休開示={r[3]:.1%}, 賞与開示={r[4]:.1%}, 残業開示={r[5]:.1%}")


def main():
    sys.stdout.reconfigure(encoding="utf-8")
    print(f"DB: {DB_PATH}")

    db = sqlite3.connect(DB_PATH)
    db.execute("PRAGMA journal_mode=WAL")
    db.execute("PRAGMA synchronous=NORMAL")

    try:
        c4 = compute_c4_vacancy_rate(db)
        s2 = compute_s2_resilience(db)
        c1 = compute_c1_transparency(db)
        db.commit()

        # インデックス作成
        print("\nインデックスを作成中...")
        db.execute("CREATE INDEX IF NOT EXISTS idx_vacancy_pref ON v2_vacancy_rate(prefecture, emp_group)")
        db.execute("CREATE INDEX IF NOT EXISTS idx_resilience_pref ON v2_regional_resilience(prefecture, emp_group)")
        db.execute("CREATE INDEX IF NOT EXISTS idx_transparency_pref ON v2_transparency_score(prefecture, emp_group)")
        db.commit()

        verify(db)
        print(f"\n完了: C-4={c4}, S-2={s2}, C-1={c1}")
    finally:
        db.close()


if __name__ == "__main__":
    main()
