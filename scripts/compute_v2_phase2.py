"""
V2 Phase 2 事前計算スクリプト
==============================
L-1: テキスト温度計 (text_temperature) - 求人原稿の緊急度/選択度密度
L-3: 異業種競合レーダー (cross_industry_competition) - 給与帯×学歴×産業の重複
A-1: 異常値検出 (anomaly_stats) - 地域平均からの逸脱検出
S-1: カスケード集計 (cascade_summary) - 3階層KPI事前計算

全指標は employment_type（正社員/パート/その他/全体）でセグメント化
最小サンプルサイズ: 3
"""
import sqlite3
import re
import math
import sys
import os
from collections import defaultdict

DB_PATH = os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))), "data", "hellowork.db")
MIN_SAMPLE = 3


def emp_group(et):
    """雇用形態グルーピング"""
    if et is None:
        return "その他"
    if "パート" in et:
        return "パート"
    if et == "正社員":
        return "正社員"
    return "その他"


# ─── L-1: テキスト温度計 ─────────────────────────────────────
URGENCY_WORDS = [
    # 直接的な緊急シグナル
    "急募", "即日", "すぐに", "至急", "人手不足", "欠員", "早急", "大至急",
    "人員不足", "大量募集",
    # 応募障壁の緩和（人手不足による条件引下げ）
    "未経験歓迎", "未経験OK", "未経験者歓迎", "経験不問", "学歴不問", "年齢不問",
    "資格不問", "誰でも", "初心者歓迎", "初心者OK", "ブランクOK", "ブランク可",
    # 増員・複数名シグナル
    "増員", "複数名募集", "増員募集",
    # シニア・多様性歓迎（間口拡大）
    "シニア歓迎", "主婦歓迎", "主夫歓迎", "Wワーク可", "副業OK",
]
SELECTIVITY_WORDS = [
    # 高選択性シグナル（汎用語を除外しハローワーク向けに調整）
    "経験者優遇", "有資格者", "即戦力", "経験必須", "資格必須", "要経験",
    "専門知識", "実務経験必須",
    # 注: "スキル"は"スキルアップ"等のポジティブ文脈を多く含むため除外
    # 注: "実務経験"は"実務経験があれば尚可"等の非選択的文脈を含むため"実務経験必須"に絞込
]
# "経験○年以上" パターン
SELECTIVITY_PATTERN = re.compile(r"経験\d+年以上")


def count_words(text, words, pattern=None):
    """テキスト中のワード出現回数を計算"""
    if not text:
        return 0
    count = 0
    for w in words:
        count += text.count(w)
    if pattern:
        count += len(pattern.findall(text))
    return count


def has_any_word(text, words, pattern=None):
    """テキスト中にワードが1つ以上含まれるか判定"""
    if not text:
        return False
    for w in words:
        if w in text:
            return True
    if pattern and pattern.search(text):
        return True
    return False


def _aggregate_temperature(stats, min_sample):
    """温度計の集計辞書からINSERTデータを生成する共通関数"""
    insert_data = []
    for (pref, muni, industry, grp), data in stats.items():
        n = len(data["temps"])
        if n < min_sample:
            continue

        temps = sorted(data["temps"])
        avg_temp = sum(temps) / n
        mid = n // 2
        median_temp = temps[mid] if n % 2 else (temps[mid - 1] + temps[mid]) / 2

        avg_urg = sum(data["urg"]) / n
        avg_sel = sum(data["sel"]) / n

        # hit_rate: ワードを1つ以上含む求人の割合 (0-1)
        urg_hit = data["urg_hit"] / n
        sel_hit = data["sel_hit"] / n

        insert_data.append((
            pref, muni, industry, grp, n,
            round(avg_temp, 4), round(median_temp, 4),
            round(avg_urg, 4), round(avg_sel, 4),
            round(urg_hit, 6), round(sel_hit, 6),
        ))
    return insert_data


