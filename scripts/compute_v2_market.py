"""
V2独自分析: Phase 3 市場構造分析 事前計算スクリプト
==================================================
3-1: 企業採用戦略の類型化 (employer_strategy) - 給与×福利厚生の4象限分類
3-2: 雇用者独占力指数 (monopsony_index) - HHI/Gini/集中度で市場寡占を検出
3-3: 空間的ミスマッチ検出 (spatial_mismatch) - 地理的な求人アクセス格差を測定

全指標は employment_type（正社員/パート/その他）でセグメント化
"""
import sqlite3
import math
import re
import sys
import os
from collections import defaultdict
from bisect import bisect_left
from hw_common import emp_group

DB_PATH = os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))), "data", "hellowork.db")

MIN_SAMPLE = 5  # 最小サンプルサイズ

# 福利厚生キーワード（amenity_score算出用）
BENEFITS_KEYWORDS = re.compile(r'退職金|住宅手当|家族手当|通勤手当|育休|産休|研修|資格取得|社宅|保育')


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


def haversine(lat1, lon1, lat2, lon2):
    """2地点間のハーバーサイン距離（km）"""
    R = 6371  # 地球の半径(km)
    dlat = math.radians(lat2 - lat1)
    dlon = math.radians(lon2 - lon1)
    a = (math.sin(dlat / 2) ** 2
         + math.cos(math.radians(lat1)) * math.cos(math.radians(lat2))
         * math.sin(dlon / 2) ** 2)
    return R * 2 * math.asin(math.sqrt(a))


# ============================================================
# 3-1: 企業採用戦略の類型化
# ============================================================

