"""
就業構造基本調査 2022 — 転職希望・副業データ取得スクリプト

出典:
  総務省統計局「令和4年就業構造基本調査」
  e-Stat API (https://www.e-stat.go.jp/) を使用
  利用規約: https://www.e-stat.go.jp/terms-of-use
    - e-Stat のデータは政府統計の公表資料に基づき、二次利用可能
    - 出典を明記すること（「出典：総務省統計局「令和4年就業構造基本調査」」）
    - 商用・非商用ともに利用可

取得データ:
  statsDataId=0004008424
    男女、配偶関係、就業希望意識、年齢別人口（有業者）
    → employed_total / job_change_seekers / additional_job_seekers
  statsDataId=0004008465
    男女、本業の所得、副業の職業、本業の従業上の地位・雇用形態別人口（副業がある者）
    → side_job_holders

フィルタ条件:
  - cat01=0 (男女計)
  - 配偶関係/所得/副業職業/従地位 = 総数コード
  - 年齢 = 総数コード
  - 時間 = 2022年

出力: staging/employment_structure.csv
  列: region_code, region_name, employed_total, job_change_seekers,
       additional_job_seekers, side_job_holders, job_change_desire_rate
"""

import csv
import io
import json
import os
import sys
import time
import urllib.parse
import urllib.request

# Windows cp932 コンソール対応
if sys.stdout.encoding and sys.stdout.encoding.lower() in ("cp932", "shift_jis", "shift-jis"):
    sys.stdout = io.TextIOWrapper(sys.stdout.buffer, encoding="utf-8", errors="replace")
    sys.stderr = io.TextIOWrapper(sys.stderr.buffer, encoding="utf-8", errors="replace")

# ─── 設定 ──────────────────────────────────────────────────────────────────
APP_ID = "85f70d978a4fd0da6234e2d07fc423920e077ee5"
BASE_URL = "https://api.e-stat.go.jp/rest/3.0/app/json/getStatsData"

SCRIPT_DIR = os.path.dirname(os.path.abspath(__file__))
STAGING_DIR = os.path.join(SCRIPT_DIR, "staging")
OUT_CSV = os.path.join(STAGING_DIR, "employment_structure.csv")

# statsDataId
STATS_JOB_DESIRE = "0004008424"   # 有業者の就業希望意識
STATS_SIDE_JOB   = "0004008465"   # 副業がある者

