"""
V2独自分析: Phase 2 テキスト分析 事前計算スクリプト
====================================================
2-1: テキスト品質分析 (text_quality) - 求人原稿の文字数・多様性・情報量を数値化
2-2: キーワードプロファイル (keyword_profile) - 求人カテゴリ別キーワード出現率
2-3: テキスト温度計 (text_temperature) - 緊急度 vs 選択度の温度指標

全指標は employment_type（正社員/パート/その他）でセグメント化

注意: DBカラムは job_description（description は存在しない）
"""
import sqlite3
import re
import sys
import os
from collections import defaultdict

DB_PATH = os.path.join(
    os.path.dirname(os.path.dirname(os.path.abspath(__file__))),
    "data",
    "hellowork.db",
)

MIN_SAMPLE = 5  # 最小サンプルサイズ

# 漢字判定用の正規表現（CJK統合漢字）
RE_KANJI = re.compile(r"[\u4e00-\u9fff\u3400-\u4dbf]")
# 数字判定（全角・半角）
RE_NUMERIC = re.compile(r"[0-9０-９]")
# 句読点判定（日本語句読点 + ピリオド・カンマ）
RE_PUNCTUATION = re.compile(r"[、。，．,.！？!?]")

# --- キーワードカテゴリ定義 ---
KEYWORD_CATEGORIES = {
    "急募系": re.compile(r"急募|すぐ|至急|即日|即戦力"),
    "未経験系": re.compile(r"未経験|初心者|ブランク|経験不問|無資格"),
    "待遇系": re.compile(r"賞与|昇給|手当|退職金|社保完備|有給"),
    "WLB系": re.compile(r"残業少|残業なし|土日休|週休[2二]|年間休日"),
    "成長系": re.compile(r"研修|教育|資格取得|スキルアップ|キャリア"),
    "安定系": re.compile(r"正社員|安定|長期|定年|無期"),
}

# --- テキスト温度計用パターン ---
RE_URGENCY = re.compile(r"急募|すぐ|至急|即日|大量募集|人手不足|欠員")
RE_SELECTIVITY = re.compile(r"経験者優遇|要経験|有資格者|経験[0-9０-９]+年|選考あり")


def emp_group(et):
    """雇用形態グルーピング"""
    if et is None:
        return "その他"
    if "パート" in et:
        return "パート"
    if et == "正社員":
        return "正社員"
    return "その他"


def _combine_text_fields(job_desc, headline, requirements, benefits, company_desc):
    """複数テキストフィールドを結合して分析対象テキストを生成。
    主に job_description を使い、他フィールドは補助的に結合する。
    """
    parts = []
    for field in [job_desc, headline, requirements, benefits, company_desc]:
        if field:
            parts.append(str(field))
    return " ".join(parts)


def _text_metrics(text):
    """テキストから各種メトリクスを計算して辞書で返す。
    戻り値: {char_count, unique_char_ratio, kanji_ratio, numeric_ratio,
             punctuation_density}
    テキストが空の場合は None を返す。
    """
    if not text or not text.strip():
        return None

    char_count = len(text)
    if char_count == 0:
        return None

    unique_chars = len(set(text))
    unique_char_ratio = unique_chars / char_count

    kanji_count = len(RE_KANJI.findall(text))
    kanji_ratio = kanji_count / char_count

    numeric_count = len(RE_NUMERIC.findall(text))
    numeric_ratio = numeric_count / char_count

    punct_count = len(RE_PUNCTUATION.findall(text))
    punctuation_density = punct_count / char_count

    return {
        "char_count": char_count,
        "unique_char_ratio": unique_char_ratio,
        "kanji_ratio": kanji_ratio,
        "numeric_ratio": numeric_ratio,
        "punctuation_density": punctuation_density,
    }


