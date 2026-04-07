# -*- coding: utf-8 -*-
"""HubSpot Companies APIからSalesNowフィールドを取得してCSV出力する。

段階実行対応: チェックポイントファイルで途中再開可能。
出力: data/salesnow_companies.csv
"""
import csv
import json
import os
import sys
import time
from pathlib import Path
from urllib.request import Request, urlopen
from urllib.error import HTTPError, URLError

# ---------- 設定 ----------
SCRIPT_DIR = Path(__file__).parent
DATA_DIR = SCRIPT_DIR.parent / "data"
OUTPUT_CSV = DATA_DIR / "salesnow_companies.csv"
CHECKPOINT_FILE = DATA_DIR / "salesnow_checkpoint.json"

# HubSpot APIトークン（.envから読み込み）
ENV_FILE = Path(r"C:\Users\fuji1\OneDrive\デスクトップ\Hubspot\.env")

# 取得するSalesNowフィールド（全44プロパティ）
PROPERTIES = [
    "hs_object_id",
    "name",
    # 企業基本情報
    "qw_snpf__sn_corporate_number__c",
    "qw_snpf__sn_company_name__c",
    "qw_snpf__sn_company_name_kana__c",
    "qw_snpf__sn_company_url__c",
    "qw_snpf__sn_established_date__c",
    "qw_snpf__sn_listing_category_new__c",
    "qw_snpf__sn_tob_toc__c",
    "qw_snpf__sn_president_name__c",
    "qw_snpf__sn_phone_number__c",
    "qw_snpf__sn_mail_address__c",
    # 所在地
    "qw_snpf__sn_prefecture__c",
    "qw_snpf__sn_address__c",
    "qw_snpf__sn_postal_code__c",
    # 業種
    "qw_snpf__sn_industry_name__c",
    "qw_snpf__sn_industry_name2__c",
    "qw_snpf__sn_industry_name_subs__c",
    "qw_snpf__sn_business_tags__c",
    "qw_snpf__sn_business_description__c",
    "qw_snpf__sn_jccode__c",
    # 従業員数・増減率
    "qw_snpf__sn_employee_number__c",
    "qw_snpf__sn_employee_number_range__c",
    "qw_snpf__sn_group_employee_number__c",
    "qw_snpf__sn_employee_number_delta_one_month__c",
    "qw_snpf__sn_employee_number_delta_three_month__c",
    "qw_snpf__sn_employee_number_delta_six_month__c",
    "qw_snpf__sn_employee_number_delta_one_year__c",
    "qw_snpf__sn_employee_number_delta_two_years__c",
    # 財務
    "qw_snpf__sn_capital_stock__c",
    "qw_snpf__sn_capital_stock_range__c",
    "qw_snpf__sn_sales__c",
    "qw_snpf__sn_sales_range__c",
    "qw_snpf__sn_net_sales__c",
    "qw_snpf__sn_profit_loss__c",
    "qw_snpf__sn_is_estimated_sales__c",
    "qw_snpf__sn_period_month__c",
    # スコア
    "qw_snpf__sn_company_credit_score__c",
    "qw_snpf__sn_salesnow_score__c",
    # 調達
    "qw_snpf__sn_latest_event_date__c",
    "qw_snpf__sn_latest_raised_series__c",
    "qw_snpf__sn_latest_round_post_valuation__c",
    "qw_snpf__sn_market_cap__c",
    # メタ
    "qw_snpf__sn_label__c",
    "qw_snpf__sn_collated_at__c",
    "qw_snpf__sn_salesnow_url__c",
]

# CSVヘッダー（読みやすい名前）
CSV_HEADERS = [
    "hubspot_id",
    "name",
    # 企業基本情報
    "corporate_number",
    "sn_company_name",
    "company_name_kana",
    "company_url",
    "established_date",
    "listing_category",
    "tob_toc",
    "president_name",
    "phone_number",
    "mail_address",
    # 所在地
    "prefecture",
    "address",
    "postal_code",
    # 業種
    "sn_industry",
    "sn_industry2",
    "sn_industry_subs",
    "business_tags",
    "business_description",
    "jccode",
    # 従業員数・増減率
    "employee_count",
    "employee_range",
    "group_employee_count",
    "employee_delta_1m",
    "employee_delta_3m",
    "employee_delta_6m",
    "employee_delta_1y",
    "employee_delta_2y",
    # 財務
    "capital_stock",
    "capital_stock_range",
    "sales_amount",
    "sales_range",
    "net_sales",
    "profit_loss",
    "is_estimated_sales",
    "period_month",
    # スコア
    "credit_score",
    "salesnow_score",
    # 調達
    "latest_event_date",
    "latest_raised_series",
    "latest_round_post_valuation",
    "market_cap",
    # メタ
    "label",
    "collated_at",
    "salesnow_url",
]

API_BASE = "https://api.hubapi.com/crm/v3/objects/companies"
PAGE_SIZE = 100  # HubSpot最大


def load_token() -> str:
    """HubSpotアクセストークンを.envファイルから読み込む"""
    if not ENV_FILE.exists():
        print(f"エラー: .envファイルが見つかりません: {ENV_FILE}")
        sys.exit(1)

    with open(ENV_FILE, "r", encoding="utf-8") as f:
        for line in f:
            line = line.strip()
            if line.startswith("HUBSPOT_ACCESS_TOKEN="):
                return line.split("=", 1)[1].strip()

    print("エラー: HUBSPOT_ACCESS_TOKENが.envに見つかりません")
    sys.exit(1)


def load_checkpoint() -> dict:
    """チェックポイントファイルを読み込む"""
    if CHECKPOINT_FILE.exists():
        with open(CHECKPOINT_FILE, "r", encoding="utf-8") as f:
            return json.load(f)
    return {"after": None, "total_fetched": 0}