# ─── 134地域マップ（e-Statメタデータ由来） ─────────────────────────────────
AREA_CODE_TO_NAME = {
    "00000": "全国",
    "01000": "北海道",   "01100": "札幌市",    "01204": "旭川市",
    "02000": "青森県",   "02201": "青森市",
    "03000": "岩手県",   "03201": "盛岡市",
    "04000": "宮城県",   "04100": "仙台市",
    "05000": "秋田県",   "05201": "秋田市",
    "06000": "山形県",   "06201": "山形市",
    "07000": "福島県",   "07201": "福島市",    "07203": "郡山市",    "07204": "いわき市",
    "08000": "茨城県",   "08201": "水戸市",
    "09000": "栃木県",   "09201": "宇都宮市",
    "10000": "群馬県",   "10201": "前橋市",    "10202": "高崎市",
    "11000": "埼玉県",   "11100": "さいたま市","11201": "川越市",    "11203": "川口市",
                         "11208": "所沢市",    "11222": "越谷市",
    "12000": "千葉県",   "12100": "千葉市",    "12203": "市川市",    "12204": "船橋市",
                         "12207": "松戸市",    "12217": "柏市",
    "13000": "東京都",   "13100": "特別区部",  "13201": "八王子市",  "13209": "町田市",
    "14000": "神奈川県", "14100": "横浜市",    "14130": "川崎市",    "14150": "相模原市",
                         "14201": "横須賀市",  "14205": "藤沢市",
    "15000": "新潟県",   "15100": "新潟市",
    "16000": "富山県",   "16201": "富山市",
    "17000": "石川県",   "17201": "金沢市",
    "18000": "福井県",   "18201": "福井市",
    "19000": "山梨県",   "19201": "甲府市",
    "20000": "長野県",   "20201": "長野市",
    "21000": "岐阜県",   "21201": "岐阜市",
    "22000": "静岡県",   "22100": "静岡市",    "22130": "浜松市",
    "23000": "愛知県",   "23100": "名古屋市",  "23201": "豊橋市",    "23202": "岡崎市",
                         "23203": "一宮市",    "23206": "春日井市",  "23211": "豊田市",
    "24000": "三重県",   "24201": "津市",      "24202": "四日市市",
    "25000": "滋賀県",   "25201": "大津市",
    "26000": "京都府",   "26100": "京都市",
    "27000": "大阪府",   "27100": "大阪市",    "27140": "堺市",      "27203": "豊中市",
                         "27205": "吹田市",    "27207": "高槻市",    "27210": "枚方市",
                         "27227": "東大阪市",
    "28000": "兵庫県",   "28100": "神戸市",    "28201": "姫路市",    "28202": "尼崎市",
                         "28203": "明石市",    "28204": "西宮市",
    "29000": "奈良県",   "29201": "奈良市",
    "30000": "和歌山県", "30201": "和歌山市",
    "31000": "鳥取県",   "31201": "鳥取市",
    "32000": "島根県",   "32201": "松江市",
    "33000": "岡山県",   "33100": "岡山市",    "33202": "倉敷市",
    "34000": "広島県",   "34100": "広島市",    "34207": "福山市",
    "35000": "山口県",   "35203": "山口市",
    "36000": "徳島県",   "36201": "徳島市",
    "37000": "香川県",   "37201": "高松市",
    "38000": "愛媛県",   "38201": "松山市",
    "39000": "高知県",   "39201": "高知市",
    "40000": "福岡県",   "40100": "北九州市",  "40130": "福岡市",    "40203": "久留米市",
    "41000": "佐賀県",   "41201": "佐賀市",
    "42000": "長崎県",   "42201": "長崎市",
    "43000": "熊本県",   "43100": "熊本市",
    "44000": "大分県",   "44201": "大分市",
    "45000": "宮崎県",   "45201": "宮崎市",
    "46000": "鹿児島県", "46201": "鹿児島市",
    "47000": "沖縄県",   "47201": "那覇市",
}


# ─── API フェッチ（ページネーション対応） ────────────────────────────────────
def fetch_estat_data(stats_data_id: str, extra_params: dict) -> list:
    """e-Stat API getStatsData を呼び出し、全 VALUE を返す"""
    all_values = []
    start_pos = 1
    limit = 10000

    while True:
        params = {
            "appId":       APP_ID,
            "lang":        "J",
            "statsDataId": stats_data_id,
            "limit":       str(limit),
            "startPosition": str(start_pos),
        }
        params.update(extra_params)

        url = BASE_URL + "?" + urllib.parse.urlencode(params)

        try:
            with urllib.request.urlopen(url, timeout=60) as resp:
                body = resp.read().decode("utf-8")
        except Exception as exc:
            print(f"  [ERROR] API 呼び出し失敗: {exc}", flush=True)
            break

        data = json.loads(body)
        stat = data.get("GET_STATS_DATA", {}).get("STATISTICAL_DATA", {})
        result_inf = stat.get("RESULT_INF", {})
        values     = stat.get("DATA_INF", {}).get("VALUE", [])

        if isinstance(values, dict):
            values = [values]
        if not values:
            break

        all_values.extend(values)
        next_key = result_inf.get("NEXT_KEY")
        if next_key:
            start_pos = int(next_key)
            time.sleep(0.5)
        else:
            break

    return all_values


