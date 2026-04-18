"""
地理的補完データ取得スクリプト

以下の3つのデータを都道府県別CSVとして出力する:
  A. 地価公示 (国土数値情報 2025年版 GeoJSON)
     → scripts/data/land_price_by_prefecture.csv
  B. 自動車保有率 (社会人口統計体系 e-Stat API)
     → scripts/data/car_ownership_by_prefecture.csv
  C. インターネット利用率 (通信利用動向調査 e-Stat API)
     → scripts/data/internet_usage_by_prefecture.csv

使用方法:
  python scripts/fetch_geo_supplement.py        # 全データ取得
  python scripts/fetch_geo_supplement.py --land  # 地価公示のみ
  python scripts/fetch_geo_supplement.py --car   # 自動車保有率のみ
  python scripts/fetch_geo_supplement.py --net   # インターネット利用率のみ

データソース:
  地価公示: https://nlftp.mlit.go.jp/ksj/gml/datalist/KsjTmplt-L01-2025.html
  自動車:   https://api.e-stat.go.jp/ (statsDataId=0000010108, 0000010101)
  通信:     https://api.e-stat.go.jp/ (statsDataId=0003161442)

注意:
  - 地価公示は全国版ZIP (約20MB) を1回だけダウンロードして解析する
  - 自動車保有は「軽自動車等台数 (H7207)」÷「総人口 (A1101)」で100人あたり算出
    （e-Statに乗用車単体の都道府県別データが存在しないため）
  - インターネット利用率は 2016年以前のデータが取得可能
    (通信利用動向調査の令和5年版データはAPIでstatsDataId特定が困難なため旧版を使用)
"""

import argparse
import csv
import io
import json
import sys
import urllib.request
import zipfile
from collections import defaultdict
from pathlib import Path

sys.stdout.reconfigure(encoding="utf-8")

# ── 定数 ──
APP_ID = "85f70d978a4fd0da6234e2d07fc423920e077ee5"
SCRIPTS_DIR = Path(__file__).parent
DATA_DIR = SCRIPTS_DIR / "data"

# 地価公示 全国ZIPのURL (2025年版)
LAND_PRICE_ZIP_URL = "https://nlftp.mlit.go.jp/ksj/gml/data/L01/L01-25/L01-25_GML.zip"

# e-Stat API基底URL
ESTAT_BASE = "https://api.e-stat.go.jp/rest/3.0/app/json/getStatsData"

# 都道府県コード→名前マッピング (e-Stat用5桁コード)
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

# 都道府県2桁コード→名前（地価公示GeoJSON用）
PREF_2DIGIT_MAP = {
    "01": "北海道", "02": "青森県", "03": "岩手県", "04": "宮城県",
    "05": "秋田県", "06": "山形県", "07": "福島県", "08": "茨城県",
    "09": "栃木県", "10": "群馬県", "11": "埼玉県", "12": "千葉県",
    "13": "東京都", "14": "神奈川県", "15": "新潟県", "16": "富山県",
    "17": "石川県", "18": "福井県", "19": "山梨県", "20": "長野県",
    "21": "岐阜県", "22": "静岡県", "23": "愛知県", "24": "三重県",
    "25": "滋賀県", "26": "京都府", "27": "大阪府", "28": "兵庫県",
    "29": "奈良県", "30": "和歌山県", "31": "鳥取県", "32": "島根県",
    "33": "岡山県", "34": "広島県", "35": "山口県", "36": "徳島県",
    "37": "香川県", "38": "愛媛県", "39": "高知県", "40": "福岡県",
    "41": "佐賀県", "42": "長崎県", "43": "熊本県", "44": "大分県",
    "45": "宮崎県", "46": "鹿児島県", "47": "沖縄県",
}

# 地目→用途分類マッピング (GeoJSONのL01_028を簡略化)
def classify_land_use(raw_use: str) -> str | None:
    """地目文字列を住宅地/商業地/工業地に分類する。"""
    if not raw_use:
        return None
    # 主用途を最初の要素で判定
    primary = raw_use.split(",")[0].strip()
    if primary == "住宅":
        return "住宅地"
    elif primary in ("店舗", "事務所", "銀行", "旅館", "医院"):
        return "商業地"
    elif primary in ("工場", "倉庫"):
        return "工業地"
    else:
        return None  # その他は除外


