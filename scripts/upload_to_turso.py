"""
ローカル hellowork.db → Turso (country-statistics) アップロード
================================================================
外部統計データ6テーブルをTursoにアップロードする。

使い方:
    python upload_to_turso.py [--db PATH] [--dry-run]

環境変数:
    TURSO_EXTERNAL_URL   Turso DB URL (https://...)
    TURSO_EXTERNAL_TOKEN Turso Auth Token
"""
import sqlite3
import requests
import json
import os
import sys
import argparse
import time

SCRIPT_DIR = os.path.dirname(os.path.abspath(__file__))
DEFAULT_DB = os.path.join(os.path.dirname(SCRIPT_DIR), "data", "hellowork.db")

# アップロード対象テーブル
TABLES = [
    "v2_external_population",
    "v2_external_migration",
    "v2_external_foreign_residents",
    "v2_external_daytime_population",
    "v2_external_population_pyramid",
    "v2_external_prefecture_stats",
    "v2_external_job_openings_ratio",
    "v2_external_labor_stats",
    "v2_external_establishments",
    "v2_external_turnover",
    "v2_external_household_spending",
    "v2_external_business_dynamics",
    "v2_external_climate",
    "v2_external_care_demand",
]

# テーブル定義（CREATE TABLE文）
TABLE_SCHEMAS = {
    "v2_external_population": """
        CREATE TABLE IF NOT EXISTS v2_external_population (
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
    """,
    "v2_external_migration": """
        CREATE TABLE IF NOT EXISTS v2_external_migration (
            prefecture TEXT,
            municipality TEXT,
            inflow INTEGER,
            outflow INTEGER,
            net_migration INTEGER,
            net_migration_rate REAL,
            reference_year INTEGER,
            PRIMARY KEY (prefecture, municipality)
        )
    """,
    "v2_external_foreign_residents": """
        CREATE TABLE IF NOT EXISTS v2_external_foreign_residents (
            prefecture TEXT,
            municipality TEXT,
            total_foreign INTEGER,
            foreign_rate REAL,
            reference_date TEXT,
            PRIMARY KEY (prefecture, municipality)
        )
    """,
    "v2_external_daytime_population": """
        CREATE TABLE IF NOT EXISTS v2_external_daytime_population (
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
    """,
    "v2_external_population_pyramid": """
        CREATE TABLE IF NOT EXISTS v2_external_population_pyramid (
            prefecture TEXT,
            municipality TEXT,
            age_group TEXT,
            male_count INTEGER,
            female_count INTEGER,
            PRIMARY KEY (prefecture, municipality, age_group)
        )
    """,
    "v2_external_prefecture_stats": """
        CREATE TABLE IF NOT EXISTS v2_external_prefecture_stats (
            prefecture TEXT PRIMARY KEY,
            min_wage INTEGER,
            job_offers_rate REAL,
            unemployment_rate REAL,
            job_change_desire_rate REAL,
            non_regular_rate REAL,
            avg_monthly_wage REAL,
            price_index REAL,
            fulfillment_rate REAL,
            real_wage_index REAL
        )
    """,
    "v2_external_job_openings_ratio": """
        CREATE TABLE IF NOT EXISTS v2_external_job_openings_ratio (
            prefecture TEXT NOT NULL,
            fiscal_year TEXT NOT NULL,
            ratio_total REAL,
            ratio_excl_part REAL,
            PRIMARY KEY (prefecture, fiscal_year)
        )
    """,
    "v2_external_labor_stats": """
        CREATE TABLE IF NOT EXISTS v2_external_labor_stats (
            prefecture TEXT NOT NULL,
            fiscal_year TEXT NOT NULL,
            unemployment_rate REAL,
            unemployment_rate_male REAL,
            unemployment_rate_female REAL,
            employment_rate REAL,
            employee_rate REAL,
            separation_rate REAL,
            turnover_rate REAL,
            job_changer_rate REAL,
            placement_rate REAL,
            fulfillment_rate REAL,
            elderly_employment_rate REAL,
            working_hours_male REAL,
            working_hours_female REAL,
            monthly_salary_male REAL,
            monthly_salary_female REAL,
            part_time_wage_female REAL,
            part_time_wage_male REAL,
            PRIMARY KEY (prefecture, fiscal_year)
        )
    """,
    "v2_external_establishments": """
        CREATE TABLE IF NOT EXISTS v2_external_establishments (
            prefecture TEXT NOT NULL,
            industry TEXT NOT NULL DEFAULT '全産業',
            establishment_count INTEGER,
            employee_count INTEGER,
            reference_year TEXT,
            PRIMARY KEY (prefecture, industry)
        )
    """,
    "v2_external_turnover": """
        CREATE TABLE IF NOT EXISTS v2_external_turnover (
            prefecture TEXT NOT NULL,
            fiscal_year TEXT NOT NULL,
            industry TEXT NOT NULL DEFAULT '産業計',
            entry_rate REAL,
            separation_rate REAL,
            net_rate REAL,
            entry_count REAL,
            separation_count REAL,
            worker_count REAL,
            PRIMARY KEY (prefecture, fiscal_year, industry)
        )
    """,
    "v2_external_household_spending": """
        CREATE TABLE IF NOT EXISTS v2_external_household_spending (
            prefecture TEXT NOT NULL,
            category TEXT NOT NULL,
            monthly_amount INTEGER,
            reference_year TEXT,
            PRIMARY KEY (prefecture, category)
        )
    """,
    "v2_external_business_dynamics": """
        CREATE TABLE IF NOT EXISTS v2_external_business_dynamics (
            prefecture TEXT NOT NULL,
            fiscal_year TEXT NOT NULL,
            total_establishments INTEGER,
            new_establishments INTEGER,
            closed_establishments INTEGER,
            survived_establishments INTEGER,
            net_change INTEGER,
            opening_rate REAL,
            closure_rate REAL,
            PRIMARY KEY (prefecture, fiscal_year)
        )
    """,
    "v2_external_climate": """
        CREATE TABLE IF NOT EXISTS v2_external_climate (
            prefecture TEXT NOT NULL,
            fiscal_year TEXT NOT NULL,
            avg_temperature REAL,
            max_temperature REAL,
            min_temperature REAL,
            rainy_days INTEGER,
            snow_days INTEGER,
            sunshine_hours REAL,
            precipitation REAL,
            PRIMARY KEY (prefecture, fiscal_year)
        )
    """,
    "v2_external_care_demand": """
        CREATE TABLE IF NOT EXISTS v2_external_care_demand (
            prefecture TEXT NOT NULL,
            fiscal_year TEXT NOT NULL,
            care_support_offices REAL, care_support_users REAL,
            day_service_offices REAL, day_service_users REAL,
            health_facility_capacity REAL, health_facility_count REAL,
            health_facility_residents REAL,
            home_care_offices REAL, home_care_users REAL,
            home_helper_count REAL,
            insurance_benefit_cases REAL,
            insurance_benefit_cases_facility REAL,
            insurance_benefit_cases_home REAL,
            insurance_benefit_cost REAL,
            insurance_benefit_cost_facility REAL,
            insurance_benefit_cost_home REAL,
            late_elderly_insured REAL, late_elderly_medical_cost REAL,
            nursing_home_capacity REAL, nursing_home_count REAL,
            nursing_home_residents REAL,
            pop_65_over REAL, pop_65_over_rate REAL, pop_75_over REAL,
            PRIMARY KEY (prefecture, fiscal_year)
        )
    """,
}