def compute_employer_strategy(db):
    """3-1: 企業採用戦略の類型化
    各求人を給与パーセンタイルとamenity_scoreで4象限に分類。
    - プレミアム型: 高給与 & 高福利
    - 給与一本勝負型: 高給与 & 低福利
    - 福利厚生重視型: 低給与 & 高福利
    - コスト優先型: 低給与 & 低福利
    """
    print("3-1: 企業採用戦略の類型化を計算中...")

    # --- 求人別テーブル ---
    db.execute("DROP TABLE IF EXISTS v2_employer_strategy")
    db.execute("""
        CREATE TABLE v2_employer_strategy (
            prefecture TEXT NOT NULL,
            municipality TEXT NOT NULL DEFAULT '',
            industry_raw TEXT NOT NULL DEFAULT '',
            emp_group TEXT NOT NULL,
            facility_name TEXT NOT NULL DEFAULT '',
            salary_percentile REAL,
            amenity_score REAL,
            strategy_type TEXT NOT NULL
        )
    """)

    # --- サマリーテーブル ---
    db.execute("DROP TABLE IF EXISTS v2_employer_strategy_summary")
    db.execute("""
        CREATE TABLE v2_employer_strategy_summary (
            prefecture TEXT NOT NULL,
            municipality TEXT NOT NULL DEFAULT '',
            industry_raw TEXT NOT NULL DEFAULT '',
            emp_group TEXT NOT NULL,
            total_count INTEGER NOT NULL,
            premium_count INTEGER NOT NULL DEFAULT 0,
            premium_pct REAL NOT NULL DEFAULT 0,
            salary_focus_count INTEGER NOT NULL DEFAULT 0,
            salary_focus_pct REAL NOT NULL DEFAULT 0,
            benefits_focus_count INTEGER NOT NULL DEFAULT 0,
            benefits_focus_pct REAL NOT NULL DEFAULT 0,
            cost_focus_count INTEGER NOT NULL DEFAULT 0,
            cost_focus_pct REAL NOT NULL DEFAULT 0,
            PRIMARY KEY (prefecture, municipality, industry_raw, emp_group)
        )
    """)

    # データ取得
    rows = db.execute("""
        SELECT prefecture, municipality, industry_raw, employment_type,
               facility_name, salary_min, bonus_months, annual_holidays,
               benefits, overtime_monthly
        FROM postings
        WHERE prefecture IS NOT NULL AND prefecture != ''
          AND salary_min > 0
    """).fetchall()

    # ---- Step 1: 地域×雇用形態ごとの給与分布を構築 ----
    # 給与パーセンタイル算出用: (pref, muni, emp_group) → [salary_min, ...]
    salary_by_region = defaultdict(list)
    # 休日パーセンタイル算出用: (pref, muni, emp_group) → [annual_holidays, ...]
    holidays_by_region = defaultdict(list)

    for pref, muni, industry, et, facility, smin, bonus, holidays, benefits, overtime in rows:
        grp = emp_group(et)
        muni = muni or ""
        # 地域集計は市区町村レベルで行い、パーセンタイルを算出
        salary_by_region[(pref, muni, grp)].append(smin)
        if holidays is not None and holidays > 0:
            holidays_by_region[(pref, muni, grp)].append(holidays)

    # ソートしておく
    for k in salary_by_region:
        salary_by_region[k].sort()
    for k in holidays_by_region:
        holidays_by_region[k].sort()

    # ---- Step 2: 各求人のスコアを計算 ----
    posting_records = []  # (pref, muni, industry, grp, facility, salary_pct, amenity, strategy)

    for pref, muni, industry, et, facility, smin, bonus, holidays, benefits_text, overtime in rows:
        grp = emp_group(et)
        muni = muni or ""
        industry = industry or ""
        facility = facility or ""

        # 給与パーセンタイル（地域×雇用形態内での位置）
        region_key = (pref, muni, grp)
        sorted_salaries = salary_by_region.get(region_key, [])
        if sorted_salaries:
            pos = bisect_left(sorted_salaries, smin)
            salary_pct = pos / len(sorted_salaries) * 100
        else:
            salary_pct = 50.0

        # amenity_score の各構成要素 (各0-1、合計0-1)
        # (1) has_bonus: 賞与あり
        has_bonus = 1.0 if (bonus is not None and bonus > 0) else 0.0

        # (2) holiday_score: 休日の地域内パーセンタイル
        sorted_holidays = holidays_by_region.get(region_key, [])
        if holidays is not None and holidays > 0 and sorted_holidays:
            h_pos = bisect_left(sorted_holidays, holidays)
            holiday_score = h_pos / len(sorted_holidays)
        else:
            holiday_score = 0.0

        # (3) benefits_keyword_count: 福利厚生キーワード出現数
        if benefits_text and isinstance(benefits_text, str):
            kw_count = len(BENEFITS_KEYWORDS.findall(benefits_text))
            benefits_kw_score = min(kw_count / 5.0, 1.0)
        else:
            benefits_kw_score = 0.0

        # M-3: overtime_monthlyが100% NULLの場合、amenity_scoreから除外
        # NULL率が高い成分は計算から除外し、残りの成分で正規化
        components = {
            "has_bonus": has_bonus,
            "holiday_score": holiday_score,
            "benefits_kw_score": benefits_kw_score,
        }
        # overtime は非NULLの場合のみ成分に追加
        if overtime is not None:
            if overtime < 20:
                components["low_overtime"] = 1.0
            else:
                components["low_overtime"] = 0.0

        # amenity_score（0-100スケール）: 各成分を均等配分
        n_components = len(components)
        weight = 100.0 / n_components if n_components > 0 else 25.0
        amenity = sum(v * weight for v in components.values())

        # 4象限分類（中央値=50を閾値として使用）
        if salary_pct >= 50 and amenity >= 50:
            strategy = "プレミアム型"
        elif salary_pct >= 50 and amenity < 50:
            strategy = "給与一本勝負型"
        elif salary_pct < 50 and amenity >= 50:
            strategy = "福利厚生重視型"
        else:
            strategy = "コスト優先型"

        posting_records.append((
            pref, muni, industry, grp, facility,
            round(salary_pct, 2), round(amenity, 2), strategy,
        ))

    # 求人別テーブルに挿入
    db.executemany("""
        INSERT INTO v2_employer_strategy
        (prefecture, municipality, industry_raw, emp_group, facility_name,
         salary_percentile, amenity_score, strategy_type)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?)
    """, posting_records)
    print(f"  → v2_employer_strategy: {len(posting_records)} 行を挿入")

    # ---- Step 3: サマリー集計（3レベル） ----
    # (pref, muni, industry, grp) → {strategy_type: count}
    summary_data = defaultdict(lambda: defaultdict(int))

    for pref, muni, industry, grp, facility, spct, amenity, strategy in posting_records:
        # 3レベル集計
        for key in [
            (pref, muni, industry, grp),  # 詳細
            (pref, muni, "", grp),         # 市区町村
            (pref, "", "", grp),           # 都道府県
        ]:
            summary_data[key][strategy] += 1

    strategy_types = ["プレミアム型", "給与一本勝負型", "福利厚生重視型", "コスト優先型"]
    summary_rows = []
    for (pref, muni, industry, grp), counts in summary_data.items():
        total = sum(counts.values())
        if total < MIN_SAMPLE:
            continue

        type_counts = [counts.get(st, 0) for st in strategy_types]
        type_pcts = [c / total for c in type_counts]

        summary_rows.append((
            pref, muni, industry, grp, total,
            type_counts[0], round(type_pcts[0], 4),
            type_counts[1], round(type_pcts[1], 4),
            type_counts[2], round(type_pcts[2], 4),
            type_counts[3], round(type_pcts[3], 4),
        ))

    db.executemany("""
        INSERT OR REPLACE INTO v2_employer_strategy_summary
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
    """, summary_rows)
    print(f"  → v2_employer_strategy_summary: {len(summary_rows)} 行を挿入")

    # 全国分布を表示
    total_by_strategy = defaultdict(int)
    for rec in posting_records:
        total_by_strategy[rec[7]] += 1
    print(f"  戦略分布: {dict(total_by_strategy)}")

    return len(posting_records), len(summary_rows)


