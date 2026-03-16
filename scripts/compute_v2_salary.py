"""
V2独自分析: Phase 1 給与分析 事前計算スクリプト
================================================
1-1: 給与構造分析 (salary_structure) - 地域×産業×雇用形態の給与5分位
1-2: 給与競争力指数 (salary_competitiveness) - 全国平均比の競争力
1-3: 報酬パッケージ総合スコア (compensation_package) - 給与+休日+賞与の総合評価

全指標は employment_type（正社員/パート/その他）でセグメント化
"""
import sqlite3
import math
import sys
import os
from collections import defaultdict
from bisect import bisect_left
from hw_common import emp_group

DB_PATH = os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))), "data", "hellowork.db")

MIN_SAMPLE = 5  # 最小サンプルサイズ


def percentile(sorted_list, pct):
    """ソート済みリストからパーセンタイル値を計算"""
    if not sorted_list:
        return None
    k = (len(sorted_list) - 1) * (pct / 100.0)
    f = int(k)
    c = f + 1
    if c >= len(sorted_list):
        return sorted_list[-1]
    return sorted_list[f] + (k - f) * (sorted_list[c] - sorted_list[f])


def compute_salary_structure(db):
    """1-1: 給与構造分析
    地域×産業×雇用形態×給与種類ごとの給与5分位、年収推定等を計算。
    """
    print("1-1: 給与構造分析を計算中...")

    db.execute("DROP TABLE IF EXISTS v2_salary_structure")
    db.execute("""
        CREATE TABLE v2_salary_structure (
            prefecture TEXT NOT NULL,
            municipality TEXT NOT NULL DEFAULT '',
            industry_raw TEXT NOT NULL DEFAULT '',
            emp_group TEXT NOT NULL,
            salary_type TEXT NOT NULL,
            total_count INTEGER NOT NULL,
            avg_salary_min REAL,
            avg_salary_max REAL,
            median_salary_min REAL,
            p10_salary_min REAL,
            p25_salary_min REAL,
            p75_salary_min REAL,
            p90_salary_min REAL,
            salary_spread REAL,
            avg_bonus_months REAL,
            bonus_disclosure_rate REAL,
            estimated_annual_min REAL,
            estimated_annual_max REAL,
            PRIMARY KEY (prefecture, municipality, industry_raw, emp_group, salary_type)
        )
    """)

    rows = db.execute("""
        SELECT prefecture, municipality, industry_raw, employment_type,
               salary_type, salary_min, salary_max, bonus_months
        FROM postings
        WHERE prefecture IS NOT NULL AND prefecture != ''
          AND salary_min > 0
    """).fetchall()

    # 集計: key → {salary_mins: [], salary_maxs: [], bonus_list: []}
    data = defaultdict(lambda: {"mins": [], "maxs": [], "bonus": []})

    for pref, muni, industry, et, stype, smin, smax, bonus in rows:
        grp = emp_group(et)
        muni = muni or ""
        industry = industry or ""
        stype = stype or "その他"

        # 3レベル集計
        for key in [
            (pref, muni, industry, grp, stype),  # 詳細
            (pref, muni, "", grp, stype),          # 市区町村
            (pref, "", "", grp, stype),            # 都道府県
        ]:
            d = data[key]
            d["mins"].append(smin)
            if smax and smax > 0:
                d["maxs"].append(smax)
            if bonus is not None and bonus > 0:
                d["bonus"].append(bonus)

    insert_rows = []
    for (pref, muni, industry, grp, stype), d in data.items():
        n = len(d["mins"])
        if n < MIN_SAMPLE:
            continue

        sorted_mins = sorted(d["mins"])
        sorted_maxs = sorted(d["maxs"]) if d["maxs"] else []

        avg_min = sum(d["mins"]) / n
        avg_max = sum(d["maxs"]) / len(d["maxs"]) if d["maxs"] else None
        med_min = percentile(sorted_mins, 50)
        p10 = percentile(sorted_mins, 10)
        p25 = percentile(sorted_mins, 25)
        p75 = percentile(sorted_mins, 75)
        p90 = percentile(sorted_mins, 90)

        spread = ((avg_max - avg_min) / avg_min) if avg_max and avg_min > 0 else None

        bonus_count = len(d["bonus"])
        bonus_rate = bonus_count / n
        avg_bonus = sum(d["bonus"]) / bonus_count if bonus_count > 0 else 0

        annual_min = avg_min * (12 + avg_bonus)
        annual_max = avg_max * (12 + avg_bonus) if avg_max else None

        insert_rows.append((
            pref, muni, industry, grp, stype, n,
            avg_min, avg_max, med_min, p10, p25, p75, p90,
            spread, avg_bonus, bonus_rate, annual_min, annual_max,
        ))

    db.executemany("""
        INSERT OR REPLACE INTO v2_salary_structure VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)
    """, insert_rows)

    print(f"  → {len(insert_rows)} 行を挿入")


