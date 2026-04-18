"""
e-Stat API からサイコグラフィックデータ（社会的行動・消費行動）を取得する。

■ データソースA: 社会生活基本調査（令和3年=2021年）
  - statsDataId 0003456573: 趣味・娯楽の種類別行動者率（10歳以上）都道府県別
  - statsDataId 0003456409: スポーツの種類別行動者率（10歳以上）都道府県別
  - statsDataId 0003455937: ボランティア活動の種類別行動者率（10歳以上）都道府県別
  - statsDataId 0003456245: 学習・自己啓発・訓練の種類別行動者率（10歳以上）都道府県別

■ データソースB: 家計調査（statsDataId: 0002070003）
  - 都道府県庁所在地別 × 10大費目 × 2025年平均

■ 出力先
  - scripts/data/social_life_survey.csv
  - scripts/data/household_spending.csv
"""

import json
import sys
import time
import urllib.request
from pathlib import Path

import pandas as pd

sys.stdout.reconfigure(encoding="utf-8")

# ──────────────────────────────────────────────
# 定数
# ──────────────────────────────────────────────
APP_ID = "85f70d978a4fd0da6234e2d07fc423920e077ee5"
BASE_URL = "https://api.e-stat.go.jp/rest/3.0/app/json"

# 出力ディレクトリ（このスクリプトと同階層の data/）
OUTPUT_DIR = Path(__file__).parent / "data"
OUTPUT_DIR.mkdir(exist_ok=True)

# API リクエスト間の待機時間（秒）
REQUEST_INTERVAL = 0.5

# ──────────────────────────────────────────────
# 社会生活基本調査 テーブル定義
# ──────────────────────────────────────────────

# 地域区分コード → 都道府県名マッピング（社会生活基本調査）
# areaコードは01000〜47000（5桁、末尾3桁は000）、全国=00000は除外
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

# 各カテゴリの statsDataId と cat02/cat03 の「総数」コード（主行動のみ）
SOCIAL_LIFE_TABLES = [
    {
        "statsDataId": "0003456573",   # 趣味・娯楽の行動者率
        "category": "趣味・娯楽",
        "cat01": "0",                 # 男女総数（実データのコード）
        "cat02": "99000",             # 人口集中地区・以外の総数
        "cat03": "00",                # 趣味・娯楽の種類：総数
        "has_cat03": True,
    },
    {
        "statsDataId": "0003456409",   # スポーツの行動者率
        "category": "スポーツ",
        "cat01": "0",
        "cat02": "99000",
        "cat03": "00",                # スポーツ種類：総数
        "has_cat03": True,
    },
    {
        "statsDataId": "0003455937",   # ボランティアの行動者率
        "category": "ボランティア",
        "cat01": "0",
        "cat02": "99000",
        "cat03": "0",                 # ボランティア活動の形態：総数
        "cat04": "00",                # ボランティア活動の種類：総数（コードは"00"）
        "has_cat03": True,
        "has_cat04": True,
    },
    {
        "statsDataId": "0003456245",   # 学習・自己啓発の行動者率
        "category": "学習・自己啓発",
        "cat01": "0",
        "cat02": "99000",
        "cat03": "0",                 # 学習種類：総数
        "has_cat03": True,
    },
]

# ──────────────────────────────────────────────
# 家計調査 テーブル定義
# ──────────────────────────────────────────────

# statsDataId: 0002070003（年次・都道府県庁所在地別 二人以上の世帯）
HOUSEHOLD_STATS_ID = "0002070003"
HOUSEHOLD_YEAR = "2025000000"