def compute_l1_text_temperature(db):
    """L-1: テキスト温度計
    求人原稿の緊急度/選択度密度を計算。
    temperature = (urgency - selectivity) / total_words * 1000 (パーミル)
    温度が高い = 人手不足で条件緩和、温度が低い = 選り好みできる余裕。

    Rust側カラム: emp_group, sample_count, temperature, urgency_density,
                  selectivity_density, urgency_hit_rate, selectivity_hit_rate
    """
    print("L-1: テキスト温度計を計算中...")

    db.execute("DROP TABLE IF EXISTS v2_text_temperature")
    db.execute("""
        CREATE TABLE v2_text_temperature (
            prefecture TEXT NOT NULL,
            municipality TEXT NOT NULL DEFAULT '',
            industry_raw TEXT NOT NULL DEFAULT '',
            emp_group TEXT NOT NULL,
            sample_count INTEGER NOT NULL,
            temperature REAL NOT NULL,
            median_temperature REAL NOT NULL,
            urgency_density REAL NOT NULL,
            selectivity_density REAL NOT NULL,
            urgency_hit_rate REAL NOT NULL DEFAULT 0,
            selectivity_hit_rate REAL NOT NULL DEFAULT 0,
            PRIMARY KEY (prefecture, municipality, industry_raw, emp_group)
        )
    """)

    # イテレータ処理: fetchall()を使わずメモリ効率を改善
    cursor = db.execute("""
        SELECT prefecture, municipality, industry_raw, employment_type,
               COALESCE(job_description, '') || ' ' ||
               COALESCE(requirements, '') || ' ' ||
               COALESCE(benefits, '') as full_text
        FROM postings
        WHERE prefecture IS NOT NULL AND prefecture != ''
    """)

    # 集計辞書: key → {temps, urg, sel, urg_hit, sel_hit}
    stats = defaultdict(lambda: {"temps": [], "urg": [], "sel": [], "urg_hit": 0, "sel_hit": 0})

    for pref, muni, industry, et, text in cursor:
        grp = emp_group(et)
        muni = muni or ""
        industry = industry or ""

        # テキスト文字数（空白除去）
        clean_text = text.strip()
        total_chars = len(clean_text.replace(" ", "").replace("\u3000", ""))
        if total_chars < 10:
            # 文字数10未満は計算不能
            continue

        urg_count = count_words(clean_text, URGENCY_WORDS)
        sel_count = count_words(clean_text, SELECTIVITY_WORDS, SELECTIVITY_PATTERN)

        # hit判定: 1つ以上含むか (bool→int)
        urg_has = 1 if has_any_word(clean_text, URGENCY_WORDS) else 0
        sel_has = 1 if has_any_word(clean_text, SELECTIVITY_WORDS, SELECTIVITY_PATTERN) else 0

        # パーミル計算
        temp = (urg_count - sel_count) / total_chars * 1000
        urg_density = urg_count / total_chars * 1000
        sel_density = sel_count / total_chars * 1000

        # 3レベル集計 + 「全体」グループ
        keys = [
            (pref, "", "", grp),              # 都道府県×雇用形態
            (pref, "", "", "全体"),            # 都道府県×全体
        ]
        if muni:
            keys.append((pref, muni, "", grp))     # 市区町村×雇用形態
            keys.append((pref, muni, "", "全体"))   # 市区町村×全体
        if industry:
            keys.append((pref, "", industry, grp))     # 産業別×雇用形態
            keys.append((pref, "", industry, "全体"))   # 産業別×全体

        for key in keys:
            d = stats[key]
            d["temps"].append(temp)
            d["urg"].append(urg_density)
            d["sel"].append(sel_density)
            d["urg_hit"] += urg_has
            d["sel_hit"] += sel_has

    # INSERT
    insert_data = _aggregate_temperature(stats, MIN_SAMPLE)

    db.executemany("""
        INSERT OR REPLACE INTO v2_text_temperature
        (prefecture, municipality, industry_raw, emp_group,
         sample_count, temperature, median_temperature,
         urgency_density, selectivity_density,
         urgency_hit_rate, selectivity_hit_rate)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
    """, insert_data)

    print(f"  → {len(insert_data)}行挿入")
    return len(insert_data)


# ─── L-3: 異業種競合レーダー ─────────────────────────────────

# 学歴正規化マッピング
EDUCATION_NORMALIZE = {
    "不問": "不問",
    "中学・義務教育学校以上": "不問",
    "高校以上": "高校以上",
    "高等学校専攻科以上": "高校以上",
    "専修学校以上": "専修以上",
    "能開校以上": "専修以上",
    "短大以上": "短大以上",
    "高専以上": "短大以上",
    "大学以上": "大学以上",
    "大学院": "大学以上",
}


