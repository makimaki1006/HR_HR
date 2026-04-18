"""
日銀短観（全国企業短期経済観測調査）から業種別・規模別の
業況判断DI および 雇用人員判断DI を取得し、CSVに出力する。

データソース: 日銀 時系列統計データ検索サイト API
  https://www.stat-search.boj.or.jp/api/v1/

系列コード体系（判明済み）:
  TK99 + F{業種コード} + {判断項目コード} + GCQ + {規模・実績/予測コード} + 000
  例: TK99F1000601GCQ01000 → D.I./業況/大企業/製造業/実績

判断項目コード:
  601 = 業況
  608 = 雇用人員

規模・実績/予測コード（GCQの後）:
  01 = 大企業 実績
  02 = 中堅企業 実績
  03 = 中小企業 実績
  10 = 全規模 予測
  11 = 大企業 予測
  12 = 中堅企業 予測
  13 = 中小企業 予測

業種コード（F????）:
  F0000 = 全産業
  F1000 = 製造業
  F2000 = 非製造業
  F2011 = 建設（F2010は2009年12月で更新停止）
  F2020 = 卸・小売
  F2040 = 運輸・郵便
  F2059 = 情報通信
  F2081 = 対事業所サービス（F2080は2009年12月で更新停止）
  F2082 = 対個人サービス（医療福祉の近似）
  F2100 = 宿泊・飲食サービス

出力先: scripts/data/boj_tankan_di.csv
カラム: survey_date, industry_code, industry_j, enterprise_size,
        di_type, result_type, di_value, series_code
"""

import time
import sys
from pathlib import Path

import requests
import pandas as pd

# 標準出力をUTF-8に設定（Windows環境対応）
sys.stdout.reconfigure(encoding="utf-8")

# ──────────────────────────────────────────────
# 設定
# ──────────────────────────────────────────────
API_BASE = "https://www.stat-search.boj.or.jp/api/v1/getDataCode"
OUTPUT_PATH = Path(__file__).parent / "data" / "boj_tankan_di.csv"

# 取得開始年度（YYYYQn形式 → yyyyqq: 202001 = 2020Q1）
START_DATE = 202001

# 業種コードと日本語名
# 注意: F2010（建設・不動産）とF2080（サービス）は2009年12月で更新停止のため
#       後継コードのF2011（建設）とF2081（対事業所サービス）を使用する
INDUSTRIES = {
    "F0000": "全産業",
    "F1000": "製造業",
    "F2000": "非製造業",
    "F2011": "建設",
    "F2020": "卸・小売",
    "F2040": "運輸・郵便",
    "F2059": "情報通信",
    "F2081": "対事業所サービス",
    "F2082": "対個人サービス（医療福祉近似）",
    "F2100": "宿泊・飲食サービス",
}

# 判断項目コード: {item_code_part: (di_type, 日本語説明)}
DI_ITEMS = {
    "601": ("business", "業況判断DI"),
    "608": ("employment", "雇用人員判断DI"),
}

# 規模・実績/予測コード: {gcq_code: (enterprise_size, result_type)}
SIZE_RESULT_CODES = {
    "01": ("大企業",   "actual"),
    "02": ("中堅企業", "actual"),
    "03": ("中小企業", "actual"),
    "11": ("大企業",   "forecast"),
    "12": ("中堅企業", "forecast"),
    "13": ("中小企業", "forecast"),
}

# リクエスト間隔（秒）— 日銀APIへの負荷軽減
REQUEST_INTERVAL = 0.2


def build_series_code(industry_code: str, item_code: str, gcq_code: str) -> str:
    """系列コードを組み立てる。

    Args:
        industry_code: 業種コード（例: "F1000"）
        item_code:     判断項目コード（例: "601"）
        gcq_code:      規模・実績/予測コード（例: "01"）

    Returns:
        系列コード文字列（例: "TK99F1000601GCQ01000"）
    """
    return f"TK99{industry_code}{item_code}GCQ{gcq_code}000"