# 都道府県庁所在地コード → (都市名, 都道府県名) マッピング
CITY_TO_PREF = {
    "01003": ("札幌市", "北海道"),
    "02003": ("青森市", "青森県"),
    "03003": ("盛岡市", "岩手県"),
    "04003": ("仙台市", "宮城県"),
    "05003": ("秋田市", "秋田県"),
    "06003": ("山形市", "山形県"),
    "07003": ("福島市", "福島県"),
    "08003": ("水戸市", "茨城県"),
    "09003": ("宇都宮市", "栃木県"),
    "10003": ("前橋市", "群馬県"),
    "11003": ("さいたま市", "埼玉県"),
    "12003": ("千葉市", "千葉県"),
    "13003": ("東京都区部", "東京都"),
    "14003": ("横浜市", "神奈川県"),
    "15003": ("新潟市", "新潟県"),
    "16003": ("富山市", "富山県"),
    "17003": ("金沢市", "石川県"),
    "18003": ("福井市", "福井県"),
    "19003": ("甲府市", "山梨県"),
    "20003": ("長野市", "長野県"),
    "21003": ("岐阜市", "岐阜県"),
    "22003": ("静岡市", "静岡県"),
    "23003": ("名古屋市", "愛知県"),
    "24003": ("津市", "三重県"),
    "25003": ("大津市", "滋賀県"),
    "26003": ("京都市", "京都府"),
    "27003": ("大阪市", "大阪府"),
    "28003": ("神戸市", "兵庫県"),
    "29003": ("奈良市", "奈良県"),
    "30003": ("和歌山市", "和歌山県"),
    "31003": ("鳥取市", "鳥取県"),
    "32003": ("松江市", "島根県"),
    "33003": ("岡山市", "岡山県"),
    "34003": ("広島市", "広島県"),
    "35003": ("山口市", "山口県"),
    "36003": ("徳島市", "徳島県"),
    "37003": ("高松市", "香川県"),
    "38003": ("松山市", "愛媛県"),
    "39003": ("高知市", "高知県"),
    "40004": ("福岡市", "福岡県"),
    "41003": ("佐賀市", "佐賀県"),
    "42003": ("長崎市", "長崎県"),
    "43003": ("熊本市", "熊本県"),
    "44003": ("大分市", "大分県"),
    "45003": ("宮崎市", "宮崎県"),
    "46003": ("鹿児島市", "鹿児島県"),
    "47003": ("那覇市", "沖縄県"),
}