def salary_band(salary_type, salary_min):
    """月給帯の判定
    注: salary_type == "月給" チェック済みのため、月給で10000以下は非現実的。
    val > 10000 の条件は月給の円→万円変換として妥当。
    """
    if salary_type != "月給" or salary_min is None or salary_min <= 0:
        return "その他"
    # 万円単位に変換（salary_minが円単位の場合）
    val = salary_min
    if val > 10000:
        val = val / 10000  # 円→万円
    if val < 18:
        return "~18万"
    elif val < 22:
        return "18~22万"
    elif val < 26:
        return "22~26万"
    elif val < 30:
        return "26~30万"
    else:
        return "30万~"


def normalize_education(ed):
    """学歴正規化"""
    if not ed or ed.strip() == "":
        return "不問"
    return EDUCATION_NORMALIZE.get(ed.strip(), "不問")


def compute_l3_cross_industry(db):
    """L-3: 異業種競合レーダー
    salary_band × education_group × prefecture の組み合わせで
    同じセルに存在する異なるindustry_rawの数を計算。
    """
    print("L-3: 異業種競合レーダーを計算中...")

    db.execute("DROP TABLE IF EXISTS v2_cross_industry_competition")
    db.execute("""
        CREATE TABLE v2_cross_industry_competition (
            prefecture TEXT NOT NULL,
            salary_band TEXT NOT NULL,
            education_group TEXT NOT NULL,
            emp_group TEXT NOT NULL,
            total_postings INTEGER NOT NULL,
            industry_count INTEGER NOT NULL,
            top_industries TEXT NOT NULL DEFAULT '',
            overlap_score REAL NOT NULL DEFAULT 0,
            PRIMARY KEY (prefecture, salary_band, education_group, emp_group)
        )
    """)

    # イテレータ処理: fetchall()を使わずメモリ効率を改善
    cursor = db.execute("""
        SELECT prefecture, employment_type, salary_type, salary_min,
               education_required, industry_raw
        FROM postings
        WHERE prefecture IS NOT NULL AND prefecture != ''
    """)

    # 集計: (pref, band, edu_group, emp_grp) → {industry: count}
    cells = defaultdict(lambda: defaultdict(int))

    for pref, et, s_type, s_min, edu, industry in cursor:
        grp = emp_group(et)
        band = salary_band(s_type or "", s_min)
        edu_grp = normalize_education(edu)
        ind = industry or "不明"

        # 雇用形態別 + 「全体」
        cells[(pref, band, edu_grp, grp)][ind] += 1
        cells[(pref, band, edu_grp, "全体")][ind] += 1

    insert_data = []
    for (pref, band, edu_grp, grp), industry_counts in cells.items():
        total = sum(industry_counts.values())
        if total < MIN_SAMPLE:
            continue

        n_industries = len(industry_counts)

        # 上位5業種
        sorted_industries = sorted(industry_counts.items(), key=lambda x: -x[1])
        top5 = ",".join([ind for ind, _ in sorted_industries[:5]])

        # overlap_score: 産業数が多いほど競合が激しい
        # HHI逆数: 分散が大きいほどスコアが高い
        hhi = sum((c / total) ** 2 for c in industry_counts.values())
        overlap_score = 1.0 / hhi if hhi > 0 else 0.0

        insert_data.append((
            pref, band, edu_grp, grp, total, n_industries,
            top5, round(overlap_score, 4),
        ))

    db.executemany("""
        INSERT OR REPLACE INTO v2_cross_industry_competition
        (prefecture, salary_band, education_group, emp_group,
         total_postings, industry_count, top_industries, overlap_score)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?)
    """, insert_data)

    print(f"  → {len(insert_data)}行挿入")
    return len(insert_data)


# ─── A-1: 異常値検出 ────────────────────────────────────────

ANOMALY_METRICS = ["salary_min", "employee_count", "annual_holidays", "bonus_months"]


