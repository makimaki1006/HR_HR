# -*- coding: utf-8 -*-
"""
build_cross_tables.py — Turso 投入用クロス集計 CSV を生成する (投入はしない)。

出力 (scripts/staging/):
  cross_future_workforce.csv  : 全国市区町村 × 将来人口指標
  cross_wage_public.csv       : 都道府県×月 × 給与・最低賃金
  cross_switcher_supply.csv   : 134地域 × 転職・副業・求人倍率

絶対ルール:
  - 介護データ (v2_external_care_demand 等) を一切使わない
  - ハローワークデータ (ts_turso_* / postings) を一切使わない
  - Turso への書き込み禁止 (SELECT のみ)
  - compute_v3.py の検証済みロジックを忠実に移植する

列名は db_columns.rs SSoT パターンに準拠:
  fiscal_year, hourly_min_wage, ratio_total (Turso 既存列)
  新テーブル列名は snake_case + 意味単語 で英語命名

検証ターゲット (モック v3/v4 の実測値):
  - 大分市 wa_decline_rate ≈ -14.1%
  - 大分県 job_change_desire_rate = 8.84%
  - 大分 2025-12 scheduled_earnings = 239,448 円
"""
import asyncio
import os
import statistics
import sys

import libsql_client
import pandas as pd

# ============================================================
# パス定義
# ============================================================
STG = "C:/Users/fuji1/OneDrive/デスクトップ/HR_HR/scripts/staging"
TOKEN_PATH = (
    "C:/Users/fuji1/AppData/Local/Temp/claude/"
    "C--Users-fuji1-OneDrive-Python--------job-medley-project/"
    "4dd6b933-db83-4383-a9ce-6a4675e01b59/scratchpad/.turso_token"
)
TURSO_URL = "https://country-statistics-makimaki1006.aws-ap-northeast-1.turso.io"

OUT_WORKFORCE  = f"{STG}/cross_future_workforce.csv"
OUT_WAGE       = f"{STG}/cross_wage_public.csv"
OUT_SWITCHER   = f"{STG}/cross_switcher_supply.csv"

# ============================================================
# 都道府県名 正規化 (monthly_labor の短縮形 → フルネーム)
# 「大分」→「大分県」, 「東京」→「東京都」等
# ============================================================
_PREF_EXCEPTIONS = {
    "全国":  "全国",
    "北海道": "北海道",
    "東京":  "東京都",
    "大阪":  "大阪府",
    "京都":  "京都府",
}

def normalize_pref(name: str) -> str:
    """monthly_labor の短縮都道府県名 → フルネーム。"""
    if name in _PREF_EXCEPTIONS:
        return _PREF_EXCEPTIONS[name]
    if name.endswith(("都", "道", "府", "県")):
        return name          # 既にフルネーム
    return name + "県"

# ============================================================
# Turso から最低賃金と有効求人倍率を取得 (SELECT のみ)
# ============================================================
async def _fetch_turso():
    """
    Returns:
        min_wage_map  : {pref_fullname -> {fiscal_year(int) -> hourly_min_wage(int)}}
        job_ratio_map : {pref_fullname -> {fiscal_year(str) -> ratio_total(float)}}
    """
    async with libsql_client.create_client(
        url=TURSO_URL, auth_token=open(TOKEN_PATH).read().strip()
    ) as c:
        # 最低賃金 (全都道府県 + 全国)
        rs_mw = await c.execute(
            "SELECT prefecture, fiscal_year, hourly_min_wage "
            "FROM v2_external_minimum_wage_history ORDER BY prefecture, fiscal_year"
        )
        min_wage_map: dict[str, dict[int, int]] = {}
        for row in rs_mw.rows:
            pref, fy, mw = str(row[0]), int(row[1]), int(row[2])
            min_wage_map.setdefault(pref, {})[fy] = mw

        # 有効求人倍率 (全都道府県 + 全国)
        rs_jo = await c.execute(
            "SELECT prefecture, fiscal_year, ratio_total "
            "FROM v2_external_job_openings_ratio ORDER BY prefecture, fiscal_year"
        )
        job_ratio_map: dict[str, dict[str, float]] = {}
        for row in rs_jo.rows:
            pref, fy, rt = str(row[0]), str(row[1]), float(row[2])
            job_ratio_map.setdefault(pref, {})[fy] = rt

    return min_wage_map, job_ratio_map