# 10大費目カテゴリコード → カテゴリ名マッピング
EXPENSE_CATEGORIES = {
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


# ──────────────────────────────────────────────
# ユーティリティ関数
# ──────────────────────────────────────────────

def fetch_json(url: str) -> dict:
    """e-Stat API からJSON取得（リトライなし・シンプル実装）"""
    req = urllib.request.Request(url)
    with urllib.request.urlopen(req, timeout=60) as resp:
        return json.loads(resp.read().decode("utf-8"))


def build_url(endpoint: str, params: dict) -> str:
    """クエリパラメータを付加したURLを生成"""
    qs = "&".join(f"{k}={urllib.request.quote(str(v), safe='')}" for k, v in params.items())
    return f"{BASE_URL}/{endpoint}?{qs}"


def extract_values(data: dict) -> list[dict]:
    """getStatsData のレスポンスから VALUE リストを抽出"""
    values = (
        data.get("GET_STATS_DATA", {})
            .get("STATISTICAL_DATA", {})
            .get("DATA_INF", {})
            .get("VALUE", [])
    )
    if isinstance(values, dict):
        return [values]
    return values


# ──────────────────────────────────────────────
# A. 社会生活基本調査データ取得
# ──────────────────────────────────────────────

def fetch_social_life_survey() -> pd.DataFrame:
    """
    社会生活基本調査（令和3年）から都道府県別行動者率を取得。

    カラム:
        prefecture, category, subcategory, participation_rate, survey_year
    """
    print("\n[社会生活基本調査] データ取得開始...")
    rows = []

    for tbl in SOCIAL_LIFE_TABLES:
        stats_id = tbl["statsDataId"]
        category = tbl["category"]
        print(f"  {category} ({stats_id}) を取得中...")

        # エリアコードを50件ずつに分割して取得（APIの上限対策）
        BATCH_SIZE = 47  # 都道府県47件はまとめて送信可能
        area_str = ",".join(PREF_CODE_TO_NAME.keys())

        # クエリパラメータ組み立て
        params: dict = {
            "appId": APP_ID,
            "lang": "J",
            "statsDataId": stats_id,
            "cdCat01": tbl["cat01"],   # 男女総数
            "cdCat02": tbl["cat02"],   # 人口集中地区・以外の総数
            "cdArea": area_str,
            "limit": "100000",
        }

        # カテゴリ種類コード（総数のみ）
        if tbl.get("has_cat03"):
            params["cdCat03"] = tbl["cat03"]
        if tbl.get("has_cat04"):
            params["cdCat04"] = tbl["cat04"]

        url = build_url("getStatsData", params)
        data = fetch_json(url)
        values = extract_values(data)
        time.sleep(REQUEST_INTERVAL)

        if not values:
            print(f"    WARNING: データが取得できませんでした")
            continue

        print(f"    取得件数: {len(values)}件")

        for v in values:
            area_code = v.get("@area", "")
            pref_name = PREF_CODE_TO_NAME.get(area_code, "")
            if not pref_name:
                continue  # 全国・地域区分等はスキップ

            rate_str = v.get("$", "")
            try:
                rate = float(rate_str)
            except (ValueError, TypeError):
                rate = None  # "-" や非数値はNoneにする

            rows.append({
                "prefecture": pref_name,
                "category": category,
                "subcategory": "総数",
                "participation_rate": rate,
                "survey_year": 2021,
            })

    df = pd.DataFrame(rows, columns=["prefecture", "category", "subcategory", "participation_rate", "survey_year"])
    print(f"[社会生活基本調査] 合計 {len(df)} 行取得")
    return df


# ──────────────────────────────────────────────
# B. 家計調査データ取得
# ──────────────────────────────────────────────

def fetch_household_spending() -> pd.DataFrame:
    """
    家計調査（2025年・二人以上の世帯）から都道府県庁所在地別消費支出を取得。

    カラム:
        city, prefecture, category, annual_amount_yen, year
    """
    print("\n[家計調査] データ取得開始...")

    city_codes = ",".join(CITY_TO_PREF.keys())
    cat_codes = ",".join(EXPENSE_CATEGORIES.keys())

    params = {
        "appId": APP_ID,
        "lang": "J",
        "statsDataId": HOUSEHOLD_STATS_ID,
        "cdCat01": cat_codes,         # 10大費目コード
        "cdCat02": "03",              # 二人以上の世帯（2000年〜）
        "cdArea": city_codes,
        "cdTime": HOUSEHOLD_YEAR,
        "limit": "100000",
    }

    url = build_url("getStatsData", params)
    print(f"  リクエスト送信中...")
    data = fetch_json(url)
    values = extract_values(data)

    if not values:
        print("  WARNING: データが取得できませんでした")
        return pd.DataFrame(columns=["city", "prefecture", "category", "annual_amount_yen", "year"])

    print(f"  取得件数: {len(values)}件")

    rows = []
    for v in values:
        area_code = v.get("@area", "")
        cat_code = v.get("@cat01", "")

        city_pref = CITY_TO_PREF.get(area_code)
        cat_name = EXPENSE_CATEGORIES.get(cat_code)

        if not city_pref or not cat_name:
            continue

        city_name, pref_name = city_pref

        # 金額は月平均（円）→ 年間（円）に換算
        amount_str = v.get("$", "")
        try:
            monthly_yen = float(amount_str)
            annual_yen = round(monthly_yen * 12)
        except (ValueError, TypeError):
            annual_yen = None

        rows.append({
            "city": city_name,
            "prefecture": pref_name,
            "category": cat_name,
            "annual_amount_yen": annual_yen,
            "year": 2025,
        })

    df = pd.DataFrame(rows, columns=["city", "prefecture", "category", "annual_amount_yen", "year"])
    print(f"[家計調査] 合計 {len(df)} 行取得")
    return df


# ──────────────────────────────────────────────
# メイン処理
# ──────────────────────────────────────────────

def main():
    print("=" * 60)
    print("サイコグラフィックデータ取得スクリプト")
    print("=" * 60)

    # ── A. 社会生活基本調査 ──
    df_social = fetch_social_life_survey()
    out_social = OUTPUT_DIR / "social_life_survey.csv"
    df_social.to_csv(out_social, index=False, encoding="utf-8-sig")
    print(f"\n[出力] {out_social}")
    print(f"  行数: {len(df_social)}")
    print("  先頭5行:")
    print(df_social.head().to_string(index=False))

    # ── B. 家計調査 ──
    df_spending = fetch_household_spending()
    out_spending = OUTPUT_DIR / "household_spending.csv"
    df_spending.to_csv(out_spending, index=False, encoding="utf-8-sig")
    print(f"\n[出力] {out_spending}")
    print(f"  行数: {len(df_spending)}")
    print("  先頭5行:")
    print(df_spending.head().to_string(index=False))

    print("\n完了")


if __name__ == "__main__":
    main()
