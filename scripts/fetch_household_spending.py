"""
e-Stat API から家計調査（statsCode: 00200561）の
都道府県庁所在地別・消費支出データを取得し、hellowork.db に格納する。

テーブル: v2_external_household_spending
ソース: 家計調査 テーブルID 0002070003（年次・都道府県庁所在地別）
"""

import json
import sqlite3
import sys
import urllib.request
from pathlib import Path

sys.stdout.reconfigure(encoding="utf-8")

# ──────────────────────────────────────────────
# 定数
# ──────────────────────────────────────────────
APP_ID = "85f70d978a4fd0da6234e2d07fc423920e077ee5"
STATS_DATA_ID = "0002070003"
REFERENCE_YEAR = "2025"
TIME_CODE = "2025000000"
DB_PATH = Path(r"C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy\data\hellowork.db")

# 都道府県庁所在地コード → 都道府県名マッピング
AREA_TO_PREF = {
    "01003": "北海道", "02003": "青森県", "03003": "岩手県", "04003": "宮城県",
    "05003": "秋田県", "06003": "山形県", "07003": "福島県", "08003": "茨城県",
    "09003": "栃木県", "10003": "群馬県", "11003": "さいたま市",  # 埼玉県
    "12003": "千葉県", "13003": "東京都", "14003": "神奈川県",
    "15003": "新潟県", "16003": "富山県", "17003": "石川県", "18003": "福井県",
    "19003": "山梨県", "20003": "長野県", "21003": "岐阜県", "22003": "静岡県",
    "23003": "愛知県", "24003": "三重県", "25003": "滋賀県", "26003": "京都府",
    "27003": "大阪府", "28003": "兵庫県", "29003": "奈良県", "30003": "和歌山県",
    "31003": "鳥取県", "32003": "島根県", "33003": "岡山県", "34003": "広島県",
    "35003": "山口県", "36003": "徳島県", "37003": "香川県", "38003": "愛媛県",
    "39003": "高知県", "40004": "福岡県", "41003": "佐賀県", "42003": "長崎県",
    "43003": "熊本県", "44003": "大分県", "45003": "宮崎県", "46003": "鹿児島県",
    "47003": "沖縄県",
}

# 修正: 県庁所在地名ではなく都道府県名を使う
AREA_TO_PREF["11003"] = "埼玉県"

# 消費支出カテゴリコード → カテゴリ名
CAT_CODES = {
    "059": "消費支出",
    "060": "食料",
    "102": "住居",
    "107": "光熱・水道",
    "112": "家具・家事用品",
    "122": "被服及び履物",
    "140": "保健医療",
    "145": "交通・通信",
    "152": "教育",
    "156": "教養娯楽",
    "165": "その他の消費支出",
}


def fetch_data() -> list[dict]:
    """e-Stat API からデータを取得"""
    area_str = ",".join(AREA_TO_PREF.keys())
    cat_str = ",".join(CAT_CODES.keys())

    url = (
        f"https://api.e-stat.go.jp/rest/3.0/app/json/getStatsData"
        f"?appId={APP_ID}"
        f"&lang=J"
        f"&statsDataId={STATS_DATA_ID}"
        f"&cdCat01={cat_str}"
        f"&cdCat02=03"  # 二人以上の世帯
        f"&cdArea={area_str}"
        f"&cdTime={TIME_CODE}"
        f"&limit=1000"
    )

    print(f"[INFO] e-Stat API にリクエスト送信中...")
    req = urllib.request.Request(url)
    with urllib.request.urlopen(req, timeout=60) as resp:
        data = json.loads(resp.read().decode("utf-8"))

    status = data.get("GET_STATS_DATA", {}).get("RESULT", {})
    if status.get("STATUS") != 0:
        raise RuntimeError(f"API エラー: {status}")

    values = (
        data.get("GET_STATS_DATA", {})
        .get("STATISTICAL_DATA", {})
        .get("DATA_INF", {})
        .get("VALUE", [])
    )
    print(f"[INFO] {len(values)} 件のデータを取得")
    return values