# ============================================================
# 3-2: 雇用者独占力指数
# ============================================================

def compute_monopsony_index(db):
    """3-2: 雇用者独占力指数
    施設ごとの求人シェアからHHI、Gini係数、Top-N集中度を算出。
    HHI高 = 少数企業が市場を独占 = 求職者に不利な市場構造。
    """
    print("3-2: 雇用者独占力指数を計算中...")

    db.execute("DROP TABLE IF EXISTS v2_monopsony_index")
    db.execute("""
        CREATE TABLE v2_monopsony_index (
            prefecture TEXT NOT NULL,
            municipality TEXT NOT NULL DEFAULT '',
            industry_raw TEXT NOT NULL DEFAULT '',
            emp_group TEXT NOT NULL,
            total_postings INTEGER NOT NULL,
            unique_facilities INTEGER NOT NULL,
            hhi REAL NOT NULL,
            gini REAL NOT NULL,
            top1_share REAL NOT NULL,
            top3_share REAL NOT NULL,
            top5_share REAL NOT NULL,
            concentration_level TEXT NOT NULL,
            PRIMARY KEY (prefecture, municipality, industry_raw, emp_group)
        )
    """)

    # データ取得
    rows = db.execute("""
        SELECT prefecture, municipality, industry_raw, employment_type,
               facility_name
        FROM postings
        WHERE prefecture IS NOT NULL AND prefecture != ''
          AND facility_name IS NOT NULL AND facility_name != ''
    """).fetchall()

    # 3レベル集計: (pref, muni, industry, grp) → {facility: count}
    facility_counts = defaultdict(lambda: defaultdict(int))

    for pref, muni, industry, et, facility in rows:
        grp = emp_group(et)
        muni = muni or ""
        industry = industry or ""

        for key in [
            (pref, muni, industry, grp),
            (pref, muni, "", grp),
            (pref, "", "", grp),
        ]:
            facility_counts[key][facility] += 1

    insert_rows = []
    for (pref, muni, industry, grp), fac_dict in facility_counts.items():
        total = sum(fac_dict.values())
        n_facilities = len(fac_dict)

        if total < MIN_SAMPLE:
            continue

        # 各施設のシェアを計算
        shares = sorted([cnt / total for cnt in fac_dict.values()], reverse=True)

        # HHI = 各シェアの二乗和（0〜1）
        hhi = sum(s * s for s in shares)

        # Gini係数の計算
        # Gini = (2 * Σ(i * y_i)) / (n * Σ(y_i)) - (n + 1) / n
        shares_asc = sorted(shares)  # 昇順
        n = len(shares_asc)
        if n > 1 and sum(shares_asc) > 0:
            numerator = sum((i + 1) * s for i, s in enumerate(shares_asc))
            denominator = n * sum(shares_asc)
            gini = (2 * numerator / denominator) - (n + 1) / n
            gini = max(0.0, min(1.0, gini))  # 0-1にクリップ
        else:
            gini = 0.0

        # Top-N シェア
        top1 = shares[0] if len(shares) >= 1 else 0
        top3 = sum(shares[:3]) if len(shares) >= 3 else sum(shares)
        top5 = sum(shares[:5]) if len(shares) >= 5 else sum(shares)

        # 集中度レベル判定
        if hhi < 0.15:
            level = "分散"
        elif hhi < 0.25:
            level = "やや集中"
        else:
            level = "高集中"

        insert_rows.append((
            pref, muni, industry, grp,
            total, n_facilities,
            round(hhi, 6), round(gini, 4),
            round(top1, 4), round(top3, 4), round(top5, 4),
            level,
        ))

    db.executemany("""
        INSERT OR REPLACE INTO v2_monopsony_index
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
    """, insert_rows)
    print(f"  → {len(insert_rows)} 行を挿入")

    # 集中度レベルの分布を表示
    level_dist = defaultdict(int)
    for row in insert_rows:
        level_dist[row[-1]] += 1
    print(f"  集中度分布: {dict(level_dist)}")

    return len(insert_rows)