def compute_salary_competitiveness(db):
    """1-2: 給与競争力指数
    地域の平均給与を全国平均と比較し、競争力指数とパーセンタイルランクを算出。
    月給のみ対象。
    """
    print("1-2: 給与競争力指数を計算中...")

    db.execute("DROP TABLE IF EXISTS v2_salary_competitiveness")
    db.execute("""
        CREATE TABLE v2_salary_competitiveness (
            prefecture TEXT NOT NULL,
            municipality TEXT NOT NULL DEFAULT '',
            industry_raw TEXT NOT NULL DEFAULT '',
            emp_group TEXT NOT NULL,
            local_avg_salary REAL NOT NULL,
            national_avg_salary REAL NOT NULL,
            competitiveness_index REAL NOT NULL,
            percentile_rank REAL NOT NULL,
            sample_count INTEGER NOT NULL,
            PRIMARY KEY (prefecture, municipality, industry_raw, emp_group)
        )
    """)

    rows = db.execute("""
        SELECT prefecture, municipality, industry_raw, employment_type, salary_min
        FROM postings
        WHERE prefecture IS NOT NULL AND prefecture != ''
          AND salary_min > 0
          AND salary_type = '月給'
    """).fetchall()

    # 3レベル集計
    local_data = defaultdict(list)  # (pref, muni, industry, grp) → [salary_min, ...]
    national_data = defaultdict(list)  # (grp, industry) → [salary_min, ...]

    for pref, muni, industry, et, smin in rows:
        grp = emp_group(et)
        muni = muni or ""
        industry = industry or ""

        for key in [
            (pref, muni, industry, grp),
            (pref, muni, "", grp),
            (pref, "", "", grp),
        ]:
            local_data[key].append(smin)

        national_data[(grp, industry)].append(smin)
        national_data[(grp, "")].append(smin)

    # 全国平均
    national_avg = {}
    for (grp, industry), vals in national_data.items():
        national_avg[(grp, industry)] = sum(vals) / len(vals)

    # パーセンタイルランク計算用: 都道府県レベルの平均給与を収集
    pref_avgs = defaultdict(list)  # (grp, industry) → [(pref, avg), ...]
    for (pref, muni, industry, grp), vals in local_data.items():
        if muni == "" and industry == "":
            pref_avgs[(grp, "")].append(sum(vals) / len(vals))

    insert_rows = []
    for (pref, muni, industry, grp), vals in local_data.items():
        n = len(vals)
        if n < MIN_SAMPLE:
            continue

        local_avg = sum(vals) / n
        nat = national_avg.get((grp, industry), national_avg.get((grp, ""), local_avg))

        comp_idx = ((local_avg - nat) / nat * 100) if nat > 0 else 0

        # パーセンタイルランク
        all_avgs = sorted(pref_avgs.get((grp, ""), []))
        if all_avgs:
            pos = bisect_left(all_avgs, local_avg)
            pct_rank = pos / len(all_avgs) * 100
        else:
            pct_rank = 50.0

        insert_rows.append((
            pref, muni, industry, grp,
            local_avg, nat, comp_idx, pct_rank, n,
        ))

    db.executemany("""
        INSERT OR REPLACE INTO v2_salary_competitiveness VALUES (?,?,?,?,?,?,?,?,?)
    """, insert_rows)

    print(f"  → {len(insert_rows)} 行を挿入")