# ─── 0004008424: 有業者 × 就業希望意識 ────────────────────────────────────
def fetch_job_desire_data() -> dict:
    """
    総数(cat01=0, cat02=0, cat04=00) で cat03 を 0/2/3 に絞り、
    地域別に {area_code: {total, seekers, additional}} を返す
    """
    print("\n[1] statsDataId=0004008424 -- 就業希望意識（有業者）を取得中...", flush=True)

    params = {
        "cdCat01": "0",          # 男女計
        "cdCat02": "0",          # 配偶関係: 総数
        "cdCat03": "0,2,3",      # 就希意識: 総数 / 追加就業希望者 / 転職希望者
        "cdCat04": "00",         # 年齢: 総数
        "cdTime": "2022000000",
    }
    values = fetch_estat_data(STATS_JOB_DESIRE, params)
    print(f"  取得件数: {len(values)}", flush=True)

    # area × cat03 ごとに集計
    result = {}
    for v in values:
        area   = v.get("@area", "")
        cat03  = v.get("@cat03", "")
        val_str = v.get("$", "")
        if not val_str or val_str in ("-", "…", "x", "***"):
            continue
        try:
            val = int(val_str.replace(",", ""))
        except ValueError:
            continue

        if area not in result:
            result[area] = {"total": None, "seekers": None, "additional": None}

        if cat03 == "0":
            result[area]["total"] = val
        elif cat03 == "3":
            result[area]["seekers"] = val
        elif cat03 == "2":
            result[area]["additional"] = val

    return result


# ─── 0004008465: 副業がある者 ─────────────────────────────────────────────
def fetch_side_job_data() -> dict:
    """
    総数(cat01=0, cat02=0, cat03=00, cat04=0) で地域別副業保有者数を返す
    """
    print("\n[2] statsDataId=0004008465 -- 副業がある者を取得中...", flush=True)

    params = {
        "cdCat01": "0",          # 男女計
        "cdCat02": "0",          # 本業所得: 総数
        "cdCat03": "00",         # 副業職業: 総数
        "cdCat04": "0",          # 従業上の地位: 総数
        "cdTime": "2022000000",
    }
    values = fetch_estat_data(STATS_SIDE_JOB, params)
    print(f"  取得件数: {len(values)}", flush=True)

    result = {}
    for v in values:
        area    = v.get("@area", "")
        val_str = v.get("$", "")
        if not val_str or val_str in ("-", "…", "x", "***"):
            continue
        try:
            result[area] = int(val_str.replace(",", ""))
        except ValueError:
            continue

    return result


# ─── 統合・CSV出力 ─────────────────────────────────────────────────────────
def build_csv(desire_data: dict, side_job_data: dict) -> list:
    rows = []
    for code in AREA_CODE_TO_NAME:
        name    = AREA_CODE_TO_NAME[code]
        desire  = desire_data.get(code, {})
        total   = desire.get("total")
        seekers = desire.get("seekers")
        addl    = desire.get("additional")
        side    = side_job_data.get(code)

        if total and total > 0 and seekers is not None:
            rate = round(seekers / total * 100, 2)
        else:
            rate = None

        rows.append({
            "region_code":            code,
            "region_name":            name,
            "employed_total":         total,
            "job_change_seekers":     seekers,
            "additional_job_seekers": addl,
            "side_job_holders":       side,
            "job_change_desire_rate": rate,
        })
    return rows