def fetch_turso():
    return asyncio.run(_fetch_turso())


# ============================================================
# テーブル 1: cross_future_workforce.csv
# ============================================================
def build_future_workforce(pp: pd.DataFrame) -> pd.DataFrame:
    """
    社人研推計から 2020年・2040年の働き手指標を抽出し、
    県内中央値基準の4象限 (quadrant) を付与する。

    quadrant ラベル (モック gen_html_v4.py の色区分に対応):
      特に厳しい         : 減少速い AND 現在の割合低い (最も優先的に対応)
      減りは速いが今は多い: 減少速い AND 現在の割合高い
      減りは緩やかだが少い: 減少遅い AND 現在の割合低い
      比較的ゆとりがある  : 減少遅い AND 現在の割合高い
    """
    r20 = pp[pp["projection_year"] == 2020].set_index("muni_code")
    r40 = pp[pp["projection_year"] == 2040].set_index("muni_code")

    common = r20.index.intersection(r40.index)
    r20 = r20.loc[common]
    r40 = r40.loc[common]

    df = pd.DataFrame({
        "muni_code":           common,
        "prefecture":          r20["prefecture"].values,
        "municipality":        r20["municipality"].values,
        # 働き手 (15〜64歳) 人数
        "wa_2020":             r20["working_age_15_64"].astype(int).values,
        "wa_2040":             r40["working_age_15_64"].astype(int).values,
        # 人口に占める働き手の割合 (2020年, %)
        "working_age_ratio_2020": r20["working_age_ratio"].round(2).values,
        # 後期高齢者 (75歳以上)
        "aged75_2020":         r20["aged_75plus"].astype(int).values,
        "aged75_2040":         r40["aged_75plus"].astype(int).values,
        # 2020年基準の人口指数 (2040年値、100=2020年水準)
        "pop_index_2040":      r40["pop_index_2020base"].round(2).values,
    })
    df["muni_code"] = df["muni_code"].astype(str)

    # 増減率 (%)
    df["wa_decline_rate"] = (
        (df["wa_2040"] / df["wa_2020"] - 1) * 100
    ).round(2)

    # 後期高齢者増加率 (%)
    df["aged75_growth"] = (
        (df["aged75_2040"] / df["aged75_2020"] - 1) * 100
    ).round(2)

    # 県内中央値基準の4象限 (compute_v3.py ロジック: 大分県のみ → 全都道府県に拡張)
    # groupby.apply を使わず、都道府県ごとに中央値を計算してベクトル演算で付与する
    pref_medians = (
        df.groupby("prefecture")[["wa_decline_rate", "working_age_ratio_2020"]]
        .median()
        .rename(columns={
            "wa_decline_rate":        "med_decline",
            "working_age_ratio_2020": "med_ratio",
        })
    )
    df = df.merge(pref_medians, on="prefecture", how="left")

    left  = df["wa_decline_rate"]        < df["med_decline"]   # 減少速い
    below = df["working_age_ratio_2020"] < df["med_ratio"]     # 割合低い

    df["quadrant"] = "比較的ゆとりがある"                       # デフォルト
    df.loc[left  &  below, "quadrant"] = "特に厳しい"
    df.loc[left  & ~below, "quadrant"] = "減りは速いが今は多い"
    df.loc[~left &  below, "quadrant"] = "減りは緩やかだが少ない"

    df = df.drop(columns=["med_decline", "med_ratio"])

    # 列順を指定
    cols = [
        "muni_code", "prefecture", "municipality",
        "wa_2020", "wa_2040", "wa_decline_rate",
        "working_age_ratio_2020",
        "aged75_2020", "aged75_2040", "aged75_growth",
        "pop_index_2040", "quadrant",
    ]
    return df[cols].reset_index(drop=True)


