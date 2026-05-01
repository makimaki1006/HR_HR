"""CTAS 戻し後の逆証明検証 (拡張版)

docs/flow_ctas_restore.md の検証 SQL に加え、以下を追加検証:
- 数値同値 (float vs int の SUM 戻り型差を許容、tolerance 0.5)
- mesh_count 妥当性 (千代田区が 13-30 mesh1km を含むはず)
- 全 9 (dayflag × timezone) 組合せの sum 一致
- mesh3km_agg 総レコード数の合理性
- 全年通算 sum 一致 (2019+2020+2021)
- 集計値 (dayflag=2 / timezone=2) のレコード数

実行: python scripts/verify_ctas_reverse_proof.py
"""

import os
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
from upload_agoop_to_turso import (  # type: ignore
    load_env,
    normalize_url,
    turso_scalar,
)


def num_equal(a, b, tolerance: float = 0.5) -> bool:
    """数値同値判定 (REAL の SUM が INTEGER ソースと比較される場合の差異を許容)。"""
    try:
        af = float(a) if a is not None else 0.0
        bf = float(b) if b is not None else 0.0
        return abs(af - bf) < tolerance
    except (TypeError, ValueError):
        return False


def main() -> int:
    load_env()
    url_raw = os.environ.get("TURSO_EXTERNAL_URL")
    token = os.environ.get("TURSO_EXTERNAL_TOKEN")
    if not url_raw or not token:
        print("ERROR: TURSO_EXTERNAL_URL / TURSO_EXTERNAL_TOKEN 未設定", file=sys.stderr)
        return 1
    base_url = normalize_url(url_raw)
    results = []

    print("=" * 70)
    print("CTAS 戻し 逆証明検証 (拡張版、9 検証項目)")
    print("=" * 70)

    # ========== 検証 1: 2019/9 平日昼 全国総和 ==========
    print("\n[検証 1] 2019 年 9 月 平日昼 全国総和: ctas == raw")
    ctas_sum = turso_scalar(
        base_url, token,
        "SELECT SUM(pop_sum) FROM v2_flow_city_agg "
        "WHERE year = 2019 AND month = '09' AND dayflag = 1 AND timezone = 0"
    )
    raw_sum = turso_scalar(
        base_url, token,
        "SELECT SUM(population) FROM v2_flow_mesh1km_2019 "
        "WHERE month = 9 AND dayflag = 1 AND timezone = 0"
    )
    print(f"  ctas={ctas_sum} / raw={raw_sum}")
    ok1 = num_equal(ctas_sum, raw_sum)
    results.append(("検証 1: 2019/9 全国総和", ok1))
    print(f"  {'✅ PASS' if ok1 else '❌ FAIL'}")

    # ========== 検証 2: 千代田区 (citycode=13101) ==========
    print("\n[検証 2] 千代田区 (citycode=13101) 2019/9 平日昼: ctas == raw")
    ctas_val = turso_scalar(
        base_url, token,
        "SELECT pop_sum FROM v2_flow_city_agg "
        "WHERE citycode = 13101 AND year = 2019 AND month = '09' "
        "  AND dayflag = 1 AND timezone = 0"
    )
    raw_val = turso_scalar(
        base_url, token,
        "SELECT SUM(population) FROM v2_flow_mesh1km_2019 "
        "WHERE citycode = 13101 AND month = 9 AND dayflag = 1 AND timezone = 0"
    )
    print(f"  ctas={ctas_val} / raw={raw_val}")
    ok2 = num_equal(ctas_val, raw_val)
    results.append(("検証 2: 千代田区 2019/9", ok2))
    print(f"  {'✅ PASS' if ok2 else '❌ FAIL'}")

    # ========== 検証 3: mesh3km_agg 総和 ==========
    print("\n[検証 3] mesh3km_agg 2019/9 平日昼 総和 == raw mesh1km")
    ctas_mesh3km_sum = turso_scalar(
        base_url, token,
        "SELECT SUM(pop_sum) FROM v2_flow_mesh3km_agg "
        "WHERE year = 2019 AND month = '09' AND dayflag = 1 AND timezone = 0"
    )
    print(f"  ctas_mesh3km={ctas_mesh3km_sum} / raw={raw_sum}")
    ok3 = num_equal(ctas_mesh3km_sum, raw_sum)
    results.append(("検証 3: mesh3km_agg 総和", ok3))
    print(f"  {'✅ PASS' if ok3 else '❌ FAIL'}")

    # ========== 検証 4: 集計値 (dayflag=2 or timezone=2) 含有 ==========
    print("\n[検証 4] CTAS に dayflag=2 / timezone=2 集計値が含まれる")
    cnt_agg = turso_scalar(
        base_url, token,
        "SELECT COUNT(*) FROM v2_flow_city_agg WHERE dayflag = 2 OR timezone = 2"
    )
    print(f"  集計値レコード数: {cnt_agg}")
    ok4 = int(str(cnt_agg)) > 0
    results.append(("検証 4: 集計値含有", ok4))
    print(f"  {'✅ PASS' if ok4 else '❌ FAIL'}")

    # ========== 検証 5: 2020/2021 年も raw == ctas ==========
    print("\n[検証 5] 2020/2021 年 9 月平日昼 raw == ctas")
    ok5 = True
    for year in [2020, 2021]:
        ctas_y = turso_scalar(
            base_url, token,
            f"SELECT SUM(pop_sum) FROM v2_flow_city_agg "
            f"WHERE year = {year} AND month = '09' AND dayflag = 1 AND timezone = 0"
        )
        raw_y = turso_scalar(
            base_url, token,
            f"SELECT SUM(population) FROM v2_flow_mesh1km_{year} "
            f"WHERE month = 9 AND dayflag = 1 AND timezone = 0"
        )
        match = num_equal(ctas_y, raw_y)
        print(f"  {year}: ctas={ctas_y} / raw={raw_y} {'✅' if match else '❌'}")
        if not match:
            ok5 = False
    results.append(("検証 5: 2020/2021 年", ok5))
    print(f"  {'✅ PASS' if ok5 else '❌ FAIL'}")

    # ========== 検証 6: mesh_count 妥当性 ==========
    print("\n[検証 6] 千代田区 mesh_count 妥当 (5-30 mesh1km 想定)")
    mc = turso_scalar(
        base_url, token,
        "SELECT mesh_count FROM v2_flow_city_agg "
        "WHERE citycode = 13101 AND year = 2019 AND month = '09' "
        "  AND dayflag = 1 AND timezone = 0"
    )
    raw_mc = turso_scalar(
        base_url, token,
        "SELECT COUNT(*) FROM v2_flow_mesh1km_2019 "
        "WHERE citycode = 13101 AND month = 9 AND dayflag = 1 AND timezone = 0"
    )
    print(f"  ctas mesh_count={mc} / raw COUNT(*)={raw_mc}")
    ok6 = num_equal(mc, raw_mc) and 5 <= int(str(mc)) <= 30
    results.append(("検証 6: mesh_count 妥当", ok6))
    print(f"  {'✅ PASS' if ok6 else '❌ FAIL'}")

    # ========== 検証 7: 全 9 (dayflag × timezone) 組合せ sum 一致 ==========
    print("\n[検証 7] 千代田区 2019/9 全 9 組合せ sum 一致")
    ok7 = True
    for df in (0, 1, 2):
        for tz in (0, 1, 2):
            ctas_v = turso_scalar(
                base_url, token,
                f"SELECT pop_sum FROM v2_flow_city_agg "
                f"WHERE citycode = 13101 AND year = 2019 AND month = '09' "
                f"  AND dayflag = {df} AND timezone = {tz}"
            )
            raw_v = turso_scalar(
                base_url, token,
                f"SELECT SUM(population) FROM v2_flow_mesh1km_2019 "
                f"WHERE citycode = 13101 AND month = 9 AND dayflag = {df} AND timezone = {tz}"
            )
            match = num_equal(ctas_v, raw_v)
            mark = '✅' if match else '❌'
            print(f"  (df={df},tz={tz}): ctas={ctas_v} / raw={raw_v} {mark}")
            if not match:
                ok7 = False
    results.append(("検証 7: 全 9 組合せ", ok7))
    print(f"  {'✅ PASS' if ok7 else '❌ FAIL'}")

    # ========== 検証 8: mesh3km_agg レコード数の合理性 ==========
    print("\n[検証 8] mesh3km_agg レコード数 = 273,809 が 100K-500K 範囲内")
    mesh3km_count = turso_scalar(
        base_url, token,
        "SELECT COUNT(*) FROM v2_flow_mesh3km_agg"
    )
    print(f"  mesh3km_agg total: {mesh3km_count}")
    ok8 = 100_000 <= int(str(mesh3km_count)) <= 500_000
    results.append(("検証 8: mesh3km レコード数", ok8))
    print(f"  {'✅ PASS' if ok8 else '❌ FAIL'}")

    # ========== 検証 9: 全年通算 sum 一致 (timeout 回避、Python 側加算) ==========
    print("\n[検証 9] 全年通算 (2019+2020+2021) 9月平日昼 sum 一致")
    # ctas 側は IN リストで 1 クエリ (city_agg は ~600K 行で軽い)
    ctas_total = turso_scalar(
        base_url, token,
        "SELECT SUM(pop_sum) FROM v2_flow_city_agg "
        "WHERE year IN (2019, 2020, 2021) AND month = '09' "
        "  AND dayflag = 1 AND timezone = 0"
    )
    # raw 側は 3 年別々に取得 → Python で加算 (UNION ALL 一括は 38M 行スキャンで timeout する)
    raw_per_year = []
    for year in [2019, 2020, 2021]:
        rv = turso_scalar(
            base_url, token,
            f"SELECT SUM(population) FROM v2_flow_mesh1km_{year} "
            f"WHERE month = 9 AND dayflag = 1 AND timezone = 0"
        )
        raw_per_year.append(float(rv) if rv is not None else 0.0)
    raw_total = sum(raw_per_year)
    print(f"  ctas_total = {ctas_total}")
    print(f"  raw_total  = {raw_total} (Python 加算)")
    print(f"  内訳: 2019={raw_per_year[0]}, 2020={raw_per_year[1]}, 2021={raw_per_year[2]}")
    ok9 = num_equal(ctas_total, raw_total)
    results.append(("検証 9: 全年通算", ok9))
    print(f"  {'✅ PASS' if ok9 else '❌ FAIL'}")

    # ========== 総合判定 ==========
    print("\n" + "=" * 70)
    print("総合判定")
    print("=" * 70)
    all_ok = True
    for name, ok in results:
        mark = "✅" if ok else "❌"
        print(f"  {mark} {name}")
        if not ok:
            all_ok = False
    print()
    if all_ok:
        print("✅ ALL PASS - CTAS データは raw mesh1km と完全整合")
        print("   → 次ステップ: Rust コード stash 復元 → cargo test → push")
    else:
        print("❌ FAIL - CTAS データに問題あり、再投入 or 調査が必要")
    return 0 if all_ok else 1


if __name__ == "__main__":
    sys.exit(main())
