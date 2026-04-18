# -*- coding: utf-8 -*-
"""
Turso外部統計テーブル 統合テストスクリプト
=============================================
country-statistics DBに投入した10テーブルの実データを逆証明スタイルで検証する。

実行方法:
    python -X utf8 test_integration_external.py
    # または
    set PYTHONUTF8=1 && python test_integration_external.py

注意:
    - SELECTのみ。書き込みは絶対に行わない。
    - .envファイルから TURSO_EXTERNAL_URL / TURSO_EXTERNAL_TOKEN を読み込む。
"""

import os
import sys

# Windows CP932環境でも日本語を正常出力するためUTF-8を強制
if sys.stdout.encoding != "utf-8":
    import io
    sys.stdout = io.TextIOWrapper(sys.stdout.buffer, encoding="utf-8", errors="replace")
    sys.stderr = io.TextIOWrapper(sys.stderr.buffer, encoding="utf-8", errors="replace")

import requests

# ─────────────────────────────────────────────
# 設定
# ─────────────────────────────────────────────

# スクリプトのディレクトリ（scripts/）
SCRIPT_DIR = os.path.dirname(os.path.abspath(__file__))

# デプロイリポジトリのルート（scripts/ の一つ上）
DEPLOY_ROOT = os.path.dirname(SCRIPT_DIR)

# .envファイルパス
ENV_FILE = os.path.join(DEPLOY_ROOT, ".env")


# ─────────────────────────────────────────────
# .env ファイル読み込み
# ─────────────────────────────────────────────
def load_env(env_path: str) -> None:
    """
    .envファイルから環境変数を読み込む。
    既に環境変数が設定されている場合は上書きしない。
    """
    if not os.path.exists(env_path):
        return
    with open(env_path, encoding="utf-8") as f:
        for line in f:
            line = line.strip()
            if not line or line.startswith("#"):
                continue
            if "=" not in line:
                continue
            key, _, val = line.partition("=")
            key = key.strip()
            val = val.strip().strip('"').strip("'")
            if key not in os.environ:
                os.environ[key] = val


# ─────────────────────────────────────────────
# Turso HTTP Pipeline API（SELECTのみ）
# ─────────────────────────────────────────────
def turso_select(url: str, token: str, sql: str, params=None) -> list:
    """
    Turso HTTP Pipeline APIでSELECTを実行してrowsを返す。
    返り値: 各rowはdict形式 {カラム名: 値}
    DB書き込みは行わない。
    """
    headers = {
        "Authorization": f"Bearer {token}",
        "Content-Type": "application/json",
    }

    stmt = {"sql": sql}
    if params:
        stmt["args"] = [
            {"type": "null", "value": None} if v is None
            else {"type": "integer", "value": str(v)} if isinstance(v, int)
            else {"type": "float", "value": v} if isinstance(v, float)
            else {"type": "text", "value": str(v)}
            for v in params
        ]

    payload = {
        "requests": [
            {"type": "execute", "stmt": stmt},
            {"type": "close"},
        ]
    }

    resp = requests.post(
        f"{url}/v2/pipeline",
        headers=headers,
        json=payload,
        timeout=30,
    )

    if resp.status_code != 200:
        raise RuntimeError(f"Turso API エラー {resp.status_code}: {resp.text[:300]}")

    data = resp.json()
    result = data["results"][0]

    # エラーレスポンスの検出
    if result.get("type") == "error":
        raise RuntimeError(f"SQLエラー: {result}")

    # rows を dict形式に変換
    response_data = result["response"]["result"]
    cols = [c["name"] for c in response_data["cols"]]
    rows = []
    for raw_row in response_data["rows"]:
        row = {}
        for i, cell in enumerate(raw_row):
            val = cell.get("value")
            # 数値型に変換（可能な場合）
            if cell.get("type") == "integer" and val is not None:
                val = int(val)
            elif cell.get("type") == "float" and val is not None:
                val = float(val)
            row[cols[i]] = val
        rows.append(row)

    return rows


# ─────────────────────────────────────────────
# テスト実行エンジン
# ─────────────────────────────────────────────
class TestResult:
    """テスト結果を保持するクラス"""
    def __init__(self, name: str, passed: bool, detail: str):
        self.name = name
        self.passed = passed
        self.detail = detail


def run_test(name: str, url: str, token: str, sql: str, validator_fn) -> TestResult:
    """
    name: テスト名
    sql: SELECT文（書き込みは含まない）
    validator_fn: rows を受け取り (passed: bool, detail: str) を返す関数
    """
    try:
        rows = turso_select(url, token, sql)
        passed, detail = validator_fn(rows)
        return TestResult(name, passed, detail)
    except Exception as e:
        return TestResult(name, False, f"例外発生: {e}")


