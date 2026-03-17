"""
Phase B: 外部CSVデータ → hellowork.db インポートスクリプト
==========================================================
市区町村レベルの外部データ（人口、社会動態、外国人、昼夜間人口）を
CSVからSQLiteにインポートする。

使い方:
    python import_external_csv.py [--data-dir DATA_DIR]

data-dirにある以下のCSVを処理:
  - population_by_age_sex.csv   → v2_external_population + v2_external_population_pyramid
  - migration_stats.csv         → v2_external_migration
  - foreign_residents.csv       → v2_external_foreign_residents
  - daytime_population.csv      → v2_external_daytime_population

CSVフォーマット仕様は末尾のコメント参照。
"""
import sqlite3
import csv
import os
import sys
import argparse

SCRIPT_DIR = os.path.dirname(os.path.abspath(__file__))
DB_PATH = os.path.join(os.path.dirname(SCRIPT_DIR), "data", "hellowork.db")
DEFAULT_DATA_DIR = os.path.join(SCRIPT_DIR, "data")

# 5歳階級のラベル一覧
AGE_GROUPS = [
    "0-4", "5-9", "10-14", "15-19", "20-24", "25-29", "30-34",
    "35-39", "40-44", "45-49", "50-54", "55-59", "60-64",
    "65-69", "70-74", "75-79", "80-84", "85+",
]


def safe_int(val, default=0):
    """数値変換（カンマ区切り対応）"""
    if val is None or val == "" or val == "-" or val == "…":
        return default
    try:
        return int(str(val).replace(",", "").replace(" ", ""))
    except (ValueError, TypeError):
        return default


def safe_float(val, default=None):
    if val is None or val == "" or val == "-" or val == "…":
        return default
    try:
        return float(str(val).replace(",", "").replace(" ", ""))
    except (ValueError, TypeError):
        return default


def import_population(db, csv_path):
    """人口データ（年齢5歳階級×男女）をインポート

    CSV必須カラム:
        prefecture, municipality, total_population, male_population, female_population,
        age_0_4_m, age_0_4_f, age_5_9_m, age_5_9_f, ..., age_85plus_m, age_85plus_f,
        reference_date
    """
    if not os.path.exists(csv_path):
        print(f"  ⚠ {csv_path} が見つかりません → スキップ")
        return

    print(f"  人口データをインポート中: {csv_path}")

    db.execute("DROP TABLE IF EXISTS v2_external_population")
    db.execute("""
        CREATE TABLE v2_external_population (
            prefecture TEXT,
            municipality TEXT,
            total_population INTEGER,
            male_population INTEGER,
            female_population INTEGER,
            age_0_14 INTEGER,
            age_15_64 INTEGER,
            age_65_over INTEGER,
            aging_rate REAL,
            working_age_rate REAL,
            youth_rate REAL,
            reference_date TEXT,
            PRIMARY KEY (prefecture, municipality)
        )
    """)

    db.execute("DROP TABLE IF EXISTS v2_external_population_pyramid")
    db.execute("""
        CREATE TABLE v2_external_population_pyramid (
            prefecture TEXT,
            municipality TEXT,
            age_group TEXT,
            male_count INTEGER,
            female_count INTEGER,
            PRIMARY KEY (prefecture, municipality, age_group)
        )
    """)

    pop_rows = []
    pyramid_rows = []

    with open(csv_path, encoding="utf-8-sig") as f:
        reader = csv.DictReader(f)
        for row in reader:
            pref = row.get("prefecture", "").strip()
            muni = row.get("municipality", "").strip()
            if not pref:
                continue

            total = safe_int(row.get("total_population"))
            male = safe_int(row.get("male_population"))
            female = safe_int(row.get("female_population"))
            ref_date = row.get("reference_date", "2025-01-01").strip()

            # 5歳階級データの集計
            age_0_14 = 0
            age_15_64 = 0
            age_65_over = 0

            for ag in AGE_GROUPS:
                ag_key = ag.replace("-", "_").replace("+", "plus")
                m_count = safe_int(row.get(f"age_{ag_key}_m"))
                f_count = safe_int(row.get(f"age_{ag_key}_f"))

                pyramid_rows.append((pref, muni, ag, m_count, f_count))

                group_total = m_count + f_count
                # 年齢3区分への集約
                age_num = int(ag.split("-")[0].replace("+", ""))
                if age_num < 15:
                    age_0_14 += group_total
                elif age_num < 65:
                    age_15_64 += group_total
                else:
                    age_65_over += group_total

            if total == 0:
                total = age_0_14 + age_15_64 + age_65_over

            aging_rate = age_65_over / total * 100 if total > 0 else None
            working_age_rate = age_15_64 / total * 100 if total > 0 else None
            youth_rate = age_0_14 / total * 100 if total > 0 else None

            pop_rows.append((
                pref, muni, total, male, female,
                age_0_14, age_15_64, age_65_over,
                aging_rate, working_age_rate, youth_rate, ref_date,
            ))

    db.executemany("INSERT OR REPLACE INTO v2_external_population VALUES (?,?,?,?,?,?,?,?,?,?,?,?)", pop_rows)
    db.executemany("INSERT OR REPLACE INTO v2_external_population_pyramid VALUES (?,?,?,?,?)", pyramid_rows)
    db.execute("CREATE INDEX IF NOT EXISTS idx_ext_pop_pref ON v2_external_population(prefecture)")
    db.execute("CREATE INDEX IF NOT EXISTS idx_ext_pyramid_pref ON v2_external_population_pyramid(prefecture, municipality)")

    print(f"    → v2_external_population: {len(pop_rows)} 行")
    print(f"    → v2_external_population_pyramid: {len(pyramid_rows)} 行")


