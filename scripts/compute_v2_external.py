"""
V2独自分析: Phase 4 外部データ統合 事前計算スクリプト
====================================================
4-1: 有効求人倍率（e-Stat API - 要APIキー → スタブ）
4-2: 賃金構造基本統計（e-Stat API - 要APIキー → スタブ）
4-3: 人口データ（国土交通DPF GraphQL → スタブ）
4-4: 最低賃金マスタ（2024年度データ埋め込み）
4-5b: 最低賃金違反チェック（時給求人×最低賃金クロス集計）
4-6: 地域間ベンチマーク（既存v2テーブル集約→レーダーチャート用）

APIキー不要の 4-4, 4-5b, 4-6 のみ実装。
全指標は employment_type（正社員/パート/その他）でセグメント化。
"""
import sqlite3
import os
import sys
from collections import defaultdict

DB_PATH = os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))), "data", "hellowork.db")

MIN_SAMPLE = 5  # 最小サンプルサイズ

# 2024年度 地域別最低賃金（時間額・円）
# 施行日: 2024-10-01（全都道府県1,000円超え達成年）
MINIMUM_WAGE_2024 = {
    "北海道": 1010, "青森県": 1000, "岩手県": 1000, "宮城県": 1004,
    "秋田県": 1000, "山形県": 1000, "福島県": 1000, "茨城県": 1005,
    "栃木県": 1004, "群馬県": 1004, "埼玉県": 1078, "千葉県": 1076,
    "東京都": 1163, "神奈川県": 1112, "新潟県": 1000, "富山県": 1000,
    "石川県": 1001, "福井県": 1000, "山梨県": 1000, "長野県": 1002,
    "岐阜県": 1001, "静岡県": 1034, "愛知県": 1077, "三重県": 1023,
    "滋賀県": 1017, "京都府": 1058, "大阪府": 1114, "兵庫県": 1052,
    "奈良県": 1000, "和歌山県": 1000, "鳥取県": 1000, "島根県": 1000,
    "岡山県": 1010, "広島県": 1020, "山口県": 1000, "徳島県": 1000,
    "香川県": 1000, "愛媛県": 1000, "高知県": 1000, "福岡県": 1015,
    "佐賀県": 1000, "長崎県": 1000, "熊本県": 1000, "大分県": 1000,
    "宮崎県": 1000, "鹿児島県": 1000, "沖縄県": 1000,
}


def emp_group(et):
    """雇用形態を3グループに分類"""
    if et is None:
        return "その他"
    if "パート" in et:
        return "パート"
    if et == "正社員":
        return "正社員"
    return "その他"


def table_exists(db, table_name):
    """テーブルが存在するか確認"""
    row = db.execute(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?",
        (table_name,)
    ).fetchone()
    return row[0] > 0


# ============================================================
# 4-1, 4-2, 4-3: APIキー依存（スタブ）
# ============================================================

def compute_job_ratio_estat(db):
    """4-1: 有効求人倍率（e-Stat API - 要APIキー）"""
    api_key = os.environ.get("ESTAT_APP_ID")
    if not api_key:
        print("4-1: ESTAT_APP_ID未設定 → スキップ（手動実行時に設定してください）")
        return
    # TODO: implement
    print("4-1: 有効求人倍率 → 未実装（APIキー設定後に実装）")


def compute_wage_structure_estat(db):
    """4-2: 賃金構造基本統計（e-Stat API - 要APIキー）"""
    print("4-2: 賃金構造基本統計 → 未実装（APIキー設定後に実装）")


def compute_population_data(db):
    """4-3: 人口データ（国土交通DPF GraphQL）"""
    print("4-3: 人口データ → 未実装（DPF API実装予定）")


# ============================================================
# 4-4: 最低賃金マスタ
# ============================================================

