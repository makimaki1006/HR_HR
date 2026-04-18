"""
e-Stat 経済センサス（令和3年）から市区町村×産業大分類別の
事業所数・従業者数を取得してCSVに出力するスクリプト。

統計表ID: 0003449718
  - 産業大分類(全産業 + A〜S の19〜21区分)
  - 表章項目: 事業所数 / 従業者数(男女計・男・女)
  - 集計範囲: 全国市区町村

出力: scripts/data/industry_structure_by_municipality.csv
  カラム: prefecture_code, city_code, city_name,
          industry_code, industry_name,
          establishments, employees_total, employees_male, employees_female

実行方法:
  python scripts/fetch_industry_structure.py
  # 途中再開: .progress ファイルに取得済み市区町村コードを記録
  # 再実行すると未取得分のみ続きから取得する
  # --reset オプションで最初からやり直す
"""

import urllib.request
import urllib.parse
import json
import csv
import os
import sys
import time
import io

# ─── 設定 ───────────────────────────────────────────────────────────────────
APP_ID = "85f70d978a4fd0da6234e2d07fc423920e077ee5"
STATS_DATA_ID = "0003449718"

# 出力CSVパス（このスクリプトの場所を基準に解決）
SCRIPT_DIR = os.path.dirname(os.path.abspath(__file__))
OUTPUT_CSV = os.path.join(SCRIPT_DIR, "data", "industry_structure_by_municipality.csv")

# 途中再開用の進捗ファイル（取得済み市区町村コードを1行1コードで記録）
PROGRESS_FILE = os.path.join(SCRIPT_DIR, "data", "industry_structure_by_municipality.progress")

# APIのリクエスト間隔（秒）- e-Statへの負荷軽減
REQUEST_INTERVAL = 1.0

# 1リクエストあたりの最大取得件数
LIMIT = 100000

# 経営組織コード: 0 = 総数（民営＋公営）で絞り込み
CD_CAT02 = "0"

# 表章項目コード（tabコード）
TAB_ESTABLISHMENTS = "102-2021"   # 事業所数
TAB_EMPLOYEES_TOTAL = "113-2021"  # 従業者数（男女計）
TAB_EMPLOYEES_MALE = "114-2021"   # 従業者数（男）
TAB_EMPLOYEES_FEMALE = "115-2021" # 従業者数（女）

# 産業コード → 産業名のマッピング
# e-Stat メタ情報から取得した実際のコード体系（21区分）
# AB=農林漁業（農業+林業+漁業の統合）、CR=非農林漁業公務除く（集計用）
INDUSTRY_CODE_TO_NAME = {
    "AS": "全産業",
    "AR": "全産業（公務を除く）",
    "AB": "農林漁業",
    "CR": "非農林漁業（公務を除く）",
    "C":  "鉱業，採石業，砂利採取業",
    "D":  "建設業",
    "E":  "製造業",
    "F":  "電気・ガス・熱供給・水道業",
    "G":  "情報通信業",
    "H":  "運輸業，郵便業",
    "I":  "卸売業，小売業",
    "J":  "金融業，保険業",
    "K":  "不動産業，物品賃貸業",
    "L":  "学術研究，専門・技術サービス業",
    "M":  "宿泊業，飲食サービス業",
    "N":  "生活関連サービス業，娯楽業",
    "O":  "教育，学習支援業",
    "P":  "医療，福祉",
    "Q":  "複合サービス事業",
    "R":  "サービス業（他に分類されないもの）",
    "S":  "公務（他に分類されるものを除く）",
}

