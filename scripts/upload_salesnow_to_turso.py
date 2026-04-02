# -*- coding: utf-8 -*-
"""SalesNow企業データ + 業界マッピングをTursoにアップロードする。

使い方:
    python upload_salesnow_to_turso.py --url URL --token TOKEN
    python upload_salesnow_to_turso.py --dry-run

環境変数でも指定可能:
    TURSO_EXTERNAL_URL, TURSO_EXTERNAL_TOKEN
"""
import csv
import json
import os
import sys
import argparse
import time
from pathlib import Path

try:
    import requests
except ImportError:
    print("requests が必要: pip install requests")
    sys.exit(1)

from industry_mapping import build_mapping_rows

SCRIPT_DIR = Path(__file__).parent
DATA_DIR = SCRIPT_DIR.parent / "data"
CSV_FILE = DATA_DIR / "salesnow_companies.csv"

BATCH_SIZE = 200

# テーブル定義
SCHEMAS = {
    "v2_salesnow_companies": """
        CREATE TABLE IF NOT EXISTS v2_salesnow_companies (
            corporate_number TEXT PRIMARY KEY,
            company_name TEXT NOT NULL,
            employee_count INTEGER,
            employee_range TEXT,
            employee_delta_1y REAL,
            sales_range TEXT,
            sn_industry TEXT,
            sn_industry2 TEXT,
            prefecture TEXT,
            credit_score REAL,
            hubspot_id TEXT
        )
    """,
    "v2_industry_mapping": """
        CREATE TABLE IF NOT EXISTS v2_industry_mapping (
            sn_industry TEXT NOT NULL,
            hw_job_type TEXT NOT NULL,
            confidence REAL DEFAULT 1.0,
            PRIMARY KEY (sn_industry, hw_job_type)
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
        timeout=120,
    )

    if resp.status_code != 200:
        raise Exception(f"Turso API error {resp.status_code}: {resp.text[:300]}")

    data = resp.json()
    errors = [r for r in data.get("results", []) if r.get("type") == "error"]
    if errors:
        raise Exception(f"SQL errors: {errors[:3]}")

    return data


def parse_int(v):
    """文字列をintに変換。空やエラーはNone"""
    if not v or v.strip() == "":
        return None
    try:
        return int(float(v))
    except (ValueError, TypeError):
        return None


def parse_float(v):
    """文字列をfloatに変換。空やエラーはNone"""
    if not v or v.strip() == "":
        return None
    try:
        return float(v)
    except (ValueError, TypeError):
        return None


def upload_companies(turso_url, turso_token, dry_run=False):
    """CSVからSalesNow企業をTursoにアップロード"""
    if not CSV_FILE.exists():
        print(f"エラー: CSVが見つかりません: {CSV_FILE}")
        sys.exit(1)

    # テーブル作成（DROP + CREATE）
    print("テーブル作成...")
    for table_name, schema in SCHEMAS.items():
        stmts = [
            (f"DROP TABLE IF EXISTS {table_name}", []),
            (schema, []),
        ]
        if not dry_run:
            turso_pipeline(turso_url, turso_token, stmts)
        print(f"  {table_name}: 作成完了")

    # 業界マッピングのアップロード
    print("\n業界マッピングアップロード...")
    mapping_rows = build_mapping_rows()
    insert_mapping = "INSERT OR REPLACE INTO v2_industry_mapping (sn_industry, hw_job_type, confidence) VALUES (?1, ?2, ?3)"
    stmts = [(insert_mapping, list(row)) for row in mapping_rows]
    if not dry_run:
        turso_pipeline(turso_url, turso_token, stmts)
    print(f"  v2_industry_mapping: {len(mapping_rows)} 行完了")

    # 企業データのアップロード
    print("\n企業データアップロード...")
    insert_sql = """INSERT OR REPLACE INTO v2_salesnow_companies
        (corporate_number, company_name, employee_count, employee_range,
         employee_delta_1y, sales_range, sn_industry, sn_industry2,
         prefecture, credit_score, hubspot_id)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)"""

    total = 0
    skipped = 0
    batch = []

    with open(CSV_FILE, "r", encoding="utf-8") as f:
        reader = csv.DictReader(f)
        for row in reader:
            corp_num = (row.get("corporate_number") or "").strip()
            if not corp_num:
                skipped += 1
                continue

            params = [
                corp_num,
                (row.get("sn_company_name") or row.get("name") or "").strip(),
                parse_int(row.get("employee_count")),
                (row.get("employee_range") or "").strip() or None,
                parse_float(row.get("employee_delta_1y")),
                (row.get("sales_range") or "").strip() or None,
                (row.get("sn_industry") or "").strip() or None,
                (row.get("sn_industry2") or "").strip() or None,
                (row.get("prefecture") or "").strip() or None,
                parse_float(row.get("credit_score")),
                (row.get("hubspot_id") or "").strip() or None,
            ]

            batch.append((insert_sql, params))

            if len(batch) >= BATCH_SIZE:
                if not dry_run:
                    turso_pipeline(turso_url, turso_token, batch)
                total += len(batch)
                batch = []

                if total % 5000 == 0:
                    print(f"    {total} 行完了...")
                    time.sleep(0.1)  # レートリミット回避

    # 残りのバッチ
    if batch:
        if not dry_run:
            turso_pipeline(turso_url, turso_token, batch)
        total += len(batch)

    print(f"  v2_salesnow_companies: {total} 行アップロード完了 (スキップ: {skipped})")
    return total


def main():
    parser = argparse.ArgumentParser(description="SalesNow → Turso アップロード")
    parser.add_argument("--url", default=os.environ.get("TURSO_EXTERNAL_URL", ""),
                        help="Turso URL")
    parser.add_argument("--token", default=os.environ.get("TURSO_EXTERNAL_TOKEN", ""),
                        help="Turso Token")
    parser.add_argument("--dry-run", action="store_true",
                        help="実際にはアップロードしない")
    args = parser.parse_args()

    turso_url = args.url
    turso_token = args.token

    if not args.dry_run and (not turso_url or not turso_token):
        print("ERROR: --url と --token が必要です（または環境変数 TURSO_EXTERNAL_URL, TURSO_EXTERNAL_TOKEN）")
        sys.exit(1)

    # libsql:// → https:// 変換
    if turso_url.startswith("libsql://"):
        turso_url = turso_url.replace("libsql://", "https://")

    print(f"CSV: {CSV_FILE}")
    print(f"Turso: {turso_url}")
    print(f"Dry run: {args.dry_run}")
    print()

    total = upload_companies(turso_url, turso_token, args.dry_run)
    print(f"\n完了: {total} 企業をTursoにアップロード")


if __name__ == "__main__":
    main()