def compute_minimum_wage(db):
    """4-4: 最低賃金マスタ
    2024年度の都道府県別最低賃金をテーブルに格納。
    外部ファイル不要（コード内にデータ埋め込み）。
    """
    print("4-4: 最低賃金マスタを作成中...")

    db.execute("DROP TABLE IF EXISTS v2_external_minimum_wage")
    db.execute("""
        CREATE TABLE v2_external_minimum_wage (
            prefecture TEXT NOT NULL PRIMARY KEY,
            hourly_min_wage INTEGER NOT NULL,
            effective_date TEXT NOT NULL DEFAULT '2024-10-01',
            fiscal_year INTEGER NOT NULL DEFAULT 2024
        )
    """)

    insert_rows = [
        (pref, wage, "2024-10-01", 2024)
        for pref, wage in MINIMUM_WAGE_2024.items()
    ]

    db.executemany("""
        INSERT OR REPLACE INTO v2_external_minimum_wage
        VALUES (?, ?, ?, ?)
    """, insert_rows)

    print(f"  → {len(insert_rows)} 都道府県の最低賃金を挿入")

    # 最高・最低を表示
    highest = max(MINIMUM_WAGE_2024.items(), key=lambda x: x[1])
    lowest = min(MINIMUM_WAGE_2024.items(), key=lambda x: x[1])
    avg_wage = sum(MINIMUM_WAGE_2024.values()) / len(MINIMUM_WAGE_2024)
    print(f"  最高: {highest[0]} {highest[1]}円 / 最低: {lowest[0]} {lowest[1]}円 / 平均: {avg_wage:.0f}円")


# ============================================================
# 4-5b: 最低賃金違反チェック
# ============================================================

def compute_wage_compliance(db):
    """4-5b: 最低賃金違反チェック
    時給求人（salary_type='時給'）の salary_min を最低賃金と照合し、
    違反率を都道府県×市区町村×雇用形態別に集計。
    """
    print("4-5b: 最低賃金違反チェックを計算中...")

    # 最低賃金テーブルが存在するか確認
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
            # 最低賃金データがない都道府県はスキップ
            continue

        below = sum(1 for w in wages if w < mw)
        below_rate = below / n

        sorted_wages = sorted(wages)
        avg_wage = sum(wages) / n
        # 中央値
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
    total_postings = sum(r[3] for r in insert_rows)
    total_below = sum(r[5] for r in insert_rows)
    # 都道府県レベルのみで集計（重複カウント回避）
    pref_rows = [r for r in insert_rows if r[1] == ""]
    pref_postings = sum(r[3] for r in pref_rows)
    pref_below = sum(r[5] for r in pref_rows)
    if pref_postings > 0:
        print(f"  全国 時給求人: {pref_postings:,}件, 最低賃金未満: {pref_below:,}件 ({pref_below/pref_postings*100:.1f}%)")

    # 違反率ワースト3（都道府県レベル、件数10以上）
    worst = sorted(
        [r for r in pref_rows if r[3] >= 10],
        key=lambda r: r[6],  # below_min_rate
        reverse=True,
    )[:3]
    if worst:
        print("  違反率ワースト3:")
        for r in worst:
            print(f"    {r[0]}: {r[6]*100:.1f}% ({r[5]}/{r[3]}件)")


# ============================================================
# 4-6: 地域間ベンチマーク
# ============================================================

