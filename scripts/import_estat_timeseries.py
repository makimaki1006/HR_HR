"""
e-Stat API から有効求人倍率の年度次時系列データを取得し hellowork.db に格納
==========================================================================
総務省「社会・人口統計体系」（statsDataId=0000010206）の都道府県別
年度次データを e-Stat JSON API 経由で取得し、ローカル SQLite に格納する。

使い方:
    python import_estat_timeseries.py --app-id YOUR_APP_ID
    python import_estat_timeseries.py --app-id YOUR_APP_ID --years 10 --dry-run

取得項目:
    - 有効求人倍率（パートタイム含む, F03105）
    - 有効求人倍率（パートタイム除く, F03103）
"""

import argparse
import json
import os
import sqlite3
import sys
import time
import urllib.error
import urllib.parse
import urllib.request

SCRIPT_DIR = os.path.dirname(os.path.abspath(__file__))
DEFAULT_DB_PATH = os.path.join(os.path.dirname(SCRIPT_DIR), "data", "hellowork.db")

# e-Stat API
ESTAT_BASE = "https://api.e-stat.go.jp/rest/3.0/app/json"
STATS_DATA_ID = "0000010206"  # 社会・人口統計体系 F労働

# 取得する指標コード
CAT_CODES = {
    "#F03105": "有効求人倍率（パートタイム含む）",
    "#F03103": "有効求人倍率（パートタイム除く）",
}

# リトライ設定
MAX_RETRIES = 3
RETRY_WAIT = 2
REQUEST_INTERVAL = 1

# 都道府県コード → 名称
AREA_CODE_TO_PREF = {
    "00000": "全国",
    "01000": "北海道", "02000": "青森県", "03000": "岩手県", "04000": "宮城県",
    "05000": "秋田県", "06000": "山形県", "07000": "福島県", "08000": "茨城県",
    "09000": "栃木県", "10000": "群馬県", "11000": "埼玉県", "12000": "千葉県",
    "13000": "東京都", "14000": "神奈川県", "15000": "新潟県", "16000": "富山県",
    "17000": "石川県", "18000": "福井県", "19000": "山梨県", "20000": "長野県",
    "21000": "岐阜県", "22000": "静岡県", "23000": "愛知県", "24000": "三重県",
    "25000": "滋賀県", "26000": "京都府", "27000": "大阪府", "28000": "兵庫県",
    "29000": "奈良県", "30000": "和歌山県", "31000": "鳥取県", "32000": "島根県",
    "33000": "岡山県", "34000": "広島県", "35000": "山口県", "36000": "徳島県",
    "37000": "香川県", "38000": "愛媛県", "39000": "高知県", "40000": "福岡県",
    "41000": "佐賀県", "42000": "長崎県", "43000": "熊本県", "44000": "大分県",
    "45000": "宮崎県", "46000": "鹿児島県", "47000": "沖縄県",
}


def api_request(url: str, params: dict) -> dict:
    """e-Stat API にリクエストを送信"""
    query = urllib.parse.urlencode(params)
    full_url = f"{url}?{query}"

    for attempt in range(1, MAX_RETRIES + 1):
        try:
            req = urllib.request.Request(full_url)
            with urllib.request.urlopen(req, timeout=60) as resp:
                return json.loads(resp.read().decode("utf-8"))
        except (urllib.error.URLError, urllib.error.HTTPError, OSError) as e:
            print(f"  [警告] リクエスト失敗 (試行 {attempt}/{MAX_RETRIES}): {e}")
            if attempt < MAX_RETRIES:
                time.sleep(RETRY_WAIT * attempt)
            else:
                raise RuntimeError(f"API リクエスト失敗: {full_url}") from e
    raise RuntimeError("unreachable")


def fetch_data(app_id: str, years: int) -> list[dict]:
    """有効求人倍率データを全都道府県×指定年数分取得"""
    # 開始年度を計算（例: years=10, 現在2026 → 2016年度から）
    from datetime import datetime
    current_year = datetime.now().year
    start_year = current_year - years

    # timeコード: 2016100000 形式
    cd_time_from = f"{start_year}100000"

    # 指標コード（カンマ区切り）
    cd_cat01 = ",".join(CAT_CODES.keys())

    print(f"  期間: {start_year}年度 〜 最新")
    print(f"  指標: {', '.join(CAT_CODES.values())}")

    params = {
        "appId": app_id,
        "lang": "J",
        "statsDataId": STATS_DATA_ID,
        "cdCat01": cd_cat01,
        "cdTimeFrom": cd_time_from,
        "metaGetFlg": "Y",
        "cntGetFlg": "N",
        "limit": 100000,
    }

    data = api_request(f"{ESTAT_BASE}/getStatsData", params)
    result = data.get("GET_STATS_DATA", {}).get("RESULT", {})
    status = int(result.get("STATUS", -1))

    if status != 0:
        raise RuntimeError(
            f"getStatsData エラー: STATUS={status}, "
            f"MESSAGE={result.get('ERROR_MSG', '不明')}"
        )

    stat_data = data["GET_STATS_DATA"]["STATISTICAL_DATA"]
    values = stat_data.get("DATA_INF", {}).get("VALUE", [])
    if isinstance(values, dict):
        values = [values]

    print(f"  取得レコード数: {len(values)}")

    # パース
    records = []
    for v in values:
        area_code = v.get("@area", "")
        pref = AREA_CODE_TO_PREF.get(area_code)
        if pref is None:
            continue

        time_code = v.get("@time", "")
        # 2024100000 → 2024
        fiscal_year = time_code[:4] if len(time_code) >= 4 else None
        if fiscal_year is None:
            continue

        cat_code = v.get("@cat01", "")
        val_str = v.get("$", "")

        # 欠損値チェック
        if val_str in ("", "-", "…", "x", "*"):
            continue

        try:
            val = float(val_str)
        except ValueError:
            continue

        # F03105=パートタイム含む（メイン）, F03103=パートタイム除く
        indicator = "ratio_total" if cat_code == "#F03105" else "ratio_excl_part"

        records.append({
            "prefecture": pref,
            "fiscal_year": fiscal_year,
            "indicator": indicator,
            "value": val,
        })

    return records