def import_migration(db, csv_path):
    """社会動態（転入転出）データをインポート

    CSV必須カラム:
        prefecture, municipality, inflow, outflow, reference_year
    """
    if not os.path.exists(csv_path):
        print(f"  ⚠ {csv_path} が見つかりません → スキップ")
        return

    print(f"  社会動態データをインポート中: {csv_path}")

    db.execute("DROP TABLE IF EXISTS v2_external_migration")
    db.execute("""
        CREATE TABLE v2_external_migration (
            prefecture TEXT,
            municipality TEXT,
            inflow INTEGER,
            outflow INTEGER,
            net_migration INTEGER,
            net_migration_rate REAL,
            reference_year INTEGER,
            PRIMARY KEY (prefecture, municipality)
        )
    """)

    rows = []
    # 人口テーブルから分母を取得
    pop_map = {}
    try:
        for r in db.execute("SELECT prefecture, municipality, total_population FROM v2_external_population").fetchall():
            pop_map[(r[0], r[1])] = r[2]
    except Exception:
        pass

    with open(csv_path, encoding="utf-8-sig") as f:
        reader = csv.DictReader(f)
        for row in reader:
            pref = row.get("prefecture", "").strip()
            muni = row.get("municipality", "").strip()
            if not pref:
                continue

            inflow = safe_int(row.get("inflow"))
            outflow = safe_int(row.get("outflow"))
            net = inflow - outflow
            ref_year = safe_int(row.get("reference_year", "2024"))

            pop = pop_map.get((pref, muni), 0)
            rate = net / pop * 1000 if pop > 0 else None  # ‰

            rows.append((pref, muni, inflow, outflow, net, rate, ref_year))

    db.executemany("INSERT OR REPLACE INTO v2_external_migration VALUES (?,?,?,?,?,?,?)", rows)
    db.execute("CREATE INDEX IF NOT EXISTS idx_ext_mig_pref ON v2_external_migration(prefecture)")
    print(f"    → v2_external_migration: {len(rows)} 行")


def import_foreign_residents(db, csv_path):
    """外国人住民数データをインポート

    CSV必須カラム:
        prefecture, municipality, total_foreign, reference_date
    """
    if not os.path.exists(csv_path):
        print(f"  ⚠ {csv_path} が見つかりません → スキップ")
        return

    print(f"  外国人住民データをインポート中: {csv_path}")

    db.execute("DROP TABLE IF EXISTS v2_external_foreign_residents")
    db.execute("""
        CREATE TABLE v2_external_foreign_residents (
            prefecture TEXT,
            municipality TEXT,
            total_foreign INTEGER,
            foreign_rate REAL,
            reference_date TEXT,
            PRIMARY KEY (prefecture, municipality)
        )
    """)

    rows = []
    pop_map = {}
    try:
        for r in db.execute("SELECT prefecture, municipality, total_population FROM v2_external_population").fetchall():
            pop_map[(r[0], r[1])] = r[2]
    except Exception:
        pass

    with open(csv_path, encoding="utf-8-sig") as f:
        reader = csv.DictReader(f)
        for row in reader:
            pref = row.get("prefecture", "").strip()
            muni = row.get("municipality", "").strip()
            if not pref:
                continue

            total_foreign = safe_int(row.get("total_foreign"))
            ref_date = row.get("reference_date", "2024-12-01").strip()

            pop = pop_map.get((pref, muni), 0)
            rate = total_foreign / pop * 100 if pop > 0 else None

            rows.append((pref, muni, total_foreign, rate, ref_date))

    db.executemany("INSERT OR REPLACE INTO v2_external_foreign_residents VALUES (?,?,?,?,?)", rows)
    db.execute("CREATE INDEX IF NOT EXISTS idx_ext_foreign_pref ON v2_external_foreign_residents(prefecture)")
    print(f"    → v2_external_foreign_residents: {len(rows)} 行")