def turso_pipeline(url, token, statements):
    """Turso HTTP Pipeline APIで複数SQLを一括実行"""
    headers = {
        "Authorization": f"Bearer {token}",
        "Content-Type": "application/json",
    }

    requests_list = []
    for sql, params in statements:
        stmt = {"sql": sql}
        if params:
            stmt["args"] = [
                {"type": "null", "value": None} if v is None
                else {"type": "integer", "value": str(v)} if isinstance(v, int)
                else {"type": "float", "value": v} if isinstance(v, float)
                else {"type": "text", "value": str(v)}
                for v in params
            ]
        requests_list.append({"type": "execute", "stmt": stmt})

    requests_list.append({"type": "close"})

    resp = requests.post(
        f"{url}/v2/pipeline",
        headers=headers,
        json={"requests": requests_list},
        timeout=60,
    )

    if resp.status_code != 200:
        raise Exception(f"Turso API error {resp.status_code}: {resp.text[:300]}")

    data = resp.json()
    errors = [r for r in data.get("results", []) if r.get("type") == "error"]
    if errors:
        raise Exception(f"SQL errors: {errors[:3]}")

    return data


def upload_table(turso_url, turso_token, local_db, table_name, dry_run=False):
    """1テーブルをTursoにアップロード"""
    # ローカルDBからデータ取得
    rows = local_db.execute(f"SELECT * FROM {table_name}").fetchall()
    if not rows:
        print(f"  {table_name}: 0行 (スキップ)")
        return 0

    # カラム名取得
    cursor = local_db.execute(f"SELECT * FROM {table_name} LIMIT 1")
    columns = [desc[0] for desc in cursor.description]

    placeholders = ", ".join(["?" for _ in columns])
    col_names = ", ".join(columns)
    insert_sql = f"INSERT OR REPLACE INTO {table_name} ({col_names}) VALUES ({placeholders})"

    if dry_run:
        print(f"  {table_name}: {len(rows)}行 (dry-run)")
        return len(rows)

    # テーブル作成（DROP + CREATE）
    stmts = [
        (f"DROP TABLE IF EXISTS {table_name}", None),
        (TABLE_SCHEMAS[table_name], None),
    ]
    turso_pipeline(turso_url, turso_token, stmts)

    # バッチINSERT（Turso pipeline は最大512文ぐらいが安全）
    BATCH_SIZE = 200
    total = 0
    for i in range(0, len(rows), BATCH_SIZE):
        batch = rows[i:i + BATCH_SIZE]
        stmts = [(insert_sql, list(row)) for row in batch]
        turso_pipeline(turso_url, turso_token, stmts)
        total += len(batch)

        if (i + BATCH_SIZE) % 1000 == 0 or i + BATCH_SIZE >= len(rows):
            print(f"    {table_name}: {total}/{len(rows)} 行完了")

    print(f"  {table_name}: {total} 行アップロード完了")
    return total


