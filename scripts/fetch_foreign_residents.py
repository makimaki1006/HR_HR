"""
e-Stat APIから都道府県×在留資格別の在留外国人数を取得し、CSVに保存するスクリプト。

データソース: e-Stat API
統計表ID: 0003147704
  「都道府県別 在留資格別 在留外国人（総数、中国、台湾、韓国、フィリピン、ブラジル）」
  出典: 法務省 在留外国人統計（2012年12月〜2017年6月）

出力: scripts/data/foreign_residents_by_prefecture.csv
カラム: prefecture, visa_status, count, survey_period

在留資格の集約ルール:
  - 技能実習: 技能実習１号イ/ロ + 技能実習２号イ/ロ
  - 特定技能: 特定活動_計（2017年時点は特定技能制度未存在のため特定活動に含む）
  - 永住者: 永住者 + 特別永住者
  - 留学: 留学
  - 技術・人文知識・国際業務: 技術・人文知識・国際業務 + 旧区分（技術、人文知識・国際業務）
  - その他: 上記以外の在留資格の合計
"""

import urllib.request
import json
import csv
import os
import sys

# ─── 設定 ─────────────────────────────────────────
APP_ID = "85f70d978a4fd0da6234e2d07fc423920e077ee5"
STATS_DATA_ID = "0003147704"
OUTPUT_CSV = os.path.join(os.path.dirname(__file__), "data", "foreign_residents_by_prefecture.csv")

# 都道府県コード → 都道府県名のマッピング（areaコードは5桁）
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

# 在留資格コード → 集約カテゴリのマッピング
# cat01コードは4桁数字
# 1010=総数（集計用のため除外）
# 集約カテゴリ:
#   技能実習: 1230(技能実習１号イ), 1240(技能実習１号ロ), 1250(技能実習２号イ), 1260(技能実習２号ロ)
#   特定技能: 1330(特定活動_計) — 2017年時点は特定技能制度未存在、特定活動を代替として使用
#   永住者:   1350(永住者), 1390(特別永住者)
#   留学:     1290(留学)
#   技術・人文知識・国際業務: 1170(技術・人文知識・国際業務), 1180(技術), 1190(人文知識・国際業務)
#   その他:   上記以外の個別在留資格（1010総数は除外）
VISA_CATEGORY_MAP = {
    "1230": "技能実習",
    "1240": "技能実習",
    "1250": "技能実習",
    "1260": "技能実習",
    "1330": "特定技能",   # 2017年時点は特定活動として計上
    "1350": "永住者",
    "1390": "永住者",     # 特別永住者を永住者に統合
    "1290": "留学",
    "1170": "技術・人文知識・国際業務",
    "1180": "技術・人文知識・国際業務",  # 旧区分「技術」
    "1190": "技術・人文知識・国際業務",  # 旧区分「人文知識・国際業務」
}

# 「総数」コード（集計に使わない）
TOTAL_CODE = "1010"

# その他扱いしないコード（総数のみ）
EXCLUDE_CODES = {TOTAL_CODE}

# 集約カテゴリの定義順（CSV出力順）
VISA_CATEGORIES = [
    "技能実習",
    "特定技能",
    "永住者",
    "留学",
    "技術・人文知識・国際業務",
    "その他",
]


def fetch_latest_survey_period():
    """メタ情報から最新の調査時点コードを取得する"""
    url = (
        f"https://api.e-stat.go.jp/rest/3.0/app/json/getMetaInfo"
        f"?appId={APP_ID}"
        f"&statsDataId={STATS_DATA_ID}"
    )
    print("メタ情報を取得中...")
    req = urllib.request.Request(url)
    with urllib.request.urlopen(req, timeout=60) as resp:
        data = json.loads(resp.read().decode("utf-8"))

    meta = data["GET_META_INFO"]["METADATA_INF"]["CLASS_INF"]["CLASS_OBJ"]
    for c in meta:
        if c["@id"] == "time":
            items = c["CLASS"]
            if isinstance(items, list):
                # 最初のコードが最新（メタ情報は降順）
                latest = items[0]
            else:
                latest = items
            print(f"最新調査時点: {latest['@code']} ({latest['@name']})")
            return latest["@code"], latest["@name"]

    raise ValueError("timeカラムがメタ情報に見つかりません")