def import_daytime_population(db, csv_path):
    """昼夜間人口データをインポート

    CSV必須カラム:
        prefecture, municipality, nighttime_pop, daytime_pop, reference_year
    """
    if not os.path.exists(csv_path):
        print(f"  ⚠ {csv_path} が見つかりません → スキップ")
        return

    print(f"  昼夜間人口データをインポート中: {csv_path}")

    db.execute("DROP TABLE IF EXISTS v2_external_daytime_population")
    db.execute("""
        CREATE TABLE v2_external_daytime_population (
            prefecture TEXT,
            municipality TEXT,
            nighttime_pop INTEGER,
            daytime_pop INTEGER,
            day_night_ratio REAL,
            inflow_pop INTEGER,
            outflow_pop INTEGER,
            reference_year INTEGER,
            PRIMARY KEY (prefecture, municipality)
        )
    """)

    rows = []
    with open(csv_path, encoding="utf-8-sig") as f:
        reader = csv.DictReader(f)
        for row in reader:
            pref = row.get("prefecture", "").strip()
            muni = row.get("municipality", "").strip()
            if not pref:
                continue

            nighttime = safe_int(row.get("nighttime_pop"))
            daytime = safe_int(row.get("daytime_pop"))
            ref_year = safe_int(row.get("reference_year", "2020"))

            ratio = daytime / nighttime * 100 if nighttime > 0 else None
            inflow = safe_int(row.get("inflow_pop", 0))
            outflow = safe_int(row.get("outflow_pop", 0))

            rows.append((pref, muni, nighttime, daytime, ratio, inflow, outflow, ref_year))

    db.executemany("INSERT OR REPLACE INTO v2_external_daytime_population VALUES (?,?,?,?,?,?,?,?)", rows)
    db.execute("CREATE INDEX IF NOT EXISTS idx_ext_daytime_pref ON v2_external_daytime_population(prefecture)")
    print(f"    → v2_external_daytime_population: {len(rows)} 行")


def verify(db):
    """インポート検証"""
    print("\n=== 検証 ===")
    tables = [
        "v2_external_population",
        "v2_external_population_pyramid",
        "v2_external_migration",
        "v2_external_foreign_residents",
        "v2_external_daytime_population",
    ]
    for table in tables:
        try:
            count = db.execute(f"SELECT COUNT(*) FROM {table}").fetchone()[0]
            print(f"  {table}: {count} 行")
        except Exception:
            print(f"  {table}: 未作成")

    # 東京都サンプル
    try:
        row = db.execute("""
            SELECT total_population, aging_rate, working_age_rate
            FROM v2_external_population WHERE prefecture = '東京都' AND municipality = ''
        """).fetchone()
        if row:
            print(f"\n  東京都: 人口{row[0]:,}人, 高齢化率{row[1]:.1f}%, 生産年齢{row[2]:.1f}%")
    except Exception:
        pass

    try:
        row = db.execute("""
            SELECT net_migration, net_migration_rate
            FROM v2_external_migration WHERE prefecture = '東京都' AND municipality = ''
        """).fetchone()
        if row:
            print(f"  東京都: 社会増減{row[0]:+,}人 ({row[1]:+.1f}‰)")
    except Exception:
        pass


def main():
    parser = argparse.ArgumentParser(description="外部CSVデータ → hellowork.db インポート")
    parser.add_argument("--data-dir", default=DEFAULT_DATA_DIR, help="CSVファイルのディレクトリ")
    args = parser.parse_args()

    if not os.path.exists(DB_PATH):
        print(f"Error: DB not found at {DB_PATH}")
        sys.exit(1)

    data_dir = args.data_dir
    print(f"DB: {DB_PATH}")
    print(f"Data dir: {data_dir}")

    db = sqlite3.connect(DB_PATH)
    db.execute("PRAGMA journal_mode=WAL")

    try:
        # 人口を最初にインポート（他テーブルが分母として使う）
        import_population(db, os.path.join(data_dir, "population_by_age_sex.csv"))
        db.commit()

        import_migration(db, os.path.join(data_dir, "migration_stats.csv"))
        db.commit()

        import_foreign_residents(db, os.path.join(data_dir, "foreign_residents.csv"))
        db.commit()

        import_daytime_population(db, os.path.join(data_dir, "daytime_population.csv"))
        db.commit()

        verify(db)
        print("\n外部CSVインポート完了")
        print("※ ベンチマーク12軸化にはcompute_v2_external.pyの再実行が必要です")

    except Exception as e:
        db.rollback()
        print(f"Error: {e}")
        raise
    finally:
        db.close()


if __name__ == "__main__":
    main()


# ============================================================
# CSVフォーマット仕様
# ============================================================
#
# 1. population_by_age_sex.csv
#    カラム: prefecture, municipality, total_population, male_population, female_population,
#            age_0_4_m, age_0_4_f, age_5_9_m, age_5_9_f, ..., age_85plus_m, age_85plus_f,
#            reference_date
#    例: 東京都,千代田区,67803,35210,32593,1420,1350,1380,1310,...,2100,3200,2025-01-01
#
# 2. migration_stats.csv
#    カラム: prefecture, municipality, inflow, outflow, reference_year
#    例: 東京都,千代田区,5234,4891,2024
#
# 3. foreign_residents.csv
#    カラム: prefecture, municipality, total_foreign, reference_date
#    例: 東京都,千代田区,3456,2024-12-01
#
# 4. daytime_population.csv
#    カラム: prefecture, municipality, nighttime_pop, daytime_pop, inflow_pop, outflow_pop, reference_year
#    例: 東京都,千代田区,67803,853068,812345,27080,2020