# 都道府県コード → 都道府県名（2桁コード → 名称）
PREF_CODE_TO_NAME = {
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

# 出力CSVのカラム定義
OUTPUT_COLUMNS = [
    "prefecture_code",   # 都道府県コード（2桁）
    "city_code",         # 市区町村コード（5桁）
    "city_name",         # 市区町村名
    "industry_code",     # 産業コード（A, B, ... S, AS）
    "industry_name",     # 産業名
    "establishments",    # 事業所数
    "employees_total",   # 従業者数（男女計）
    "employees_male",    # 従業者数（男）
    "employees_female",  # 従業者数（女）
]


def fetch_meta_info():
    """
    メタ情報APIから全市区町村コードを取得する。
    @level="2" が市区町村（政令市も含む親市），"3" が政令市の区。
    ここでは @level="2" の市区町村コードのみを対象とする。
    """
    url = (
        f"https://api.e-stat.go.jp/rest/3.0/app/json/getMetaInfo"
        f"?appId={APP_ID}&statsDataId={STATS_DATA_ID}&lang=J"
    )
    print("メタ情報取得中...")
    req = urllib.request.Request(url)
    with urllib.request.urlopen(req, timeout=60) as resp:
        data = json.loads(resp.read().decode("utf-8"))

    status = data["GET_META_INFO"]["RESULT"]["STATUS"]
    if status != 0:
        msg = data["GET_META_INFO"]["RESULT"]["ERROR_MSG"]
        print(f"メタ情報APIエラー: {msg}")
        sys.exit(1)

    # CLASS_OBJからareaを取得
    class_objs = data["GET_META_INFO"]["METADATA_INF"]["CLASS_INF"]["CLASS_OBJ"]
    area_obj = None
    for obj in class_objs:
        if obj["@id"] == "area":
            area_obj = obj
            break

    if area_obj is None:
        print("エラー: areaのCLASS_OBJが見つかりません")
        sys.exit(1)

    classes = area_obj["CLASS"]
    # リストでない場合（1件のみ）はリストに変換
    if isinstance(classes, dict):
        classes = [classes]

    # 市区町村レベル（@level="2"）かつ5桁コードのみ抽出
    # 全国(00000)や都道府県(XY000)は除外
    municipalities = []
    for cls in classes:
        code = cls.get("@code", "")
        level = cls.get("@level", "")
        name = cls.get("@name", "")
        parent = cls.get("@parentCode", "")

        # 全国・都道府県レベルはスキップ
        if level not in ("2",):
            continue
        # 5桁コードであること
        if len(code) != 5:
            continue
        # 末尾3桁が "000" は都道府県集計値なのでスキップ
        if code[2:] == "000":
            continue

        municipalities.append({
            "code": code,
            "name": name,
            "pref_code": code[:2],  # 先頭2桁が都道府県コード
        })

    print(f"市区町村数: {len(municipalities)} 件")
    return municipalities


def fetch_stats_data_for_city(city_code):
    """
    指定した市区町村コードの産業別データをAPIから取得する。
    ページネーション対応（1回100000件、複数ページ自動処理）。

    戻り値: {(tab_code, industry_code): 値} の辞書
    """
    all_values = []
    start_position = 1

    while True:
        params = {
            "appId": APP_ID,
            "lang": "J",
            "statsDataId": STATS_DATA_ID,
            "cdArea": city_code,
            "cdCat02": CD_CAT02,   # 総数（民営＋公営）
            "limit": str(LIMIT),
            "startPosition": str(start_position),
        }
        url = "https://api.e-stat.go.jp/rest/3.0/app/json/getStatsData?" + urllib.parse.urlencode(params)

        req = urllib.request.Request(url)
        data = None
        for attempt in range(5):
            try:
                with urllib.request.urlopen(req, timeout=90) as resp:
                    data = json.loads(resp.read().decode("utf-8"))
                break
            except Exception as e:
                wait = (attempt + 1) * 3
                print(f"  [リトライ {attempt+1}/5] {city_code}: {e} → {wait}秒待機")
                time.sleep(wait)
        if data is None:
            print(f"  [警告] {city_code} のAPIリクエスト5回失敗、スキップ")
            return {}

        result = data["GET_STATS_DATA"]["RESULT"]
        if result["STATUS"] != 0:
            # 該当データなし（離島等）は空で返す
            return {}

        stat_data = data["GET_STATS_DATA"]["STATISTICAL_DATA"]
        result_inf = stat_data["RESULT_INF"]
        total = int(result_inf.get("TOTAL_NUMBER", 0))
        to_num = int(result_inf.get("TO_NUMBER", 0))

        values = stat_data.get("DATA_INF", {}).get("VALUE", [])
        if isinstance(values, dict):
            values = [values]

        all_values.extend(values)

        # 全件取得済みか確認
        if to_num >= total:
            break
        start_position = to_num + 1
        time.sleep(REQUEST_INTERVAL)

    # (tab_code, industry_code) → 値 の辞書に変換
    result_map = {}
    for v in all_values:
        tab = v.get("@tab", "")
        cat01 = v.get("@cat01", "")
        val_str = v.get("$", "")

        # 秘匿値("-", "x" 等)はNoneに変換
        try:
            val = int(val_str)
        except (ValueError, TypeError):
            val = None

        result_map[(tab, cat01)] = val

    return result_map


def load_progress():
    """
    進捗ファイルから取得済み市区町村コードのセットを返す（途中再開用）。
    CSVではなく別ファイルに記録するため、CSV書き込み中でも安全に読める。
    """
    done = set()
    if not os.path.exists(PROGRESS_FILE):
        return done
    with open(PROGRESS_FILE, "r", encoding="utf-8") as f:
        for line in f:
            code = line.strip()
            if code:
                done.add(code)
    return done


def save_progress(city_code):
    """取得済み市区町村コードを進捗ファイルに追記する。"""
    with open(PROGRESS_FILE, "a", encoding="utf-8") as f:
        f.write(city_code + "\n")


def main():
    # --reset オプションが指定された場合は進捗・CSVをリセット
    reset_mode = "--reset" in sys.argv
    if reset_mode:
        print("[--reset] 進捗ファイルとCSVを削除して最初からやり直します")
        for f in [OUTPUT_CSV, PROGRESS_FILE]:
            if os.path.exists(f):
                try:
                    os.remove(f)
                    print(f"  削除: {f}")
                except PermissionError:
                    print(f"  [警告] 削除できません（別プロセスが使用中）: {f}")
                    sys.exit(1)

    print("=" * 60)
    print("経済センサス（令和3年）産業構造データ取得")
    print(f"統計表ID: {STATS_DATA_ID}")
    print(f"出力先: {OUTPUT_CSV}")
    print("=" * 60)
    print()

    # 出力ディレクトリ作成
    os.makedirs(os.path.dirname(OUTPUT_CSV), exist_ok=True)

    # メタ情報から全市区町村コードを取得
    municipalities = fetch_meta_info()

    # 途中再開: 進捗ファイルから取得済み市区町村コードを読み込む
    done_codes = load_progress()
    if done_codes:
        print(f"途中再開: 取得済み {len(done_codes)} 市区町村をスキップ")

    # 未取得の市区町村のみ処理対象
    targets = [m for m in municipalities if m["code"] not in done_codes]
    print(f"取得対象: {len(targets)} 市区町村")
    print()

    # CSVファイル書き込み（新規 or 追記モード）
    is_new_file = not os.path.exists(OUTPUT_CSV) or len(done_codes) == 0
    csv_mode = "w" if is_new_file else "a"
    csv_file = open(OUTPUT_CSV, csv_mode, encoding="utf-8-sig", newline="")
    writer = csv.DictWriter(csv_file, fieldnames=OUTPUT_COLUMNS)

    # 新規ファイルの場合はヘッダーを書き込む
    if is_new_file:
        writer.writeheader()

    processed = 0
    errors = 0

    for i, muni in enumerate(targets):
        city_code = muni["code"]
        city_name = muni["name"]
        pref_code = muni["pref_code"]
        total_targets = len(targets)

        # 進捗表示（50件ごと + 最初の1件）
        if (i + 1) % 50 == 0 or i == 0:
            print(f"進捗: {i + 1}/{total_targets} ({city_code} {city_name})")
            sys.stdout.flush()

        # APIからデータ取得
        result_map = fetch_stats_data_for_city(city_code)
        if not result_map:
            errors += 1
            # データなしの場合も産業ごとにNullレコードを書き込む（欠損追跡のため）
            for industry_code, industry_name in INDUSTRY_CODE_TO_NAME.items():
                writer.writerow({
                    "prefecture_code": pref_code,
                    "city_code": city_code,
                    "city_name": city_name,
                    "industry_code": industry_code,
                    "industry_name": industry_name,
                    "establishments": None,
                    "employees_total": None,
                    "employees_male": None,
                    "employees_female": None,
                })
        else:
            # 産業ごとに1行ずつ書き込む
            for industry_code, industry_name in INDUSTRY_CODE_TO_NAME.items():
                establishments = result_map.get((TAB_ESTABLISHMENTS, industry_code))
                employees_total = result_map.get((TAB_EMPLOYEES_TOTAL, industry_code))
                employees_male = result_map.get((TAB_EMPLOYEES_MALE, industry_code))
                employees_female = result_map.get((TAB_EMPLOYEES_FEMALE, industry_code))

                writer.writerow({
                    "prefecture_code": pref_code,
                    "city_code": city_code,
                    "city_name": city_name,
                    "industry_code": industry_code,
                    "industry_name": industry_name,
                    "establishments": establishments,
                    "employees_total": employees_total,
                    "employees_male": employees_male,
                    "employees_female": employees_female,
                })
            processed += 1

        # CSVへの書き込みを確定 + 進捗ファイルに記録
        csv_file.flush()
        save_progress(city_code)

        time.sleep(REQUEST_INTERVAL)

    csv_file.close()

    # ─── 結果サマリー ─────────────────────────────────────────────────────
    print()
    print("=" * 60)
    print("取得完了")
    print(f"  処理件数 : {processed} 市区町村（データあり）")
    print(f"  スキップ  : {errors} 市区町村（データなし）")

    # CSVの行数確認
    with open(OUTPUT_CSV, "r", encoding="utf-8-sig", newline="") as f:
        row_count = sum(1 for _ in f) - 1  # ヘッダー除く

    print(f"  CSV総行数: {row_count} 行")
    print(f"  出力先   : {OUTPUT_CSV}")

    # 先頭5行を表示
    print()
    print("=== 先頭5行 ===")
    with open(OUTPUT_CSV, "r", encoding="utf-8-sig", newline="") as f:
        reader = csv.reader(f)
        for j, row in enumerate(reader):
            print("  " + ", ".join(str(x) for x in row))
            if j >= 5:
                break


if __name__ == "__main__":
    main()
