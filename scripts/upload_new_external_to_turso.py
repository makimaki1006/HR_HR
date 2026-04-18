"""
新規外部統計CSV → Turso (country-statistics) アップロード
=============================================================
10個の外部統計CSVをTurso DBにアップロードする。

使い方:
    # 全テーブルをアップロード
    python upload_new_external_to_turso.py

    # 特定テーブルのみ
    python upload_new_external_to_turso.py --table v2_external_boj_tankan

    # CSVパースとバリデーションのみ（DBへの書き込みなし）
    python upload_new_external_to_turso.py --dry-run

環境変数（.envから自動読み込み）:
    TURSO_EXTERNAL_URL   Turso DB URL (https://...)
    TURSO_EXTERNAL_TOKEN Turso Auth Token
"""

import csv
import os
import sys
import argparse
import time
import requests

# スクリプトのディレクトリ（scripts/）
SCRIPT_DIR = os.path.dirname(os.path.abspath(__file__))

# デプロイリポジトリのルート（scripts/ の一つ上）
DEPLOY_ROOT = os.path.dirname(SCRIPT_DIR)

# CSVファイルのディレクトリ（scripts/data/）
DATA_DIR = os.path.join(SCRIPT_DIR, "data")

# .envファイルパス
ENV_FILE = os.path.join(DEPLOY_ROOT, ".env")


# ─────────────────────────────────────────────
# テーブル定義（CREATE TABLE文）
# ─────────────────────────────────────────────
TABLE_SCHEMAS = {
    "v2_external_foreign_residents": """
        CREATE TABLE IF NOT EXISTS v2_external_foreign_residents (
            prefecture   TEXT NOT NULL,
            visa_status  TEXT NOT NULL,
            count        INTEGER,
            survey_period TEXT,
            PRIMARY KEY (prefecture, visa_status)
        )
    """,
    "v2_external_education": """
        CREATE TABLE IF NOT EXISTS v2_external_education (
            prefecture       TEXT NOT NULL,
            education_level  TEXT NOT NULL,
            male_count       INTEGER,
            female_count     INTEGER,
            total_count      INTEGER,
            PRIMARY KEY (prefecture, education_level)
        )
    """,
    "v2_external_household": """
        CREATE TABLE IF NOT EXISTS v2_external_household (
            prefecture     TEXT NOT NULL,
            household_type TEXT NOT NULL,
            count          INTEGER,
            ratio          REAL,
            PRIMARY KEY (prefecture, household_type)
        )
    """,
    "v2_external_boj_tankan": """
        CREATE TABLE IF NOT EXISTS v2_external_boj_tankan (
            survey_date      TEXT NOT NULL,
            industry_code    TEXT,
            industry_j       TEXT NOT NULL,
            enterprise_size  TEXT NOT NULL,
            di_type          TEXT NOT NULL,
            result_type      TEXT NOT NULL,
            di_value         INTEGER,
            series_code      TEXT,
            UNIQUE (survey_date, series_code)
        )
    """,
    "v2_external_social_life": """
        CREATE TABLE IF NOT EXISTS v2_external_social_life (
            prefecture         TEXT NOT NULL,
            category           TEXT NOT NULL,
            subcategory        TEXT,
            participation_rate REAL,
            survey_year        INTEGER,
            PRIMARY KEY (prefecture, category)
        )
    """,
    "v2_external_household_spending": """
        CREATE TABLE IF NOT EXISTS v2_external_household_spending (
            city              TEXT NOT NULL,
            prefecture        TEXT NOT NULL,
            category          TEXT NOT NULL,
            annual_amount_yen INTEGER,
            year              INTEGER,
            PRIMARY KEY (city, category, year)
        )
    """,
    "v2_external_industry_structure": """
        CREATE TABLE IF NOT EXISTS v2_external_industry_structure (
            prefecture_code  TEXT NOT NULL,
            city_code        TEXT NOT NULL,
            city_name        TEXT,
            industry_code    TEXT NOT NULL,
            industry_name    TEXT,
            establishments   INTEGER,
            employees_total  INTEGER,
            employees_male   INTEGER,
            employees_female INTEGER,
            PRIMARY KEY (city_code, industry_code)
        )
    """,
    "v2_external_land_price": """
        CREATE TABLE IF NOT EXISTS v2_external_land_price (
            prefecture         TEXT NOT NULL,
            land_use           TEXT NOT NULL,
            avg_price_per_sqm  REAL,
            yoy_change_pct     REAL,
            year               INTEGER,
            point_count        INTEGER,
            PRIMARY KEY (prefecture, land_use, year)
        )
    """,
    "v2_external_car_ownership": """
        CREATE TABLE IF NOT EXISTS v2_external_car_ownership (
            prefecture         TEXT NOT NULL,
            cars_per_100people REAL,
            total_kei_cars     INTEGER,
            total_population   INTEGER,
            year               INTEGER,
            note               TEXT,
            PRIMARY KEY (prefecture)
        )
    """,
    "v2_external_internet_usage": """
        CREATE TABLE IF NOT EXISTS v2_external_internet_usage (
            prefecture                TEXT NOT NULL,
            internet_usage_rate       REAL,
            smartphone_ownership_rate REAL,
            year                      INTEGER,
            data_source               TEXT,
            note                      TEXT,
            PRIMARY KEY (prefecture)
        )
    """,
}