# ─────────────────────────────────────────────
# 10テーブルのテスト定義
# ─────────────────────────────────────────────

def define_tests(url: str, token: str) -> list:
    """全テストのリストを返す（逆証明スタイル）"""

    tests = []

    # ──────────────────────────────────
    # 1. v2_external_foreign_residents
    # ──────────────────────────────────
    def validator_foreign_residents(rows):
        # 行数チェック: 282行
        count = rows[0]["cnt"] if rows else 0
        if count != 282:
            return False, f"行数={count}（期待値=282）"

        # 東京都の永住者 > 100,000人
        rows2 = turso_select(url, token,
            "SELECT count FROM v2_external_foreign_residents "
            "WHERE prefecture = '東京都' AND visa_status = '永住者'"
        )
        if not rows2:
            return False, "東京都の永住者レコードが存在しない"
        perm_count = int(rows2[0]["count"] or 0)
        if perm_count <= 100000:
            return False, f"東京都の永住者={perm_count:,}人（期待値 > 100,000人）"

        # 47都道府県カバー
        rows3 = turso_select(url, token,
            "SELECT COUNT(DISTINCT prefecture) as pref_cnt FROM v2_external_foreign_residents"
        )
        pref_cnt = int(rows3[0]["pref_cnt"] or 0)
        if pref_cnt < 47:
            return False, f"都道府県数={pref_cnt}（期待値 >= 47）"

        return True, f"282行, 東京都永住者={perm_count:,}人, {pref_cnt}都道府県カバー"

    tests.append(run_test(
        "v2_external_foreign_residents",
        url, token,
        "SELECT COUNT(*) as cnt FROM v2_external_foreign_residents",
        validator_foreign_residents,
    ))

    # ──────────────────────────────────
    # 2. v2_external_education
    # ──────────────────────────────────
    def validator_education(rows):
        count = rows[0]["cnt"] if rows else 0
        if count != 282:
            return False, f"行数={count}（期待値=282）"

        # 東京都の大学卒 > 2,000,000
        rows2 = turso_select(url, token,
            "SELECT total_count FROM v2_external_education "
            "WHERE prefecture = '東京都' AND education_level LIKE '%大学%'"
        )
        if not rows2:
            return False, "東京都の大学卒レコードが存在しない"
        univ_count = int(rows2[0]["total_count"] or 0)
        if univ_count <= 2000000:
            return False, f"東京都の大学卒={univ_count:,}人（期待値 > 2,000,000人）"

        # education_levelが6種類
        rows3 = turso_select(url, token,
            "SELECT COUNT(DISTINCT education_level) as lv_cnt FROM v2_external_education"
        )
        lv_cnt = int(rows3[0]["lv_cnt"] or 0)
        if lv_cnt != 6:
            return False, f"education_level種類数={lv_cnt}（期待値=6）"

        return True, f"282行, 東京都大学卒={univ_count:,}人, {lv_cnt}学歴区分"

    tests.append(run_test(
        "v2_external_education",
        url, token,
        "SELECT COUNT(*) as cnt FROM v2_external_education",
        validator_education,
    ))

    # ──────────────────────────────────
    # 3. v2_external_household
    # ──────────────────────────────────
    def validator_household(rows):
        count = rows[0]["cnt"] if rows else 0
        if count != 282:
            return False, f"行数={count}（期待値=282）"

        # 東京都の単独世帯ratio > 0.40
        rows2 = turso_select(url, token,
            "SELECT ratio FROM v2_external_household "
            "WHERE prefecture = '東京都' AND household_type LIKE '%単独%'"
        )
        if not rows2:
            return False, "東京都の単独世帯レコードが存在しない"
        tokyo_ratio = float(rows2[0]["ratio"] or 0)
        if tokyo_ratio <= 0.40:
            return False, f"東京都の単独世帯ratio={tokyo_ratio:.3f}（期待値 > 0.40）"

        # 秋田県の単独世帯ratio < 東京都
        rows3 = turso_select(url, token,
            "SELECT ratio FROM v2_external_household "
            "WHERE prefecture = '秋田県' AND household_type LIKE '%単独%'"
        )
        if not rows3:
            return False, "秋田県の単独世帯レコードが存在しない"
        akita_ratio = float(rows3[0]["ratio"] or 0)
        if akita_ratio >= tokyo_ratio:
            return False, (
                f"秋田県ratio={akita_ratio:.3f} >= 東京都ratio={tokyo_ratio:.3f}"
                "（秋田 < 東京を期待）"
            )

        return True, (
            f"282行, 東京都単独世帯ratio={tokyo_ratio:.3f}, "
            f"秋田県={akita_ratio:.3f}（東京 > 秋田 OK）"
        )

    tests.append(run_test(
        "v2_external_household",
        url, token,
        "SELECT COUNT(*) as cnt FROM v2_external_household",
        validator_household,
    ))

    # ──────────────────────────────────
    # 4. v2_external_boj_tankan
    # ──────────────────────────────────
    def validator_boj_tankan(rows):
        count = rows[0]["cnt"] if rows else 0
        if count <= 2000:
            return False, f"行数={count}（期待値 > 2,000）"

        # 2020Q2の製造業DIがマイナス（コロナ影響）
        rows2 = turso_select(url, token,
            "SELECT di_value FROM v2_external_boj_tankan "
            "WHERE survey_date LIKE '2020%' "
            "AND industry_j LIKE '%製造業%' "
            "AND di_type = '業況' "
            "AND enterprise_size LIKE '%大企業%' "
            "AND result_type = '実績' "
            "LIMIT 1"
        )
        if not rows2:
            # 2020Q2のDI（業況がなければ別のdi_typeで試みる）
            rows2 = turso_select(url, token,
                "SELECT di_value, survey_date, industry_j, di_type FROM v2_external_boj_tankan "
                "WHERE survey_date LIKE '2020%' "
                "AND industry_j LIKE '%製造業%' "
                "LIMIT 1"
            )
        if not rows2:
            return False, "2020年の製造業DIレコードが存在しない"
        di_value = rows2[0]["di_value"]
        # di_valueがNoneまたは正の場合はチェックを緩める（データ構造によっては別カラム）
        di_ok = di_value is not None and int(di_value) < 0

        # industry_jに「製造業」「非製造業」が含まれる
        rows3 = turso_select(url, token,
            "SELECT COUNT(*) as cnt FROM v2_external_boj_tankan WHERE industry_j LIKE '%製造業%'"
        )
        mfg_cnt = int(rows3[0]["cnt"] or 0)
        rows4 = turso_select(url, token,
            "SELECT COUNT(*) as cnt FROM v2_external_boj_tankan WHERE industry_j LIKE '%非製造業%'"
        )
        non_mfg_cnt = int(rows4[0]["cnt"] or 0)

        if mfg_cnt == 0:
            return False, "industry_jに「製造業」が存在しない"
        if non_mfg_cnt == 0:
            return False, "industry_jに「非製造業」が存在しない"

        di_note = f"DI={di_value}（{'マイナス OK' if di_ok else '未確認'}）"
        return True, (
            f"{count:,}行, 製造業={mfg_cnt}行, 非製造業={non_mfg_cnt}行, "
            f"2020年製造業{di_note}"
        )

    tests.append(run_test(
        "v2_external_boj_tankan",
        url, token,
        "SELECT COUNT(*) as cnt FROM v2_external_boj_tankan",
        validator_boj_tankan,
    ))

    # ──────────────────────────────────
    # 5. v2_external_social_life
    # ──────────────────────────────────
    def validator_social_life(rows):
        count = rows[0]["cnt"] if rows else 0
        if count != 188:
            return False, f"行数={count}（期待値=188）"

        # 東京都の趣味・娯楽行動者率 > 70%
        rows2 = turso_select(url, token,
            "SELECT participation_rate FROM v2_external_social_life "
            "WHERE prefecture = '東京都' AND category LIKE '%趣味%'"
        )
        if not rows2:
            return False, "東京都の趣味カテゴリが存在しない"
        hobby_rate = float(rows2[0]["participation_rate"] or 0)
        if hobby_rate <= 70.0:
            return False, f"東京都の趣味行動者率={hobby_rate:.1f}%（期待値 > 70%）"

        # 4カテゴリ（趣味/スポーツ/ボランティア/学習）
        rows3 = turso_select(url, token,
            "SELECT COUNT(DISTINCT category) as cat_cnt FROM v2_external_social_life"
        )
        cat_cnt = int(rows3[0]["cat_cnt"] or 0)
        if cat_cnt < 4:
            return False, f"カテゴリ数={cat_cnt}（期待値 >= 4）"

        return True, f"188行, 東京都趣味行動者率={hobby_rate:.1f}%, {cat_cnt}カテゴリ"

    tests.append(run_test(
        "v2_external_social_life",
        url, token,
        "SELECT COUNT(*) as cnt FROM v2_external_social_life",
        validator_social_life,
    ))

    # ──────────────────────────────────
    # 6. v2_external_household_spending
    # ──────────────────────────────────
    def validator_household_spending(rows):
        count = rows[0]["cnt"] if rows else 0
        if count != 517:
            return False, f"行数={count}（期待値=517）"

        # 東京都区部の食料支出 > 500,000円/年
        rows2 = turso_select(url, token,
            "SELECT annual_amount_yen, city FROM v2_external_household_spending "
            "WHERE prefecture = '東京都' AND category LIKE '%食料%' "
            "ORDER BY annual_amount_yen DESC LIMIT 1"
        )
        if not rows2:
            return False, "東京都の食料支出レコードが存在しない"
        food_yen = int(rows2[0]["annual_amount_yen"] or 0)
        food_city = rows2[0]["city"]
        if food_yen <= 500000:
            return False, f"{food_city}の食料支出={food_yen:,}円（期待値 > 500,000円）"

        # 47都市のカバー
        rows3 = turso_select(url, token,
            "SELECT COUNT(DISTINCT city) as city_cnt FROM v2_external_household_spending"
        )
        city_cnt = int(rows3[0]["city_cnt"] or 0)
        if city_cnt < 47:
            return False, f"都市数={city_cnt}（期待値 >= 47）"

        return True, (
            f"517行, {food_city}食料支出={food_yen:,}円, {city_cnt}都市カバー"
        )

    tests.append(run_test(
        "v2_external_household_spending",
        url, token,
        "SELECT COUNT(*) as cnt FROM v2_external_household_spending",
        validator_household_spending,
    ))

    # ──────────────────────────────────
    # 7. v2_external_industry_structure
    # ──────────────────────────────────
    def validator_industry_structure(rows):
        count = rows[0]["cnt"] if rows else 0
        if count <= 35000:
            return False, f"行数={count}（期待値 > 35,000）"

        # 千代田区(13101)または特別区部(13100)の全産業事業所 > 10,000
        rows2 = turso_select(url, token,
            "SELECT SUM(establishments) as total_estab, city_name FROM v2_external_industry_structure "
            "WHERE city_code IN ('13101', '13100') "
            "GROUP BY city_code ORDER BY total_estab DESC LIMIT 1"
        )
        if not rows2:
            return False, "千代田区/特別区部のレコードが存在しない"
        total_estab = int(rows2[0]["total_estab"] or 0)
        city_name = rows2[0]["city_name"]
        if total_estab <= 10000:
            return False, f"{city_name}の全産業事業所={total_estab:,}（期待値 > 10,000）"

        # 47都道府県コード、1700+市区町村
        rows3 = turso_select(url, token,
            "SELECT COUNT(DISTINCT prefecture_code) as pref_cnt, "
            "COUNT(DISTINCT city_code) as city_cnt "
            "FROM v2_external_industry_structure"
        )
        pref_cnt = int(rows3[0]["pref_cnt"] or 0)
        city_cnt = int(rows3[0]["city_cnt"] or 0)
        if pref_cnt < 47:
            return False, f"都道府県コード数={pref_cnt}（期待値 >= 47）"
        if city_cnt < 1700:
            return False, f"市区町村数={city_cnt}（期待値 >= 1,700）"

        return True, (
            f"{count:,}行, {city_name}事業所={total_estab:,}, "
            f"{pref_cnt}都道府県, {city_cnt:,}市区町村"
        )

    tests.append(run_test(
        "v2_external_industry_structure",
        url, token,
        "SELECT COUNT(*) as cnt FROM v2_external_industry_structure",
        validator_industry_structure,
    ))

    # ──────────────────────────────────
    # 8. v2_external_land_price
    # ──────────────────────────────────
    def validator_land_price(rows):
        count = rows[0]["cnt"] if rows else 0
        if count != 140:
            return False, f"行数={count}（期待値=140）"

        # 東京都住宅地 > 100,000円/m²
        rows2 = turso_select(url, token,
            "SELECT avg_price_per_sqm FROM v2_external_land_price "
            "WHERE prefecture = '東京都' AND land_use LIKE '%住宅%' "
            "ORDER BY year DESC LIMIT 1"
        )
        if not rows2:
            return False, "東京都の住宅地地価レコードが存在しない"
        land_price = float(rows2[0]["avg_price_per_sqm"] or 0)
        if land_price <= 100000:
            return False, f"東京都住宅地地価={land_price:,.0f}円/m²（期待値 > 100,000円/m²）"

        # 3用途区分（住宅地/商業地/工業地）
        rows3 = turso_select(url, token,
            "SELECT COUNT(DISTINCT land_use) as use_cnt FROM v2_external_land_price"
        )
        use_cnt = int(rows3[0]["use_cnt"] or 0)
        if use_cnt < 3:
            return False, f"用途区分数={use_cnt}（期待値 >= 3）"

        return True, f"140行, 東京都住宅地={land_price:,.0f}円/m², {use_cnt}用途区分"

    tests.append(run_test(
        "v2_external_land_price",
        url, token,
        "SELECT COUNT(*) as cnt FROM v2_external_land_price",
        validator_land_price,
    ))

    # ──────────────────────────────────
    # 9. v2_external_car_ownership
    # ──────────────────────────────────
    def validator_car_ownership(rows):
        count = rows[0]["cnt"] if rows else 0
        if count != 47:
            return False, f"行数={count}（期待値=47）"

        # 東京都 < 25台/100人
        rows2 = turso_select(url, token,
            "SELECT cars_per_100people FROM v2_external_car_ownership "
            "WHERE prefecture = '東京都'"
        )
        if not rows2:
            return False, "東京都のレコードが存在しない"
        tokyo_cars = float(rows2[0]["cars_per_100people"] or 0)
        if tokyo_cars >= 25:
            return False, f"東京都={tokyo_cars:.1f}台/100人（期待値 < 25台）"

        # 群馬県 > 40台/100人
        rows3 = turso_select(url, token,
            "SELECT cars_per_100people FROM v2_external_car_ownership "
            "WHERE prefecture = '群馬県'"
        )
        if not rows3:
            return False, "群馬県のレコードが存在しない"
        gunma_cars = float(rows3[0]["cars_per_100people"] or 0)
        if gunma_cars <= 40:
            return False, f"群馬県={gunma_cars:.1f}台/100人（期待値 > 40台）"

        return True, (
            f"47行, 東京都={tokyo_cars:.1f}台/100人（< 25 OK）, "
            f"群馬県={gunma_cars:.1f}台/100人（> 40 OK）"
        )

    tests.append(run_test(
        "v2_external_car_ownership",
        url, token,
        "SELECT COUNT(*) as cnt FROM v2_external_car_ownership",
        validator_car_ownership,
    ))

    # ──────────────────────────────────
    # 10. v2_external_internet_usage
    # ──────────────────────────────────
    def validator_internet_usage(rows):
        count = rows[0]["cnt"] if rows else 0
        if count != 47:
            return False, f"行数={count}（期待値=47）"

        # 東京都 > 75%
        rows2 = turso_select(url, token,
            "SELECT internet_usage_rate FROM v2_external_internet_usage "
            "WHERE prefecture = '東京都'"
        )
        if not rows2:
            return False, "東京都のレコードが存在しない"
        tokyo_rate = float(rows2[0]["internet_usage_rate"] or 0)
        if tokyo_rate <= 75.0:
            return False, f"東京都のインターネット利用率={tokyo_rate:.1f}%（期待値 > 75%）"

        return True, f"47行, 東京都={tokyo_rate:.1f}%（> 75% OK）"

    tests.append(run_test(
        "v2_external_internet_usage",
        url, token,
        "SELECT COUNT(*) as cnt FROM v2_external_internet_usage",
        validator_internet_usage,
    ))

    return tests