def fetch_estat_data(
    stats_data_id: str,
    cd_cat01: str,
    cd_time: str,
    cd_tab: str | None = None,
) -> list[dict]:
    """e-Stat APIからデータを取得する。

    Args:
        stats_data_id: 統計データID
        cd_cat01: カテゴリ01コード (カンマ区切りで複数指定可)
        cd_time: 時間軸コード
        cd_tab: 表章項目コード (00100=割合%, 00200=回答数 など)
    """
    cd_area = ",".join(PREF_CODE_MAP.keys())
    params = (
        f"?appId={APP_ID}"
        f"&lang=J"
        f"&statsDataId={stats_data_id}"
        f"&cdCat01={cd_cat01}"
        f"&cdTime={cd_time}"
        f"&cdArea={cd_area}"
        f"&metaGetFlg=N"
        f"&cntGetFlg=N"
        f"&limit=2000"
    )
    # tabフィルタを指定した場合のみ追加
    if cd_tab:
        params += f"&cdTab={cd_tab}"

    url = ESTAT_BASE + params
    print(f"  API取得中: statsDataId={stats_data_id}, cat01={cd_cat01}, time={cd_time}")

    with urllib.request.urlopen(url, timeout=60) as resp:
        raw = json.loads(resp.read().decode("utf-8"))

    stat_data = raw.get("GET_STATS_DATA", {}).get("STATISTICAL_DATA", {})
    data_inf = stat_data.get("DATA_INF", {})
    values = data_inf.get("VALUE", [])
    if not isinstance(values, list):
        values = [values] if values else []

    print(f"  取得レコード数: {len(values)}")
    return values


# ═══════════════════════════════════════════════
# A. 地価公示
# ═══════════════════════════════════════════════

def fetch_land_price() -> None:
    """
    国土数値情報 地価公示2025年版をダウンロードし、
    都道府県×用途別の平均地価をCSVに出力する。

    フィールド仕様 (GeoJSON):
      L01_001: 市区町村コード (最初の2桁が都道府県)
      L01_007: 年 (2025)
      L01_008: 価格 (円/m²)
      L01_028: 地目 (住宅, 店舗, 工場, etc.)
      L01_103: 前年度価格 (円/m²)
    """
    out_path = DATA_DIR / "land_price_by_prefecture.csv"
    print("\n[A] 地価公示データ取得開始")
    print(f"  ダウンロード中: {LAND_PRICE_ZIP_URL}")

    with urllib.request.urlopen(LAND_PRICE_ZIP_URL, timeout=120) as resp:
        zip_bytes = resp.read()
    print(f"  ダウンロード完了: {len(zip_bytes) // 1024}KB")

    # 都道府県×用途別に価格を集計
    # key: (pref_name, land_use), value: [price, ...]
    price_map: dict[tuple[str, str], list[float]] = defaultdict(list)
    prev_price_map: dict[tuple[str, str], list[float]] = defaultdict(list)
    year = 2025

    print("  GeoJSONを解析中...")
    total_points = 0

    with zipfile.ZipFile(io.BytesIO(zip_bytes)) as zf:
        # 全都道府県のGeoJSONファイルを処理
        geojson_files = [n for n in zf.namelist() if n.endswith(".geojson")]
        print(f"  GeoJSONファイル数: {len(geojson_files)}")

        for geojson_path in geojson_files:
            gj_data = json.loads(zf.read(geojson_path).decode("utf-8"))
            features = gj_data.get("features", [])
            total_points += len(features)

            for feat in features:
                props = feat.get("properties", {})

                # 都道府県コード (L01_001は市区町村コード、最初の2桁が都道府県)
                muni_code = str(props.get("L01_001", ""))
                pref_code = muni_code[:2].zfill(2)
                pref_name = PREF_2DIGIT_MAP.get(pref_code)
                if not pref_name:
                    continue

                # 用途分類
                raw_use = str(props.get("L01_028", ""))
                land_use = classify_land_use(raw_use)
                if not land_use:
                    continue

                # 現在価格・前年度価格
                price = props.get("L01_008")
                prev_price = props.get("L01_103")
                if price is None or price == 0:
                    continue

                try:
                    price_f = float(price)
                    price_map[(pref_name, land_use)].append(price_f)
                    if prev_price and float(prev_price) > 0:
                        prev_price_map[(pref_name, land_use)].append(float(prev_price))
                except (ValueError, TypeError):
                    continue

    print(f"  総地点数: {total_points}点")

    # CSV出力
    rows = []
    for (pref, land_use), prices in sorted(price_map.items()):
        avg_price = sum(prices) / len(prices)
        prev_prices = prev_price_map.get((pref, land_use), [])
        if prev_prices:
            avg_prev = sum(prev_prices) / len(prev_prices)
            yoy_change = round((avg_price - avg_prev) / avg_prev * 100, 2) if avg_prev > 0 else None
        else:
            yoy_change = None

        rows.append({
            "prefecture": pref,
            "land_use": land_use,
            "avg_price_per_sqm": round(avg_price, 0),
            "yoy_change_pct": yoy_change,
            "year": year,
            "point_count": len(prices),
        })

    with open(out_path, "w", newline="", encoding="utf-8") as f:
        writer = csv.DictWriter(f, fieldnames=[
            "prefecture", "land_use", "avg_price_per_sqm",
            "yoy_change_pct", "year", "point_count"
        ])
        writer.writeheader()
        writer.writerows(rows)

    print(f"  出力完了: {out_path} ({len(rows)}行)")
    _preview_csv(out_path, 5)