def verify_turso(turso_url, turso_token):
    """Turso側のデータ検証"""
    print("\n=== Turso検証 ===")

    for table in TABLES:
        try:
            data = turso_pipeline(turso_url, turso_token, [
                (f"SELECT COUNT(*) as cnt FROM {table}", None),
            ])
            count = data["results"][0]["response"]["result"]["rows"][0][0]["value"]
            print(f"  {table}: {count} 行")
        except Exception as e:
            print(f"  {table}: エラー ({e})")

    # サンプル検証: 千代田区の昼夜間人口
    try:
        data = turso_pipeline(turso_url, turso_token, [
            ("SELECT nighttime_pop, daytime_pop, day_night_ratio FROM v2_external_daytime_population WHERE municipality = '千代田区'", None),
        ])
        row = data["results"][0]["response"]["result"]["rows"][0]
        night = row[0]["value"]
        day = row[1]["value"]
        ratio = row[2]["value"]
        print(f"\n  千代田区: 夜間{int(night):,}人, 昼間{int(day):,}人, 昼夜比{float(ratio):.1f}%")
    except Exception as e:
        print(f"  千代田区検証エラー: {e}")


def main():
    parser = argparse.ArgumentParser(description="ローカルDB → Turso アップロード")
    parser.add_argument("--db", default=DEFAULT_DB, help="ローカルDBパス")
    parser.add_argument("--dry-run", action="store_true", help="実際にはアップロードしない")
    parser.add_argument("--url", default=os.environ.get("TURSO_EXTERNAL_URL", ""), help="Turso URL")
    parser.add_argument("--token", default=os.environ.get("TURSO_EXTERNAL_TOKEN", ""), help="Turso Token")
    args = parser.parse_args()

    turso_url = args.url
    turso_token = args.token

    if not turso_url or not turso_token:
        print("ERROR: --url と --token が必要です（または環境変数 TURSO_EXTERNAL_URL, TURSO_EXTERNAL_TOKEN）")
        sys.exit(1)

    # libsql:// → https:// 変換
    if turso_url.startswith("libsql://"):
        turso_url = turso_url.replace("libsql://", "https://")

    if not os.path.exists(args.db):
        print(f"ERROR: DB not found: {args.db}")
        sys.exit(1)

    print(f"Local DB: {args.db}")
    print(f"Turso:    {turso_url}")
    print(f"Dry-run:  {args.dry_run}")
    print()

    local_db = sqlite3.connect(args.db)

    start = time.time()
    total_rows = 0
    for table in TABLES:
        try:
            count = upload_table(turso_url, turso_token, local_db, table, args.dry_run)
            total_rows += count
        except Exception as e:
            print(f"  {table}: エラー ({e})")

    elapsed = time.time() - start
    print(f"\n合計: {total_rows:,} 行, {elapsed:.1f}秒")

    local_db.close()

    if not args.dry_run:
        verify_turso(turso_url, turso_token)


if __name__ == "__main__":
    main()
