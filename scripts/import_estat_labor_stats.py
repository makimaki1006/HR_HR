"""
e-Stat API から労働・雇用関連指標の時系列データを取得し hellowork.db に格納
==========================================================================
総務省「社会・人口統計体系」（statsDataId=0000010206）の都道府県別
年度次データを e-Stat JSON API 経由で取得し、ローカル SQLite に格納する。

有効求人倍率（v2_external_job_openings_ratio）以外の労働関連指標を取得する。

使い方:
    python import_estat_labor_stats.py --app-id YOUR_APP_ID
    python import_estat_labor_stats.py --app-id YOUR_APP_ID --years 10 --dry-run

取得項目:
    - 完全失業率（総合/男/女）
    - 就業者比率 / 雇用者比率
    - 離職率 / 転職率 / 転職者比率
    - 就職率 / 充足率
    - 高齢就業者割合（65歳以上）
    - 月間平均実労働時間数（男/女）
    - きまって支給する現金給与月額（男/女）
    - パートタイム時給（男/女）
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
from collections import defaultdict
from datetime import datetime

SCRIPT_DIR = os.path.dirname(os.path.abspath(__file__))
DEFAULT_DB_PATH = os.path.join(os.path.dirname(SCRIPT_DIR), "data", "hellowork.db")

# e-Stat API 設定
ESTAT_BASE = "https://api.e-stat.go.jp/rest/3.0/app/json"
STATS_DATA_ID = "0000010206"  # 社会・人口統計体系 F労働

# リトライ設定
MAX_RETRIES = 3
RETRY_WAIT = 2

# 取得する指標の定義
# キー: e-Stat cat01コード
# 値: (DBカラム名, 説明, 単位)
INDICATORS = {
    # === 雇用状況指標 ===
    "#F01301":   ("unemployment_rate",         "完全失業率",                "％"),
    "#F0130101": ("unemployment_rate_male",     "完全失業率（男）",          "％"),
    "#F0130102": ("unemployment_rate_female",   "完全失業率（女）",          "％"),
    "#F01102":   ("employment_rate",            "就業者比率",                "％"),
    "#F02301":   ("employee_rate",              "雇用者比率",                "％"),

    # === 労働移動指標 ===
    "#F04102":   ("separation_rate",            "離職率",                    "％"),
    "#F04101":   ("turnover_rate",              "転職率",                    "％"),
    "#F04105":   ("job_changer_rate",           "転職者比率",                "％"),

    # === 求職・求人マッチング指標 ===
    "#F03101":   ("placement_rate",             "就職率",                    "％"),
    "#F03104":   ("fulfillment_rate",           "充足率",                    "％"),

    # === 高齢者就業 ===
    "#F0350303": ("elderly_employment_rate",    "高齢就業者割合（65歳以上）", "％"),

    # === 労働時間（2020年以降の新系列）===
    "#F0610103": ("working_hours_male",         "月間平均実労働時間数（男）", "時間"),
    "#F0610104": ("working_hours_female",       "月間平均実労働時間数（女）", "時間"),

    # === 賃金（2020年以降の新系列）===
    "#F0620103": ("monthly_salary_male",        "きまって支給する現金給与月額（男）", "千円"),
    "#F0620104": ("monthly_salary_female",      "きまって支給する現金給与月額（女）", "千円"),

    # === パートタイム時給（2020年以降の新系列）===
    "#F06206":   ("part_time_wage_female",      "パートタイム時給（女）",     "円"),
    "#F06207":   ("part_time_wage_male",        "パートタイム時給（男）",     "円"),
}

# 旧系列（2019年以前）→ 新系列と同じDBカラムにマッピング
# データ期間が重複しないため安全にマージ可能
LEGACY_INDICATORS = {
    "#F0610101": ("working_hours_male",         "月間平均実労働時間数（男）（旧系列）", "時間"),
    "#F0610102": ("working_hours_female",       "月間平均実労働時間数（女）（旧系列）", "時間"),
    "#F0620101": ("monthly_salary_male",        "きまって支給する現金給与月額（男）（旧系列）", "千円"),
    "#F0620102": ("monthly_salary_female",      "きまって支給する現金給与月額（女）（旧系列）", "千円"),
    "#F06204":   ("part_time_wage_female",      "パートタイム時給（女）（旧系列）",     "円"),
    "#F06205":   ("part_time_wage_male",        "パートタイム時給（男）（旧系列）",     "円"),
}

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
    """e-Stat API にリクエストを送信（リトライ付き）"""
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


def fetch_indicators(app_id: str, cat_codes: dict, years: int,
                     label: str = "") -> list[dict]:
    """指定した指標コード群のデータを取得してパースする"""
    current_year = datetime.now().year
    start_year = current_year - years
    cd_time_from = f"{start_year}100000"
    cd_cat01 = ",".join(cat_codes.keys())

    if label:
        print(f"\n  --- {label} ---")
    print(f"  指標数: {len(cat_codes)}")
    print(f"  期間: {start_year}年度 〜 最新")

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
    skipped = 0
    for v in values:
        area_code = v.get("@area", "")
        pref = AREA_CODE_TO_PREF.get(area_code)
        if pref is None:
            continue

        time_code = v.get("@time", "")
        fiscal_year = time_code[:4] if len(time_code) >= 4 else None
        if fiscal_year is None:
            continue

        cat_code = v.get("@cat01", "")
        val_str = v.get("$", "")

        # 欠損値チェック
        if val_str in ("", "-", "…", "x", "*"):
            skipped += 1
            continue

        try:
            val = float(val_str)
        except ValueError:
            skipped += 1
            continue

        # 指標コードからDBカラム名を取得
        indicator_info = cat_codes.get(cat_code)
        if indicator_info is None:
            continue
        col_name = indicator_info[0]

        records.append({
            "prefecture": pref,
            "fiscal_year": fiscal_year,
            "indicator": col_name,
            "value": val,
        })

    print(f"  有効レコード: {len(records)}, スキップ: {skipped}")
    return records


def pivot_records(records: list[dict]) -> list[dict]:
    """
    レコードを (prefecture, fiscal_year) でピボットし、
    各指標をカラムとして横持ちにする
    """
    # 全カラム名を収集
    all_columns = set()
    for ind_info in INDICATORS.values():
        all_columns.add(ind_info[0])
    for ind_info in LEGACY_INDICATORS.values():
        all_columns.add(ind_info[0])

    pivot = {}
    for r in records:
        key = (r["prefecture"], r["fiscal_year"])
        if key not in pivot:
            row = {
                "prefecture": r["prefecture"],
                "fiscal_year": r["fiscal_year"],
            }
            for col in all_columns:
                row[col] = None
            pivot[key] = row
        pivot[key][r["indicator"]] = r["value"]

    return list(pivot.values())


def save_to_db(db_path: str, rows: list[dict], dry_run: bool = False):
    """SQLite に保存"""
    if not rows:
        print("\n[警告] 保存するデータがありません")
        return

    if dry_run:
        print(f"\n[dry-run] {len(rows)} 行を表示（DB書き込みなし）")
        # 指標ごとの非NULL数を表示
        col_counts = defaultdict(int)
        for row in rows:
            for k, v in row.items():
                if k not in ("prefecture", "fiscal_year") and v is not None:
                    col_counts[k] += 1
        print("\n  指標別データ数:")
        for col, cnt in sorted(col_counts.items()):
            print(f"    {col}: {cnt} 件")

        # サンプル表示（東京都の直近3年度）
        tokyo_rows = [r for r in rows if r["prefecture"] == "東京都"]
        tokyo_rows.sort(key=lambda x: x["fiscal_year"], reverse=True)
        print("\n  サンプル（東京都・直近3年度）:")
        for row in tokyo_rows[:3]:
            vals = {k: v for k, v in row.items()
                    if k not in ("prefecture", "fiscal_year") and v is not None}
            print(f"    {row['fiscal_year']}年度: {vals}")
        return

    conn = sqlite3.connect(db_path)
    cur = conn.cursor()

    # 全カラム名を収集
    all_columns = sorted(set(
        ind[0] for ind in list(INDICATORS.values()) + list(LEGACY_INDICATORS.values())
    ))

    # テーブル作成
    col_defs = ",\n            ".join(f"{col} REAL" for col in all_columns)
    cur.execute(f"""
        CREATE TABLE IF NOT EXISTS v2_external_labor_stats (
            prefecture TEXT NOT NULL,
            fiscal_year TEXT NOT NULL,
            {col_defs},
            PRIMARY KEY (prefecture, fiscal_year)
        )
    """)

    # 既存テーブルのカラムを確認し、不足分を追加
    existing_cols = set()
    for col_info in cur.execute("PRAGMA table_info(v2_external_labor_stats)"):
        existing_cols.add(col_info[1])
    for col in all_columns:
        if col not in existing_cols:
            cur.execute(f"ALTER TABLE v2_external_labor_stats ADD COLUMN {col} REAL")
            print(f"  [INFO] カラム追加: {col}")

    # INSERT OR REPLACE
    placeholders = ", ".join(f":{col}" for col in ["prefecture", "fiscal_year"] + all_columns)
    col_names = ", ".join(["prefecture", "fiscal_year"] + all_columns)
    cur.executemany(f"""
        INSERT OR REPLACE INTO v2_external_labor_stats
        ({col_names})
        VALUES ({placeholders})
    """, rows)

    conn.commit()

    # 検証
    count = cur.execute("SELECT COUNT(*) FROM v2_external_labor_stats").fetchone()[0]
    prefs = cur.execute(
        "SELECT COUNT(DISTINCT prefecture) FROM v2_external_labor_stats"
    ).fetchone()[0]
    years_count = cur.execute(
        "SELECT COUNT(DISTINCT fiscal_year) FROM v2_external_labor_stats"
    ).fetchone()[0]
    yr_range = cur.execute(
        "SELECT MIN(fiscal_year), MAX(fiscal_year) FROM v2_external_labor_stats"
    ).fetchone()

    # 指標ごとの非NULL件数を表示
    print(f"\n[検証] DB書き込み完了:")
    print(f"  総行数: {count}")
    print(f"  都道府県数: {prefs}（全国含む）")
    print(f"  年度数: {years_count}")
    print(f"  期間: {yr_range[0]}年度 〜 {yr_range[1]}年度")

    print("\n  指標別非NULL件数:")
    for col in all_columns:
        non_null = cur.execute(
            f"SELECT COUNT({col}) FROM v2_external_labor_stats WHERE {col} IS NOT NULL"
        ).fetchone()[0]
        # 対応する日本語名を取得
        jp_name = col
        for info in list(INDICATORS.values()) + list(LEGACY_INDICATORS.values()):
            if info[0] == col:
                jp_name = info[1]
                break
        print(f"    {col} ({jp_name}): {non_null} 件")

    # サンプルデータ表示（東京都の直近年度）
    sample = cur.execute("""
        SELECT * FROM v2_external_labor_stats
        WHERE prefecture = '東京都'
        ORDER BY fiscal_year DESC
        LIMIT 3
    """).fetchall()
    col_names_db = [desc[0] for desc in cur.description]
    print("\n  サンプル（東京都・直近3年度）:")
    for row in sample:
        row_dict = dict(zip(col_names_db, row))
        fy = row_dict["fiscal_year"]
        vals = {k: v for k, v in row_dict.items()
                if k not in ("prefecture", "fiscal_year") and v is not None}
        print(f"    {fy}年度: {vals}")

    conn.close()


def main():
    parser = argparse.ArgumentParser(
        description="e-Stat APIから労働・雇用関連指標の時系列データを取得"
    )
    parser.add_argument("--app-id", required=True, help="e-Stat APIのappId")
    parser.add_argument("--db-path", default=DEFAULT_DB_PATH, help="hellowork.dbパス")
    parser.add_argument("--years", type=int, default=10, help="取得年数（デフォルト: 10）")
    parser.add_argument("--dry-run", action="store_true", help="DB書き込みなし")
    args = parser.parse_args()

    print("=" * 60)
    print("e-Stat 労働・雇用関連指標 時系列データ取得")
    print(f"  取得年数: {args.years}")
    print(f"  DB: {args.db_path}")
    print(f"  dry-run: {args.dry_run}")
    print(f"  取得指標数: {len(INDICATORS)} + 旧系列 {len(LEGACY_INDICATORS)}")
    print("=" * 60)

    # データ取得（新系列と旧系列を分けてリクエスト）
    all_records = []

    print("\n[1/4] 新系列指標の取得中...")
    records_new = fetch_indicators(
        args.app_id, INDICATORS, args.years,
        label="新系列（主要労働指標）"
    )
    all_records.extend(records_new)

    # APIレート制限対策
    time.sleep(1)

    print("\n[2/4] 旧系列指標の取得中...")
    records_legacy = fetch_indicators(
        args.app_id, LEGACY_INDICATORS, args.years,
        label="旧系列（労働時間・賃金 〜2019）"
    )
    all_records.extend(records_legacy)

    print(f"\n  全レコード合計: {len(all_records)}")

    # ピボット
    print("\n[3/4] データ整形中...")
    rows = pivot_records(all_records)
    print(f"  ピボット後: {len(rows)} 行（都道府県×年度の組み合わせ）")

    # 保存
    print("\n[4/4] DB保存中...")
    save_to_db(args.db_path, rows, args.dry_run)

    print("\n完了!")


if __name__ == "__main__":
    main()