# ============================================================
# テーブル 2: cross_wage_public.csv
# ============================================================
def build_wage_public(
    ml: pd.DataFrame,
    min_wage_map: dict,
) -> pd.DataFrame:
    """
    毎月勤労統計 (月次) × 最低賃金 (年次) を結合する。

    最低賃金の改定月: 毎年10月発効。
      1〜9月  → fiscal_year = 暦年 - 1
      10〜12月 → fiscal_year = 暦年

    min_wage_monthly_160h = 時給 × 160時間 (固定)
    理由: 実労働時間は月毎にばらつくため、説明用として月160時間固定換算が
    読み手に伝わりやすい (compute_v3.py v3 変更点と同じ方針)。
    """
    FIXED_HOURS = 160

    sub = ml[
        (ml["size_class"] == "5人以上") &
        (ml["industry"] == "調査産業計")
    ].copy()

    # 都道府県名をフルネームに正規化
    sub["prefecture"] = sub["prefecture"].map(normalize_pref)

    rows = []
    for _, r in sub.iterrows():
        pref     = r["prefecture"]
        ym       = r["year_month"]
        year     = int(ym[:4])
        month    = int(ym[5:7])
        # 最低賃金適用年度: 10月改定なので 1-9月は前年度
        fy       = year if month >= 10 else year - 1
        mw_by_fy = min_wage_map.get(pref, {})
        hourly_mw = mw_by_fy.get(fy, None)
        monthly_160h = int(hourly_mw * FIXED_HOURS) if hourly_mw is not None else None

        rows.append({
            "prefecture":            pref,
            "year_month":            ym,
            "scheduled_earnings":    int(r["scheduled_earnings"]),
            "min_wage_hourly":       hourly_mw,
            "min_wage_monthly_160h": monthly_160h,
        })

    df = pd.DataFrame(rows, columns=[
        "prefecture", "year_month", "scheduled_earnings",
        "min_wage_hourly", "min_wage_monthly_160h",
    ])
    return df.sort_values(["prefecture", "year_month"]).reset_index(drop=True)


# ============================================================
# テーブル 3: cross_switcher_supply.csv
# ============================================================
def build_switcher_supply(
    es: pd.DataFrame,
    job_ratio_map: dict,
) -> pd.DataFrame:
    """
    就業構造基本調査 (134地域) に有効求人倍率 (2024年度、都道府県値) を結合する。

    有効求人倍率は都道府県単位のため、市区町村行には同一都道府県の値を付与する。
    region_code 先頭2桁が都道府県コード。"00" = 全国。
    """
    # 都道府県コード → 都道府県名 (employment_structure の XXX000 行から構築)
    pref_rows = es[es["region_code"].str.endswith("000")]
    pref_code_to_name: dict[str, str] = {
        r["region_code"][:2]: r["region_name"]
        for _, r in pref_rows.iterrows()
    }
    # {"00": "全国", "01": "北海道", ..., "44": "大分県", ...}

    # 有効求人倍率 (2024年度) を都道府県名 → ratio 辞書に整理
    ratio_2024: dict[str, float] = {
        pref: fy_map.get("2024", None)
        for pref, fy_map in job_ratio_map.items()
    }

    rows = []
    for _, r in es.iterrows():
        pref_code = r["region_code"][:2]
        pref_name = pref_code_to_name.get(pref_code, None)
        pref_ratio = ratio_2024.get(pref_name, None) if pref_name else None
        rows.append({
            "region_code":            r["region_code"],
            "region_name":            r["region_name"],
            "employed_total":         int(r["employed_total"]),
            "job_change_seekers":     int(r["job_change_seekers"]),
            "job_change_desire_rate": float(r["job_change_desire_rate"]),
            "additional_job_seekers": int(r["additional_job_seekers"]),
            "side_job_holders":       int(r["side_job_holders"]),
            "pref_job_openings_ratio": pref_ratio,
        })

    df = pd.DataFrame(rows, columns=[
        "region_code", "region_name",
        "employed_total", "job_change_seekers", "job_change_desire_rate",
        "additional_job_seekers", "side_job_holders",
        "pref_job_openings_ratio",
    ])
    return df.reset_index(drop=True)


# ============================================================
# 検証ロジック
# ============================================================
ERRORS: list[str] = []
WARNINGS: list[str] = []


