"""職種カルテ(driver) データの逆引きテスト。

検証内容:
  A. 表5（賃金構造基本統計調査）から計算した年齢階級別年収が、ジョブタグAPI応答
     (PopupChartWage)のスナップショットと完全一致する
  B. JILPT解説系CSVに driver12職業がすべて収録されている
  C. JILPT数値系CSVに driver12職業のスコア（興味/価値観/スキル）が必要数だけ存在する
  D. ドメイン不変条件:
     - annual_salary_man_yen = (monthly_total × 12 + annual_bonus) / 10 で計算可能
     - 年齢階級は13行（総計 + 12階級）
     - 年齢階級内の avg_age が妥当範囲(15〜85)
     - scheduled_hours が妥当範囲(120〜200時間/月)
  E. 候補職業JSONの構造・重複・カテゴリ分布検証（135職業対応）

インポート戦略:
  import_jobtag_occupations が作成済みであればそちらを優先使用する。
  未作成の場合は import_jobtag_driver にフォールバックし、load_all_occupations()を
  ローカルで定義する（汎用化完了後にフォールバック節を除去予定）。
"""

from __future__ import annotations

import csv
import json
import sys
from pathlib import Path
from typing import Any

import openpyxl
import pytest

# scripts ディレクトリ親 = hellowork-deploy
ROOT = Path(__file__).resolve().parents[1]
RAW = ROOT / "data" / "jobtag_raw"

# scripts/import_jobtag_occupations.py が作成済みであれば優先インポート。
# まだ存在しない場合は import_jobtag_driver にフォールバックする。
# TODO: import_jobtag_occupations 汎用化完了後にフォールバック節を除去すること。
sys.path.insert(0, str(ROOT / "scripts"))
try:
    from import_jobtag_occupations import (  # noqa: E402
        AGE_RANGE_LABELS,
        DRIVER_OCCUPATIONS_LEGACY as DRIVER_OCCUPATIONS,
        WAGE_CENSUS_NAME_BY_CODE,
        load_wage_age,
        load_descriptions,
        load_numeric,
        load_all_occupations,
    )
except ImportError:
    # フォールバック: import_jobtag_driver から既存シンボルを再利用する
    from import_jobtag_driver import (  # noqa: E402
        AGE_RANGE_LABELS,
        DRIVER_OCCUPATIONS,
        WAGE_CENSUS_NAME_BY_CODE,
        load_wage_age,
        load_descriptions,
        load_numeric,
    )

    def load_all_occupations() -> list[dict[str, Any]]:
        """フォールバック実装: candidate_occupations.json を読み込んで返す。

        import_jobtag_occupations が作成されるまでの暫定実装。
        """
        candidate_path = RAW / "candidate_occupations.json"
        with open(candidate_path, encoding="utf-8") as f:
            return json.load(f)


# ジョブタグAPI(/Occupation/PopupChartWage)から取得した正解値（令和7年度）
# 各 wage_census_code (=API応答の wageCensusOccupationCategoryCode) について
# 年齢階級12行(～19, 20-24, ..., 70+) の annual_salary_man_yen
# ※ driverカテゴリの5コードのみ。他カテゴリのスナップショットは後で追加予定。
JOBTAG_API_SNAPSHOT: dict[str, list[float]] = {
    # 1611=バス運転者 (路線/観光/送迎バス運転手 が参照)
    "1611": [309.5, 393.1, 467.94, 454.63, 466.08, 490.02, 506.78, 499.86, 487.48, 433.04, 346.04, 295.89],
    # 1612=タクシー運転者 (タクシー運転手/介護タクシー が参照)
    "1612": [0.0, 483.36, 482.87, 517.92, 491.87, 498.55, 521.31, 494.31, 481.35, 440.13, 379.05, 347.59],
    # 1614=営業用大型貨物自動車運転者 (トラックドライバー/トレーラー/ダンプ が参照)
    "1614": [353.8, 446.26, 471.86, 511.92, 523.53, 527.09, 529.8, 531.59, 518.0, 471.14, 405.68, 361.29],
    # 1703=その他の運搬従事者 (ルート配送/宅配便/フードデリ が参照)
    "1703": [277.29, 338.32, 377.47, 395.65, 447.83, 440.02, 439.22, 424.03, 424.04, 353.66, 291.02, 258.61],
    # 1601=鉄道運転従事者 (電車運転士)
    "1601": [0.0, 448.64, 576.62, 621.73, 739.59, 804.25, 816.93, 844.96, 821.47, 624.41, 361.85, 0.0],
}