def save_checkpoint(after: str | None, total_fetched: int):
    """チェックポイントを保存"""
    with open(CHECKPOINT_FILE, "w", encoding="utf-8") as f:
        json.dump({"after": after, "total_fetched": total_fetched}, f)


def fetch_page(token: str, after: str | None) -> dict:
    """HubSpot Companies APIから1ページ取得"""
    props = "&".join(f"properties={p}" for p in PROPERTIES)
    url = f"{API_BASE}?limit={PAGE_SIZE}&{props}"
    if after:
        url += f"&after={after}"

    req = Request(url)
    req.add_header("Authorization", f"Bearer {token}")
    req.add_header("Content-Type", "application/json")

    for attempt in range(3):
        try:
            with urlopen(req, timeout=30) as resp:
                return json.loads(resp.read().decode("utf-8"))
        except HTTPError as e:
            if e.code == 429:
                # レートリミット: 10秒待機
                retry_after = int(e.headers.get("Retry-After", "10"))
                print(f"  レートリミット。{retry_after}秒待機...")
                time.sleep(retry_after)
                continue
            raise
        except (URLError, TimeoutError) as e:
            if attempt < 2:
                print(f"  接続エラー（リトライ {attempt+1}/3）: {e}")
                time.sleep(5)
                continue
            raise

    return {}


def extract_row(company: dict) -> list:
    """APIレスポンスからCSV行を抽出"""
    props = company.get("properties", {})
    return [props.get(p, "") or "" for p in PROPERTIES]


def main():
    token = load_token()
    checkpoint = load_checkpoint()
    after = checkpoint["after"]
    total_fetched = checkpoint["total_fetched"]

    # CSV書き込みモード: 再開時はappend、新規はwrite
    is_resume = after is not None and OUTPUT_CSV.exists()
    mode = "a" if is_resume else "w"

    if is_resume:
        print(f"チェックポイントから再開: {total_fetched}件取得済み, after={after[:20]}...")
    else:
        print("新規取得開始")

    DATA_DIR.mkdir(parents=True, exist_ok=True)

    with open(OUTPUT_CSV, mode, newline="", encoding="utf-8") as f:
        writer = csv.writer(f)
        if not is_resume:
            writer.writerow(CSV_HEADERS)

        page_num = 0
        while True:
            page_num += 1
            print(f"ページ {page_num}: 取得中... (累計: {total_fetched}件)", end="", flush=True)

            try:
                data = fetch_page(token, after)
            except Exception as e:
                print(f"\nエラー: {e}")
                print(f"チェックポイント保存。再実行で再開できます。")
                save_checkpoint(after, total_fetched)
                sys.exit(1)

            results = data.get("results", [])
            if not results:
                print(" → 結果なし。終了。")
                break

            for company in results:
                writer.writerow(extract_row(company))

            total_fetched += len(results)
            print(f" → {len(results)}件")

            # 次ページ
            paging = data.get("paging", {})
            next_page = paging.get("next", {})
            after = next_page.get("after")

            # チェックポイント保存（10ページごと）
            if page_num % 10 == 0:
                save_checkpoint(after, total_fetched)
                f.flush()
                print(f"  [チェックポイント保存: {total_fetched}件]")

            if not after:
                print("全ページ取得完了。")
                break

            # レートリミット回避: 100ms待機
            time.sleep(0.1)

    # 完了: チェックポイント削除
    if CHECKPOINT_FILE.exists():
        CHECKPOINT_FILE.unlink()

    print(f"\n完了: {total_fetched}件 → {OUTPUT_CSV}")

    # 業界ユニーク値の集計
    print("\n--- 業界フィールド集計 ---")
    analyze_industries(OUTPUT_CSV)


def analyze_industries(csv_path: Path):
    """業界フィールドのユニーク値と充填率を集計"""
    from collections import Counter

    sn_industry = Counter()
    sn_industry2 = Counter()
    total = 0
    filled_industry = 0
    filled_industry2 = 0
    filled_corp_num = 0
    filled_pref = 0

    with open(csv_path, "r", encoding="utf-8") as f:
        reader = csv.DictReader(f)
        for row in reader:
            total += 1
            ind = row.get("sn_industry", "").strip()
            ind2 = row.get("sn_industry2", "").strip()
            corp = row.get("corporate_number", "").strip()
            pref = row.get("prefecture", "").strip()

            if ind:
                sn_industry[ind] += 1
                filled_industry += 1
            if ind2:
                sn_industry2[ind2] += 1
                filled_industry2 += 1
            if corp:
                filled_corp_num += 1
            if pref:
                filled_pref += 1

    print(f"総件数: {total}")
    print(f"法人番号充填率: {filled_corp_num}/{total} ({filled_corp_num/total*100:.1f}%)" if total else "")
    print(f"都道府県充填率: {filled_pref}/{total} ({filled_pref/total*100:.1f}%)" if total else "")
    print(f"大業界充填率: {filled_industry}/{total} ({filled_industry/total*100:.1f}%)" if total else "")
    print(f"中業界充填率: {filled_industry2}/{total} ({filled_industry2/total*100:.1f}%)" if total else "")

    print(f"\n大業界ユニーク値 ({len(sn_industry)}種):")
    for name, cnt in sn_industry.most_common():
        print(f"  {name}: {cnt}件")

    print(f"\n中業界ユニーク値 上位30 ({len(sn_industry2)}種中):")
    for name, cnt in sn_industry2.most_common(30):
        print(f"  {name}: {cnt}件")


if __name__ == "__main__":
    main()