# prefectureカラムを持つテーブル（インデックス作成対象）
PREFECTURE_INDEX_TABLES = {
    "v2_external_foreign_residents",
    "v2_external_education",
    "v2_external_household",
    "v2_external_social_life",
    "v2_external_household_spending",
    "v2_external_industry_structure",
    "v2_external_land_price",
    "v2_external_car_ownership",
    "v2_external_internet_usage",
}

# テーブル名 → CSVファイル名のマッピング
TABLE_CSV_MAP = {
    "v2_external_foreign_residents":  "foreign_residents_by_prefecture.csv",
    "v2_external_education":          "education_by_prefecture.csv",
    "v2_external_household":          "household_by_prefecture.csv",
    "v2_external_boj_tankan":         "boj_tankan_di.csv",
    "v2_external_social_life":        "social_life_survey.csv",
    "v2_external_household_spending": "household_spending.csv",
    "v2_external_industry_structure": "industry_structure_by_municipality.csv",
    "v2_external_land_price":         "land_price_by_prefecture.csv",
    "v2_external_car_ownership":      "car_ownership_by_prefecture.csv",
    "v2_external_internet_usage":     "internet_usage_by_prefecture.csv",
}

# アップロード順序（定義された順番通り）
ALL_TABLES = list(TABLE_CSV_MAP.keys())


# ─────────────────────────────────────────────
# .env ファイル読み込み
# ─────────────────────────────────────────────
def load_env(env_path):
    """
    .envファイルから環境変数を読み込む。
    既に環境変数が設定されている場合は上書きしない。
    """
    if not os.path.exists(env_path):
        return
    with open(env_path, encoding="utf-8") as f:
        for line in f:
            line = line.strip()
            # 空行・コメント行はスキップ
            if not line or line.startswith("#"):
                continue
            if "=" not in line:
                continue
            key, _, val = line.partition("=")
            key = key.strip()
            val = val.strip().strip('"').strip("'")
            # 既存の環境変数を上書きしない
            if key not in os.environ:
                os.environ[key] = val


# ─────────────────────────────────────────────
# Turso HTTP Pipeline API
# ─────────────────────────────────────────────
def turso_pipeline(url, token, statements):
    """
    Turso HTTP Pipeline APIで複数SQLを一括実行する。
    statements: [(sql, params_or_None), ...]
    """
    headers = {
        "Authorization": f"Bearer {token}",
        "Content-Type": "application/json",
    }

    requests_list = []
    for sql, params in statements:
        stmt = {"sql": sql}
        if params:
            stmt["args"] = [
                {"type": "null",    "value": None}       if v is None
                else {"type": "integer", "value": str(v)} if isinstance(v, int)
                else {"type": "float",   "value": v}      if isinstance(v, float)
                else {"type": "text",    "value": str(v)}
                for v in params
            ]
        requests_list.append({"type": "execute", "stmt": stmt})

    # パイプラインのクローズ
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


