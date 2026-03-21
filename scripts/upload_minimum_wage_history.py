"""
最低賃金の年度別推移データを Turso にアップロード
==================================================
v2_external_minimum_wage_history テーブルに以下を格納:
  - 全国加重平均: 2016-2025 (10年分)
  - 都道府県別: 2023-2025 (3年分)

使い方:
    python upload_minimum_wage_history.py --url <TURSO_URL> --token <TURSO_TOKEN>
    python upload_minimum_wage_history.py --dry-run  # 確認のみ

環境変数でも指定可:
    TURSO_EXTERNAL_URL   Turso DB URL (https://...)
    TURSO_EXTERNAL_TOKEN Turso Auth Token

注意: 2025年の都道府県別データは既存テーブル v2_external_minimum_wage から取得する。
"""
import json
import os
import sys
import argparse
import urllib.request
import urllib.error

# === 全国加重平均（2016-2025） ===
NATIONAL_AVERAGES = {
    2016: 823,
    2017: 848,
    2018: 874,
    2019: 901,
    2020: 902,
    2021: 930,
    2022: 961,
    2023: 1004,
    2024: 1055,
    2025: 1113,
}

# === 都道府県別データ（2023年度） ===
PREF_2023 = {
    "北海道": 960, "青森県": 898, "岩手県": 893, "宮城県": 923, "秋田県": 897,
    "山形県": 900, "福島県": 900, "茨城県": 953, "栃木県": 954, "群馬県": 935,
    "埼玉県": 1028, "千葉県": 1026, "東京都": 1113, "神奈川県": 1112,
    "新潟県": 931, "富山県": 948, "石川県": 933, "福井県": 931, "山梨県": 938,
    "長野県": 948, "岐阜県": 950, "静岡県": 984, "愛知県": 1027, "三重県": 973,
    "滋賀県": 967, "京都府": 1008, "大阪府": 1064, "兵庫県": 1001, "奈良県": 936,
    "和歌山県": 929, "鳥取県": 900, "島根県": 904, "岡山県": 932, "広島県": 970,
    "山口県": 928, "徳島県": 896, "香川県": 918, "愛媛県": 897, "高知県": 897,
    "福岡県": 941, "佐賀県": 900, "長崎県": 898, "熊本県": 898, "大分県": 899,
    "宮崎県": 897, "鹿児島県": 897, "沖縄県": 896,
}

# === 都道府県別データ（2024年度） ===
PREF_2024 = {
    "北海道": 1010, "青森県": 953, "岩手県": 952, "宮城県": 973, "秋田県": 951,
    "山形県": 955, "福島県": 955, "茨城県": 1005, "栃木県": 1004, "群馬県": 985,
    "埼玉県": 1078, "千葉県": 1076, "東京都": 1163, "神奈川県": 1162,
    "新潟県": 985, "富山県": 998, "石川県": 984, "福井県": 984, "山梨県": 988,
    "長野県": 998, "岐阜県": 1001, "静岡県": 1034, "愛知県": 1077, "三重県": 1023,
    "滋賀県": 1017, "京都府": 1058, "大阪府": 1114, "兵庫県": 1052, "奈良県": 986,
    "和歌山県": 980, "鳥取県": 957, "島根県": 962, "岡山県": 982, "広島県": 1020,
    "山口県": 979, "徳島県": 955, "香川県": 970, "愛媛県": 956, "高知県": 952,
    "福岡県": 992, "佐賀県": 956, "長崎県": 953, "熊本県": 952, "大分県": 954,
    "宮崎県": 952, "鹿児島県": 953, "沖縄県": 952,
}

TABLE_NAME = "v2_external_minimum_wage_history"

CREATE_TABLE_SQL = f"""
    CREATE TABLE IF NOT EXISTS {TABLE_NAME} (
        fiscal_year INTEGER NOT NULL,
        prefecture TEXT NOT NULL,
        hourly_min_wage INTEGER NOT NULL,
        PRIMARY KEY (fiscal_year, prefecture)
    )
"""


def turso_pipeline(url, token, statements):
    """Turso HTTP Pipeline API で複数SQLを一括実行（urllib版）"""
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

    body = json.dumps({"requests": requests_list}).encode("utf-8")
    req = urllib.request.Request(
        f"{url}/v2/pipeline",
        data=body,
        headers=headers,
        method="POST",
    )

    try:
        with urllib.request.urlopen(req, timeout=60) as resp:
            data = json.loads(resp.read().decode("utf-8"))
    except urllib.error.HTTPError as e:
        raise Exception(f"Turso API error {e.code}: {e.read().decode()[:300]}")

    errors = [r for r in data.get("results", []) if r.get("type") == "error"]
    if errors:
        raise Exception(f"SQL errors: {errors[:3]}")

    return data


def fetch_2025_from_existing(url, token):
    """既存テーブル v2_external_minimum_wage から2025年データを取得"""
    sql = "SELECT prefecture, hourly_min_wage FROM v2_external_minimum_wage"
    try:
        data = turso_pipeline(url, token, [(sql, None)])
        result = data["results"][0]["response"]["result"]
        cols = [c["name"] for c in result["cols"]]
        pref_idx = cols.index("prefecture")
        wage_idx = cols.index("hourly_min_wage")

        pref_wages = {}
        for row in result["rows"]:
            pref = row[pref_idx]["value"]
            wage = int(row[wage_idx]["value"])
            pref_wages[pref] = wage

        print(f"  v2_external_minimum_wage から {len(pref_wages)} 都道府県の2025年データを取得")
        return pref_wages
    except Exception as e:
        print(f"  WARNING: 2025年データ取得失敗 ({e})")
        print(f"  → 全国平均のみ使用します")
        return {}