def parse_records(values: list[dict]) -> list[tuple]:
    """API レスポンスを (prefecture, category, monthly_amount, reference_year) に変換"""
    records = []
    skipped = 0

    for v in values:
        area_code = v.get("@area", "")
        cat_code = v.get("@cat01", "")
        raw_value = v.get("$", "")

        prefecture = AREA_TO_PREF.get(area_code)
        category = CAT_CODES.get(cat_code)

        if not prefecture or not category:
            skipped += 1
            continue

        # 値が「-」や空の場合はNULL
        try:
            amount = int(float(raw_value))
        except (ValueError, TypeError):
            amount = None

        records.append((prefecture, category, amount, REFERENCE_YEAR))

    if skipped > 0:
        print(f"[WARN] {skipped} 件スキップ（未知のコード）")

    return records


def store_to_db(records: list[tuple]) -> None:
    """SQLite DB に格納"""
    conn = sqlite3.connect(str(DB_PATH))
    cur = conn.cursor()

    # テーブル作成
    cur.execute("""
        CREATE TABLE IF NOT EXISTS v2_external_household_spending (
            prefecture TEXT NOT NULL,
            category TEXT NOT NULL,
            monthly_amount INTEGER,
            reference_year TEXT,
            PRIMARY KEY (prefecture, category)
        )
    """)

    # 既存データを削除して再挿入（冪等性）
    cur.execute("DELETE FROM v2_external_household_spending")
    deleted = cur.rowcount
    if deleted > 0:
        print(f"[INFO] 既存データ {deleted} 件を削除")

    cur.executemany(
        """
        INSERT INTO v2_external_household_spending
            (prefecture, category, monthly_amount, reference_year)
        VALUES (?, ?, ?, ?)
        """,
        records,
    )

    conn.commit()

    # 検証
    cur.execute("SELECT COUNT(*) FROM v2_external_household_spending")
    total = cur.fetchone()[0]

    cur.execute("SELECT COUNT(DISTINCT prefecture) FROM v2_external_household_spending")
    pref_count = cur.fetchone()[0]

    cur.execute("SELECT COUNT(DISTINCT category) FROM v2_external_household_spending")
    cat_count = cur.fetchone()[0]

    print(f"\n[検証結果]")
    print(f"  総レコード数: {total}")
    print(f"  都道府県数: {pref_count}")
    print(f"  カテゴリ数: {cat_count}")

    # サンプルデータ表示
    print(f"\n[サンプル: 東京都]")
    cur.execute(
        """
        SELECT category, monthly_amount
        FROM v2_external_household_spending
        WHERE prefecture = '東京都'
        ORDER BY category
        """,
    )
    for row in cur.fetchall():
        print(f"  {row[0]}: {row[1]:,}円/月")

    print(f"\n[サンプル: 北海道]")
    cur.execute(
        """
        SELECT category, monthly_amount
        FROM v2_external_household_spending
        WHERE prefecture = '北海道'
        ORDER BY category
        """,
    )
    for row in cur.fetchall():
        print(f"  {row[0]}: {row[1]:,}円/月")

    # 全都道府県リスト
    print(f"\n[全都道府県の消費支出]")
    cur.execute(
        """
        SELECT prefecture, monthly_amount
        FROM v2_external_household_spending
        WHERE category = '消費支出'
        ORDER BY monthly_amount DESC
        """,
    )
    for rank, row in enumerate(cur.fetchall(), 1):
        print(f"  {rank:2d}. {row[0]}: {row[1]:,}円/月")

    conn.close()


def main():
    print("=" * 60)
    print("家計調査 都道府県庁所在地別 消費支出データ取得")
    print(f"  ソース: e-Stat 家計調査 (statsCode=00200561)")
    print(f"  テーブル: {STATS_DATA_ID} (年次・都道府県庁所在地別)")
    print(f"  対象年: {REFERENCE_YEAR}年")
    print(f"  DB: {DB_PATH}")
    print("=" * 60)

    values = fetch_data()
    records = parse_records(values)
    print(f"[INFO] {len(records)} 件のレコードをパース完了")
    store_to_db(records)
    print("\n[完了] v2_external_household_spending テーブルへの格納が完了しました")


if __name__ == "__main__":
    main()