def compute_a1_anomaly_stats(db):
    """A-1: 異常値検出
    地域平均から2σ超の求人を検出し、統計を集計。
    """
    print("A-1: 異常値検出を計算中...")

    db.execute("DROP TABLE IF EXISTS v2_anomaly_stats")
    db.execute("""
        CREATE TABLE v2_anomaly_stats (
            prefecture TEXT NOT NULL,
            municipality TEXT NOT NULL DEFAULT '',
            emp_group TEXT NOT NULL,
            metric_name TEXT NOT NULL,
            total_count INTEGER NOT NULL,
            anomaly_count INTEGER NOT NULL,
            anomaly_rate REAL NOT NULL,
            avg_value REAL NOT NULL,
            stddev_value REAL NOT NULL,
            anomaly_high_count INTEGER NOT NULL DEFAULT 0,
            anomaly_low_count INTEGER NOT NULL DEFAULT 0,
            PRIMARY KEY (prefecture, municipality, emp_group, metric_name)
        )
    """)

    # 各指標ごとにデータ取得・集計
    for metric in ANOMALY_METRICS:
        print(f"  {metric} を処理中...")

        # イテレータ処理
        cursor = db.execute(f"""
            SELECT prefecture, municipality, employment_type, {metric}
            FROM postings
            WHERE prefecture IS NOT NULL AND prefecture != ''
              AND {metric} IS NOT NULL AND {metric} > 0
        """)

        # 集計: (pref, muni, emp_grp) → [values]
        region_values = defaultdict(list)

        for pref, muni, et, val in cursor:
            grp = emp_group(et)
            muni = muni or ""
            fval = float(val)

            # 市区町村レベル（雇用形態別 + 全体）
            region_values[(pref, muni, grp)].append(fval)
            region_values[(pref, muni, "全体")].append(fval)
            # 都道府県集計（雇用形態別 + 全体）
            region_values[(pref, "", grp)].append(fval)
            region_values[(pref, "", "全体")].append(fval)

        insert_data = []
        for (pref, muni, grp), values in region_values.items():
            n = len(values)
            if n < MIN_SAMPLE:
                continue

            avg_val = sum(values) / n
            variance = sum((v - avg_val) ** 2 for v in values) / n
            std_val = math.sqrt(variance) if variance > 0 else 0.0

            if std_val == 0:
                # 全値同一 → 異常値なし
                insert_data.append((
                    pref, muni, grp, metric, n, 0, 0.0,
                    round(avg_val, 2), 0.0, 0, 0,
                ))
                continue

            threshold_high = avg_val + 2 * std_val
            threshold_low = avg_val - 2 * std_val

            high_count = sum(1 for v in values if v > threshold_high)
            low_count = sum(1 for v in values if v < threshold_low)
            anomaly_count = high_count + low_count
            anomaly_rate = anomaly_count / n if n > 0 else 0.0

            insert_data.append((
                pref, muni, grp, metric, n, anomaly_count,
                round(anomaly_rate, 6),
                round(avg_val, 2), round(std_val, 2),
                high_count, low_count,
            ))

        db.executemany("""
            INSERT OR REPLACE INTO v2_anomaly_stats
            (prefecture, municipality, emp_group, metric_name,
             total_count, anomaly_count, anomaly_rate,
             avg_value, stddev_value, anomaly_high_count, anomaly_low_count)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        """, insert_data)

        print(f"    → {len(insert_data)}行挿入")

    total = db.execute("SELECT COUNT(*) FROM v2_anomaly_stats").fetchone()[0]
    print(f"  合計: {total}行")
    return total


# ─── S-1: カスケード集計 ────────────────────────────────────