# ============================================================
# 3-3: 空間的ミスマッチ検出
# ============================================================

def compute_spatial_mismatch(db):
    """3-3: 空間的ミスマッチ検出
    市区町村の重心間距離から30km/60km圏内のアクセス可能求人数を算出。
    isolation_score: 周辺求人が少ない孤立地域を検出。
    salary_gap: 地域と周辺の給与差を測定。
    """
    print("3-3: 空間的ミスマッチ検出を計算中...")

    db.execute("DROP TABLE IF EXISTS v2_spatial_mismatch")
    db.execute("""
        CREATE TABLE v2_spatial_mismatch (
            prefecture TEXT NOT NULL,
            municipality TEXT NOT NULL,
            emp_group TEXT NOT NULL,
            posting_count INTEGER NOT NULL,
            avg_salary_min REAL,
            centroid_lat REAL,
            centroid_lon REAL,
            accessible_postings_30km INTEGER NOT NULL DEFAULT 0,
            accessible_avg_salary_30km REAL,
            accessible_postings_60km INTEGER NOT NULL DEFAULT 0,
            accessible_avg_salary_60km REAL,
            salary_gap_vs_accessible REAL,
            isolation_score REAL,
            PRIMARY KEY (prefecture, municipality, emp_group)
        )
    """)

    # ---- Step 1: 市区町村の重心を計算 ----
    rows = db.execute("""
        SELECT prefecture, municipality, employment_type,
               latitude, longitude, salary_min
        FROM postings
        WHERE prefecture IS NOT NULL AND prefecture != ''
          AND municipality IS NOT NULL AND municipality != ''
          AND latitude > 0 AND longitude > 0
    """).fetchall()

    # (pref, muni, grp) → {lats: [], lons: [], salaries: []}
    muni_data = defaultdict(lambda: {"lats": [], "lons": [], "salaries": []})

    for pref, muni, et, lat, lon, smin in rows:
        grp = emp_group(et)
        key = (pref, muni, grp)
        muni_data[key]["lats"].append(lat)
        muni_data[key]["lons"].append(lon)
        if smin is not None and smin > 0:
            muni_data[key]["salaries"].append(smin)

    # 重心と集計値を計算
    # key → {lat, lon, count, avg_salary}
    centroids = {}
    for (pref, muni, grp), d in muni_data.items():
        n = len(d["lats"])
        if n < MIN_SAMPLE:
            continue
        clat = sum(d["lats"]) / n
        clon = sum(d["lons"]) / n
        avg_sal = sum(d["salaries"]) / len(d["salaries"]) if d["salaries"] else None
        centroids[(pref, muni, grp)] = {
            "lat": clat, "lon": clon,
            "count": n, "avg_salary": avg_sal,
        }

    print(f"  重心計算完了: {len(centroids)} 地域")

    # ---- Step 2: 雇用形態別に近隣市区町村距離を事前計算 ----
    # 雇用形態ごとにグルーピング
    by_grp = defaultdict(list)  # grp → [(pref, muni, lat, lon, count, avg_salary), ...]
    for (pref, muni, grp), c in centroids.items():
        by_grp[grp].append((pref, muni, c["lat"], c["lon"], c["count"], c["avg_salary"]))

    insert_rows = []

    for grp, muni_list in by_grp.items():
        n_muni = len(muni_list)
        print(f"  {grp}: {n_muni} 市区町村の近隣計算中...")

        # 全市区町村の30km/60km圏内アクセス可能求人を計算
        # 近接候補を緯度差2度で事前フィルタ
        accessible_30km_counts = []  # median計算用

        # 各市区町村について計算
        muni_results = []
        for i, (pref_i, muni_i, lat_i, lon_i, count_i, sal_i) in enumerate(muni_list):
            acc_30_count = 0
            acc_30_salary_sum = 0.0
            acc_30_salary_n = 0
            acc_60_count = 0
            acc_60_salary_sum = 0.0
            acc_60_salary_n = 0

            for j, (pref_j, muni_j, lat_j, lon_j, count_j, sal_j) in enumerate(muni_list):
                if i == j:
                    continue
                # 緯度差で粗いフィルタ（2度 ≈ 220km）
                if abs(lat_i - lat_j) > 2.0:
                    continue
                # 経度差でも粗いフィルタ
                if abs(lon_i - lon_j) > 2.0:
                    continue

                dist = haversine(lat_i, lon_i, lat_j, lon_j)

                if dist <= 60:
                    acc_60_count += count_j
                    if sal_j is not None:
                        acc_60_salary_sum += sal_j * count_j
                        acc_60_salary_n += count_j

                    if dist <= 30:
                        acc_30_count += count_j
                        if sal_j is not None:
                            acc_30_salary_sum += sal_j * count_j
                            acc_30_salary_n += count_j

            acc_30_avg_sal = acc_30_salary_sum / acc_30_salary_n if acc_30_salary_n > 0 else None
            acc_60_avg_sal = acc_60_salary_sum / acc_60_salary_n if acc_60_salary_n > 0 else None

            # 給与ギャップ（自地域 - 30km圏内平均）
            if sal_i is not None and acc_30_avg_sal is not None:
                salary_gap = sal_i - acc_30_avg_sal
            else:
                salary_gap = None

            muni_results.append({
                "pref": pref_i, "muni": muni_i,
                "count": count_i, "avg_salary": sal_i,
                "lat": lat_i, "lon": lon_i,
                "acc_30": acc_30_count, "acc_30_sal": acc_30_avg_sal,
                "acc_60": acc_60_count, "acc_60_sal": acc_60_avg_sal,
                "salary_gap": salary_gap,
            })
            accessible_30km_counts.append(acc_30_count)

        # isolation_score計算: 中央値ベースの孤立度
        if accessible_30km_counts:
            sorted_acc = sorted(accessible_30km_counts)
            median_acc = percentile(sorted_acc, 50) or 1
        else:
            median_acc = 1

        for r in muni_results:
            if median_acc > 0:
                isolation = 1.0 - min(r["acc_30"] / median_acc, 1.0)
            else:
                isolation = 1.0

            insert_rows.append((
                r["pref"], r["muni"], grp,
                r["count"],
                round(r["avg_salary"], 0) if r["avg_salary"] is not None else None,
                round(r["lat"], 6), round(r["lon"], 6),
                r["acc_30"],
                round(r["acc_30_sal"], 0) if r["acc_30_sal"] is not None else None,
                r["acc_60"],
                round(r["acc_60_sal"], 0) if r["acc_60_sal"] is not None else None,
                round(r["salary_gap"], 0) if r["salary_gap"] is not None else None,
                round(isolation, 4),
            ))

    db.executemany("""
        INSERT OR REPLACE INTO v2_spatial_mismatch
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
    """, insert_rows)
    print(f"  → {len(insert_rows)} 行を挿入")

    # 孤立度の分布を表示
    isolation_values = [r[-1] for r in insert_rows if r[-1] is not None]
    if isolation_values:
        high_isolation = sum(1 for v in isolation_values if v >= 0.8)
        mid_isolation = sum(1 for v in isolation_values if 0.4 <= v < 0.8)
        low_isolation = sum(1 for v in isolation_values if v < 0.4)
        print(f"  孤立度分布: 高(>=0.8)={high_isolation}, 中(0.4-0.8)={mid_isolation}, 低(<0.4)={low_isolation}")

    return len(insert_rows)