# ─────────────────────────────────────────────
# CSV → 型変換ユーティリティ
# ─────────────────────────────────────────────
def _cast_value(col_name, raw_value, schema_sql):
    """
    スキーマ定義に基づいてCSVの文字列値を適切な型にキャストする。
    空文字列はNoneとして扱う。
    """
    if raw_value == "" or raw_value is None:
        return None

    # スキーマ内のカラム定義からデータ型を推定
    schema_upper = schema_sql.upper()
    col_upper = col_name.upper()

    # カラム名が含まれる行を探す
    for line in schema_sql.splitlines():
        stripped = line.strip()
        if stripped.upper().startswith(col_upper + " ") or stripped.upper().startswith(col_upper + "\t"):
            if "INTEGER" in stripped.upper():
                try:
                    # 小数点付き文字列（例: "123.0"）も整数として扱う
                    return int(float(raw_value))
                except (ValueError, TypeError):
                    return raw_value
            elif "REAL" in stripped.upper():
                try:
                    return float(raw_value)
                except (ValueError, TypeError):
                    return raw_value
            break

    return raw_value


# ─────────────────────────────────────────────
# CSVパース
# ─────────────────────────────────────────────
def parse_csv(csv_path, table_name):
    """
    CSVファイルを読み込み、(columns, rows)を返す。
    CSVヘッダーとDBカラムのマッピングは列名一致前提。
    """
    schema_sql = TABLE_SCHEMAS[table_name]

    with open(csv_path, encoding="utf-8-sig", newline="") as f:
        reader = csv.DictReader(f)
        columns = reader.fieldnames
        if not columns:
            raise ValueError(f"CSVにヘッダーがありません: {csv_path}")

        rows = []
        for row_dict in reader:
            row = [
                _cast_value(col, row_dict.get(col, ""), schema_sql)
                for col in columns
            ]
            rows.append(row)

    return columns, rows


# ─────────────────────────────────────────────
# 1テーブルのアップロード処理
# ─────────────────────────────────────────────
def upload_table(turso_url, turso_token, table_name, dry_run=False, refresh=False):
    """
    指定テーブルのCSVを読み込み、Tursoにアップロードする。
    dry_run=Trueの場合、CSVパースとバリデーションのみ実行。
    refresh=Trueの場合、DROP TABLE → CREATE TABLE でスキーマ再作成。
    """
    csv_file = TABLE_CSV_MAP[table_name]
    csv_path = os.path.join(DATA_DIR, csv_file)

    # CSVファイルが存在しない場合はスキップ
    if not os.path.exists(csv_path):
        print(f"  {table_name}: CSVなし ({csv_file}) → スキップ")
        return 0

    # CSVパース
    try:
        columns, rows = parse_csv(csv_path, table_name)
    except Exception as e:
        print(f"  {table_name}: CSVパースエラー ({e})")
        return 0

    if not rows:
        print(f"  {table_name}: 0行 → スキップ")
        return 0

    if dry_run:
        # dry-runモード: パース結果を表示するのみ
        print(f"  {table_name}: {len(rows)}行パース済み [dry-run] カラム={columns}")
        return len(rows)

    # ── 実際のDBアップロード ──

    # refreshモード: 既存テーブルをDROPしてからCREATE
    schema_sql = TABLE_SCHEMAS[table_name]
    setup_stmts = []
    if refresh:
        setup_stmts.append((f"DROP TABLE IF EXISTS {table_name}", None))
        print(f"    {table_name}: DROP TABLE (refresh)")
    setup_stmts.append((schema_sql, None))
    # prefectureカラムにインデックスを作成
    if table_name in PREFECTURE_INDEX_TABLES:
        idx_name = f"idx_{table_name}_prefecture"
        # industry_structureはprefecture_codeカラム
        pref_col = "prefecture_code" if table_name == "v2_external_industry_structure" else "prefecture"
        setup_stmts.append((
            f"CREATE INDEX IF NOT EXISTS {idx_name} ON {table_name} ({pref_col})",
            None,
        ))
    turso_pipeline(turso_url, turso_token, setup_stmts)

    # INSERT OR REPLACE（UPSERT）
    col_names    = ", ".join(columns)
    placeholders = ", ".join(["?" for _ in columns])
    insert_sql   = f"INSERT OR REPLACE INTO {table_name} ({col_names}) VALUES ({placeholders})"

    # バッチINSERT（500行/バッチ）
    BATCH_SIZE = 500
    total = 0

    for i in range(0, len(rows), BATCH_SIZE):
        batch = rows[i : i + BATCH_SIZE]

        # BEGIN/COMMITでトランザクション管理
        stmts = [("BEGIN", None)]
        stmts += [(insert_sql, row) for row in batch]
        stmts += [("COMMIT", None)]

        turso_pipeline(turso_url, turso_token, stmts)
        total += len(batch)

        # 1000行ごとに中間進捗を表示
        if total % 1000 == 0 or total == len(rows):
            print(f"    {table_name}: {total}/{len(rows)} 行完了")

    print(f"  {table_name}: {total} 行アップロード完了")
    return total