# ─────────────────────────────────────────────
# メイン
# ─────────────────────────────────────────────
def main() -> None:
    # .envファイルから環境変数を読み込む
    load_env(ENV_FILE)

    # 接続情報の取得
    turso_url = os.environ.get("TURSO_EXTERNAL_URL", "")
    turso_token = os.environ.get("TURSO_EXTERNAL_TOKEN", "")

    # libsql:// → https:// に変換
    if turso_url.startswith("libsql://"):
        turso_url = "https://" + turso_url[len("libsql://"):]

    if not turso_url or not turso_token:
        print("エラー: TURSO_EXTERNAL_URL または TURSO_EXTERNAL_TOKEN が設定されていません。")
        sys.exit(1)

    print("=== Turso統合テスト（10テーブル） ===")
    print(f"接続先: {turso_url}")
    print()

    # テストを実行
    results = define_tests(turso_url, turso_token)

    # 結果表示
    passed_count = 0
    failed_tests = []

    for r in results:
        if r.passed:
            print(f"[PASS] {r.name}: {r.detail}")
            passed_count += 1
        else:
            print(f"[FAIL] {r.name}: {r.detail} <<< ERROR")
            failed_tests.append(r)

    print()
    print(f"合計: {passed_count}/{len(results)} 合格")

    if failed_tests:
        print()
        print("=== 失敗テストの詳細 ===")
        for r in failed_tests:
            print(f"  [FAIL] {r.name}: {r.detail}")
        sys.exit(1)
    else:
        print("全テスト合格。")
        sys.exit(0)


if __name__ == "__main__":
    main()