# ============================================================
# 検証
# ============================================================

def verify(db):
    """計算結果の検証"""
    print("\n=== 検証 ===")
    for table in ["v2_employer_strategy", "v2_employer_strategy_summary",
                   "v2_monopsony_index", "v2_spatial_mismatch"]:
        cnt = db.execute(f"SELECT COUNT(*) FROM {table}").fetchone()[0]
        print(f"  {table}: {cnt} 行")

    # 3-1: 企業採用戦略サンプル（東京都、正社員）
    print("\n  3-1 サンプル（東京都集計、正社員）:")
    r = db.execute("""
        SELECT total_count, premium_count, premium_pct,
               salary_focus_count, salary_focus_pct,
               benefits_focus_count, benefits_focus_pct,
               cost_focus_count, cost_focus_pct
        FROM v2_employer_strategy_summary
        WHERE prefecture='東京都' AND municipality='' AND industry_raw=''
          AND emp_group='正社員'
    """).fetchone()
    if r:
        print(f"    件数={r[0]}")
        print(f"    プレミアム型: {r[1]}件({r[2]:.1%})")
        print(f"    給与一本勝負型: {r[3]}件({r[4]:.1%})")
        print(f"    福利厚生重視型: {r[5]}件({r[6]:.1%})")
        print(f"    コスト優先型: {r[7]}件({r[8]:.1%})")

    # 3-2: 独占力指数サンプル（東京都、正社員）
    print("\n  3-2 サンプル（東京都集計、正社員）:")
    r = db.execute("""
        SELECT total_postings, unique_facilities, hhi, gini,
               top1_share, top3_share, top5_share, concentration_level
        FROM v2_monopsony_index
        WHERE prefecture='東京都' AND municipality='' AND industry_raw=''
          AND emp_group='正社員'
    """).fetchone()
    if r:
        print(f"    求人数={r[0]}, 施設数={r[1]}")
        print(f"    HHI={r[2]:.6f}, Gini={r[3]:.4f}")
        print(f"    Top1={r[4]:.1%}, Top3={r[5]:.1%}, Top5={r[6]:.1%}")
        print(f"    集中度: {r[7]}")

    # 3-3: 空間的ミスマッチサンプル（東京都内の市区町村）
    print("\n  3-3 サンプル（東京都内、正社員、孤立度上位5）:")
    for r in db.execute("""
        SELECT municipality, posting_count, avg_salary_min,
               accessible_postings_30km, accessible_avg_salary_30km,
               salary_gap_vs_accessible, isolation_score
        FROM v2_spatial_mismatch
        WHERE prefecture='東京都' AND emp_group='正社員'
        ORDER BY isolation_score DESC
        LIMIT 5
    """):
        gap_str = f"{r[5]:+,.0f}円" if r[5] is not None else "N/A"
        sal_str = f"{r[2]:,.0f}円" if r[2] is not None else "N/A"
        print(f"    {r[1]}: 求人{r[0]}件, 平均給与{sal_str}, "
              f"30km圏{r[3]}件, ギャップ{gap_str}, 孤立度{r[6]:.3f}")