def compute_s1_cascade_summary(db):
    """S-1: カスケード集計
    都道府県→市区町村→産業の3階層でKPIを事前集計。
    """
    print("S-1: カスケード集計を計算中...")

    db.execute("DROP TABLE IF EXISTS v2_cascade_summary")
    db.execute("""
        CREATE TABLE v2_cascade_summary (
            prefecture TEXT NOT NULL,
            municipality TEXT NOT NULL DEFAULT '',
            industry_raw TEXT NOT NULL DEFAULT '',
            emp_group TEXT NOT NULL,
            posting_count INTEGER NOT NULL,
            facility_count INTEGER NOT NULL DEFAULT 0,
            avg_salary_min REAL,
            median_salary_min REAL,
            avg_employee_count REAL,
            avg_annual_holidays REAL,
            vacancy_rate REAL,
            PRIMARY KEY (prefecture, municipality, industry_raw, emp_group)
        )
    """)

    # イテレータ処理: fetchall()を使わずメモリ効率を改善
    cursor = db.execute("""
        SELECT prefecture, municipality, industry_raw, employment_type,
               facility_name, salary_min, salary_type,
               employee_count, annual_holidays, recruitment_reason
        FROM postings
        WHERE prefecture IS NOT NULL AND prefecture != ''
    """)

    # 集計辞書
    # key → {count, facilities(set), salaries[], employees[], holidays[], vacancy_count}
    stats = defaultdict(lambda: {
        "count": 0, "facilities": set(),
        "salaries": [], "employees": [], "holidays": [],
        "vacancy": 0,
    })

    for pref, muni, industry, et, facility, s_min, s_type, emp_cnt, holidays, reason in cursor:
        grp = emp_group(et)
        muni = muni or ""
        industry = industry or ""
        facility = facility or ""

        # 欠員補充判定（全グループ共通で事前計算）
        is_vacancy = 1 if (reason and "欠員" in str(reason)) else 0

        # 3レベル集計キー（雇用形態別 + 全体）
        keys = [
            (pref, "", "", grp),                                # 都道府県×雇用形態
            (pref, "", "", "全体"),                              # 都道府県×全体
        ]
        if muni:
            keys.append((pref, muni, "", grp))                  # 市区町村×雇用形態
            keys.append((pref, muni, "", "全体"))                # 市区町村×全体
        if industry:
            keys.append((pref, "", industry, grp))              # 産業別×雇用形態
            keys.append((pref, "", industry, "全体"))            # 産業別×全体

        for key in keys:
            d = stats[key]
            d["count"] += 1
            if facility:
                d["facilities"].add(facility)
            if s_min and s_min > 0:
                d["salaries"].append(float(s_min))
            if emp_cnt and emp_cnt > 0:
                d["employees"].append(float(emp_cnt))
            if holidays and holidays > 0:
                d["holidays"].append(float(holidays))
            d["vacancy"] += is_vacancy

    insert_data = []
    for (pref, muni, industry, grp), d in stats.items():
        n = d["count"]
        if n < MIN_SAMPLE:
            continue

        facility_count = len(d["facilities"])

        # 給与統計
        sals = sorted(d["salaries"])
        avg_sal = sum(sals) / len(sals) if sals else None
        if sals:
            mid = len(sals) // 2
            median_sal = sals[mid] if len(sals) % 2 else (sals[mid - 1] + sals[mid]) / 2
        else:
            median_sal = None

        avg_emp = sum(d["employees"]) / len(d["employees"]) if d["employees"] else None
        avg_hol = sum(d["holidays"]) / len(d["holidays"]) if d["holidays"] else None
        vr = d["vacancy"] / n if n > 0 else None

        insert_data.append((
            pref, muni, industry, grp, n, facility_count,
            round(avg_sal, 2) if avg_sal is not None else None,
            round(median_sal, 2) if median_sal is not None else None,
            round(avg_emp, 2) if avg_emp is not None else None,
            round(avg_hol, 2) if avg_hol is not None else None,
            round(vr, 6) if vr is not None else None,
        ))

    db.executemany("""
        INSERT OR REPLACE INTO v2_cascade_summary
        (prefecture, municipality, industry_raw, emp_group,
         posting_count, facility_count,
         avg_salary_min, median_salary_min,
         avg_employee_count, avg_annual_holidays, vacancy_rate)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
    """, insert_data)

    print(f"  → {len(insert_data)}行挿入")
    return len(insert_data)


# ─── 検証 ──────────────────────────────────────────────────