def compute_compensation_package(db):
    """1-3: 報酬パッケージ総合スコア
    給与(45%) + 年間休日(30%) + 賞与(25%) の総合評価。
    overtime_monthlyは全件NULLのため除外。
    """
    print("1-3: 報酬パッケージ総合スコアを計算中...")

    db.execute("DROP TABLE IF EXISTS v2_compensation_package")
    db.execute("""
        CREATE TABLE v2_compensation_package (
            prefecture TEXT NOT NULL,
            municipality TEXT NOT NULL DEFAULT '',
            industry_raw TEXT NOT NULL DEFAULT '',
            emp_group TEXT NOT NULL,
            total_count INTEGER NOT NULL,
            avg_salary_min REAL,
            avg_annual_holidays REAL,
            avg_bonus_months REAL,
            salary_pctile REAL,
            holidays_pctile REAL,
            bonus_pctile REAL,
            composite_score REAL NOT NULL,
            rank_label TEXT NOT NULL,
            PRIMARY KEY (prefecture, municipality, industry_raw, emp_group)
        )
    """)

    rows = db.execute("""
        SELECT prefecture, municipality, industry_raw, employment_type,
               salary_min, annual_holidays, bonus_months
        FROM postings
        WHERE prefecture IS NOT NULL AND prefecture != ''
          AND salary_min > 0
          AND salary_type = '月給'
    """).fetchall()

    # 全国分布を構築
    all_salaries = []
    all_holidays = []
    all_bonus = []

    # 3レベル集計
    local_data = defaultdict(lambda: {"salary": [], "holidays": [], "bonus": []})

    for pref, muni, industry, et, smin, holidays, bonus in rows:
        grp = emp_group(et)
        muni = muni or ""
        industry = industry or ""

        all_salaries.append(smin)
        if holidays and holidays > 0:
            all_holidays.append(holidays)
        if bonus is not None and bonus > 0:
            all_bonus.append(bonus)

        for key in [
            (pref, muni, industry, grp),
            (pref, muni, "", grp),
            (pref, "", "", grp),
        ]:
            d = local_data[key]
            d["salary"].append(smin)
            if holidays and holidays > 0:
                d["holidays"].append(holidays)
            if bonus is not None and bonus > 0:
                d["bonus"].append(bonus)

    all_salaries.sort()
    all_holidays.sort()
    all_bonus.sort()

    def pctile_of(value, sorted_dist):
        if not sorted_dist:
            return 50.0
        pos = bisect_left(sorted_dist, value)
        return pos / len(sorted_dist) * 100

    insert_rows = []
    for (pref, muni, industry, grp), d in local_data.items():
        n = len(d["salary"])
        if n < MIN_SAMPLE:
            continue

        avg_sal = sum(d["salary"]) / n
        avg_hol = sum(d["holidays"]) / len(d["holidays"]) if d["holidays"] else None
        avg_bon = sum(d["bonus"]) / len(d["bonus"]) if d["bonus"] else None

        sal_pct = pctile_of(avg_sal, all_salaries)
        hol_pct = pctile_of(avg_hol, all_holidays) if avg_hol else 50.0
        bon_pct = pctile_of(avg_bon, all_bonus) if avg_bon else 50.0

        # 給与45% + 休日30% + 賞与25%（overtime除外のため再配分）
        composite = sal_pct * 0.45 + hol_pct * 0.30 + bon_pct * 0.25

        if composite >= 80:
            rank = "S"
        elif composite >= 65:
            rank = "A"
        elif composite >= 50:
            rank = "B"
        elif composite >= 35:
            rank = "C"
        else:
            rank = "D"

        insert_rows.append((
            pref, muni, industry, grp, n,
            avg_sal, avg_hol, avg_bon,
            sal_pct, hol_pct, bon_pct,
            composite, rank,
        ))

    db.executemany("""
        INSERT OR REPLACE INTO v2_compensation_package VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?)
    """, insert_rows)

    print(f"  → {len(insert_rows)} 行を挿入")

    # ランク分布を表示
    rank_dist = defaultdict(int)
    for row in insert_rows:
        rank_dist[row[-1]] += 1
    print(f"  ランク分布: {dict(sorted(rank_dist.items()))}")


def verify(db):
    """検証: テーブル行数とサンプル値を確認"""
    print("\n=== 検証 ===")
    for table in ["v2_salary_structure", "v2_salary_competitiveness", "v2_compensation_package"]:
        count = db.execute(f"SELECT COUNT(*) FROM {table}").fetchone()[0]
        print(f"  {table}: {count} 行")

    # 東京都正社員月給のサンプル
    row = db.execute("""
        SELECT avg_salary_min, median_salary_min, p25_salary_min, p75_salary_min,
               avg_bonus_months, estimated_annual_min
        FROM v2_salary_structure
        WHERE prefecture='東京都' AND municipality='' AND industry_raw=''
          AND emp_group='正社員' AND salary_type='月給'
    """).fetchone()
    if row:
        print(f"\n  東京都 正社員 月給:")
        print(f"    平均: {row[0]:,.0f}円, 中央値: {row[1]:,.0f}円")
        print(f"    p25-p75: {row[2]:,.0f}〜{row[3]:,.0f}円")
        print(f"    賞与平均: {row[4]:.1f}ヶ月, 推定年収: {row[5]:,.0f}円")

    row = db.execute("""
        SELECT competitiveness_index, percentile_rank
        FROM v2_salary_competitiveness
        WHERE prefecture='東京都' AND municipality='' AND industry_raw=''
          AND emp_group='正社員'
    """).fetchone()
    if row:
        print(f"    競争力指数: {row[0]:+.1f}%, 全国順位: {row[1]:.0f}パーセンタイル")

    row = db.execute("""
        SELECT composite_score, rank_label, salary_pctile, holidays_pctile, bonus_pctile
        FROM v2_compensation_package
        WHERE prefecture='東京都' AND municipality='' AND industry_raw=''
          AND emp_group='正社員'
    """).fetchone()
    if row:
        print(f"    総合スコア: {row[0]:.1f} (ランク{row[1]})")
        print(f"    給与pct: {row[2]:.0f}, 休日pct: {row[3]:.0f}, 賞与pct: {row[4]:.0f}")


def main():
    if not os.path.exists(DB_PATH):
        print(f"Error: DB not found at {DB_PATH}")
        sys.exit(1)

    print(f"DB: {DB_PATH}")
    db = sqlite3.connect(DB_PATH)
    db.execute("PRAGMA journal_mode=WAL")

    try:
        compute_salary_structure(db)
        db.commit()

        compute_salary_competitiveness(db)
        db.commit()

        compute_compensation_package(db)
        db.commit()

        verify(db)
        print("\nPhase 1 給与分析 完了")
    except Exception as e:
        db.rollback()
        print(f"Error: {e}")
        raise
    finally:
        db.close()


if __name__ == "__main__":
    main()
