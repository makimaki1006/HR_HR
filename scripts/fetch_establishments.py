"""
経済センサス-活動調査（令和3年）から都道府県別・産業大分類別の事業所数を取得し、
hellowork.dbのv2_external_establishmentsテーブルに格納するスクリプト。

データソース: e-Stat API
統計表ID: 0004005687 (産業(小分類)別全事業所数 - 全国、都道府県、市区町村)
調査年: 令和3年（2021年6月）
"""
import urllib.request
import json
import sqlite3
import sys
import os

# ─── 設定 ─────────────────────────────────────────
APP_ID = "85f70d978a4fd0da6234e2d07fc423920e077ee5"
STATS_DATA_ID = "0004005687"
DB_PATH = r"C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy\data\hellowork.db"

# 産業大分類コード (全産業 + A-S)
INDUSTRY_CODES = "AS,A,B,C,D,E,F,G,H,I,J,K,L,M,N,O,P,Q,R,S"

# 都道府県コード → 都道府県名のマッピング
PREF_CODE_TO_NAME = {
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

# 産業コード → 産業名のマッピング
INDUSTRY_CODE_TO_NAME = {
    "AS": "全産業",
    "A": "農業，林業",
    "B": "漁業",
    "C": "鉱業，採石業，砂利採取業",
    "D": "建設業",
    "E": "製造業",
    "F": "電気・ガス・熱供給・水道業",
    "G": "情報通信業",
    "H": "運輸業，郵便業",
    "I": "卸売業，小売業",
    "J": "金融業，保険業",
    "K": "不動産業，物品賃貸業",
    "L": "学術研究，専門・技術サービス業",
    "M": "宿泊業，飲食サービス業",
    "N": "生活関連サービス業，娯楽業",
    "O": "教育，学習支援業",
    "P": "医療，福祉",
    "Q": "複合サービス事業",
    "R": "サービス業（他に分類されないもの）",
    "S": "公務（他に分類されるものを除く）",
}


def fetch_estat_data():
    """e-Stat APIから都道府県別・産業大分類別の事業所数を取得"""
    url = (
        f"https://api.e-stat.go.jp/rest/3.0/app/json/getStatsData"
        f"?appId={APP_ID}"
        f"&lang=J"
        f"&statsDataId={STATS_DATA_ID}"
        f"&cdCat01={INDUSTRY_CODES}"
        f"&lvArea=1"
        f"&limit=10000"
    )

    print(f"e-Stat API にリクエスト送信中...")
    req = urllib.request.Request(url)
    with urllib.request.urlopen(req, timeout=60) as resp:
        data = json.loads(resp.read().decode("utf-8"))

    status = data["GET_STATS_DATA"]["RESULT"]["STATUS"]
    if status != 0:
        msg = data["GET_STATS_DATA"]["RESULT"]["ERROR_MSG"]
        print(f"API エラー: {msg}")
        sys.exit(1)

    total = data["GET_STATS_DATA"]["STATISTICAL_DATA"]["RESULT_INF"]["TOTAL_NUMBER"]
    print(f"取得件数: {total}")

    values = data["GET_STATS_DATA"]["STATISTICAL_DATA"]["DATA_INF"].get("VALUE", [])
    return values


def parse_data(values):
    """APIレスポンスを解析して (都道府県, 産業, 事業所数) のリストに変換"""
    records = []
    skipped = 0

    for v in values:
        area_code = v.get("@area", "")
        cat01_code = v.get("@cat01", "")
        value_str = v.get("$", "")

        # 全国(00000)はスキップ（都道府県のみ対象）
        if area_code == "00000":
            continue

        # 都道府県名を取得
        pref_name = PREF_CODE_TO_NAME.get(area_code)
        if pref_name is None:
            skipped += 1
            continue

        # 産業名を取得
        industry_name = INDUSTRY_CODE_TO_NAME.get(cat01_code)
        if industry_name is None:
            skipped += 1
            continue

        # 値を数値に変換（秘匿値 "-" や "x" は None）
        try:
            count = int(value_str)
        except (ValueError, TypeError):
            count = None

        records.append({
            "prefecture": pref_name,
            "industry": industry_name,
            "establishment_count": count,
            "reference_year": "2021",
        })

    print(f"有効レコード: {len(records)} 件, スキップ: {skipped} 件")
    return records


def store_to_db(records):
    """SQLiteデータベースにテーブルを作成してデータを格納"""
    if not os.path.exists(DB_PATH):
        print(f"DB ファイルが見つかりません: {DB_PATH}")
        sys.exit(1)

    conn = sqlite3.connect(DB_PATH)
    cur = conn.cursor()

    # テーブル作成
    cur.execute("""
        CREATE TABLE IF NOT EXISTS v2_external_establishments (
            prefecture TEXT NOT NULL,
            industry TEXT NOT NULL DEFAULT '全産業',
            establishment_count INTEGER,
            employee_count INTEGER,
            reference_year TEXT,
            PRIMARY KEY (prefecture, industry)
        )
    """)

    # 既存データを削除（冪等性確保）
    cur.execute("DELETE FROM v2_external_establishments")
    deleted = cur.rowcount
    if deleted > 0:
        print(f"既存データ {deleted} 件を削除")

    # データ挿入
    inserted = 0
    for rec in records:
        cur.execute(
            """
            INSERT OR REPLACE INTO v2_external_establishments
                (prefecture, industry, establishment_count, employee_count, reference_year)
            VALUES (?, ?, ?, NULL, ?)
            """,
            (rec["prefecture"], rec["industry"], rec["establishment_count"], rec["reference_year"]),
        )
        inserted += 1

    conn.commit()

    # 検証
    cur.execute("SELECT COUNT(*) FROM v2_external_establishments")
    total_rows = cur.fetchone()[0]

    cur.execute("SELECT COUNT(DISTINCT prefecture) FROM v2_external_establishments")
    pref_count = cur.fetchone()[0]

    cur.execute("SELECT COUNT(DISTINCT industry) FROM v2_external_establishments")
    industry_count = cur.fetchone()[0]

    print(f"\n=== DB格納結果 ===")
    print(f"挿入件数: {inserted}")
    print(f"テーブル総行数: {total_rows}")
    print(f"都道府県数: {pref_count}")
    print(f"産業分類数: {industry_count}")

    # サンプルデータ表示（全産業のみ、上位5県）
    print(f"\n=== サンプルデータ（全産業・上位5県） ===")
    cur.execute("""
        SELECT prefecture, establishment_count
        FROM v2_external_establishments
        WHERE industry = '全産業'
        ORDER BY establishment_count DESC
        LIMIT 5
    """)
    for row in cur.fetchall():
        print(f"  {row[0]}: {row[1]:,} 事業所")

    # 医療・福祉の都道府県別（ハローワーク求人との比較で重要）
    print(f"\n=== サンプルデータ（医療，福祉・上位5県） ===")
    cur.execute("""
        SELECT prefecture, establishment_count
        FROM v2_external_establishments
        WHERE industry = '医療，福祉'
        ORDER BY establishment_count DESC
        LIMIT 5
    """)
    for row in cur.fetchall():
        print(f"  {row[0]}: {row[1]:,} 事業所")

    # 全産業の全国合計
    print(f"\n=== 全産業の全国合計 ===")
    cur.execute("""
        SELECT SUM(establishment_count)
        FROM v2_external_establishments
        WHERE industry = '全産業'
    """)
    total = cur.fetchone()[0]
    print(f"  全国合計: {total:,} 事業所")

    conn.close()
    print(f"\nDB格納完了: {DB_PATH}")


def main():
    print("=" * 60)
    print("経済センサス-活動調査（令和3年）事業所数データ取得")
    print("=" * 60)
    print()

    # 1. e-Stat APIからデータ取得
    values = fetch_estat_data()

    # 2. データ解析
    records = parse_data(values)

    # 3. DB格納
    store_to_db(records)


if __name__ == "__main__":
    main()
