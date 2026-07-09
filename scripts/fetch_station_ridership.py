#!/usr/bin/env python3
"""
駅別乗降客数データ取得スクリプト
==============================

出典: 国土交通省 国土数値情報 駅別乗降客数 S12-25
URL : https://nlftp.mlit.go.jp/ksj/gml/data/S12/S12-25/S12-25_GML.zip
ライセンス: CC BY 4.0（国土情報利用約款）
  本データは国土交通省の「国土数値情報（駅別乗降客数）」を利用しています。
  https://www.e-stat.go.jp/help/stat-search-3-5
  クレジット表記例: 国土交通省「国土数値情報（駅別乗降客数）」(S12-25)
                    (https://nlftp.mlit.go.jp/ksj/gml/datalist/KsjTmplt-S12.html)
                    を加工して作成。

属性定義 (XSD v3.3 に基づく):
  S12_001   駅名 (stationName)
  S12_001c  駅コード (stationCode, 6桁) ※市区町村コードではない
  S12_001g  グループコード (groupCode)
  S12_002   運営会社 (administrationCompany)
  S12_003   路線名 (routeName)
  S12_004   鉄道区分コード (railroadDivision)
  S12_005   事業者種別コード (railroadCompanyClassification)
  年次データブロック (2011-2024, 各4フィールド):
    S12_{6+4*(Y-2011)+0}  重複コード (duplicate)
    S12_{6+4*(Y-2011)+1}  データ有無コード (dataEorN): 1=有, 2=なし, 3=非公開, 4=駅なし
    S12_{6+4*(Y-2011)+2}  備考 (remarks)
    S12_{6+4*(Y-2011)+3}  乗降客数 (passengers, 1日平均)
  2019年: S12_038 (dup), S12_039 (dataEorN), S12_040 (remarks), S12_041 (passengers)
  2024年: S12_058 (dup), S12_059 (dataEorN), S12_060 (remarks), S12_061 (passengers)

注意:
  - GeoJSONに市区町村コード列は含まれない。本スクリプトでは
    geojson_gz/ (N03行政区域) の空間インデックスを用いて座標から割り当てる。
  - 乗降客数の単位: 1日平均乗降客数 (人/日)
  - NULL/非公開駅 (dataEorN≠1) は集計から除外し、counted_stations で管理。
  - geopandas は使用しない (ray-casting による純Python実装)。

出力:
  scripts/staging/station_ridership_muni.csv  市区町村集計
  scripts/staging/station_ridership_top.csv   市区町村内上位5駅

使用方法:
  python scripts/fetch_station_ridership.py
"""

import csv
import gzip
import io
import json
import os
import sys
import urllib.request
import zipfile
from collections import defaultdict
from pathlib import Path

sys.stdout.reconfigure(encoding="utf-8")
sys.stderr.reconfigure(encoding="utf-8")

# =========================================================
# 定数
# =========================================================

S12_ZIP_URL = (
    "https://nlftp.mlit.go.jp/ksj/gml/data/S12/S12-25/S12-25_GML.zip"
)
S12_GEOJSON_IN_ZIP = "S12-25_GML/UTF-8/S12-25_NumberOfPassengers.geojson"

SCRIPTS_DIR = Path(__file__).parent
STAGING_DIR = SCRIPTS_DIR / "staging"
GEOJSON_DIR = SCRIPTS_DIR.parent / "data" / "geojson_gz"

OUTPUT_MUNI = STAGING_DIR / "station_ridership_muni.csv"
OUTPUT_TOP  = STAGING_DIR / "station_ridership_top.csv"

# 年次フィールドマッピング (XSD v3.3, 2011起点)
#  duplicate: S12_{6 + (Y-2011)*4}
#  dataEorN : S12_{7 + (Y-2011)*4}
#  remarks  : S12_{8 + (Y-2011)*4}
#  passengers: S12_{9 + (Y-2011)*4}

def _year_fields(year: int) -> tuple[str, str, str, str]:
    """(dup, dataEorN, remarks, passengers) フィールド名を返す."""
    offset = (year - 2011) * 4
    base = 6 + offset
    return (
        f"S12_{base:03d}",
        f"S12_{base+1:03d}",
        f"S12_{base+2:03d}",
        f"S12_{base+3:03d}",
    )

_, EON_2019, _, PASS_2019 = _year_fields(2019)  # S12_039, S12_041
_, EON_2024, _, PASS_2024 = _year_fields(2024)  # S12_059, S12_061
_, EON_2023, _, PASS_2023 = _year_fields(2023)  # S12_055, S12_057 (spot check用)

DATAEON_VALID = 1   # dataEorN = 1 → データ有

# =========================================================
# ユーティリティ: 空間演算 (geopandas不使用)
# =========================================================

