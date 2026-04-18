"""
令和２年国勢調査から都道府県別の学歴分布・世帯構成データを取得し、
CSV ファイルに出力するスクリプト。

出力ファイル:
  scripts/data/education_by_prefecture.csv
    prefecture, education_level, male_count, female_count, total_count
  scripts/data/household_by_prefecture.csv
    prefecture, household_type, count, ratio

データソース: e-Stat API (令和2年国勢調査)
  学歴: statsDataId=0003450543 (就業状態等基本集計 教育)
  世帯: statsDataId=0003445080 (人口等基本集計 世帯の家族類型)
"""

import csv
import json
import os
import sys
import urllib.parse
import urllib.request

# ─── 設定 ──────────────────────────────────────────────────────────────────
APP_ID = "85f70d978a4fd0da6234e2d07fc423920e077ee5"

# 出力先ディレクトリ（スクリプト配置ディレクトリ直下の data/ フォルダ）
SCRIPT_DIR = os.path.dirname(os.path.abspath(__file__))
DATA_DIR = os.path.join(SCRIPT_DIR, "data")
EDU_CSV = os.path.join(DATA_DIR, "education_by_prefecture.csv")
HH_CSV = os.path.join(DATA_DIR, "household_by_prefecture.csv")

# 都道府県コード → 都道府県名
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

# 学歴コード → 表示名
# cat03: 11=小学校, 12=中学校, 13=高校・旧中, 14=短大・高専, 15=大学, 16=大学院
EDU_CODE_TO_NAME = {
    "11": "小学校",
    "12": "中学校",
    "13": "高校",
    "14": "短大・高専",
    "15": "大学",
    "16": "大学院",
}

# 世帯家族類型コード → 表示名（summary用の主要4類型のみ使用）
# cat02: 0=総数, 1=親族のみ, 11=核家族, 12=核家族以外(三世代等), 2=非親族含む世帯, 3=単独世帯
HH_CODE_TO_NAME = {
    "0": "総数",
    "1": "親族のみの世帯",
    "11": "核家族世帯",
    "12": "核家族以外の世帯",
    "2": "非親族を含む世帯",
    "3": "単独世帯",
}


def call_estat_api(stats_data_id: str, extra_params: dict) -> list:
    """
    e-Stat API の getStatsData エンドポイントを呼び出してデータを取得する。
    ページングが必要な場合は自動で複数回リクエストする。

    Args:
        stats_data_id: 統計表 ID
        extra_params: 絞り込みパラメータの辞書

    Returns:
        VALUE リストの全レコード
    """
    base_url = "https://api.e-stat.go.jp/rest/3.0/app/json/getStatsData"
    page_size = 10000
    start_pos = 1
    all_values = []

    while True:
        params = {
            "appId": APP_ID,
            "lang": "J",
            "statsDataId": stats_data_id,
            "limit": page_size,
            "startPosition": start_pos,
        }
        params.update(extra_params)

        url = f"{base_url}?{urllib.parse.urlencode(params)}"
        print(f"  API リクエスト中 (startPosition={start_pos})...")

        with urllib.request.urlopen(url, timeout=60) as resp:
            data = json.loads(resp.read().decode("utf-8"))

        result_status = data["GET_STATS_DATA"]["RESULT"]["STATUS"]
        if result_status != 0:
            msg = data["GET_STATS_DATA"]["RESULT"]["ERROR_MSG"]
            print(f"  API エラー (status={result_status}): {msg}", file=sys.stderr)
            sys.exit(1)

        stat_data = data["GET_STATS_DATA"]["STATISTICAL_DATA"]
        total_number = int(stat_data["RESULT_INF"]["TOTAL_NUMBER"])
        values = stat_data["DATA_INF"].get("VALUE", [])
        if isinstance(values, dict):
            values = [values]

        all_values.extend(values)

        # 全件取得済みかチェック
        if start_pos + len(values) - 1 >= total_number:
            print(f"  取得完了: {total_number} 件")
            break

        start_pos += page_size

    return all_values


# ─── 学歴データ取得・整形 ──────────────────────────────────────────────────

def fetch_education_data() -> list[dict]:
    """
    令和2年国勢調査の最終卒業学校データを取得して整形する。

    絞り込み条件:
      - cat01 (男女): 0=総数, 1=男, 2=女
      - cat02 (年齢): 00=総数
      - cat03 (在学・学歴): 11-16 (各学歴の卒業者)
      - lvArea=2: 都道府県レベル

    Returns:
        prefecture, education_level, male_count, female_count, total_count の辞書リスト
    """
    print("学歴データ取得中...")
    values = call_estat_api("0003450543", {
        "cdCat01": "0,1,2",          # 総数・男・女
        "cdCat02": "00",             # 年齢=総数
        "cdCat03": "11,12,13,14,15,16",  # 各学歴
        "lvArea": "2",               # 都道府県レベル
    })

    # (都道府県コード, 学歴コード) → {総数/男/女: count} の辞書に集約
    agg: dict[tuple, dict] = {}
    skipped = 0

    for v in values:
        area = v.get("@area", "")
        cat01 = v.get("@cat01", "")  # 男女コード
        cat03 = v.get("@cat03", "")  # 学歴コード
        val_str = v.get("$", "")

        # 都道府県のみ対象
        if area not in PREF_CODE_TO_NAME:
            skipped += 1
            continue

        # 数値変換（秘匿値 "-" 等は None）
        try:
            count = int(val_str)
        except (ValueError, TypeError):
            count = None

        key = (area, cat03)
        if key not in agg:
            agg[key] = {"0": None, "1": None, "2": None}
        agg[key][cat01] = count

    print(f"  有効集計キー: {len(agg)}, スキップ: {skipped}")

    # dict → レコードリストに変換
    records = []
    for (area, edu_code), counts in sorted(agg.items()):
        pref_name = PREF_CODE_TO_NAME[area]
        edu_name = EDU_CODE_TO_NAME.get(edu_code, edu_code)
        records.append({
            "prefecture": pref_name,
            "education_level": edu_name,
            "male_count": counts.get("1"),
            "female_count": counts.get("2"),
            "total_count": counts.get("0"),
        })

    return records