def check(cond: bool, msg: str, is_warning: bool = False) -> None:
    if cond:
        print(f"  OK  {msg}")
    else:
        label = "WARN" if is_warning else "FAIL"
        print(f"  {label} {msg}")
        (WARNINGS if is_warning else ERRORS).append(msg)


def validate_workforce(df: pd.DataFrame) -> None:
    print("\n[cross_future_workforce.csv 検証]")

    # 行数
    n = len(df)
    check(1700 <= n <= 2000, f"行数 {n} (期待: 1,700〜2,000)")

    # 必須列
    required = [
        "muni_code", "prefecture", "municipality",
        "wa_2020", "wa_2040", "wa_decline_rate",
        "working_age_ratio_2020",
        "aged75_2020", "aged75_2040", "aged75_growth",
        "pop_index_2040", "quadrant",
    ]
    for col in required:
        check(col in df.columns, f"列 '{col}' が存在する")

    # NULL なし
    nulls = df.isnull().sum()
    for col in required:
        check(nulls.get(col, 0) == 0, f"列 '{col}' に NULL なし")

    # wa_decline_rate の値域 (人口は急増はしない、-99〜+20 程度)
    vmin, vmax = df["wa_decline_rate"].min(), df["wa_decline_rate"].max()
    check(-99 <= vmin and vmax <= 30,
          f"wa_decline_rate 値域 [{vmin:.1f}%, {vmax:.1f}%]")

    # pop_index_2040 > 0
    check((df["pop_index_2040"] > 0).all(),
          f"pop_index_2040 > 0 (min={df['pop_index_2040'].min():.1f})")

    # quadrant の4種類
    q_vals = set(df["quadrant"].unique())
    expected_q = {"特に厳しい", "減りは速いが今は多い", "減りは緩やかだが少ない", "比較的ゆとりがある"}
    check(q_vals == expected_q, f"quadrant 4種類が全て存在: {q_vals}")

    # === スポット検証 (大分市、モック v3 との照合) ===
    oita = df[(df["prefecture"] == "大分県") & (df["municipality"] == "大分市")]
    if len(oita) == 1:
        row = oita.iloc[0]
        actual_decline = row["wa_decline_rate"]
        check(
            abs(actual_decline - (-14.1)) < 0.2,
            f"大分市 wa_decline_rate = {actual_decline:.2f}% (期待 ≈ -14.1%)",
        )
        check(
            row["wa_2020"] == 280585,
            f"大分市 wa_2020 = {row['wa_2020']:,} (期待 280,585)",
        )
        check(
            row["wa_2040"] == 241092,
            f"大分市 wa_2040 = {row['wa_2040']:,} (期待 241,092)",
        )
        check(
            abs(row["pop_index_2040"] - 93.12) < 0.5,
            f"大分市 pop_index_2040 = {row['pop_index_2040']:.2f} (期待 ≈ 93.1)",
        )
        check(
            row["quadrant"] == "比較的ゆとりがある",
            f"大分市 quadrant = '{row['quadrant']}' (期待: 比較的ゆとりがある)",
        )
    else:
        ERRORS.append(f"大分市 のレコードが見つからない (found {len(oita)}件)")
        print(f"  FAIL 大分市 のレコードが見つからない")

    # 都道府県数
    pref_count = df["prefecture"].nunique()
    check(45 <= pref_count <= 47, f"都道府県数 {pref_count} (期待: 45〜47)")