def build_rows(pref_2025):
    """全挿入行を構築"""
    rows = []

    # 全国加重平均: 2016-2025
    for year, wage in NATIONAL_AVERAGES.items():
        rows.append((year, "全国", wage))

    # 都道府県別: 2023
    for pref, wage in PREF_2023.items():
        rows.append((2023, pref, wage))

    # 都道府県別: 2024
    for pref, wage in PREF_2024.items():
        rows.append((2024, pref, wage))

    # 都道府県別: 2025（既存テーブルから取得）
    for pref, wage in pref_2025.items():
        rows.append((2025, pref, wage))

    return rows


def main():
    parser = argparse.ArgumentParser(
        description="最低賃金の年度別推移データをTursoにアップロード"
    )
    parser.add_argument("--url", default=os.environ.get("TURSO_EXTERNAL_URL", ""),
                        help="Turso URL")
    parser.add_argument("--token", default=os.environ.get("TURSO_EXTERNAL_TOKEN", ""),
                        help="Turso Token")
    parser.add_argument("--dry-run", action="store_true",
                        help="実際にはアップロードしない")
    args = parser.parse_args()

    turso_url = args.url
    turso_token = args.token

    if not turso_url or not turso_token:
        print("ERROR: --url と --token が必要です")
        print("  （または環境変数 TURSO_EXTERNAL_URL, TURSO_EXTERNAL_TOKEN）")
        sys.exit(1)

    # libsql:// → https:// 変換
    if turso_url.startswith("libsql://"):
        turso_url = turso_url.replace("libsql://", "https://")

    print(f"Turso:    {turso_url}")
    print(f"Dry-run:  {args.dry_run}")
    print()

    # 2025年データを既存テーブルから取得
    print("=== Step 1: 2025年都道府県データ取得 ===")
    pref_2025 = fetch_2025_from_existing(turso_url, turso_token)

    # 全行構築
    all_rows = build_rows(pref_2025)
    print(f"\n=== Step 2: データ構築完了 ===")
    print(f"  全国平均: {len(NATIONAL_AVERAGES)} 年分 (2016-2025)")
    print(f"  2023都道府県: {len(PREF_2023)} 件")
    print(f"  2024都道府県: {len(PREF_2024)} 件")
    print(f"  2025都道府県: {len(pref_2025)} 件")
    print(f"  合計: {len(all_rows)} 行")

    if args.dry_run:
        print("\n[dry-run] 実際のアップロードはスキップしました")
        # サンプル表示
        print("\nサンプルデータ:")
        for row in all_rows[:5]:
            print(f"  {row}")
        print("  ...")
        for row in all_rows[-3:]:
            print(f"  {row}")
        return

    # テーブル作成（DROP + CREATE で冪等性を確保）
    print(f"\n=== Step 3: テーブル作成 ({TABLE_NAME}) ===")
    turso_pipeline(turso_url, turso_token, [
        (f"DROP TABLE IF EXISTS {TABLE_NAME}", None),
        (CREATE_TABLE_SQL, None),
    ])
    print("  テーブル作成完了")

    # バッチINSERT
    print(f"\n=== Step 4: データ挿入 ===")
    insert_sql = (
        f"INSERT INTO {TABLE_NAME} (fiscal_year, prefecture, hourly_min_wage) "
        f"VALUES (?1, ?2, ?3)"
    )

    BATCH_SIZE = 100
    total = 0
    for i in range(0, len(all_rows), BATCH_SIZE):
        batch = all_rows[i:i + BATCH_SIZE]
        stmts = [(insert_sql, list(row)) for row in batch]
        turso_pipeline(turso_url, turso_token, stmts)
        total += len(batch)
        print(f"  {total}/{len(all_rows)} 行完了")

    print(f"\n=== Step 5: 検証 ===")
    # 行数検証
    data = turso_pipeline(turso_url, turso_token, [
        (f"SELECT COUNT(*) as cnt FROM {TABLE_NAME}", None),
    ])
    count = data["results"][0]["response"]["result"]["rows"][0][0]["value"]
    print(f"  テーブル行数: {count}")

    # 全国平均のサンプル検証
    data = turso_pipeline(turso_url, turso_token, [
        (f"SELECT fiscal_year, hourly_min_wage FROM {TABLE_NAME} "
         f"WHERE prefecture = '全国' ORDER BY fiscal_year", None),
    ])
    result = data["results"][0]["response"]["result"]
    print("  全国加重平均:")
    for row in result["rows"]:
        fy = row[0]["value"]
        wage = row[1]["value"]
        print(f"    {fy}年度: {wage}円")

    # 都道府県サンプル（東京都）
    data = turso_pipeline(turso_url, turso_token, [
        (f"SELECT fiscal_year, hourly_min_wage FROM {TABLE_NAME} "
         f"WHERE prefecture = '東京都' ORDER BY fiscal_year", None),
    ])
    result = data["results"][0]["response"]["result"]
    print("  東京都:")
    for row in result["rows"]:
        fy = row[0]["value"]
        wage = row[1]["value"]
        print(f"    {fy}年度: {wage}円")

    print(f"\n完了: {TABLE_NAME} に {total} 行アップロードしました")


if __name__ == "__main__":
    main()
