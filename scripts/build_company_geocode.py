# -*- coding: utf-8 -*-
"""SalesNow企業の座標データを生成する。

既存のCSIS geocoded住所データ(csis_checkpoint.csv)を活用し、
郵便番号レベルの代表座標をSalesNow企業にマッチングする。

使い方:
    python build_company_geocode.py
    python build_company_geocode.py --output company_geocode.csv

出力: corporate_number, lat, lng, geocode_source, geocode_confidence
"""
import csv
import sys
import re
import random
import argparse
from collections import defaultdict
from pathlib import Path

SCRIPT_DIR = Path(__file__).parent
DATA_DIR = SCRIPT_DIR.parent / "data"

CSIS_FILE = DATA_DIR / "csis_checkpoint.csv"
SALESNOW_FILE = DATA_DIR / "salesnow_companies.csv"
OUTPUT_FILE = DATA_DIR / "company_geocode.csv"

# 都道府県パターン
PREF_PATTERN = re.compile(
    r"(北海道|青森県|岩手県|宮城県|秋田県|山形県|福島県|茨城県|栃木県|群馬県|"
    r"埼玉県|千葉県|東京都|神奈川県|新潟県|富山県|石川県|福井県|山梨県|長野県|"
    r"岐阜県|静岡県|愛知県|三重県|滋賀県|京都府|大阪府|兵庫県|奈良県|和歌山県|"
    r"鳥取県|島根県|岡山県|広島県|山口県|徳島県|香川県|愛媛県|高知県|福岡県|"
    r"佐賀県|長崎県|熊本県|大分県|宮崎県|鹿児島県|沖縄県)"
)

# 市区町村パターン（都道府県の後の市区町村名を抽出）
MUNI_PATTERN = re.compile(
    r"(?:北海道|青森県|岩手県|宮城県|秋田県|山形県|福島県|茨城県|栃木県|群馬県|"
    r"埼玉県|千葉県|東京都|神奈川県|新潟県|富山県|石川県|福井県|山梨県|長野県|"
    r"岐阜県|静岡県|愛知県|三重県|滋賀県|京都府|大阪府|兵庫県|奈良県|和歌山県|"
    r"鳥取県|島根県|岡山県|広島県|山口県|徳島県|香川県|愛媛県|高知県|福岡県|"
    r"佐賀県|長崎県|熊本県|大分県|宮崎県|鹿児島県|沖縄県)"
    r"(.+?[市区町村郡])"
)


def extract_postal_from_address(address):
    """住所から郵便番号3桁エリアを推定（都道府県+市区町村）"""
    m = PREF_PATTERN.search(address)
    return m.group(1) if m else ""


def build_csis_postal_centroids():
    """CSIS geocoded住所データから郵便番号レベルの代表座標を構築。

    住所に郵便番号がないため、都道府県+市区町村別に重心を計算する。
    """
    # 都道府県+市区町村別の座標を蓄積
    region_coords = defaultdict(list)  # key: (pref, muni) → [(lat, lng)]

    print(f"CSISデータ読込中: {CSIS_FILE}")
    with open(CSIS_FILE, encoding="utf-8") as f:
        reader = csv.DictReader(f)
        count = 0
        for row in reader:
            lat_str = row.get("lat", "").strip()
            lng_str = row.get("lng", "").strip()
            address = row.get("address", "").strip()
            if not lat_str or not lng_str or not address:
                continue
            try:
                lat = float(lat_str)
                lng = float(lng_str)
            except ValueError:
                continue
            # 日本の範囲チェック
            if lat < 24 or lat > 46 or lng < 122 or lng > 154:
                continue

            pref = extract_postal_from_address(address)
            if not pref:
                continue

            m = MUNI_PATTERN.search(address)
            muni = m.group(1) if m else ""

            region_coords[(pref, muni)].append((lat, lng))
            count += 1

    print(f"  → {count}件のCSIS座標読込完了、{len(region_coords)}地域")
    return region_coords