# ═══════════════════════════════════════════════
# B. 自動車保有率
# ═══════════════════════════════════════════════

def fetch_car_ownership() -> None:
    """
    社会人口統計体系 e-Stat APIから都道府県別の自動車保有率を取得する。

    使用指標:
      statsDataId=0000010108 (H 居住)
        H7207: 軽自動車等台数 (2024年度)
        H7150: 自動車走行台キロ (2020年度) ※最新年が異なる場合あり
      statsDataId=0000010101 (A 人口)
        A1101: 総人口 (2024年度)

    算出方法:
      cars_per_100people = H7207 / A1101 * 100
      (注: H7207は軽自動車等の台数。乗用車全体の都道府県別データは
           e-Statでは提供されていないため軽自動車台数で代替する)
    """
    out_path = DATA_DIR / "car_ownership_by_prefecture.csv"
    print("\n[B] 自動車保有率データ取得開始")

    TARGET_YEAR = "2024100000"  # 2024年度 (最新)

    # 軽自動車台数取得
    car_values = fetch_estat_data(
        stats_data_id="0000010108",
        cd_cat01="H7207",
        cd_time=TARGET_YEAR,
    )
    # 人口取得
    pop_values = fetch_estat_data(
        stats_data_id="0000010101",
        cd_cat01="A1101",
        cd_time=TARGET_YEAR,
    )

    # 都道府県コード→値のマップ構築
    car_map: dict[str, float] = {}
    pop_map: dict[str, float] = {}

    for v in car_values:
        area = v.get("@area", "")
        val = v.get("$", "")
        if val and val not in ("-", "***", "…", "x", ""):
            try:
                car_map[area] = float(val)
            except ValueError:
                pass

    for v in pop_values:
        area = v.get("@area", "")
        val = v.get("$", "")
        if val and val not in ("-", "***", "…", "x", ""):
            try:
                pop_map[area] = float(val)
            except ValueError:
                pass

    # CSV出力
    rows = []
    for area_code, pref_name in PREF_CODE_MAP.items():
        cars = car_map.get(area_code)
        population = pop_map.get(area_code)

        if cars is None or population is None or population == 0:
            cars_per_100 = None
        else:
            cars_per_100 = round(cars / population * 100, 2)

        rows.append({
            "prefecture": pref_name,
            "cars_per_100people": cars_per_100,
            "total_kei_cars": int(cars) if cars else None,
            "total_population": int(population) if population else None,
            "year": 2024,
            "note": "軽自動車等台数(H7207)÷総人口(A1101)×100。乗用車全体台数はe-Statに都道府県別データなし",
        })

    with open(out_path, "w", newline="", encoding="utf-8") as f:
        writer = csv.DictWriter(f, fieldnames=[
            "prefecture", "cars_per_100people",
            "total_kei_cars", "total_population", "year", "note"
        ])
        writer.writeheader()
        writer.writerows(rows)

    print(f"  出力完了: {out_path} ({len(rows)}行)")
    _preview_csv(out_path, 5)


# ═══════════════════════════════════════════════
# C. インターネット利用率
# ═══════════════════════════════════════════════