def fetch_series(series_code: str) -> dict | None:
    """日銀APIから1系列のデータを取得する。

    Args:
        series_code: 取得する系列コード

    Returns:
        APIのRESULTSETの最初の要素、またはNone（取得失敗時）
    """
    params = {
        "db":     "CO",
        "code":   series_code,
        "lang":   "JP",
        "format": "json",
    }
    try:
        resp = requests.get(API_BASE, params=params, timeout=15)
        data = resp.json()
        rs = data.get("RESULTSET", [])
        if rs:
            return rs[0]
        # APIエラーの場合
        print(f"  [SKIP] {series_code}: {data.get('MESSAGE', 'データなし')}")
        return None
    except Exception as e:
        print(f"  [ERROR] {series_code}: {e}")
        return None


def parse_survey_date(yyyyqq: int) -> str:
    """日銀の日付形式（yyyyqq）をISO形式の四半期文字列に変換する。

    Args:
        yyyyqq: 例 202001 → 2020年第1四半期

    Returns:
        例 "2020Q1"
    """
    yyyy = yyyyqq // 100
    qq = yyyyqq % 100
    return f"{yyyy}Q{qq}"


def main() -> None:
    """全系列を取得してCSVに出力するメイン処理。"""
    records = []
    total = len(INDUSTRIES) * len(DI_ITEMS) * len(SIZE_RESULT_CODES)
    count = 0

    print(f"日銀短観データ取得開始 (全{total}系列)")
    print(f"取得期間: {START_DATE}以降 ({parse_survey_date(START_DATE)}以降)")
    print("-" * 60)

    for industry_code, industry_j in INDUSTRIES.items():
        for item_code, (di_type, di_j) in DI_ITEMS.items():
            for gcq_code, (size, result_type) in SIZE_RESULT_CODES.items():
                count += 1
                series_code = build_series_code(industry_code, item_code, gcq_code)
                print(
                    f"[{count:3d}/{total}] {series_code} "
                    f"({industry_j} / {di_j} / {size} / "
                    f"{'実績' if result_type == 'actual' else '先行き'})"
                )

                item = fetch_series(series_code)
                if item is None:
                    time.sleep(REQUEST_INTERVAL)
                    continue

                # 日付と値を取得
                dates = item["VALUES"]["SURVEY_DATES"]
                values = item["VALUES"]["VALUES"]

                # START_DATE以降のデータのみ抽出
                for date_int, val in zip(dates, values):
                    if date_int < START_DATE:
                        continue
                    if val is None:
                        continue
                    records.append({
                        "survey_date":     parse_survey_date(date_int),
                        "industry_code":   industry_code,
                        "industry_j":      industry_j,
                        "enterprise_size": size,
                        "di_type":         di_type,
                        "result_type":     result_type,
                        "di_value":        val,
                        "series_code":     series_code,
                    })

                time.sleep(REQUEST_INTERVAL)

    # ──────────────────────────────────────────────
    # CSV出力
    # ──────────────────────────────────────────────
    if not records:
        print("\n[ERROR] 取得できたデータが0件。出力をスキップします。")
        return

    df = pd.DataFrame(records)

    # 列順を整理
    df = df[[
        "survey_date", "industry_code", "industry_j",
        "enterprise_size", "di_type", "result_type",
        "di_value", "series_code",
    ]]

    # ソート（日付 → 業種 → DI種別 → 規模 → 実績/先行き）
    df = df.sort_values(
        ["survey_date", "industry_code", "di_type", "enterprise_size", "result_type"]
    ).reset_index(drop=True)

    OUTPUT_PATH.parent.mkdir(parents=True, exist_ok=True)
    df.to_csv(OUTPUT_PATH, index=False, encoding="utf-8-sig")

    # ──────────────────────────────────────────────
    # 結果レポート
    # ──────────────────────────────────────────────
    print("\n" + "=" * 60)
    print(f"出力完了: {OUTPUT_PATH}")
    print(f"総行数:   {len(df):,} 行")
    print(f"期間:     {df['survey_date'].min()} ～ {df['survey_date'].max()}")
    print(f"業種数:   {df['industry_j'].nunique()}")
    print("\n--- 先頭5行 ---")
    print(df.head(5).to_string(index=False))
    print("\n--- 業種×DI種別×規模のデータ件数 ---")
    summary = (
        df.groupby(["industry_j", "di_type", "enterprise_size", "result_type"])
        .size()
        .reset_index(name="件数")
    )
    print(summary.to_string(index=False))


if __name__ == "__main__":
    main()