def write_education_csv(records: list[dict]) -> None:
    """学歴データを CSV に書き出す"""
    os.makedirs(DATA_DIR, exist_ok=True)
    fieldnames = ["prefecture", "education_level", "male_count", "female_count", "total_count"]

    with open(EDU_CSV, "w", newline="", encoding="utf-8-sig") as f:
        writer = csv.DictWriter(f, fieldnames=fieldnames)
        writer.writeheader()
        writer.writerows(records)

    print(f"  出力: {EDU_CSV} ({len(records)} 行)")

    # 先頭5行を表示して確認
    print("\n  --- 先頭5行 ---")
    for row in records[:5]:
        print(f"    {row}")


# ─── 世帯データ取得・整形 ──────────────────────────────────────────────────

def fetch_household_data() -> list[dict]:
    """
    令和2年国勢調査の世帯家族類型データを取得して整形する。

    絞り込み条件:
      - cat01 (世帯人員): 0=総数のみ
      - cat02 (家族類型): 0=総数, 1=親族のみ, 11=核家族, 12=核家族以外, 2=非親族含む, 3=単独
      - lvArea=2: 都道府県レベル

    Returns:
        prefecture, household_type, count, ratio の辞書リスト
    """
    print("世帯データ取得中...")
    values = call_estat_api("0003445080", {
        "cdCat01": "0",              # 世帯人員=総数
        "cdCat02": "0,1,11,12,2,3", # 家族類型
        "lvArea": "2",               # 都道府県レベル
    })

    # (都道府県コード, 家族類型コード) → count の辞書に集約
    raw: dict[tuple, int | None] = {}
    skipped = 0

    for v in values:
        area = v.get("@area", "")
        cat02 = v.get("@cat02", "")  # 家族類型コード
        val_str = v.get("$", "")

        if area not in PREF_CODE_TO_NAME:
            skipped += 1
            continue

        try:
            count = int(val_str)
        except (ValueError, TypeError):
            count = None

        raw[(area, cat02)] = count

    print(f"  有効集計キー: {len(raw)}, スキップ: {skipped}")

    # ratio 計算と整形
    records = []
    # 都道府県ごとに総数を取得してから各類型の比率を算出
    prefectures = sorted({area for (area, _) in raw.keys()})

    for area in prefectures:
        pref_name = PREF_CODE_TO_NAME[area]
        total = raw.get((area, "0"))  # 総世帯数

        for hh_code, hh_name in HH_CODE_TO_NAME.items():
            count = raw.get((area, hh_code))

            # 比率計算（総数ゼロや None の場合は None）
            if total and count is not None and total > 0:
                ratio = round(count / total, 6)
            else:
                ratio = None

            records.append({
                "prefecture": pref_name,
                "household_type": hh_name,
                "count": count,
                "ratio": ratio,
            })

    return records


def write_household_csv(records: list[dict]) -> None:
    """世帯データを CSV に書き出す"""
    os.makedirs(DATA_DIR, exist_ok=True)
    fieldnames = ["prefecture", "household_type", "count", "ratio"]

    with open(HH_CSV, "w", newline="", encoding="utf-8-sig") as f:
        writer = csv.DictWriter(f, fieldnames=fieldnames)
        writer.writeheader()
        writer.writerows(records)

    print(f"  出力: {HH_CSV} ({len(records)} 行)")

    # 先頭5行を表示して確認
    print("\n  --- 先頭5行 ---")
    for row in records[:5]:
        print(f"    {row}")


# ─── メイン ───────────────────────────────────────────────────────────────

def main() -> None:
    print("=" * 60)
    print("令和2年国勢調査 学歴・世帯構成データ取得")
    print("=" * 60)
    print()

    # 1. 学歴データ
    print("[1/2] 学歴分布データ")
    edu_records = fetch_education_data()
    write_education_csv(edu_records)
    print()

    # 2. 世帯データ
    print("[2/2] 世帯構成データ")
    hh_records = fetch_household_data()
    write_household_csv(hh_records)
    print()

    # 最終サマリー
    print("=" * 60)
    print("取得完了")
    print(f"  学歴CSV: {len(edu_records)} 行  → {EDU_CSV}")
    print(f"  世帯CSV: {len(hh_records)} 行  → {HH_CSV}")

    # 都道府県数チェック
    edu_prefs = len({r["prefecture"] for r in edu_records})
    hh_prefs = len({r["prefecture"] for r in hh_records})
    print(f"  学歴: 都道府県数={edu_prefs} (期待値=47)")
    print(f"  世帯: 都道府県数={hh_prefs} (期待値=47)")

    if edu_prefs != 47:
        print(f"  警告: 学歴データの都道府県数が47ではありません (実際={edu_prefs})", file=sys.stderr)
    if hh_prefs != 47:
        print(f"  警告: 世帯データの都道府県数が47ではありません (実際={hh_prefs})", file=sys.stderr)

    print("=" * 60)


if __name__ == "__main__":
    main()