def validate_wage(df: pd.DataFrame) -> None:
    print("\n[cross_wage_public.csv 検証]")

    n = len(df)
    check(500 <= n <= 650, f"行数 {n} (期待: 500〜650)")

    required = [
        "prefecture", "year_month", "scheduled_earnings",
        "min_wage_hourly", "min_wage_monthly_160h",
    ]
    for col in required:
        check(col in df.columns, f"列 '{col}' が存在する")

    # prefecture はフルネーム (「都道府県」終わり または 全国)
    prefs = df["prefecture"].unique()
    bad = [p for p in prefs
           if not (p == "全国" or p.endswith(("都", "道", "府", "県")))]
    check(len(bad) == 0, f"都道府県名フルネーム (NG例: {bad[:3]})")

    # 数値の値域
    check(
        (df["scheduled_earnings"] > 100_000).all() and
        (df["scheduled_earnings"] < 600_000).all(),
        f"scheduled_earnings 値域 [{df['scheduled_earnings'].min():,}〜{df['scheduled_earnings'].max():,}]",
    )
    check(
        (df["min_wage_hourly"] > 700).all() and
        (df["min_wage_hourly"] < 2000).all(),
        f"min_wage_hourly 値域 [{df['min_wage_hourly'].min()}〜{df['min_wage_hourly'].max()}]",
    )

    # === スポット検証 (大分 2025-12、モック v3 の 239,448 と照合) ===
    oita_dec = df[(df["prefecture"] == "大分県") & (df["year_month"] == "2025-12")]
    if len(oita_dec) == 1:
        row = oita_dec.iloc[0]
        check(
            row["scheduled_earnings"] == 239_448,
            f"大分県 2025-12 scheduled_earnings = {row['scheduled_earnings']:,} (期待 239,448)",
        )
        check(
            row["min_wage_hourly"] == 1035,
            f"大分県 2025-12 min_wage_hourly = {row['min_wage_hourly']} (期待 1,035)",
        )
        check(
            row["min_wage_monthly_160h"] == 1035 * 160,
            f"大分県 2025-12 min_wage_monthly_160h = {row['min_wage_monthly_160h']:,} "
            f"(期待 {1035*160:,})",
        )
    else:
        ERRORS.append("大分県 2025-12 のレコードが見つからない")
        print("  FAIL 大分県 2025-12 のレコードが見つからない")

    # 大分 2025-01 → FY2024 → min_wage = 954
    oita_jan = df[(df["prefecture"] == "大分県") & (df["year_month"] == "2025-01")]
    if len(oita_jan) == 1:
        row = oita_jan.iloc[0]
        check(
            row["min_wage_hourly"] == 954,
            f"大分県 2025-01 min_wage_hourly = {row['min_wage_hourly']} (期待 954, FY2024)",
        )

    # NULL チェック (min_wage は全行埋まるはず)
    null_mw = df["min_wage_hourly"].isnull().sum()
    check(null_mw == 0, f"min_wage_hourly の NULL = {null_mw}件", is_warning=(null_mw > 0))


def validate_switcher(df: pd.DataFrame) -> None:
    print("\n[cross_switcher_supply.csv 検証]")

    n = len(df)
    check(n == 134, f"行数 {n} (期待: 134)")

    required = [
        "region_code", "region_name",
        "employed_total", "job_change_seekers", "job_change_desire_rate",
        "additional_job_seekers", "side_job_holders", "pref_job_openings_ratio",
    ]
    for col in required:
        check(col in df.columns, f"列 '{col}' が存在する")

    # desire_rate は 0〜100 の範囲
    vmin, vmax = df["job_change_desire_rate"].min(), df["job_change_desire_rate"].max()
    check(0 < vmin and vmax < 100,
          f"job_change_desire_rate 値域 [{vmin:.2f}%〜{vmax:.2f}%]")

    # pref_job_openings_ratio: 都道府県レベルは全て埋まるはず
    pref_only = df[df["region_code"].str.endswith("000")]
    null_ratio = pref_only["pref_job_openings_ratio"].isnull().sum()
    check(null_ratio == 0,
          f"都道府県行の pref_job_openings_ratio NULL = {null_ratio}件",
          is_warning=(null_ratio > 0))

    # === スポット検証 (大分県、モック v3 との照合) ===
    oita = df[df["region_code"] == "44000"]
    if len(oita) == 1:
        row = oita.iloc[0]
        check(
            row["job_change_desire_rate"] == 8.84,
            f"大分県 job_change_desire_rate = {row['job_change_desire_rate']} (期待 8.84%)",
        )
        check(
            row["side_job_holders"] == 19500,
            f"大分県 side_job_holders = {row['side_job_holders']:,} (期待 19,500)",
        )
        check(
            abs(row["pref_job_openings_ratio"] - 1.35) < 0.01,
            f"大分県 pref_job_openings_ratio = {row['pref_job_openings_ratio']} (期待 1.35)",
        )
    else:
        ERRORS.append("大分県 (44000) のレコードが見つからない")
        print("  FAIL 大分県 (44000) のレコードが見つからない")

    # 大分市のスポット
    oitashi = df[df["region_code"] == "44201"]
    if len(oitashi) == 1:
        row = oitashi.iloc[0]
        check(
            row["job_change_desire_rate"] == 9.3,
            f"大分市 job_change_desire_rate = {row['job_change_desire_rate']} (期待 9.3%)",
            is_warning=True,
        )
        # 大分市も pref_job_openings_ratio は大分県値のはず
        check(
            abs(row["pref_job_openings_ratio"] - 1.35) < 0.01,
            f"大分市 pref_job_openings_ratio = {row['pref_job_openings_ratio']} (期待 1.35 = 大分県値)",
        )


