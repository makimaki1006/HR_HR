# -*- coding: utf-8 -*-
"""SalesNow企業データ（44フィールド）+ 業界マッピングをTursoにアップロードする。

使い方:
    python upload_salesnow_to_turso.py --url URL --token TOKEN
    python upload_salesnow_to_turso.py --dry-run
    python upload_salesnow_to_turso.py --resume

環境変数でも指定可能:
    SALESNOW_TURSO_URL, SALESNOW_TURSO_TOKEN

最適化: バッチ500 + 3並列 + BEGIN/COMMITトランザクション
"""
import csv
import json
import os
import sys
import time
import argparse
import threading
from pathlib import Path
from concurrent.futures import ThreadPoolExecutor, as_completed

try:
    import requests
except ImportError:
    print("requests が必要: pip install requests")
    sys.exit(1)

from industry_mapping import build_mapping_rows

SCRIPT_DIR = Path(__file__).parent
DATA_DIR = SCRIPT_DIR.parent / "data"
CSV_FILE = DATA_DIR / "salesnow_companies.csv"

BATCH_SIZE = 500
MAX_WORKERS = 3

# テーブル定義（43カラム）
SCHEMAS = {
    "v2_salesnow_companies": """
        CREATE TABLE IF NOT EXISTS v2_salesnow_companies (
            corporate_number TEXT PRIMARY KEY,
            company_name TEXT NOT NULL,
            company_name_kana TEXT,
            company_url TEXT,
            established_date TEXT,
            listing_category TEXT,
            tob_toc TEXT,
            president_name TEXT,
            phone_number TEXT,
            mail_address TEXT,
            prefecture TEXT,
            address TEXT,
            postal_code TEXT,
            sn_industry TEXT,
            sn_industry2 TEXT,
            sn_industry_subs TEXT,
            business_tags TEXT,
            business_description TEXT,
            jccode TEXT,
            employee_count INTEGER,
            employee_range TEXT,
            group_employee_count INTEGER,
            employee_delta_1m REAL,
            employee_delta_3m REAL,
            employee_delta_6m REAL,
            employee_delta_1y REAL,
            employee_delta_2y REAL,
            capital_stock INTEGER,
            capital_stock_range TEXT,
            sales_amount INTEGER,
            sales_range TEXT,
            net_sales INTEGER,
            profit_loss INTEGER,
            is_estimated_sales TEXT,
            period_month TEXT,
            credit_score REAL,
            salesnow_score REAL,
            latest_event_date TEXT,
            latest_raised_series TEXT,
            latest_round_post_valuation INTEGER,
            market_cap INTEGER,
            hubspot_id TEXT,
            collated_at TEXT,
            salesnow_url TEXT
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

# スレッドセーフなカウンター
_lock = threading.Lock()
_uploaded_count = 0
_error_count = 0


def turso_pipeline(url, token, statements, timeout=120):
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
        timeout=timeout,
    )

    if resp.status_code != 200:
        raise Exception(f"Turso API error {resp.status_code}: {resp.text[:300]}")

    data = resp.json()
    errors = [r for r in data.get("results", []) if r.get("type") == "error"]
    if errors:
        raise Exception(f"SQL errors: {errors[:3]}")

    return data


def parse_int(v):
    if not v or str(v).strip() == "":
        return None
    try:
        return int(float(v))
    except (ValueError, TypeError):
        return None


def parse_float(v):
    if not v or str(v).strip() == "":
        return None
    try:
        return float(v)
    except (ValueError, TypeError):
        return None


def str_or_none(row, key):
    v = (row.get(key) or "").strip()
    return v if v else None


def _wrap_in_transaction(stmts):
    return [("BEGIN", [])] + stmts + [("COMMIT", [])]


def _upload_batch(url, token, batch, batch_idx):
    global _uploaded_count, _error_count
    stmts = _wrap_in_transaction(batch)
    for attempt in range(3):
        try:
            turso_pipeline(url, token, stmts, timeout=max(120, len(batch) // 2))
            with _lock:
                _uploaded_count += len(batch)
            return True
        except Exception as e:
            if attempt < 2:
                time.sleep(1 * (attempt + 1))
                continue
            with _lock:
                _error_count += len(batch)
            print(f"  batch#{batch_idx} error ({len(batch)}): {e}")
            return False


INSERT_SQL = """INSERT OR REPLACE INTO v2_salesnow_companies
    (corporate_number, company_name, company_name_kana, company_url,
     established_date, listing_category, tob_toc, president_name,
     phone_number, mail_address,
     prefecture, address, postal_code,
     sn_industry, sn_industry2, sn_industry_subs, business_tags,
     business_description, jccode,
     employee_count, employee_range, group_employee_count,
     employee_delta_1m, employee_delta_3m, employee_delta_6m,
     employee_delta_1y, employee_delta_2y,
     capital_stock, capital_stock_range, sales_amount, sales_range,
     net_sales, profit_loss, is_estimated_sales, period_month,
     credit_score, salesnow_score,
     latest_event_date, latest_raised_series, latest_round_post_valuation,
     market_cap, hubspot_id, collated_at, salesnow_url)
    VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,
            ?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,
            ?21,?22,?23,?24,?25,?26,?27,?28,?29,?30,
            ?31,?32,?33,?34,?35,?36,?37,?38,?39,?40,
            ?41,?42,?43,?44)"""


def build_params(corp_num, row):
    """CSVの1行から44パラメータを構築"""
    return [
        corp_num,                                              # 1
        (row.get("sn_company_name") or row.get("name") or "").strip(),  # 2
        str_or_none(row, "company_name_kana"),                 # 3
        str_or_none(row, "company_url"),                       # 4
        str_or_none(row, "established_date"),                  # 5
        str_or_none(row, "listing_category"),                  # 6
        str_or_none(row, "tob_toc"),                           # 7
        str_or_none(row, "president_name"),                    # 8
        str_or_none(row, "phone_number"),                      # 9
        str_or_none(row, "mail_address"),                      # 10
        str_or_none(row, "prefecture"),                        # 11
        str_or_none(row, "address"),                           # 12
        str_or_none(row, "postal_code"),                       # 13
        str_or_none(row, "sn_industry"),                       # 14
        str_or_none(row, "sn_industry2"),                      # 15
        str_or_none(row, "sn_industry_subs"),                  # 16
        str_or_none(row, "business_tags"),                     # 17
        str_or_none(row, "business_description"),              # 18
        str_or_none(row, "jccode"),                            # 19
        parse_int(row.get("employee_count")),                  # 20
        str_or_none(row, "employee_range"),                    # 21
        parse_int(row.get("group_employee_count")),            # 22
        parse_float(row.get("employee_delta_1m")),             # 23
        parse_float(row.get("employee_delta_3m")),             # 24
        parse_float(row.get("employee_delta_6m")),             # 25
        parse_float(row.get("employee_delta_1y")),             # 26
        parse_float(row.get("employee_delta_2y")),             # 27
        parse_int(row.get("capital_stock")),                   # 28
        str_or_none(row, "capital_stock_range"),               # 29
        parse_int(row.get("sales_amount")),                    # 30
        str_or_none(row, "sales_range"),                       # 31
        parse_int(row.get("net_sales")),                       # 32
        parse_int(row.get("profit_loss")),                     # 33
        str_or_none(row, "is_estimated_sales"),                # 34
        str_or_none(row, "period_month"),                      # 35
        parse_float(row.get("credit_score")),                  # 36
        parse_float(row.get("salesnow_score")),                # 37
        str_or_none(row, "latest_event_date"),                 # 38
        str_or_none(row, "latest_raised_series"),              # 39
        parse_int(row.get("latest_round_post_valuation")),     # 40
        parse_int(row.get("market_cap")),                      # 41
        str_or_none(row, "hubspot_id"),                        # 42
        str_or_none(row, "collated_at"),                       # 43
        str_or_none(row, "salesnow_url"),                      # 44
    ]


def upload_companies(turso_url, turso_token, dry_run=False, resume=False):
    """CSVからSalesNow企業をTursoにアップロード"""
    global _uploaded_count, _error_count
    _uploaded_count = 0
    _error_count = 0

    if not CSV_FILE.exists():
        print(f"error: CSV not found: {CSV_FILE}")
        sys.exit(1)

    if not resume:
        print("Creating tables...")
        for table_name, schema in SCHEMAS.items():
            stmts = [
                (f"DROP TABLE IF EXISTS {table_name}", []),
                (schema, []),
            ]
            if not dry_run:
                turso_pipeline(turso_url, turso_token, stmts)
            print(f"  {table_name}: done")

        print("\nUploading industry mapping...")
        mapping_rows = build_mapping_rows()
        insert_mapping = "INSERT OR REPLACE INTO v2_industry_mapping (sn_industry, hw_job_type, confidence) VALUES (?1, ?2, ?3)"
        stmts = [(insert_mapping, list(row)) for row in mapping_rows]
        if not dry_run:
            turso_pipeline(turso_url, turso_token, stmts)
        print(f"  v2_industry_mapping: {len(mapping_rows)} rows")
    else:
        print("Resume mode: skipping DROP TABLE")

    # Dedup: best record per corporate_number
    print("\nReading CSV + dedup...")
    best_records = {}

    with open(CSV_FILE, "r", encoding="utf-8", errors="replace") as f:
        reader = csv.DictReader(f)
        for row in reader:
            corp_num = (row.get("corporate_number") or "").strip()
            if not corp_num:
                continue

            emp = parse_int(row.get("employee_count")) or 0
            # 重み付き充填スコア
            HIGH = ["sn_industry", "sn_industry2", "sales_range", "credit_score",
                    "employee_delta_1y", "capital_stock", "sales_amount"]
            MED = ["sn_industry_subs", "business_tags", "employee_delta_1m",
                   "employee_delta_3m", "salesnow_score", "listing_category"]
            fill_score = (sum(2 for k in HIGH if (row.get(k) or "").strip())
                        + sum(1 for k in MED if (row.get(k) or "").strip()))

            if corp_num not in best_records:
                best_records[corp_num] = (row, emp, fill_score)
            else:
                _, prev_emp, prev_fill = best_records[corp_num]
                if (emp, fill_score) > (prev_emp, prev_fill):
                    best_records[corp_num] = (row, emp, fill_score)

    dedup_count = len(best_records)
    print(f"  Dedup: {dedup_count} unique companies")

    # Build batches
    all_batches = []
    current_batch = []
    for corp_num, (row, _, _) in best_records.items():
        params = build_params(corp_num, row)
        current_batch.append((INSERT_SQL, params))
        if len(current_batch) >= BATCH_SIZE:
            all_batches.append(current_batch)
            current_batch = []
    if current_batch:
        all_batches.append(current_batch)

    num_batches = len(all_batches)
    print(f"  Batches: {num_batches} (max {BATCH_SIZE} rows each)")

    if dry_run:
        print(f"\nDry run complete: {dedup_count} companies, {num_batches} batches")
        return dedup_count

    # Parallel upload
    start_time = time.time()
    print(f"\nUploading ({MAX_WORKERS} parallel)...")

    with ThreadPoolExecutor(max_workers=MAX_WORKERS) as executor:
        futures = {}
        for idx, batch in enumerate(all_batches):
            future = executor.submit(_upload_batch, turso_url, turso_token, batch, idx)
            futures[future] = idx

        completed = 0
        for future in as_completed(futures):
            completed += 1
            if completed % 20 == 0 or completed == num_batches:
                elapsed = time.time() - start_time
                rate = _uploaded_count / elapsed if elapsed > 0 else 0
                eta = (dedup_count - _uploaded_count) / rate if rate > 0 else 0
                print(f"  {completed}/{num_batches} batches "
                      f"({_uploaded_count} rows, {rate:.0f} rows/sec, ETA {eta:.0f}s)")

    elapsed = time.time() - start_time
    print(f"\nDone: {elapsed:.1f}s")
    print(f"  Uploaded: {_uploaded_count}")
    print(f"  Errors: {_error_count}")
    if elapsed > 0:
        print(f"  Throughput: {_uploaded_count / elapsed:.0f} rows/sec")

    return _uploaded_count


def main():
    parser = argparse.ArgumentParser(description="SalesNow -> Turso upload (44 fields)")
    parser.add_argument("--url", default=os.environ.get("SALESNOW_TURSO_URL", ""),
                        help="Turso URL")
    parser.add_argument("--token", default=os.environ.get("SALESNOW_TURSO_TOKEN", ""),
                        help="Turso Token")
    parser.add_argument("--dry-run", action="store_true",
                        help="Validate without uploading")
    parser.add_argument("--resume", action="store_true",
                        help="Skip DROP TABLE (for interrupted uploads)")
    args = parser.parse_args()

    turso_url = args.url
    turso_token = args.token

    if not args.dry_run and (not turso_url or not turso_token):
        print("ERROR: --url and --token required (or env SALESNOW_TURSO_URL/TOKEN)")
        sys.exit(1)

    if turso_url.startswith("libsql://"):
        turso_url = turso_url.replace("libsql://", "https://")

    print(f"CSV: {CSV_FILE}")
    print(f"Turso: {turso_url}")
    print(f"Dry run: {args.dry_run}")
    print(f"Batch: {BATCH_SIZE}, Workers: {MAX_WORKERS}")
    print()

    total = upload_companies(turso_url, turso_token, args.dry_run, args.resume)
    print(f"\nComplete: {total} companies uploaded to Turso")


if __name__ == "__main__":
    main()