def fetch_estat_data(time_code):
    """e-Stat APIから指定時点の都道府県×在留資格別データを取得する

    国籍(cat03)=000(総数)のみ取得。
    100000件制限内に収まるかページネーションで対応。
    """
    # 総数（cat03=000）に絞ることでデータ量を削減
    base_url = (
        f"https://api.e-stat.go.jp/rest/3.0/app/json/getStatsData"
        f"?appId={APP_ID}"
        f"&lang=J"
        f"&statsDataId={STATS_DATA_ID}"
        f"&cdTime={time_code}"    # 最新時点のみ
        f"&cdCat03=000"           # 国籍=総数のみ
        f"&limit=100000"
    )

    all_values = []
    start_position = 1

    while True:
        url = base_url + f"&startPosition={start_position}"
        print(f"  API取得中... startPosition={start_position}")
        req = urllib.request.Request(url)
        with urllib.request.urlopen(req, timeout=60) as resp:
            data = json.loads(resp.read().decode("utf-8"))

        status = data["GET_STATS_DATA"]["RESULT"]["STATUS"]
        if status != 0:
            msg = data["GET_STATS_DATA"]["RESULT"]["ERROR_MSG"]
            raise RuntimeError(f"API エラー (status={status}): {msg}")

        stat_data = data["GET_STATS_DATA"]["STATISTICAL_DATA"]
        result_inf = stat_data.get("RESULT_INF", {})
        total_number = int(result_inf.get("TOTAL_NUMBER", 0))
        from_number = int(result_inf.get("FROM_NUMBER", start_position))
        to_number = int(result_inf.get("TO_NUMBER", start_position))

        if start_position == 1:
            print(f"  総データ件数: {total_number}")

        values = stat_data.get("DATA_INF", {}).get("VALUE", [])
        if isinstance(values, dict):
            # 1件のみの場合はdictで返る
            values = [values]

        all_values.extend(values)
        print(f"  取得済み: {len(all_values)} / {total_number}")

        # 全件取得完了チェック
        if to_number >= total_number:
            break

        # 次ページへ
        start_position = to_number + 1

    return all_values


def aggregate_by_category(values, survey_period_name):
    """取得データを都道府県×集約在留資格カテゴリに集計する

    Args:
        values: API VALUEリスト
        survey_period_name: 調査時点の表示名（例: "2017年6月"）
    Returns:
        records: [{"prefecture":..., "visa_status":..., "count":..., "survey_period":...}, ...]
    """
    # {都道府県名: {カテゴリ: カウント}} の形で集計
    agg = {}  # agg[prefecture][visa_category] = int

    skipped_area = 0
    skipped_total = 0

    for v in values:
        area_code = v.get("@area", "")
        cat01_code = v.get("@cat01", "")
        value_str = v.get("$", "")

        # 全国(00000)はスキップ
        if area_code == "00000":
            continue

        # 都道府県名の解決
        pref_name = PREF_CODE_TO_NAME.get(area_code)
        if pref_name is None:
            skipped_area += 1
            continue

        # 総数コードはスキップ（個別在留資格のみ集計）
        if cat01_code in EXCLUDE_CODES:
            skipped_total += 1
            continue

        # 数値変換（"-"や"x"など秘匿値はゼロ扱い）
        try:
            count = int(value_str)
        except (ValueError, TypeError):
            count = 0

        # 集約カテゴリを決定
        category = VISA_CATEGORY_MAP.get(cat01_code, "その他")

        # 集計
        if pref_name not in agg:
            agg[pref_name] = {cat: 0 for cat in VISA_CATEGORIES}
        agg[pref_name][category] += count

    print(f"  スキップ（未知都道府県）: {skipped_area} 件")
    print(f"  スキップ（総数コード）: {skipped_total} 件")

    # recordsに変換
    records = []
    for pref_name in sorted(agg.keys()):
        for cat in VISA_CATEGORIES:
            records.append({
                "prefecture": pref_name,
                "visa_status": cat,
                "count": agg[pref_name][cat],
                "survey_period": survey_period_name,
            })

    return records