def compute_region_benchmark(db):
    """4-6: 地域間ベンチマーク
    既存v2テーブルから6軸の指標を統合し、レーダーチャート用のベンチマークを作成。
    ソーステーブルが存在しない場合はその軸をスキップ。
    """
    print("4-6: 地域間ベンチマークを計算中...")

    db.execute("DROP TABLE IF EXISTS v2_region_benchmark")
    db.execute("""
        CREATE TABLE v2_region_benchmark (
            prefecture TEXT NOT NULL,
            municipality TEXT NOT NULL DEFAULT '',
            emp_group TEXT NOT NULL,
            posting_activity REAL,
            salary_competitiveness REAL,
            talent_retention REAL,
            industry_diversity REAL,
            info_transparency REAL,
            text_temperature REAL,
            composite_benchmark REAL,
            PRIMARY KEY (prefecture, municipality, emp_group)
        )
    """)

    # 各ソーステーブルからデータを辞書に読み込む
    # キー: (pref, muni, emp_grp) → 値

    # --- 1. posting_activity: v2_vacancy_rate から ---
    # (1 - vacancy_rate) * 100 → 欠員補充でない=活発な採用活動
    activity_data = {}
    if table_exists(db, "v2_vacancy_rate"):
        for row in db.execute("""
            SELECT prefecture, municipality, emp_group, vacancy_rate
            FROM v2_vacancy_rate
            WHERE industry_raw = ''
        """).fetchall():
            key = (row[0], row[1] or "", row[2])
            activity_data[key] = (1.0 - row[3]) * 100
        print(f"  posting_activity: {len(activity_data)} 件ロード")
    else:
        print("  posting_activity: v2_vacancy_rate テーブル未検出 → スキップ")

    # --- 2. salary_competitiveness: v2_salary_competitiveness から ---
    # competitiveness_index をそのまま使用（全国平均比%）
    # 0-100スケールに変換: index は -30〜+30程度 → 50 + index でクリップ
    salary_data = {}
    if table_exists(db, "v2_salary_competitiveness"):
        for row in db.execute("""
            SELECT prefecture, municipality, emp_group, competitiveness_index
            FROM v2_salary_competitiveness
            WHERE industry_raw = ''
        """).fetchall():
            key = (row[0], row[1] or "", row[2])
            # -50〜+50の範囲を0〜100にマッピング
            val = max(0.0, min(100.0, 50.0 + row[3]))
            salary_data[key] = val
        print(f"  salary_competitiveness: {len(salary_data)} 件ロード")
    else:
        print("  salary_competitiveness: v2_salary_competitiveness テーブル未検出 → スキップ")

    # --- 3. talent_retention: v2_vacancy_rate から ---
    # (1 - vacancy_rate) * 100 → 欠員率が低い=人材定着良好
    # posting_activity と同じソースだが意味が異なる
    retention_data = {}
    if table_exists(db, "v2_vacancy_rate"):
        for row in db.execute("""
            SELECT prefecture, municipality, emp_group, vacancy_rate
            FROM v2_vacancy_rate
            WHERE industry_raw = ''
        """).fetchall():
            key = (row[0], row[1] or "", row[2])
            retention_data[key] = (1.0 - row[3]) * 100
        print(f"  talent_retention: {len(retention_data)} 件ロード")
    else:
        print("  talent_retention: v2_vacancy_rate テーブル未検出 → スキップ")

    # --- 4. industry_diversity: v2_regional_resilience から ---
    # diversity_index (Shannon) * 100 → 0〜100スケール
    # Shannon指数は通常0〜3程度 → *100/3でクリップ
    diversity_data = {}
    if table_exists(db, "v2_regional_resilience"):
        for row in db.execute("""
            SELECT prefecture, municipality, emp_group, diversity_index
            FROM v2_regional_resilience
        """).fetchall():
            key = (row[0], row[1] or "", row[2])
            # Shannon指数を0-100に正規化（最大値を3.0と仮定）
            val = max(0.0, min(100.0, row[3] / 3.0 * 100))
            diversity_data[key] = val
        print(f"  industry_diversity: {len(diversity_data)} 件ロード")
    else:
        print("  industry_diversity: v2_regional_resilience テーブル未検出 → スキップ")

    # --- 5. info_transparency: v2_transparency_score から ---
    # avg_transparency * 100 → 0〜100スケール
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

    # --- 6. text_temperature: v2_text_temperature から ---
    # avg_temperature を 0-100 に正規化
    # temperatureは -1〜+1 の範囲 → (val + 1) / 2 * 100
    temperature_data = {}
    if table_exists(db, "v2_text_temperature"):
        for row in db.execute("""
            SELECT prefecture, municipality, emp_group, avg_temperature
            FROM v2_text_temperature
            WHERE industry_raw = ''
        """).fetchall():
            key = (row[0], row[1] or "", row[2])
            # -1〜+1 を 0〜100にマッピング
            val = max(0.0, min(100.0, (row[3] + 1.0) / 2.0 * 100))
            temperature_data[key] = val
        print(f"  text_temperature: {len(temperature_data)} 件ロード")
    else:
        print("  text_temperature: v2_text_temperature テーブル未検出 → スキップ")

    # 全キーを収集
    all_keys = set()
    for d in [activity_data, salary_data, retention_data,
              diversity_data, transparency_data, temperature_data]:
        all_keys.update(d.keys())

    if not all_keys:
        print("  → ソーステーブルが1つも存在しない → スキップ")
        return

    insert_rows = []
    for key in all_keys:
        pref, muni, grp = key

        activity = activity_data.get(key)
        salary = salary_data.get(key)
        retention = retention_data.get(key)
        diversity = diversity_data.get(key)
        transparency = transparency_data.get(key)
        temperature = temperature_data.get(key)

        # 非NULLの軸の平均でcomposite算出
        axes = [v for v in [activity, salary, retention,
                            diversity, transparency, temperature]
                if v is not None]

        if not axes:
            continue

        composite = sum(axes) / len(axes)

        insert_rows.append((
            pref, muni, grp,
            activity, salary, retention,
            diversity, transparency, temperature,
            composite,
        ))

    db.executemany("""
        INSERT OR REPLACE INTO v2_region_benchmark
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
    """, insert_rows)

    print(f"  → {len(insert_rows)} 行を挿入")