def pivot_records(records: list[dict]) -> list[dict]:
    """レコードを (prefecture, fiscal_year) でピボットする"""
    pivot = {}
    for r in records:
        key = (r["prefecture"], r["fiscal_year"])
        if key not in pivot:
            pivot[key] = {
                "prefecture": r["prefecture"],
                "fiscal_year": r["fiscal_year"],
                "ratio_total": None,
                "ratio_excl_part": None,
            }
        pivot[key][r["indicator"]] = r["value"]

    return list(pivot.values())


def save_to_db(db_path: str, rows: list[dict], dry_run: bool = False):
    """SQLiteに保存"""
    if dry_run:
        print(f"\n[dry-run] {len(rows)} 行を表示（DB書き込みなし）")
        for row in rows[:20]:
            print(f"  {row['prefecture']} {row['fiscal_year']}年度: "
                  f"含む={row['ratio_total']} 除く={row['ratio_excl_part']}")
        if len(rows) > 20:
            print(f"  ... 残り {len(rows) - 20} 行")
        return

    conn = sqlite3.connect(db_path)
    cur = conn.cursor()

    # テーブル作成
    cur.execute("""
        CREATE TABLE IF NOT EXISTS v2_external_job_openings_ratio (
            prefecture TEXT NOT NULL,
            fiscal_year TEXT NOT NULL,
            ratio_total REAL,
            ratio_excl_part REAL,
            PRIMARY KEY (prefecture, fiscal_year)
        )
    """)

    # INSERT OR REPLACE
    cur.executemany("""
        INSERT OR REPLACE INTO v2_external_job_openings_ratio
        (prefecture, fiscal_year, ratio_total, ratio_excl_part)
        VALUES (:prefecture, :fiscal_year, :ratio_total, :ratio_excl_part)
    """, rows)

    conn.commit()

    # 検証
    count = cur.execute("SELECT COUNT(*) FROM v2_external_job_openings_ratio").fetchone()[0]
    prefs = cur.execute("SELECT COUNT(DISTINCT prefecture) FROM v2_external_job_openings_ratio").fetchone()[0]
    years = cur.execute("SELECT COUNT(DISTINCT fiscal_year) FROM v2_external_job_openings_ratio").fetchone()[0]
    yr_range = cur.execute(
        "SELECT MIN(fiscal_year), MAX(fiscal_year) FROM v2_external_job_openings_ratio"
    ).fetchone()

    conn.close()

    print(f"\n[検証] DB書き込み完了:")
    print(f"  総行数: {count}")
    print(f"  都道府県数: {prefs}（全国含む）")
    print(f"  年度数: {years}")
    print(f"  期間: {yr_range[0]}年度 〜 {yr_range[1]}年度")


def main():
    parser = argparse.ArgumentParser(
        description="e-Stat APIから有効求人倍率時系列を取得"
    )
    parser.add_argument("--app-id", required=True, help="e-Stat APIのappId")
    parser.add_argument("--db-path", default=DEFAULT_DB_PATH, help="hellowork.dbパス")
    parser.add_argument("--years", type=int, default=10, help="取得年数（デフォルト: 10）")
    parser.add_argument("--dry-run", action="store_true", help="DB書き込みなし")
    args = parser.parse_args()

    print("=" * 60)
    print("e-Stat 有効求人倍率 時系列データ取得")
    print(f"  取得年数: {args.years}")
    print(f"  DB: {args.db_path}")
    print(f"  dry-run: {args.dry_run}")
    print("=" * 60)

    # データ取得
    print("\n[1/3] e-Stat APIからデータ取得中...")
    records = fetch_data(args.app_id, args.years)
    print(f"  パース済みレコード: {len(records)}")

    # ピボット
    print("\n[2/3] データ整形中...")
    rows = pivot_records(records)
    print(f"  ピボット後: {len(rows)} 行")

    # 保存
    print("\n[3/3] DB保存中...")
    save_to_db(args.db_path, rows, args.dry_run)

    print("\n完了!")


if __name__ == "__main__":
    main()