# =====================================================================
# 2-1: テキスト品質分析
# =====================================================================
def compute_text_quality(db):
    """2-1: テキスト品質分析
    求人原稿(job_description + 補助フィールド)のテキスト品質を
    地域×産業×雇用形態別に集計。

    メトリクス:
      - avg_char_count: 平均文字数
      - avg_unique_char_ratio: ユニーク文字率（文字の多様性）
      - avg_kanji_ratio: 漢字比率
      - avg_numeric_ratio: 数字比率（具体的な条件提示の指標）
      - avg_punctuation_density: 句読点密度
      - information_score: 情報量スコア
        = char_count正規化 × unique_char_ratio × (1 + numeric_ratio)
    """
    print("2-1: テキスト品質分析を計算中...")

    db.execute("DROP TABLE IF EXISTS v2_text_quality")
    db.execute("""
        CREATE TABLE v2_text_quality (
            prefecture TEXT NOT NULL,
            municipality TEXT NOT NULL DEFAULT '',
            industry_raw TEXT NOT NULL DEFAULT '',
            emp_group TEXT NOT NULL,
            total_count INTEGER NOT NULL,
            avg_char_count REAL NOT NULL,
            avg_unique_char_ratio REAL NOT NULL,
            avg_kanji_ratio REAL NOT NULL,
            avg_numeric_ratio REAL NOT NULL,
            avg_punctuation_density REAL NOT NULL,
            information_score REAL NOT NULL,
            PRIMARY KEY (prefecture, municipality, industry_raw, emp_group)
        )
    """)

    # テキスト分析に必要なカラムを取得
    rows = db.execute("""
        SELECT prefecture, municipality, industry_raw, employment_type,
               job_description, headline, requirements, benefits, company_description
        FROM postings
        WHERE prefecture IS NOT NULL AND prefecture != ''
    """).fetchall()

    # 集計用辞書
    # key = (pref, muni, industry, emp_grp)
    # val = {char_counts: [], unique_ratios: [], kanji_ratios: [],
    #        numeric_ratios: [], punct_densities: []}
    stats = defaultdict(lambda: {
        "char_counts": [],
        "unique_ratios": [],
        "kanji_ratios": [],
        "numeric_ratios": [],
        "punct_densities": [],
    })

    skipped = 0
    for pref, muni, industry, et, job_desc, headline, req, ben, comp_desc in rows:
        text = _combine_text_fields(job_desc, headline, req, ben, comp_desc)
        metrics = _text_metrics(text)
        if metrics is None:
            skipped += 1
            continue

        grp = emp_group(et)
        muni = muni or ""
        industry = industry or ""

        # 3レベル集計
        for key in [
            (pref, muni, industry, grp),  # 詳細: 都道府県×市区町村×産業
            (pref, muni, "", grp),         # 中間: 都道府県×市区町村
            (pref, "", "", grp),           # 集約: 都道府県
        ]:
            if key == (pref, muni, "", grp) and not muni:
                continue  # 市区町村が空の場合、中間レベルは都道府県集約と重複
            s = stats[key]
            s["char_counts"].append(metrics["char_count"])
            s["unique_ratios"].append(metrics["unique_char_ratio"])
            s["kanji_ratios"].append(metrics["kanji_ratio"])
            s["numeric_ratios"].append(metrics["numeric_ratio"])
            s["punct_densities"].append(metrics["punctuation_density"])

    print(f"  テキスト空でスキップ: {skipped}件")

    # 情報量スコア計算のため全体のchar_count分布を取得（正規化用）
    # 都道府県集約レベルの全char_countsを使用
    all_char_counts = []
    for key, s in stats.items():
        if key[1] == "" and key[2] == "":  # 都道府県集約レベル
            all_char_counts.extend(s["char_counts"])
    if all_char_counts:
        max_char_count = max(all_char_counts) if all_char_counts else 1
    else:
        max_char_count = 1

    insert_data = []
    for (pref, muni, industry, grp), s in stats.items():
        n = len(s["char_counts"])
        if n < MIN_SAMPLE:
            continue

        avg_cc = sum(s["char_counts"]) / n
        avg_ur = sum(s["unique_ratios"]) / n
        avg_kr = sum(s["kanji_ratios"]) / n
        avg_nr = sum(s["numeric_ratios"]) / n
        avg_pd = sum(s["punct_densities"]) / n

        # 情報量スコア = char_count正規化 × unique_char_ratio × (1 + numeric_ratio)
        # char_count正規化: avg_cc / max_char_count（0-1スケール）
        cc_norm = avg_cc / max_char_count if max_char_count > 0 else 0
        info_score = cc_norm * avg_ur * (1 + avg_nr)

        insert_data.append((
            pref, muni, industry, grp, n,
            round(avg_cc, 1),
            round(avg_ur, 4),
            round(avg_kr, 4),
            round(avg_nr, 4),
            round(avg_pd, 4),
            round(info_score, 6),
        ))

    db.executemany("""
        INSERT OR REPLACE INTO v2_text_quality
        (prefecture, municipality, industry_raw, emp_group, total_count,
         avg_char_count, avg_unique_char_ratio, avg_kanji_ratio,
         avg_numeric_ratio, avg_punctuation_density, information_score)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
    """, insert_data)

    print(f"  -> {len(insert_data)}行挿入")
    return len(insert_data)


