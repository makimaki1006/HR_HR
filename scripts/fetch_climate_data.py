"""
e-Stat APIから都道府県別気象データを取得し、hellowork.dbに格納する。

データソース: 社会・人口統計体系 B 自然環境 (statsDataId=0000010102)
対象指標:
  B4101: 年平均気温(℃)
  B4102: 最高気温(℃)
  B4103: 最低気温(℃)
  B4106: 降水日数(年間・日)
  B4107: 雪日数(年間・日)  ※2018年度まで充実、以降欠損あり
  B4108: 日照時間(年間・時間)
  B4109: 降水量(年間・mm)

テーブル: v2_external_climate
取得年度: 2015-2024（10年間）
"""

import json
import sqlite3
import sys
import urllib.request
from pathlib import Path

sys.stdout.reconfigure(encoding="utf-8")

# ── 定数 ──
APP_ID = "85f70d978a4fd0da6234e2d07fc423920e077ee5"
STATS_DATA_ID = "0000010102"
DB_PATH = Path(r"C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy\data\hellowork.db")

# 取得したい気象指標
CLIMATE_CATS = {
    "B4101": "avg_temperature",      # 年平均気温(℃)
    "B4102": "max_temperature",      # 最高気温(℃)
    "B4103": "min_temperature",      # 最低気温(℃)
    "B4106": "rainy_days",           # 降水日数(年間・日)
    "B4107": "snow_days",            # 雪日数(年間・日)
    "B4108": "sunshine_hours",       # 日照時間(年間・時間)
    "B4109": "precipitation",        # 降水量(年間・mm)
}

# 整数型カラム
INT_COLS = {"rainy_days", "snow_days"}