# ============================================================
# 検証
# ============================================================

def verify(db):
    """検証: テーブル行数とサンプル値を確認"""
    print("\n=== 検証 ===")

    tables = [
        "v2_external_minimum_wage",
        "v2_wage_compliance",
        "v2_region_benchmark",
    ]

    for table in tables:
        if table_exists(db, table):
            count = db.execute(f"SELECT COUNT(*) FROM {table}").fetchone()[0]
            print(f"  {table}: {count} 行")
        else:
            print(f"  {table}: 未作成")

    # 最低賃金サンプル: 東京都
    if table_exists(db, "v2_external_minimum_wage"):
        row = db.execute("""
            SELECT hourly_min_wage, effective_date
            FROM v2_external_minimum_wage
            WHERE prefecture = '東京都'
        """).fetchone()
        if row:
            print(f"\n  東京都 最低賃金: {row[0]}円（{row[1]}施行）")

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
            SELECT posting_activity, salary_competitiveness, talent_retention,
                   industry_diversity, info_transparency, text_temperature,
                   composite_benchmark
            FROM v2_region_benchmark
            WHERE prefecture = '東京都' AND municipality = '' AND emp_group = '正社員'
        """).fetchone()
        if row:
            print(f"\n  東京都 正社員 地域ベンチマーク:")
            labels = ["採用活発度", "給与競争力", "人材定着度",
                      "産業多様性", "情報透明性", "テキスト温度"]
            for label, val in zip(labels, row[:6]):
                if val is not None:
                    print(f"    {label}: {val:.1f}")
                else:
                    print(f"    {label}: データなし")
            print(f"    総合ベンチマーク: {row[6]:.1f}")


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
        # APIキー依存（スタブ表示のみ）
        compute_job_ratio_estat(db)
        compute_wage_structure_estat(db)
        compute_population_data(db)

        print()

        # 4-4: 最低賃金マスタ
        compute_minimum_wage(db)
        db.commit()

        # 4-5b: 最低賃金違反チェック
        compute_wage_compliance(db)
        db.commit()

        # 4-6: 地域間ベンチマーク
        compute_region_benchmark(db)
        db.commit()

        # インデックス作成
        db.execute("CREATE INDEX IF NOT EXISTS idx_wage_compliance_pref ON v2_wage_compliance(prefecture, emp_group)")
        db.execute("CREATE INDEX IF NOT EXISTS idx_region_bench_pref ON v2_region_benchmark(prefecture, emp_group)")
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