def fetch_internet_usage() -> None:
    """
    通信利用動向調査 e-Stat APIから都道府県別インターネット利用率を取得する。

    使用statsDataId:
      0003161442: 世帯での過去1年間のインターネット利用経験 都道府県
                  (通信利用動向調査 世帯全体編, 平成18年以降)
                  ※ 2016年以前のデータが取得可能
      0003161355: 情報通信機器の保有状況 都道府県 (スマートフォン保有率)

    取得する最新年:
      2016年 (通信利用動向調査の令和5年版はAPIのstatsDataId特定が困難なため
              継続データ系列の最終年を使用)

    カテゴリコード (cat01):
      110: 利用あり (インターネット利用世帯率%)
      120: 利用なし
    """
    out_path = DATA_DIR / "internet_usage_by_prefecture.csv"
    print("\n[C] インターネット利用率データ取得開始")

    TARGET_YEAR = "2016000000"  # 2016年 (利用可能な最新年)

    # インターネット利用経験 (世帯全体編)
    # tab=00100: 回答数割合(%), cat01=110: 利用あり
    inet_values = fetch_estat_data(
        stats_data_id="0003161442",
        cd_cat01="110",  # 少なくとも1人はインターネットを利用したことがある
        cd_time=TARGET_YEAR,
        cd_tab="00100",  # 回答数割合(%)
    )

    # スマートフォン保有率 (情報通信機器の保有状況)
    # statsDataId=0003161355, cat01=260: スマートフォン, tab=00100: 回答数割合
    smartphone_values = fetch_estat_data(
        stats_data_id="0003161355",
        cd_cat01="260",  # スマートフォン
        cd_time=TARGET_YEAR,
        cd_tab="00100",  # 回答数割合(%)
    )

    # 利用率マップ構築
    inet_map: dict[str, float] = {}
    for v in inet_values:
        area = v.get("@area", "")
        val = v.get("$", "")
        if val and val not in ("-", "***", "…", "x", ""):
            try:
                inet_map[area] = float(val)
            except ValueError:
                pass

    smartphone_map: dict[str, float] = {}
    for v in smartphone_values:
        area = v.get("@area", "")
        val = v.get("$", "")
        if val and val not in ("-", "***", "…", "x", ""):
            try:
                smartphone_map[area] = float(val)
            except ValueError:
                pass

    # CSV出力
    rows = []
    for area_code, pref_name in PREF_CODE_MAP.items():
        inet_rate = inet_map.get(area_code)
        sp_rate = smartphone_map.get(area_code) if smartphone_map else None

        rows.append({
            "prefecture": pref_name,
            "internet_usage_rate": inet_rate,
            "smartphone_ownership_rate": sp_rate,
            "year": 2016,
            "data_source": "通信利用動向調査 世帯全体編 statsDataId=0003161442",
            "note": "2016年が取得可能な最新年。令和5年版(tstat=000001218300)はAPIで都道府県別statsDataId未特定",
        })

    with open(out_path, "w", newline="", encoding="utf-8") as f:
        writer = csv.DictWriter(f, fieldnames=[
            "prefecture", "internet_usage_rate",
            "smartphone_ownership_rate", "year", "data_source", "note"
        ])
        writer.writeheader()
        writer.writerows(rows)

    print(f"  出力完了: {out_path} ({len(rows)}行)")
    _preview_csv(out_path, 5)


# ═══════════════════════════════════════════════
# ユーティリティ
# ═══════════════════════════════════════════════

def _preview_csv(path: Path, n: int = 5) -> None:
    """CSVの先頭n行をプレビュー表示する。"""
    print(f"\n  --- {path.name} 先頭{n}行 ---")
    with open(path, "r", encoding="utf-8") as f:
        reader = csv.DictReader(f)
        for i, row in enumerate(reader):
            if i >= n:
                break
            print(f"  {dict(row)}")
    # 行数確認
    with open(path, "r", encoding="utf-8") as f:
        total = sum(1 for _ in f) - 1  # ヘッダー除く
    print(f"  合計: {total}行")


def main() -> None:
    """コマンドライン引数を解析して対象データを取得する。"""
    parser = argparse.ArgumentParser(
        description="地価公示・自動車保有率・インターネット利用率のCSVを取得する"
    )
    parser.add_argument("--land", action="store_true", help="地価公示のみ取得")
    parser.add_argument("--car", action="store_true", help="自動車保有率のみ取得")
    parser.add_argument("--net", action="store_true", help="インターネット利用率のみ取得")
    args = parser.parse_args()

    # どのフラグも指定されていなければ全て実行
    run_all = not (args.land or args.car or args.net)

    # 出力ディレクトリを確保
    DATA_DIR.mkdir(parents=True, exist_ok=True)

    if run_all or args.land:
        fetch_land_price()

    if run_all or args.car:
        fetch_car_ownership()

    if run_all or args.net:
        fetch_internet_usage()

    print("\n完了しました。")


if __name__ == "__main__":
    main()