# 都道府県コード→名前マッピング
PREF_CODE_MAP = {
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

# 取得対象年度（10年間）
TARGET_YEARS = [str(y) for y in range(2015, 2025)]


def fetch_estat_data(cat_codes: list[str], years: list[str]) -> list[dict]:
    """e-Stat APIからデータを取得する。"""
    cd_cat01 = ",".join(cat_codes)
    cd_time = ",".join(f"{y}100000" for y in years)
    cd_area = ",".join(PREF_CODE_MAP.keys())

    base_url = "https://api.e-stat.go.jp/rest/3.0/app/json/getStatsData"
    params = (
        f"?appId={APP_ID}"
        f"&lang=J"
        f"&statsDataId={STATS_DATA_ID}"
        f"&cdCat01={cd_cat01}"
        f"&cdTime={cd_time}"
        f"&cdArea={cd_area}"
        f"&metaGetFlg=N"
        f"&cntGetFlg=N"
        f"&sectionHeaderFlg=2"
    )
    url = base_url + params
    print(f"API呼び出し中... (指標数={len(cat_codes)}, 年度数={len(years)})")

    with urllib.request.urlopen(url, timeout=60) as resp:
        raw = json.loads(resp.read().decode("utf-8"))

    stat_data = raw.get("GET_STATS_DATA", {}).get("STATISTICAL_DATA", {})
    data_inf = stat_data.get("DATA_INF", {})
    values = data_inf.get("VALUE", [])

    if not isinstance(values, list):
        values = [values] if values else []

    print(f"  取得レコード数: {len(values)}")
    return values


def parse_records(values: list[dict]) -> dict:
    """APIレスポンスを (prefecture, fiscal_year) -> {column: value} 形式に変換。"""
    records: dict[tuple[str, str], dict] = {}

    for v in values:
        area_code = v.get("@area", "")
        cat01_code = v.get("@cat01", "")
        time_code = v.get("@time", "")
        raw_val = v.get("$", "")

        pref = PREF_CODE_MAP.get(area_code)
        if not pref:
            continue

        col = CLIMATE_CATS.get(cat01_code)
        if not col:
            continue

        fiscal_year = time_code[:4] if len(time_code) >= 4 else time_code

        # 値変換（欠損値を None に）
        numeric_val = None
        if raw_val and raw_val not in ("-", "***", "…", "x", "X", ""):
            try:
                numeric_val = float(raw_val)
            except (ValueError, TypeError):
                pass

        key = (pref, fiscal_year)
        if key not in records:
            records[key] = {}
        records[key][col] = numeric_val

    return records


def create_table(conn: sqlite3.Connection) -> None:
    """v2_external_climateテーブルを作成する（DROP + CREATE）。"""
    conn.execute("DROP TABLE IF EXISTS v2_external_climate")
    conn.execute("""
        CREATE TABLE v2_external_climate (
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
    """)
    conn.commit()
    print("テーブル v2_external_climate 作成完了")


def insert_records(conn: sqlite3.Connection, records: dict) -> int:
    """レコードをDBに挿入する。"""
    sql = """
        INSERT INTO v2_external_climate
            (prefecture, fiscal_year, avg_temperature, max_temperature,
             min_temperature, rainy_days, snow_days, sunshine_hours, precipitation)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
    """

    rows = []
    for (pref, fy), cols in records.items():
        rows.append((
            pref,
            fy,
            cols.get("avg_temperature"),
            cols.get("max_temperature"),
            cols.get("min_temperature"),
            int(cols["rainy_days"]) if cols.get("rainy_days") is not None else None,
            int(cols["snow_days"]) if cols.get("snow_days") is not None else None,
            cols.get("sunshine_hours"),
            cols.get("precipitation"),
        ))

    conn.executemany(sql, rows)
    conn.commit()
    return len(rows)


def verify_data(conn: sqlite3.Connection) -> None:
    """格納データを検証する。"""
    print("\n" + "=" * 60)
    print("データ検証")
    print("=" * 60)

    # 行数
    count = conn.execute("SELECT COUNT(*) FROM v2_external_climate").fetchone()[0]
    print(f"総レコード数: {count}")

    # 都道府県数
    pref_count = conn.execute(
        "SELECT COUNT(DISTINCT prefecture) FROM v2_external_climate"
    ).fetchone()[0]
    print(f"都道府県数: {pref_count}")

    # 年度別件数
    rows = conn.execute(
        "SELECT fiscal_year, COUNT(*) FROM v2_external_climate GROUP BY fiscal_year ORDER BY fiscal_year"
    ).fetchall()
    print("\n年度別件数:")
    for fy, cnt in rows:
        print(f"  {fy}年度: {cnt}件")

    # NULL率確認
    cols = ["avg_temperature", "max_temperature", "min_temperature",
            "rainy_days", "snow_days", "sunshine_hours", "precipitation"]
    print("\nNULL率:")
    for col in cols:
        null_count = conn.execute(
            f"SELECT COUNT(*) FROM v2_external_climate WHERE {col} IS NULL"
        ).fetchone()[0]
        pct = (null_count / count * 100) if count > 0 else 0
        status = "OK" if pct < 5 else ("注意" if pct < 30 else "欠損多い")
        print(f"  {col}: {null_count}/{count} ({pct:.1f}%) [{status}]")

    # 雪日数の年度別充足率
    print("\n雪日数の年度別充足率:")
    for fy_row in conn.execute(
        """SELECT fiscal_year,
                  COUNT(*) as total,
                  SUM(CASE WHEN snow_days IS NOT NULL THEN 1 ELSE 0 END) as valid
           FROM v2_external_climate
           GROUP BY fiscal_year ORDER BY fiscal_year"""
    ).fetchall():
        fy, total, valid = fy_row
        print(f"  {fy}年度: {valid}/{total}")

    # サンプルデータ（積雪・猛暑の特徴的な県）
    print("\nサンプルデータ（2023年度）:")
    sample_prefs = ["北海道", "青森県", "秋田県", "新潟県", "東京都", "大阪府", "沖縄県"]
    for pref in sample_prefs:
        row = conn.execute(
            """SELECT avg_temperature, max_temperature, min_temperature,
                      rainy_days, snow_days, sunshine_hours, precipitation
               FROM v2_external_climate
               WHERE prefecture = ? AND fiscal_year = '2023'""",
            (pref,)
        ).fetchone()
        if row:
            snow_str = f"{row[4]}日" if row[4] is not None else "N/A"
            print(f"  {pref}: 平均{row[0]}℃ 最高{row[1]}℃ 最低{row[2]}℃ "
                  f"降水日{row[3]}日 雪日数{snow_str} 日照{row[5]}h 降水{row[6]}mm")

    # 2018年度（雪日数充実）のサンプル
    print("\nサンプルデータ（2018年度 - 雪日数充実）:")
    for pref in ["北海道", "青森県", "秋田県", "新潟県", "東京都", "沖縄県"]:
        row = conn.execute(
            """SELECT avg_temperature, max_temperature, snow_days, precipitation
               FROM v2_external_climate
               WHERE prefecture = ? AND fiscal_year = '2018'""",
            (pref,)
        ).fetchone()
        if row:
            print(f"  {pref}: 平均{row[0]}℃ 最高{row[1]}℃ 雪日数{row[2]}日 降水{row[3]}mm")


def main() -> None:
    print("=" * 60)
    print("e-Stat 気象データ取得 → hellowork.db 格納")
    print("=" * 60)

    # 1. APIからデータ取得
    cat_codes = list(CLIMATE_CATS.keys())
    values = fetch_estat_data(cat_codes, TARGET_YEARS)

    if not values:
        print("エラー: データが取得できませんでした")
        sys.exit(1)

    # 2. パース
    records = parse_records(values)
    print(f"パース結果: {len(records)} レコード（都道府県×年度の組み合わせ）")

    # 3. DB格納
    conn = sqlite3.connect(str(DB_PATH))
    try:
        create_table(conn)
        inserted = insert_records(conn, records)
        print(f"DB格納完了: {inserted} レコード")

        # 4. 検証
        verify_data(conn)
    finally:
        conn.close()

    print("\n" + "=" * 60)
    print("処理完了")
    print("=" * 60)


if __name__ == "__main__":
    main()