# =====================================================================
# 2-2: キーワードプロファイル
# =====================================================================
def compute_keyword_profile(db):
    """2-2: キーワードプロファイル
    求人原稿中の特定キーワードカテゴリの出現率を
    地域×産業×雇用形態別に集計。

    カテゴリ:
      - 急募系: 急募/すぐ/至急/即日/即戦力
      - 未経験系: 未経験/初心者/ブランク/経験不問/無資格
      - 待遇系: 賞与/昇給/手当/退職金/社保完備/有給
      - WLB系: 残業少/残業なし/土日休/週休2/年間休日
      - 成長系: 研修/教育/資格取得/スキルアップ/キャリア
      - 安定系: 正社員/安定/長期/定年/無期
    """
    print("2-2: キーワードプロファイルを計算中...")

    db.execute("DROP TABLE IF EXISTS v2_keyword_profile")
    db.execute("""
        CREATE TABLE v2_keyword_profile (
            prefecture TEXT NOT NULL,
            municipality TEXT NOT NULL DEFAULT '',
            industry_raw TEXT NOT NULL DEFAULT '',
            emp_group TEXT NOT NULL,
            keyword_category TEXT NOT NULL,
            total_count INTEGER NOT NULL,
            hit_count INTEGER NOT NULL,
            density REAL NOT NULL,
            avg_count_per_posting REAL NOT NULL,
            PRIMARY KEY (prefecture, municipality, industry_raw, emp_group, keyword_category)
        )
    """)

    rows = db.execute("""
        SELECT prefecture, municipality, industry_raw, employment_type,
               job_description, headline, requirements, benefits, company_description
        FROM postings
        WHERE prefecture IS NOT NULL AND prefecture != ''
    """).fetchall()

    # 集計用辞書
    # key = (pref, muni, industry, grp, category)
    # val = {total: int, hit: int, match_counts: []}
    stats = defaultdict(lambda: {"total": 0, "hit": 0, "match_counts": []})

    for pref, muni, industry, et, job_desc, headline, req, ben, comp_desc in rows:
        text = _combine_text_fields(job_desc, headline, req, ben, comp_desc)
        if not text or not text.strip():
            continue

        grp = emp_group(et)
        muni = muni or ""
        industry = industry or ""

        # 各カテゴリのマッチ数を計算
        category_matches = {}
        for cat_name, pattern in KEYWORD_CATEGORIES.items():
            matches = pattern.findall(text)
            category_matches[cat_name] = len(matches)

        # 3レベル集計
        for key_base in [
            (pref, muni, industry, grp),  # 詳細
            (pref, muni, "", grp),         # 中間
            (pref, "", "", grp),           # 集約
        ]:
            if key_base == (pref, muni, "", grp) and not muni:
                continue

            for cat_name, match_count in category_matches.items():
                key = (*key_base, cat_name)
                s = stats[key]
                s["total"] += 1
                if match_count > 0:
                    s["hit"] += 1
                s["match_counts"].append(match_count)

    insert_data = []
    for (pref, muni, industry, grp, category), s in stats.items():
        total = s["total"]
        if total < MIN_SAMPLE:
            continue

        hit = s["hit"]
        density = hit / total  # 出現率（該当求人の割合）
        avg_count = sum(s["match_counts"]) / total  # 1求人あたりの平均マッチ数

        insert_data.append((
            pref, muni, industry, grp, category, total, hit,
            round(density, 4),
            round(avg_count, 4),
        ))

    db.executemany("""
        INSERT OR REPLACE INTO v2_keyword_profile
        (prefecture, municipality, industry_raw, emp_group, keyword_category,
         total_count, hit_count, density, avg_count_per_posting)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
    """, insert_data)

    print(f"  -> {len(insert_data)}行挿入")
    return len(insert_data)