def save_to_csv(records, output_path):
    """集計結果をCSVに保存する"""
    os.makedirs(os.path.dirname(output_path), exist_ok=True)

    with open(output_path, "w", newline="", encoding="utf-8-sig") as f:
        writer = csv.DictWriter(f, fieldnames=["prefecture", "visa_status", "count", "survey_period"])
        writer.writeheader()
        writer.writerows(records)

    print(f"\nCSV保存完了: {output_path}")
    print(f"  総行数（ヘッダー除く）: {len(records)}")


def validate_output(records):
    """出力データの妥当性を検証する"""
    import collections

    prefs = {r["prefecture"] for r in records}
    cats = {r["visa_status"] for r in records}

    pref_count = len(prefs)
    cat_count = len(cats)

    print(f"\n=== 検証結果 ===")
    print(f"都道府県数: {pref_count} （期待値: 47）")
    print(f"在留資格カテゴリ数: {cat_count} （期待値: {len(VISA_CATEGORIES)}）")

    if pref_count != 47:
        print(f"  ⚠ 都道府県数が47ではありません。実際: {sorted(prefs)}")
    else:
        print(f"  OK: 47都道府県すべて揃っています")

    if cat_count != len(VISA_CATEGORIES):
        print(f"  ⚠ カテゴリ数が{len(VISA_CATEGORIES)}ではありません")
    else:
        print(f"  OK: 全カテゴリ揃っています")

    # 東京都のデータを表示（サンプル確認）
    print(f"\n=== 東京都のデータ（サンプル） ===")
    tokyo_rows = [r for r in records if r["prefecture"] == "東京都"]
    for row in tokyo_rows:
        print(f"  {row['visa_status']}: {row['count']:,} 人")

    # 全国合計（全都道府県の在留外国人総数）
    total_all = sum(r["count"] for r in records if r["visa_status"] != "その他")
    total_with_other = sum(r["count"] for r in records)
    print(f"\n=== 全国合計（全都道府県合計） ===")
    print(f"  在留外国人総数（主要カテゴリ合計）: {total_all:,} 人")

    # カテゴリ別全国合計
    print(f"\n=== カテゴリ別全国合計 ===")
    cat_totals = collections.Counter()
    for r in records:
        cat_totals[r["visa_status"]] += r["count"]
    for cat in VISA_CATEGORIES:
        print(f"  {cat}: {cat_totals[cat]:,} 人")


def show_sample_csv(output_path, n=5):
    """CSVの先頭n行を表示する"""
    print(f"\n=== CSV先頭{n}行 ===")
    with open(output_path, "r", encoding="utf-8-sig") as f:
        for i, line in enumerate(f):
            if i > n:
                break
            print(f"  {line.rstrip()}")


def main():
    print("=" * 60)
    print("在留外国人統計（都道府県×在留資格別）データ取得")
    print(f"statsDataId: {STATS_DATA_ID}")
    print("=" * 60)
    print()

    # 1. 最新調査時点を取得
    time_code, time_name = fetch_latest_survey_period()

    # 2. e-Stat APIからデータ取得
    print(f"\n[ステップ1] データ取得中（調査時点: {time_name}）...")
    values = fetch_estat_data(time_code)
    print(f"  取得完了: {len(values)} 件")

    # 3. 都道府県×在留資格カテゴリに集計
    print(f"\n[ステップ2] 在留資格カテゴリに集約中...")
    records = aggregate_by_category(values, time_name)
    print(f"  集計完了: {len(records)} 行")

    # 4. CSV保存
    print(f"\n[ステップ3] CSV保存中...")
    save_to_csv(records, OUTPUT_CSV)

    # 5. 検証
    validate_output(records)

    # 6. CSVサンプル表示
    show_sample_csv(OUTPUT_CSV, n=5)

    print(f"\n完了。出力ファイル: {OUTPUT_CSV}")


if __name__ == "__main__":
    main()