# ============================================================
# メイン
# ============================================================

def main():
    sys.stdout.reconfigure(encoding="utf-8")

    if not os.path.exists(DB_PATH):
        print(f"Error: DB not found at {DB_PATH}")
        sys.exit(1)

    print(f"DB: {DB_PATH}")
    db = sqlite3.connect(DB_PATH)
    db.execute("PRAGMA journal_mode=WAL")
    db.execute("PRAGMA synchronous=NORMAL")

    try:
        n_strategy, n_summary = compute_employer_strategy(db)
        db.commit()

        n_monopsony = compute_monopsony_index(db)
        db.commit()

        n_spatial = compute_spatial_mismatch(db)
        db.commit()

        # インデックス作成
        print("\nインデックスを作成中...")
        db.execute("CREATE INDEX IF NOT EXISTS idx_strategy_pref ON v2_employer_strategy(prefecture, emp_group)")
        db.execute("CREATE INDEX IF NOT EXISTS idx_strategy_sum_pref ON v2_employer_strategy_summary(prefecture, emp_group)")
        db.execute("CREATE INDEX IF NOT EXISTS idx_monopsony_pref ON v2_monopsony_index(prefecture, emp_group)")
        db.execute("CREATE INDEX IF NOT EXISTS idx_spatial_pref ON v2_spatial_mismatch(prefecture, emp_group)")
        db.commit()

        verify(db)
        print(f"\nPhase 3 市場構造分析 完了: "
              f"3-1={n_strategy}+{n_summary}, 3-2={n_monopsony}, 3-3={n_spatial}")
    except Exception as e:
        db.rollback()
        print(f"Error: {e}")
        raise
    finally:
        db.close()


if __name__ == "__main__":
    main()