# =====================================================================
# 2-3: テキスト温度計 (L-1)
# =====================================================================
def compute_text_temperature(db):
    """2-3: テキスト温度計 (L-1)
    求人原稿の「緊急度」vs「選択度」を温度として数値化。

    温度 = (urgency_density - selectivity_density) / (urgency_density + selectivity_density + 0.01)
      - +1.0に近い: 非常に緊急（人手不足・即日募集）
      - -1.0に近い: 非常に選択的（経験者優遇・厳選採用）
      - 0.0付近: バランス型

    緊急ワード（hot）: 急募/すぐ/至急/即日/大量募集/人手不足/欠員
    選択ワード（cold）: 経験者優遇/要経験/有資格者/経験N年/選考あり
    """
    print("2-3: テキスト温度計を計算中...")

    db.execute("DROP TABLE IF EXISTS v2_text_temperature")
    db.execute("""
        CREATE TABLE v2_text_temperature (
            prefecture TEXT NOT NULL,
            municipality TEXT NOT NULL DEFAULT '',
            industry_raw TEXT NOT NULL DEFAULT '',
            emp_group TEXT NOT NULL,
            sample_count INTEGER NOT NULL,
            urgency_density REAL NOT NULL,
            selectivity_density REAL NOT NULL,
            temperature REAL NOT NULL,
            urgency_hit_rate REAL NOT NULL,
            selectivity_hit_rate REAL NOT NULL,
            PRIMARY KEY (prefecture, municipality, industry_raw, emp_group)
        )
    """)

    rows = db.execute("""
        SELECT prefecture, municipality, industry_raw, employment_type,
               job_description, headline, requirements, benefits, company_description
        FROM postings
        WHERE prefecture IS NOT NULL AND prefecture != ''
    """).fetchall()

    # 集計用辞書
    # key = (pref, muni, industry, grp)
    # val = {total: int, urgency_hits: int, selectivity_hits: int,
    #        urgency_counts: [], selectivity_counts: []}
    stats = defaultdict(lambda: {
        "total": 0,
        "urgency_hits": 0,
        "selectivity_hits": 0,
        "urgency_counts": [],
        "selectivity_counts": [],
    })

    for pref, muni, industry, et, job_desc, headline, req, ben, comp_desc in rows:
        text = _combine_text_fields(job_desc, headline, req, ben, comp_desc)
        if not text or not text.strip():
            continue

        grp = emp_group(et)
        muni = muni or ""
        industry = industry or ""

        urgency_matches = len(RE_URGENCY.findall(text))
        selectivity_matches = len(RE_SELECTIVITY.findall(text))

        # 3レベル集計
        for key in [
            (pref, muni, industry, grp),  # 詳細
            (pref, muni, "", grp),         # 中間
            (pref, "", "", grp),           # 集約
        ]:
            if key == (pref, muni, "", grp) and not muni:
                continue
            s = stats[key]
            s["total"] += 1
            if urgency_matches > 0:
                s["urgency_hits"] += 1
            if selectivity_matches > 0:
                s["selectivity_hits"] += 1
            s["urgency_counts"].append(urgency_matches)
            s["selectivity_counts"].append(selectivity_matches)

    insert_data = []
    for (pref, muni, industry, grp), s in stats.items():
        total = s["total"]
        if total < MIN_SAMPLE:
            continue

        # density = 1求人あたりの平均マッチ数
        urgency_density = sum(s["urgency_counts"]) / total
        selectivity_density = sum(s["selectivity_counts"]) / total

        # 温度計算: -1.0（選択的）〜 +1.0（緊急）
        denominator = urgency_density + selectivity_density + 0.01
        temperature = (urgency_density - selectivity_density) / denominator

        # ヒット率（該当求人の割合）
        urgency_hit_rate = s["urgency_hits"] / total
        selectivity_hit_rate = s["selectivity_hits"] / total

        insert_data.append((
            pref, muni, industry, grp, total,
            round(urgency_density, 4),
            round(selectivity_density, 4),
            round(temperature, 4),
            round(urgency_hit_rate, 4),
            round(selectivity_hit_rate, 4),
        ))

    db.executemany("""
        INSERT OR REPLACE INTO v2_text_temperature
        (prefecture, municipality, industry_raw, emp_group, sample_count,
         urgency_density, selectivity_density, temperature,
         urgency_hit_rate, selectivity_hit_rate)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
    """, insert_data)

    print(f"  -> {len(insert_data)}行挿入")
    return len(insert_data)