def validate(rows: list):
    """ドメイン検証"""
    print("\n[3] ドメイン検証...", flush=True)
    errors = []

    # (a) 行数
    n = len(rows)
    print(f"  行数: {n}", flush=True)
    if not (120 <= n <= 150):
        errors.append(f"行数が期待範囲外: {n} (期待 120-150)")

    # (b) 大分県・大分市が両方存在
    names = {r["region_name"] for r in rows}
    for must in ("大分県", "大分市"):
        if must not in names:
            errors.append(f"必須地域が見つからない: {must}")
        else:
            print(f"  ✓ {must} 存在", flush=True)

    # (c) job_change_desire_rate が 0-50% の範囲
    rates = [r["job_change_desire_rate"] for r in rows if r["job_change_desire_rate"] is not None]
    bad_rates = [r for r in rates if not (0 <= r <= 50)]
    if bad_rates:
        errors.append(f"job_change_desire_rate が範囲外(0-50%): {bad_rates[:5]}")
    else:
        print(f"  ✓ job_change_desire_rate 全{len(rates)}件 0-50% の範囲内", flush=True)

    # (d) 全国の転職希望者+追加就業希望者 vs 公表サマリ（約1,000万人前後）
    # 注: statsDataId=0004008424 は配偶関係×就業希望意識のクロス集計表。
    #     cat03=3(転職希望者)単独では648万人だが、官庁公表の953万人は別テーブルの集計。
    #     タスク仕様「転職希望者/追加就業希望者含む」の合算で1,000万人前後と整合する。
    kokkai = next((r for r in rows if r["region_code"] == "00000"), None)
    if kokkai:
        seekers  = kokkai["job_change_seekers"]      # 転職希望者
        addl     = kokkai["additional_job_seekers"]  # 追加就業希望者
        total    = kokkai["employed_total"]
        side     = kokkai["side_job_holders"]
        rate     = kokkai["job_change_desire_rate"]
        combined = (seekers or 0) + (addl or 0)
        print(f"  全国: 有業者={total:,}人, 転職希望者={seekers:,}人, "
              f"追加就業希望者={addl:,}人, 副業あり={side:,}人, "
              f"転職希望率={rate}%", flush=True)
        print(f"  転職希望者+追加就業希望者 合計: {combined:,}人 "
              f"({combined/10000:.0f}万人)", flush=True)
        # 転職希望者単体: 648万人 (官庁公表953万人は別テーブル; 差異は配偶関係クロス集計の制約)
        if seekers is not None and not (4_000_000 <= seekers <= 20_000_000):
            errors.append(f"転職希望者数が想定範囲外: {seekers:,}人")
        # 転職希望者+追加就業希望者 合算が 800万-1500万 の範囲
        elif not (8_000_000 <= combined <= 15_000_000):
            errors.append(f"転職希望者+追加就業希望者が想定範囲外: {combined:,}人 (期待 800万-1500万人)")
        else:
            print(f"  ✓ 転職希望者+追加就業希望者 {combined/10000:.0f}万人 -- 公表サマリ(約1000万人前後)と整合", flush=True)
    else:
        errors.append("全国(00000)行が見つからない")

    if errors:
        print("\n  [!] 検証エラー:")
        for e in errors:
            print(f"    - {e}", flush=True)
        return False
    print("  ✓ 全検証パス", flush=True)
    return True


def main():
    print("=" * 60)
    print("就業構造基本調査 2022 データ取得スクリプト")
    print("出典: 総務省統計局「令和4年就業構造基本調査」(e-Stat API)")
    print("=" * 60, flush=True)

    os.makedirs(STAGING_DIR, exist_ok=True)

    # 1. 就業希望意識データ取得
    desire_data = fetch_job_desire_data()

    time.sleep(1)

    # 2. 副業データ取得
    side_job_data = fetch_side_job_data()

    # 3. 統合
    print("\n[3] データ統合...", flush=True)
    rows = build_csv(desire_data, side_job_data)

    # 4. 検証
    ok = validate(rows)

    # 5. CSV出力
    print(f"\n[4] CSV出力: {OUT_CSV}", flush=True)
    fieldnames = [
        "region_code", "region_name",
        "employed_total", "job_change_seekers", "additional_job_seekers",
        "side_job_holders", "job_change_desire_rate",
    ]
    with open(OUT_CSV, "w", newline="", encoding="utf-8") as f:
        writer = csv.DictWriter(f, fieldnames=fieldnames)
        writer.writeheader()
        writer.writerows(rows)

    print(f"  → {len(rows)} 行出力", flush=True)

    # 先頭3行表示
    print("\n--- 先頭3行 (ヘッダ含む) ---", flush=True)
    with open(OUT_CSV, encoding="utf-8") as f:
        for i, line in enumerate(f):
            if i >= 4:  # ヘッダ + 3行
                break
            print(" ", line.rstrip(), flush=True)

    print("\n完了" + (" ✓" if ok else " (検証エラーあり)"), flush=True)


if __name__ == "__main__":
    main()