def compute_centroids(region_coords):
    """各地域の重心座標を計算"""
    centroids = {}
    for (pref, muni), coords in region_coords.items():
        avg_lat = sum(c[0] for c in coords) / len(coords)
        avg_lng = sum(c[1] for c in coords) / len(coords)
        centroids[(pref, muni)] = (avg_lat, avg_lng, len(coords))
    return centroids


def add_jitter(lat, lng, radius_km=0.5):
    """重心座標にランダムオフセットを追加（同一地点への集中を回避）。
    radius_km: 散らす半径（km）。0.5km = 約500m圏内にランダム配置。
    """
    # 1度 ≈ 111km (緯度), 1度 ≈ 91km (経度、日本の緯度帯)
    lat_offset = (random.random() - 0.5) * 2 * (radius_km / 111.0)
    lng_offset = (random.random() - 0.5) * 2 * (radius_km / 91.0)
    return lat + lat_offset, lng + lng_offset


def match_company_to_centroid(company_pref, company_address, company_postal, centroids):
    """企業の住所情報から最適な重心座標をマッチング。
    同一市区町村の企業が1点に集中しないよう、±500mのランダムオフセットを追加。
    """
    # 1. 住所から市区町村を抽出してマッチ
    if company_address:
        m = MUNI_PATTERN.search(company_address)
        if m:
            muni = m.group(1)
            key = (company_pref, muni)
            if key in centroids:
                lat, lng, cnt = centroids[key]
                lat, lng = add_jitter(lat, lng, radius_km=0.5)
                confidence = min(3, 1 + (cnt // 100))
                return lat, lng, "address_muni_centroid", confidence

    # 2. 都道府県レベルのフォールバック
    pref_coords = []
    for (p, m), (lat, lng, cnt) in centroids.items():
        if p == company_pref:
            pref_coords.extend([(lat, lng)] * min(cnt, 10))
    if pref_coords:
        avg_lat = sum(c[0] for c in pref_coords) / len(pref_coords)
        avg_lng = sum(c[1] for c in pref_coords) / len(pref_coords)
        avg_lat, avg_lng = add_jitter(avg_lat, avg_lng, radius_km=2.0)
        return avg_lat, avg_lng, "pref_centroid", 1

    return None, None, None, 0


def main():
    parser = argparse.ArgumentParser(description="SalesNow企業のジオコードデータを生成")
    parser.add_argument("--output", default=str(OUTPUT_FILE), help="出力CSVパス")
    args = parser.parse_args()

    # Step 1: CSISデータから地域別座標を構築
    region_coords = build_csis_postal_centroids()
    centroids = compute_centroids(region_coords)
    print(f"重心座標計算完了: {len(centroids)}地域")

    # Step 2: SalesNow企業にマッチング
    print(f"\nSalesNow企業マッチング中: {SALESNOW_FILE}")
    matched = 0
    skipped = 0
    total = 0

    output_path = Path(args.output)
    with open(SALESNOW_FILE, encoding="utf-8") as fin, \
         open(output_path, "w", encoding="utf-8", newline="") as fout:

        reader = csv.DictReader(fin)
        writer = csv.writer(fout)
        writer.writerow(["corporate_number", "lat", "lng", "geocode_source", "geocode_confidence"])

        for row in reader:
            total += 1
            corp_num = row.get("corporate_number", "").strip()
            if not corp_num:
                skipped += 1
                continue

            pref = row.get("prefecture", "").strip()
            address = row.get("address", "").strip()
            postal = row.get("postal_code", "").strip()

            if not pref:
                skipped += 1
                continue

            lat, lng, source, confidence = match_company_to_centroid(
                pref, address, postal, centroids
            )

            if lat is not None:
                writer.writerow([corp_num, f"{lat:.6f}", f"{lng:.6f}", source, confidence])
                matched += 1
            else:
                skipped += 1

            if total % 50000 == 0:
                print(f"  ... {total}件処理済 ({matched}件マッチ)")

    print(f"\n完了:")
    print(f"  総企業数: {total}")
    print(f"  マッチ成功: {matched} ({matched/total*100:.1f}%)")
    print(f"  スキップ: {skipped}")
    print(f"  出力: {output_path}")


if __name__ == "__main__":
    main()