def verify(db):
    """計算結果の検証"""
    print("\n" + "=" * 60)
    print("検証結果")
    print("=" * 60)

    tables = [
        "v2_text_temperature",
        "v2_cross_industry_competition",
        "v2_anomaly_stats",
        "v2_cascade_summary",
    ]
    for table in tables:
        cnt = db.execute(f"SELECT COUNT(*) FROM {table}").fetchone()[0]
        print(f"\n  {table}: {cnt}行")

    # L-1: テキスト温度計サンプル
    print("\n--- L-1: テキスト温度計 ---")
    print("  東京都集計:")
    for r in db.execute("""
        SELECT emp_group, sample_count, temperature, median_temperature,
               urgency_density, selectivity_density,
               urgency_hit_rate, selectivity_hit_rate
        FROM v2_text_temperature
        WHERE prefecture='東京都' AND municipality='' AND industry_raw=''
        ORDER BY emp_group
    """):
        print(f"    {r[0]}: n={r[1]}, 温度={r[2]:.3f}\u2030, 中央値={r[3]:.3f}\u2030, "
              f"緊急密度={r[4]:.3f}\u2030, 選択密度={r[5]:.3f}\u2030, "
              f"緊急hit={r[6]:.1%}, 選択hit={r[7]:.1%}")

    # 温度トップ/ボトム都道府県
    print("  温度トップ5都道府県（正社員）:")
    for r in db.execute("""
        SELECT prefecture, temperature, sample_count
        FROM v2_text_temperature
        WHERE municipality='' AND industry_raw='' AND emp_group='正社員'
        ORDER BY temperature DESC LIMIT 5
    """):
        print(f"    {r[0]}: {r[1]:.3f}\u2030 (n={r[2]})")

    print("  温度ボトム5都道府県（正社員）:")
    for r in db.execute("""
        SELECT prefecture, temperature, sample_count
        FROM v2_text_temperature
        WHERE municipality='' AND industry_raw='' AND emp_group='正社員'
        ORDER BY temperature ASC LIMIT 5
    """):
        print(f"    {r[0]}: {r[1]:.3f}\u2030 (n={r[2]})")

    # 「全体」グループの確認
    total_cnt = db.execute("""
        SELECT COUNT(*) FROM v2_text_temperature WHERE emp_group='全体'
    """).fetchone()[0]
    print(f"  「全体」グループ行数: {total_cnt}")

    # L-3: 異業種競合レーダーサンプル
    print("\n--- L-3: 異業種競合レーダー ---")
    print("  東京都サンプル（正社員）:")
    for r in db.execute("""
        SELECT salary_band, education_group, total_postings, industry_count,
               overlap_score, top_industries
        FROM v2_cross_industry_competition
        WHERE prefecture='東京都' AND emp_group='正社員'
        ORDER BY total_postings DESC LIMIT 5
    """):
        top3 = ",".join(r[5].split(",")[:3]) if r[5] else ""
        print(f"    {r[0]}×{r[1]}: n={r[2]}, 産業数={r[3]}, "
              f"overlap={r[4]:.1f}, トップ3={top3}")

    # 給与帯別分布
    print("  給与帯別合計（正社員）:")
    for r in db.execute("""
        SELECT salary_band, SUM(total_postings), AVG(industry_count)
        FROM v2_cross_industry_competition
        WHERE emp_group='正社員'
        GROUP BY salary_band
        ORDER BY salary_band
    """):
        print(f"    {r[0]}: 求人数={r[1]}, 平均産業数={r[2]:.1f}")

    # A-1: 異常値検出サンプル
    print("\n--- A-1: 異常値検出 ---")
    print("  東京都集計（正社員）:")
    for r in db.execute("""
        SELECT metric_name, total_count, anomaly_count, anomaly_rate,
               avg_value, stddev_value, anomaly_high_count, anomaly_low_count
        FROM v2_anomaly_stats
        WHERE prefecture='東京都' AND municipality='' AND emp_group='正社員'
        ORDER BY metric_name
    """):
        print(f"    {r[0]}: n={r[1]}, 異常={r[2]}({r[3]:.2%}), "
              f"平均={r[4]:.1f}, \u03c3={r[5]:.1f}, 高={r[6]}/低={r[7]}")

    # 指標別の全国異常率
    print("  全国異常率（都道府県レベル集計、正社員）:")
    for r in db.execute("""
        SELECT metric_name, AVG(anomaly_rate), MAX(anomaly_rate),
               SUM(anomaly_count), SUM(total_count)
        FROM v2_anomaly_stats
        WHERE municipality='' AND emp_group='正社員'
        GROUP BY metric_name
        ORDER BY metric_name
    """):
        print(f"    {r[0]}: 平均異常率={r[1]:.2%}, 最大={r[2]:.2%}, "
              f"異常計={r[3]}/{r[4]}")

    # S-1: カスケード集計サンプル
    print("\n--- S-1: カスケード集計 ---")
    print("  東京都集計:")
    for r in db.execute("""
        SELECT emp_group, posting_count, facility_count,
               avg_salary_min, median_salary_min,
               avg_employee_count, avg_annual_holidays, vacancy_rate
        FROM v2_cascade_summary
        WHERE prefecture='東京都' AND municipality='' AND industry_raw=''
        ORDER BY emp_group
    """):
        sal = f"平均給与={r[3]:.0f}" if r[3] else "平均給与=N/A"
        med = f"中央値={r[4]:.0f}" if r[4] else "中央値=N/A"
        emp = f"平均従業員={r[5]:.0f}" if r[5] else "平均従業員=N/A"
        hol = f"平均年休={r[6]:.0f}" if r[6] else "平均年休=N/A"
        vr = f"欠員率={r[7]:.2%}" if r[7] is not None else "欠員率=N/A"
        print(f"    {r[0]}: n={r[1]}, 施設数={r[2]}, {sal}, {med}, {emp}, {hol}, {vr}")

    # 集計レベル別行数
    print("\n  集計レベル別行数:")
    for label, cond in [
        ("都道府県集計", "municipality='' AND industry_raw=''"),
        ("市区町村集計", "municipality!='' AND industry_raw=''"),
        ("産業別集計", "municipality='' AND industry_raw!=''"),
    ]:
        cnt = db.execute(f"SELECT COUNT(*) FROM v2_cascade_summary WHERE {cond}").fetchone()[0]
        print(f"    {label}: {cnt}行")

    # 雇用形態別行数
    print("\n  雇用形態セグメント別行数:")
    for table in tables:
        counts = db.execute(f"SELECT emp_group, COUNT(*) FROM {table} GROUP BY emp_group ORDER BY emp_group").fetchall()
        print(f"    {table}:")
        for grp, cnt in counts:
            print(f"      {grp}: {cnt}行")