# ───────────────────────── ヘルパ ─────────────────────────

def _calc_annual(monthly_total, annual_bonus) -> float | None:
    """賃金センサスの値から年収(万円)を計算。'-'(データなし)はNoneに正規化。"""
    if monthly_total in (None, "-") or annual_bonus in (None, "-"):
        return None
    try:
        return round((float(monthly_total) * 12 + float(annual_bonus)) / 10, 2)
    except (TypeError, ValueError):
        return None


# ───────────────────────── fixtures ─────────────────────────

@pytest.fixture(scope="module")
def wage_table() -> dict:
    return load_wage_age(RAW / "table5_age.xlsx")


@pytest.fixture(scope="module")
def descriptions() -> dict:
    # driverカテゴリのみを対象とする
    ids = {o["jobtag_id"] for o in DRIVER_OCCUPATIONS}
    return load_descriptions(RAW / "jobtag_desc.csv", ids)


@pytest.fixture(scope="module")
def numerics() -> dict:
    # driverカテゴリのみを対象とする
    ids = {o["jobtag_id"] for o in DRIVER_OCCUPATIONS}
    return load_numeric(RAW / "jobtag_numeric.csv", ids)


@pytest.fixture(scope="module")
def all_occupations() -> list[dict[str, Any]]:
    """候補全職業リスト（candidate_occupations.json 経由）。"""
    return load_all_occupations()


# ───────────────────────── A. 表5↔API スナップショット ─────────────────────────

@pytest.mark.parametrize("wage_code,expected", JOBTAG_API_SNAPSHOT.items())
def test_wage_table_matches_jobtag_api_snapshot(wage_table, wage_code, expected):
    """各 wage_census_code の年齢階級12行が、ジョブタグAPI応答と完全一致する。"""
    wage_name = WAGE_CENSUS_NAME_BY_CODE[wage_code]
    rows = wage_table.get(wage_name)
    assert rows is not None, f"{wage_name} が表5に存在しない"
    assert len(rows) == 13, f"{wage_name}: 13行(総計+12階級)必要"
    age_rows = rows[1:]  # 総計を除く
    assert len(expected) == 12
    for i, (row, expected_value) in enumerate(zip(age_rows, expected)):
        actual = _calc_annual(row["monthly_total_thousand_yen"], row["annual_bonus_thousand_yen"])
        actual_norm = 0.0 if actual is None else actual
        assert abs(actual_norm - expected_value) <= 0.01, (
            f"{wage_name} age_range[{i}]={row['age_range']}: "
            f"calc={actual} api={expected_value}"
        )


# ───────────────────────── B. JILPT 収録 ─────────────────────────

@pytest.mark.parametrize("driver", DRIVER_OCCUPATIONS)
def test_jilpt_description_has_driver(descriptions, driver):
    """driver12職業すべてJILPT解説系に収録されている。"""
    assert driver["jobtag_id"] in descriptions, f"{driver['name']}: 解説系CSV未収録"
    d = descriptions[driver["jobtag_id"]]
    body = d["description"]
    for k in ("summary", "what_is_the_job", "how_to_become", "working_conditions"):
        assert k in body and len(body[k]) > 0, f"{driver['name']}: {k} が空"


# ───────────────────────── C. スコア項目数 ─────────────────────────

@pytest.mark.parametrize("driver", DRIVER_OCCUPATIONS)
def test_jilpt_numeric_score_categories(numerics, driver):
    """興味=0 or 6項目、価値観=11項目、スキル>=30項目。

    新規追加職業(フードデリバリー等)は興味の調査が未実施なことがあり0を許容。
    """
    jid = driver["jobtag_id"]
    items = numerics.get(jid, [])
    by_cat: dict[str, int] = {}
    for it in items:
        by_cat[it["category"]] = by_cat.get(it["category"], 0) + 1
    interest_n = by_cat.get("interest", 0)
    assert interest_n in (0, 6), f"{driver['name']}: 興味は0または6項目"
    assert by_cat.get("values", 0) == 11, f"{driver['name']}: 価値観は11項目必要"
    assert by_cat.get("skills", 0) >= 30, f"{driver['name']}: スキルは30項目以上必要"