# =====================================================================
# 検証
# =====================================================================
def verify(db):
    """計算結果の検証: テーブル行数 + 東京都サンプル"""
    print("\n=== 検証 ===")
    for table in ["v2_text_quality", "v2_keyword_profile", "v2_text_temperature"]:
        cnt = db.execute(f"SELECT COUNT(*) FROM {table}").fetchone()[0]
        print(f"  {table}: {cnt}行")

    # 2-1: テキスト品質サンプル（東京都集計）
    print("\n  2-1 テキスト品質サンプル（東京都集計）:")
    for r in db.execute("""
        SELECT emp_group, total_count, avg_char_count, avg_unique_char_ratio,
               avg_kanji_ratio, avg_numeric_ratio, avg_punctuation_density,
               information_score
        FROM v2_text_quality
        WHERE prefecture='東京都' AND municipality='' AND industry_raw=''
        ORDER BY emp_group
    """):
        print(f"    {r[0]}: 件数={r[1]}, 平均文字数={r[2]:.0f}, "
              f"ユニーク率={r[3]:.3f}, 漢字率={r[4]:.3f}, "
              f"数字率={r[5]:.3f}, 句読点={r[6]:.4f}, "
              f"情報量={r[7]:.5f}")

    # 2-2: キーワードプロファイルサンプル（東京都・正社員）
    print("\n  2-2 キーワードプロファイルサンプル（東京都・正社員）:")
    for r in db.execute("""
        SELECT keyword_category, total_count, hit_count, density, avg_count_per_posting
        FROM v2_keyword_profile
        WHERE prefecture='東京都' AND municipality='' AND industry_raw=''
          AND emp_group='正社員'
        ORDER BY density DESC
    """):
        print(f"    {r[0]}: 件数={r[1]}, ヒット={r[2]}, "
              f"出現率={r[3]:.1%}, 平均回数={r[4]:.2f}")

    # 2-3: テキスト温度計サンプル（東京都集計）
    print("\n  2-3 テキスト温度計サンプル（東京都集計）:")
    for r in db.execute("""
        SELECT emp_group, sample_count, urgency_density, selectivity_density,
               temperature, urgency_hit_rate, selectivity_hit_rate
        FROM v2_text_temperature
        WHERE prefecture='東京都' AND municipality='' AND industry_raw=''
        ORDER BY emp_group
    """):
        # 温度の解釈ラベル
        temp = r[4]
        if temp > 0.3:
            label = "緊急寄り"
        elif temp < -0.3:
            label = "選択寄り"
        else:
            label = "バランス"
        print(f"    {r[0]}: 件数={r[1]}, "
              f"緊急密度={r[2]:.3f}, 選択密度={r[3]:.3f}, "
              f"温度={r[4]:+.3f}({label}), "
              f"緊急率={r[5]:.1%}, 選択率={r[6]:.1%}")


# =====================================================================
# メイン
# =====================================================================
def main():
    sys.stdout.reconfigure(encoding="utf-8")

    if not os.path.exists(DB_PATH):
        print(f"エラー: DBが見つかりません: {DB_PATH}")
        sys.exit(1)

    print(f"DB: {DB_PATH}")

    db = sqlite3.connect(DB_PATH)
    db.execute("PRAGMA journal_mode=WAL")
    db.execute("PRAGMA synchronous=NORMAL")

    try:
        tq = compute_text_quality(db)
        db.commit()

        kp = compute_keyword_profile(db)
        db.commit()

        tt = compute_text_temperature(db)
        db.commit()

        # インデックス作成
        print("\nインデックスを作成中...")
        db.execute(
            "CREATE INDEX IF NOT EXISTS idx_text_quality_pref "
            "ON v2_text_quality(prefecture, emp_group)"
        )
        db.execute(
            "CREATE INDEX IF NOT EXISTS idx_keyword_profile_pref "
            "ON v2_keyword_profile(prefecture, emp_group)"
        )
        db.execute(
            "CREATE INDEX IF NOT EXISTS idx_text_temperature_pref "
            "ON v2_text_temperature(prefecture, emp_group)"
        )
        db.commit()

        verify(db)
        print(f"\n完了: 2-1={tq}, 2-2={kp}, 2-3={tt}")

    except Exception as e:
        db.rollback()
        print(f"エラー: {e}")
        raise
    finally:
        db.close()


if __name__ == "__main__":
    main()