# ─────────────────────────────────────────────
# Turso検証
# ─────────────────────────────────────────────
def verify_turso(turso_url, turso_token, tables):
    """アップロード後のTurso側行数を確認する"""
    print("\n=== Turso検証 ===")
    for table in tables:
        try:
            data = turso_pipeline(turso_url, turso_token, [
                (f"SELECT COUNT(*) as cnt FROM {table}", None),
            ])
            count = data["results"][0]["response"]["result"]["rows"][0][0]["value"]
            print(f"  {table}: {count} 行")
        except Exception as e:
            print(f"  {table}: エラー ({e})")


# ─────────────────────────────────────────────
# エントリーポイント
# ─────────────────────────────────────────────
def main():
    # .envファイルから環境変数を読み込む
    load_env(ENV_FILE)

    parser = argparse.ArgumentParser(
        description="新規外部統計CSV → Turso (country-statistics) アップロード"
    )
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="CSVパースとバリデーションのみ実行（DBへの書き込みなし）",
    )
    parser.add_argument(
        "--table",
        metavar="TABLE_NAME",
        help="特定テーブルのみアップロード（例: v2_external_boj_tankan）",
    )
    parser.add_argument(
        "--refresh",
        action="store_true",
        help="DROP TABLE → CREATE TABLE でスキーマ再作成（既存データ削除）",
    )
    args = parser.parse_args()

    # 初回実行は --dry-run を推奨
    if not args.dry_run:
        print("=" * 60)
        print("注意: 初回実行は --dry-run オプションで事前確認を推奨します。")
        print("      python upload_new_external_to_turso.py --dry-run")
        print("=" * 60)
        print()

    # Turso接続情報の取得
    turso_url   = os.environ.get("TURSO_EXTERNAL_URL", "")
    turso_token = os.environ.get("TURSO_EXTERNAL_TOKEN", "")

    if not args.dry_run:
        if not turso_url or not turso_token:
            print("ERROR: TURSO_EXTERNAL_URL と TURSO_EXTERNAL_TOKEN が必要です")
            print(f"       .envファイル確認先: {ENV_FILE}")
            sys.exit(1)

        # libsql:// → https:// に変換
        if turso_url.startswith("libsql://"):
            turso_url = turso_url.replace("libsql://", "https://")

    # アップロード対象テーブルの決定
    if args.table:
        if args.table not in TABLE_SCHEMAS:
            print(f"ERROR: 不明なテーブル名: {args.table}")
            print(f"       有効なテーブル: {', '.join(ALL_TABLES)}")
            sys.exit(1)
        target_tables = [args.table]
    else:
        target_tables = ALL_TABLES

    # 設定の表示
    print(f"モード:    {'dry-run（書き込みなし）' if args.dry_run else '実際にアップロード'}")
    if not args.dry_run:
        print(f"Turso:     {turso_url}")
    print(f"CSVフォルダ: {DATA_DIR}")
    print(f"対象テーブル: {len(target_tables)} 件")
    print()

    # 各テーブルの処理
    start = time.time()
    total_rows = 0

    for table in target_tables:
        try:
            count = upload_table(turso_url, turso_token, table, dry_run=args.dry_run, refresh=args.refresh)
            total_rows += count
        except Exception as e:
            print(f"  {table}: エラー ({e})")

    elapsed = time.time() - start
    print(f"\n合計: {total_rows:,} 行, {elapsed:.1f}秒")

    # dry-runでなければ検証を実行
    if not args.dry_run:
        # スキップされたテーブルを除外して検証
        uploaded_tables = [
            t for t in target_tables
            if os.path.exists(os.path.join(DATA_DIR, TABLE_CSV_MAP[t]))
        ]
        if uploaded_tables:
            verify_turso(turso_url, turso_token, uploaded_tables)


if __name__ == "__main__":
    main()