# ───────────────────────── D. ドメイン不変条件 ─────────────────────────

@pytest.mark.parametrize("wage_code", WAGE_CENSUS_NAME_BY_CODE.keys())
def test_wage_invariants(wage_table, wage_code):
    """avg_ageは15〜85、scheduled_hoursは120〜200の範囲内（'-'やNoneは検証対象外）"""
    rows = wage_table[WAGE_CENSUS_NAME_BY_CODE[wage_code]]
    for r in rows:
        age = r["avg_age"]
        if isinstance(age, (int, float)):
            assert 15 <= age <= 85, f"{wage_code} {r['age_range']}: 年齢{age}が範囲外"
        hours = r["scheduled_hours"]
        if isinstance(hours, (int, float)):
            assert 120 <= hours <= 200, (
                f"{wage_code} {r['age_range']}: 労働時間{hours}が範囲外"
            )


def test_all_drivers_have_valid_wage_code():
    """driver12職業すべての wage_census_code が WAGE_CENSUS_NAME_BY_CODE に存在する。"""
    for d in DRIVER_OCCUPATIONS:
        assert d["wage_census_code"] in WAGE_CENSUS_NAME_BY_CODE, (
            f"{d['name']}: wage_census_code={d['wage_census_code']} がマスタ未登録"
        )


def test_age_range_labels_consistent_with_rows(wage_table):
    """全 wage_census_code で年齢階級ラベルが AGE_RANGE_LABELS と一致する。"""
    for wage_name, rows in wage_table.items():
        labels = [r["age_range"] for r in rows]
        assert labels == AGE_RANGE_LABELS, f"{wage_name}: 年齢ラベル不一致 {labels}"


# ───────────────────────── E. 候補職業JSON検証（135職業対応） ─────────────────────────

CANDIDATE_JSON_PATH = RAW / "candidate_occupations.json"
REQUIRED_ENTRY_KEYS = {"jobtag_id", "name", "category", "mhlw_code"}
REQUIRED_CATEGORIES = {"logistics", "manufacturing", "construction", "cleaning", "labor"}


def test_candidate_json_exists_and_valid(all_occupations):
    """candidate_occupations.json が読み込め、各エントリに必須キーを持つ。"""
    assert len(all_occupations) > 0, "candidate_occupations.json が空"
    for entry in all_occupations:
        missing = REQUIRED_ENTRY_KEYS - set(entry.keys())
        assert not missing, (
            f"jobtag_id={entry.get('jobtag_id')} でキー不足: {missing}"
        )
        # jobtag_id は整数、name/category/mhlw_code は非空文字列
        assert isinstance(entry["jobtag_id"], int), (
            f"jobtag_id が整数でない: {entry['jobtag_id']}"
        )
        for key in ("name", "category", "mhlw_code"):
            assert isinstance(entry[key], str) and len(entry[key].strip()) > 0, (
                f"jobtag_id={entry['jobtag_id']}: {key} が空または非文字列"
            )


def test_no_duplicate_jobtag_id_across_categories(all_occupations):
    """全候補職業で jobtag_id の重複がない。"""
    seen: dict[int, str] = {}
    duplicates: list[str] = []
    for entry in all_occupations:
        jid = entry["jobtag_id"]
        if jid in seen:
            duplicates.append(
                f"jobtag_id={jid} が重複 ({seen[jid]} と {entry['name']})"
            )
        else:
            seen[jid] = entry["name"]
    assert not duplicates, "jobtag_id 重複: " + "; ".join(duplicates)


def test_category_distribution(all_occupations):
    """候補職業が必須カテゴリ logistics/manufacturing/construction/cleaning/labor を含む。

    件数の絶対値は固定しない（>0 であることのみ検証）。
    """
    by_category: dict[str, int] = {}
    for entry in all_occupations:
        cat = entry["category"]
        by_category[cat] = by_category.get(cat, 0) + 1

    for required_cat in REQUIRED_CATEGORIES:
        count = by_category.get(required_cat, 0)
        assert count > 0, (
            f"カテゴリ '{required_cat}' のエントリが存在しない "
            f"(実際のカテゴリ: {sorted(by_category.keys())})"
        )