def _midpoint(coords: list) -> tuple[float, float] | tuple[None, None]:
    """LineString座標リストの中点 (lon, lat) を返す."""
    if not coords:
        return None, None
    n = len(coords)
    mid = n // 2
    if mid == 0:
        return float(coords[0][0]), float(coords[0][1])
    lon = (float(coords[mid - 1][0]) + float(coords[mid][0])) / 2
    lat = (float(coords[mid - 1][1]) + float(coords[mid][1])) / 2
    return lon, lat


def _ring_bbox(ring: list) -> tuple[float, float, float, float]:
    """ポリゴンリングのBounding Box (min_lon, min_lat, max_lon, max_lat)."""
    lons = [c[0] for c in ring]
    lats = [c[1] for c in ring]
    return min(lons), min(lats), max(lons), max(lats)


def _in_bbox(lon: float, lat: float,
             bbox: tuple[float, float, float, float]) -> bool:
    return bbox[0] <= lon <= bbox[2] and bbox[1] <= lat <= bbox[3]


def _in_ring(lon: float, lat: float, ring: list) -> bool:
    """Ray-casting 法によるポリゴン内外判定 (O(n))."""
    n = len(ring)
    inside = False
    j = n - 1
    for i in range(n):
        xi, yi = ring[i][0], ring[i][1]
        xj, yj = ring[j][0], ring[j][1]
        if ((yi > lat) != (yj > lat)) and \
           (lon < (xj - xi) * (lat - yi) / (yj - yi) + xi):
            inside = not inside
        j = i
    return inside


# =========================================================
# 空間インデックス構築
# =========================================================

def build_spatial_index(geojson_dir: Path) -> list[dict]:
    """
    geojson_gz/ ディレクトリから N03 行政区域データを読み込み、
    市区町村ごとの空間インデックスを構築する。

    各エントリ:
      muni_code : str (N03_007, 5桁)
      pref_name : str (N03_001)
      muni_name : str (N03_004)
      bbox      : (min_lon, min_lat, max_lon, max_lat)  全ポリゴンのUnion
      polygons  : list of ring  (各ring = [[lon, lat], ...])
    """
    gz_files = sorted(geojson_dir.glob("*.json.gz"))
    if not gz_files:
        raise FileNotFoundError(
            f"geojson_gz ファイルが見つかりません: {geojson_dir}"
        )
    print(f"  市区町村GeoJSONファイル数: {len(gz_files)} (都道府県別)")

    muni_dict: dict[str, dict] = {}

    for gz_path in gz_files:
        with gzip.open(gz_path, "rb") as f:
            gj = json.loads(f.read().decode("utf-8"))

        for feat in gj.get("features", []):
            props = feat.get("properties", {})
            muni_code = props.get("N03_007")
            if not muni_code:
                continue

            geom = feat.get("geometry", {})
            if geom.get("type") != "Polygon":
                continue

            coords = geom.get("coordinates")
            if not coords or not coords[0]:
                continue

            ring = coords[0]  # exterior ring (no-hole Polygon)
            ring_bbox = _ring_bbox(ring)

            pref_name = props.get("N03_001", "")
            muni_name = props.get("N03_004", "")

            if muni_code not in muni_dict:
                muni_dict[muni_code] = {
                    "muni_code": muni_code,
                    "pref_name": pref_name,
                    "muni_name": muni_name,
                    "bbox": ring_bbox,
                    "polygons": [ring],
                }
            else:
                # bbox を Union に拡張
                eb = muni_dict[muni_code]["bbox"]
                muni_dict[muni_code]["bbox"] = (
                    min(eb[0], ring_bbox[0]),
                    min(eb[1], ring_bbox[1]),
                    max(eb[2], ring_bbox[2]),
                    max(eb[3], ring_bbox[3]),
                )
                muni_dict[muni_code]["polygons"].append(ring)

    result = list(muni_dict.values())
    print(f"  市区町村エントリ数: {len(result)}")
    return result


def find_muni(lon: float, lat: float, spatial_index: list[dict]) -> dict | None:
    """
    座標 (lon, lat) を含む市区町村エントリを返す。

    1. Bounding Box フィルタで候補を絞り込む
    2. 候補 1件 → bbox マッチを採用
    3. 候補複数 → 精密 point-in-polygon で確認
    4. ポリゴン未マッチ → bbox 中心が最近傍の候補を返す
    """
    candidates = [m for m in spatial_index if _in_bbox(lon, lat, m["bbox"])]

    if not candidates:
        return None
    if len(candidates) == 1:
        return candidates[0]

    # 精密検査
    for muni in candidates:
        for ring in muni["polygons"]:
            if _in_ring(lon, lat, ring):
                return muni

    # フォールバック: bbox 中心への最近傍
    def _dist2(m: dict) -> float:
        b = m["bbox"]
        cx, cy = (b[0] + b[2]) / 2, (b[1] + b[3]) / 2
        return (lon - cx) ** 2 + (lat - cy) ** 2

    return min(candidates, key=_dist2)