def main():
    sys.stdout.reconfigure(encoding="utf-8")
    print(f"DB: {DB_PATH}")
    print(f"最小サンプルサイズ: {MIN_SAMPLE}")
    print()

    if not os.path.exists(DB_PATH):
        print(f"エラー: DBファイルが見つかりません: {DB_PATH}")
        sys.exit(1)

    db = sqlite3.connect(DB_PATH)
    db.execute("PRAGMA journal_mode=WAL")
    db.execute("PRAGMA synchronous=NORMAL")

    try:
        l1 = compute_l1_text_temperature(db)
        l3 = compute_l3_cross_industry(db)
        a1 = compute_a1_anomaly_stats(db)
        s1 = compute_s1_cascade_summary(db)
        db.commit()

        # インデックス作成
        print("\nインデックスを作成中...")
        db.execute("CREATE INDEX IF NOT EXISTS idx_text_temp_pref ON v2_text_temperature(prefecture, emp_group)")
        db.execute("CREATE INDEX IF NOT EXISTS idx_cross_ind_pref ON v2_cross_industry_competition(prefecture, emp_group)")
        db.execute("CREATE INDEX IF NOT EXISTS idx_anomaly_pref ON v2_anomaly_stats(prefecture, emp_group)")
        db.execute("CREATE INDEX IF NOT EXISTS idx_cascade_pref ON v2_cascade_summary(prefecture, emp_group)")
        db.execute("CREATE INDEX IF NOT EXISTS idx_cascade_pref_muni ON v2_cascade_summary(prefecture, municipality, emp_group)")
        db.commit()

        verify(db)

        print(f"\n{'=' * 60}")
        print(f"Phase 2 計算完了")
        print(f"  L-1 テキスト温度計:       {l1}行")
        print(f"  L-3 異業種競合レーダー:   {l3}行")
        print(f"  A-1 異常値検出:           {a1}行")
        print(f"  S-1 カスケード集計:       {s1}行")
        print(f"{'=' * 60}")
    finally:
        db.close()


if __name__ == "__main__":
    main()