# ============================================================
# メイン
# ============================================================
def main() -> None:
    sys.stdout.reconfigure(encoding="utf-8")

    print("=== Turso からデータを取得 (SELECT のみ) ===")
    min_wage_map, job_ratio_map = fetch_turso()
    print(f"  最低賃金: {len(min_wage_map)} 都道府県/地域")
    print(f"  有効求人倍率: {len(job_ratio_map)} 都道府県/地域")

    print("\n=== CSV 読み込み ===")
    pp = pd.read_csv(
        f"{STG}/population_projection.csv",
        dtype={"muni_code": str},
        encoding="utf-8",
    )
    es = pd.read_csv(
        f"{STG}/employment_structure.csv",
        dtype={"region_code": str},
        encoding="utf-8",
    )
    ml = pd.read_csv(f"{STG}/monthly_labor.csv", encoding="utf-8")
    print(f"  population_projection: {len(pp):,} 行")
    print(f"  employment_structure:  {len(es):,} 行")
    print(f"  monthly_labor:         {len(ml):,} 行")

    # ---- テーブル 1 ----
    print("\n=== cross_future_workforce.csv を構築 ===")
    df_wf = build_future_workforce(pp)
    print(f"  出力行数: {len(df_wf):,}")

    # ---- テーブル 2 ----
    print("\n=== cross_wage_public.csv を構築 ===")
    df_wg = build_wage_public(ml, min_wage_map)
    print(f"  出力行数: {len(df_wg):,}")

    # ---- テーブル 3 ----
    print("\n=== cross_switcher_supply.csv を構築 ===")
    df_sw = build_switcher_supply(es, job_ratio_map)
    print(f"  出力行数: {len(df_sw):,}")

    # ---- 検証 ----
    validate_workforce(df_wf)
    validate_wage(df_wg)
    validate_switcher(df_sw)

    # ---- CSV 書き込み (Turso には書かない) ----
    print("\n=== CSV 保存 ===")
    df_wf.to_csv(OUT_WORKFORCE, index=False, encoding="utf-8-sig")
    print(f"  {OUT_WORKFORCE}")

    df_wg.to_csv(OUT_WAGE, index=False, encoding="utf-8-sig")
    print(f"  {OUT_WAGE}")

    df_sw.to_csv(OUT_SWITCHER, index=False, encoding="utf-8-sig")
    print(f"  {OUT_SWITCHER}")

    # ---- サマリー ----
    print("\n=== サマリー ===")
    print(f"  cross_future_workforce  : {len(df_wf):,} 行 × {len(df_wf.columns)} 列")
    print(f"  cross_wage_public       : {len(df_wg):,} 行 × {len(df_wg.columns)} 列")
    print(f"  cross_switcher_supply   : {len(df_sw):,} 行 × {len(df_sw.columns)} 列")
    print(f"\n  検証エラー : {len(ERRORS)} 件")
    print(f"  検証警告   : {len(WARNINGS)} 件")
    if ERRORS:
        print("\n[エラー一覧]")
        for e in ERRORS:
            print(f"  - {e}")
    if WARNINGS:
        print("\n[警告一覧]")
        for w in WARNINGS:
            print(f"  - {w}")

    if ERRORS:
        sys.exit(1)

    print("\n完了 (Turso への書き込みは一切行っていない)")


if __name__ == "__main__":
    main()