# =========================================================
# メイン
# =========================================================

def main() -> None:
    STAGING_DIR.mkdir(parents=True, exist_ok=True)

    # --------------------------------------------------
    # Step 1: S12-25 ZIP ダウンロード
    # --------------------------------------------------
    print(f"\n[1/5] S12-25 ダウンロード中...")
    print(f"      {S12_ZIP_URL}")
    with urllib.request.urlopen(S12_ZIP_URL, timeout=120) as resp:
        zip_bytes = resp.read()
    print(f"      完了: {len(zip_bytes) // 1024} KB")

    # --------------------------------------------------
    # Step 2: GeoJSON パース
    # --------------------------------------------------
    print("[2/5] GeoJSON パース中...")
    with zipfile.ZipFile(io.BytesIO(zip_bytes)) as zf:
        raw_gj = zf.read(S12_GEOJSON_IN_ZIP)
    gj = json.loads(raw_gj.decode("utf-8"))
    features = gj.get("features", [])
    print(f"      フィーチャ数: {len(features):,}")

    # --------------------------------------------------
    # Step 3: 空間インデックス構築
    # --------------------------------------------------
    print("[3/5] 空間インデックス構築中 (N03 行政区域)...")
    if not GEOJSON_DIR.exists():
        raise FileNotFoundError(
            f"geojson_gz ディレクトリが見つかりません: {GEOJSON_DIR}"
        )
    spatial_index = build_spatial_index(GEOJSON_DIR)

    # --------------------------------------------------
    # Step 4: 各フィーチャに市区町村を割り当て
    # --------------------------------------------------
    print("[4/5] 市区町村割り当て中...")
    stations: list[dict] = []
    unmatched_count = 0

    for i, feat in enumerate(features):
        if i > 0 and i % 2000 == 0:
            print(f"      {i:,}/{len(features):,} 処理中...")

        props = feat.get("properties", {})
        geom  = feat.get("geometry", {})
        coords = geom.get("coordinates", [])
        lon, lat = _midpoint(coords)

        if lon is None:
            unmatched_count += 1
            muni_code, pref_name, muni_name = "UNMATCHED", "", ""
        else:
            muni_data = find_muni(lon, lat, spatial_index)
            if muni_data is None:
                unmatched_count += 1
                muni_code, pref_name, muni_name = "UNMATCHED", "", ""
            else:
                muni_code = muni_data["muni_code"]
                pref_name = muni_data["pref_name"]
                muni_name = muni_data["muni_name"]

        # 乗降客数・データ有無コード取得
        r2024   = props.get(PASS_2024)
        r2019   = props.get(PASS_2019)
        r2023   = props.get(PASS_2023)   # spot check 用
        eon2024 = props.get(EON_2024)
        eon2019 = props.get(EON_2019)

        valid_2024 = (eon2024 == DATAEON_VALID and r2024 is not None)
        valid_2019 = (eon2019 == DATAEON_VALID and r2019 is not None)

        stations.append({
            "muni_code":    muni_code,
            "pref_name":    pref_name,
            "muni_name":    muni_name,
            "station_name": props.get("S12_001", ""),
            "operator":     props.get("S12_002", ""),
            "line_name":    props.get("S12_003", ""),
            "ridership_2024": int(r2024) if valid_2024 else None,
            "ridership_2019": int(r2019) if valid_2019 else None,
            "ridership_2023": int(r2023) if r2023 is not None else None,  # spot check
            "valid_2024": valid_2024,
            "valid_2019": valid_2019,
        })

    matched = len(stations) - unmatched_count
    print(f"      マッチ: {matched:,}, アンマッチ: {unmatched_count:,}")

    # --------------------------------------------------
    # Step 5: 集計 → CSV 出力
    # --------------------------------------------------
    print("[5/5] 集計・CSV 出力中...")

    # --- 市区町村集計 ---
    muni_agg: dict[str, dict] = defaultdict(lambda: {
        "pref_name": "",
        "muni_name": "",
        "station_count": 0,
        "counted_stations": 0,
        "ridership_2024_sum": 0,
        "ridership_2019_sum": 0,
    })

    for s in stations:
        a = muni_agg[s["muni_code"]]
        a["pref_name"] = s["pref_name"]
        a["muni_name"] = s["muni_name"]
        a["station_count"] += 1
        if s["valid_2024"]:
            a["counted_stations"] += 1
            a["ridership_2024_sum"] += s["ridership_2024"]
        if s["valid_2019"]:
            a["ridership_2019_sum"] += s["ridership_2019"]

    muni_rows: list[dict] = []
    for code, a in sorted(muni_agg.items()):
        r24 = a["ridership_2024_sum"]
        r19 = a["ridership_2019_sum"]
        trend = round(r24 / r19, 4) if r19 > 0 else None
        muni_rows.append({
            "muni_code":          code,
            "prefecture":         a["pref_name"],
            "municipality":       a["muni_name"],
            "station_count":      a["station_count"],
            "counted_stations":   a["counted_stations"],
            "ridership_2024_sum": r24,
            "ridership_2019_sum": r19,
            "trend_ratio":        trend,
        })

    with open(OUTPUT_MUNI, "w", newline="", encoding="utf-8") as f:
        w = csv.DictWriter(f, fieldnames=[
            "muni_code", "prefecture", "municipality",
            "station_count", "counted_stations",
            "ridership_2024_sum", "ridership_2019_sum", "trend_ratio",
        ])
        w.writeheader()
        w.writerows(muni_rows)
    print(f"      {OUTPUT_MUNI.name}: {len(muni_rows):,} 行")

    # --- 上位5駅 CSV ---
    # valid_2024 のある駅のみ対象
    muni_valid_stations: dict[str, list] = defaultdict(list)
    for s in stations:
        if s["valid_2024"] and s["muni_code"] != "UNMATCHED":
            muni_valid_stations[s["muni_code"]].append(s)

    top_rows: list[dict] = []
    for code in sorted(muni_valid_stations.keys()):
        slist = sorted(
            muni_valid_stations[code],
            key=lambda x: x["ridership_2024"] or 0,
            reverse=True,
        )
        for s in slist[:5]:
            top_rows.append({
                "muni_code":    code,
                "municipality": s["muni_name"],
                "station_name": s["station_name"],
                "line_name":    s["line_name"],
                "operator":     s["operator"],
                "ridership_2024": s["ridership_2024"],
                "ridership_2019": s["ridership_2019"],
            })

    with open(OUTPUT_TOP, "w", newline="", encoding="utf-8") as f:
        w = csv.DictWriter(f, fieldnames=[
            "muni_code", "municipality", "station_name", "line_name",
            "operator", "ridership_2024", "ridership_2019",
        ])
        w.writeheader()
        w.writerows(top_rows)
    print(f"      {OUTPUT_TOP.name}: {len(top_rows):,} 行")

    # --------------------------------------------------
    # ドメイン検証
    # --------------------------------------------------
    print("\n[検証] ドメインチェック:")

    # 東京都千代田区 (13101) or 新宿区 (13104) の確認
    for code, name in [("13101", "千代田区"), ("13104", "新宿区")]:
        if code in muni_agg:
            a = muni_agg[code]
            print(f"  {name}({code}): stations={a['station_count']}, "
                  f"2024={a['ridership_2024_sum']:,}")

    # trend_ratio 値域チェック (0.1〜3.0 が常識レンジ)
    out_of_range = [
        r for r in muni_rows
        if r["trend_ratio"] is not None and not (0.1 <= r["trend_ratio"] <= 3.0)
    ]
    print(f"  trend_ratio 範囲外(0.1-3.0): {len(out_of_range)} 件")
    for r in out_of_range[:5]:
        print(f"    {r['muni_code']} {r['municipality']}: "
              f"ratio={r['trend_ratio']}, "
              f"2024={r['ridership_2024_sum']}, 2019={r['ridership_2019_sum']}")

    # 大分・別府スポット検証 (WF12 実測値との突合)
    # WF12: 大分駅≈30,854/日, 別府駅≈10,084/日 (令和5年度=2023年度 passengers2023)
    print("\n[スポット検証] 大分・別府 (WF12 裏取り値と突合):")
    print("  WF12 基準: 大分駅≈30,854/日, 別府駅≈10,084/日 (2023年度=passengers2023=S12_057)")
    for s in stations:
        if s["station_name"] in ("大分", "別府") and s["muni_code"].startswith("44"):
            print(f"  [{s['station_name']}] line={s['line_name']}, "
                  f"2024={s['ridership_2024']}, 2019={s['ridership_2019']}, "
                  f"2023={s['ridership_2023']}")

    # 全国上位5駅 (2024年)
    print("\n[常識検証] 全国乗降客数 TOP5 (2024年):")
    all_valid = [s for s in stations if s["valid_2024"]]
    top5 = sorted(all_valid, key=lambda x: x["ridership_2024"], reverse=True)[:5]
    for s in top5:
        print(f"  {s['station_name']} ({s['line_name']}): {s['ridership_2024']:,}/日")

    print("\n=== 完了 ===")
    print(f"  {OUTPUT_MUNI}")
    print(f"  {OUTPUT_TOP}")


if __name__ == "__main__":
    main()
