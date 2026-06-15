"""
表記揺れ補完スクリプト: rematch_with_kana_variants.py

問題:
  JILPT「牽引免許」が全8ソースで未ヒット。
  漢字/かな/カタカナ揺れ、全角半角揺れ、略記差異でマッチ漏れが発生。

入力:
  data/generated/{brushup,sikaku7,shikakude,ucan,jpsk,exam_or_jp,anzeninfo,
                  wikipedia_qualifications_v2}_qualifications.json
  (JILPT名は各 jilpt_name フィールドから収集)

出力:
  data/generated/ 内の各 JSON を上書き補完
  data/license_*_turso_import.sql を再生成

手順:
  1. 全JSONのjilpt_nameから既マッチ集合を構築
  2. 表記揺れ正規化ルール (VARIANTS) を定義
  3. 各ソースのタイトルマップに対してバリアント展開でリマッチング
  4. 新規マッチをJSONに追記 (PKはjilpt_name単位、重複不可)
  5. build_*_sql.py を subprocess で再実行

使用するペアリング方針:
  - shikakude「運転免許」URL -> JILPT の全種別免許 (牽引免許含む)
  - jpsk「jidosya.html」URL -> JILPT の全種別免許 (牽引免許含む)
  - 警備員「1級/2級」(半角) == 「１級/２級」(全角)
  - クレーン「限定なし」(ひらがな) == 「限定無し」(漢字) + 全角括弧
  - MOS 全角スペースエントリ == 半角エントリを同一視
"""

from __future__ import annotations

import json
import re
import subprocess
import sys
import unicodedata
from datetime import datetime, timezone
from pathlib import Path

sys.stdout.reconfigure(encoding="utf-8")

BASE_DIR = Path(__file__).resolve().parent.parent
GENERATED = BASE_DIR / "data" / "generated"
SCRIPTS = BASE_DIR / "scripts"

# ---------------------------------------------------------------------------
# 表記揺れ変換テーブル
# (from_pattern, to_replacement) --- 適用順序に依存しないよう独立に定義
# ---------------------------------------------------------------------------
VARIANTS: list[tuple[str, str]] = [
    # 漢字 <-> ひらがな
    ("牽引", "けん引"),
    ("牽引", "けん引き"),
    ("牽引", "ケンイン"),
    # 「なし」<-> 「無し」
    ("なし", "無し"),
    ("なし", "ない"),
    # 国家試験/国家資格
    ("国家試験", "国家資格"),
    # 全角数字 -> 半角
    ("１", "1"), ("２", "2"), ("３", "3"), ("４", "4"), ("５", "5"),
    ("６", "6"), ("７", "7"), ("８", "8"), ("９", "9"), ("０", "0"),
    # 1類 <-> 第1類
    ("1類", "第1類"), ("2類", "第2類"), ("3類", "第3類"),
    ("4類", "第4類"), ("5類", "第5類"), ("6類", "第6類"),
    # 省略形「（限定なし）」「(限定無し)」の統一は normalize() で吸収済み
]

# ---------------------------------------------------------------------------
# strip_level / normalize (_tmp_brush_match2.py 流用)
# ---------------------------------------------------------------------------
LEVEL_PATTERNS = [
    (re.compile(r"^[特1-9一二三四五六七八九十]+級"), ""),
    (re.compile(r"^第[1-9一二三四五六七八九十]+[種類部]"), ""),
    (re.compile(r"[1-9一二三四五六七八九十]+級$"), ""),
    (re.compile(r"第[1-9一二三四五六七八九十]+[種類部]$"), ""),
    (re.compile(r"補$"), ""),
    (re.compile(r"（[^）]+部門）$"), ""),
    (re.compile(r"\([^)]+部門\)$"), ""),
    (re.compile(r"（[^）]+科目）$"), ""),
    (re.compile(r"^(専門|上級|中級|初級|高等|基礎)"), ""),
]


def strip_level(s: str) -> str:
    s = unicodedata.normalize("NFKC", s)
    s = re.sub(r"\s+", "", s)
    s = s.translate(str.maketrans("０１２３４５６７８９", "0123456789"))
    prev = ""
    while prev != s:
        prev = s
        for pat, rep in LEVEL_PATTERNS:
            s = pat.sub(rep, s)
    return s


def normalize(s: str) -> str:
    s = unicodedata.normalize("NFKC", s)
    # 括弧・記号・スペース除去
    s = re.sub(r"[\s（）()【】「」『』，、,.・/／・　]+", "", s)
    return s.lower()


def expand_variants(name: str) -> list[str]:
    """
    JILPT 名からバリアント展開したキー列を生成する。
    最初に NFKC 正規化 (全角数字等を半角へ) を適用してから展開。
    """
    # NFKC 正規化済みのベース
    base = unicodedata.normalize("NFKC", name)
    base_norm = normalize(strip_level(base))

    results: set[str] = {base_norm}

    # バリアント展開: frm -> to / to -> frm
    for frm, to in VARIANTS:
        frm_n = normalize(frm)
        to_n = normalize(to)
        for r in list(results):
            if frm_n in r:
                results.add(r.replace(frm_n, to_n))
            if to_n in r:
                results.add(r.replace(to_n, frm_n))

    return list(results)


# ---------------------------------------------------------------------------
# JSON設定: ソース名, ファイル, タイトルキー, build SQLスクリプト
# ---------------------------------------------------------------------------
SOURCES: list[dict] = [
    {
        "name": "shikakude",
        "json": GENERATED / "shikakude_qualifications.json",
        "title_key": "shikakude_title",
        "url_key": "shikakude_url",
        "build_script": SCRIPTS / "build_shikakude_sql.py",
    },
    {
        "name": "jpsk",
        "json": GENERATED / "jpsk_qualifications.json",
        "title_key": "jpsk_title",
        "url_key": "jpsk_url",
        "build_script": SCRIPTS / "build_jpsk_sql.py",
    },
    {
        "name": "brushup",
        "json": GENERATED / "brushup_qualifications.json",
        "title_key": "brushup_name",
        "url_key": "brushup_url",
        "build_script": SCRIPTS / "build_brushup_sql.py",
    },
    {
        "name": "anzeninfo",
        "json": GENERATED / "anzeninfo_qualifications.json",
        "title_key": "anzeninfo_title",
        "url_key": "anzeninfo_url",
        "build_script": SCRIPTS / "build_anzeninfo_sql.py",
    },
    {
        "name": "sikaku7",
        "json": GENERATED / "sikaku7_qualifications.json",
        "title_key": "sikaku7_title",
        "url_key": "sikaku7_url",
        "build_script": SCRIPTS / "build_sikaku7_sql.py",
    },
    {
        "name": "ucan",
        "json": GENERATED / "ucan_qualifications.json",
        "title_key": "ucan_title",
        "url_key": "ucan_url",
        "build_script": SCRIPTS / "build_ucan_sql.py",
    },
    {
        "name": "exam_or_jp",
        "json": GENERATED / "exam_or_jp_qualifications.json",
        "title_key": "exam_title",
        "url_key": "exam_url",
        "build_script": SCRIPTS / "build_exam_or_jp_sql.py",
    },
    {
        "name": "wikipedia_v2",
        "json": GENERATED / "wikipedia_qualifications_v2.json",
        "title_key": "wikipedia_title",
        "url_key": "wikipedia_url",
        "build_script": SCRIPTS / "build_wikipedia_v2_sql.py",
    },
]


def load_all_matched() -> set[str]:
    """全JSONから既マッチ済みjilpt_nameを収集。"""
    matched: set[str] = set()
    for src in SOURCES:
        if src["json"].exists():
            data = json.loads(src["json"].read_text(encoding="utf-8"))
            for item in data:
                matched.add(item.get("jilpt_name", ""))
    return matched


def load_jilpt_all() -> list[str]:
    """unmatched_qualifications.json から未マッチJILPT名を読む。"""
    p = GENERATED / "unmatched_qualifications.json"
    with open(p, encoding="utf-8", errors="replace") as f:
        return json.load(f)


# ---------------------------------------------------------------------------
# ソース別のリマッチング戦略
# ---------------------------------------------------------------------------

def build_source_norm_map(data: list[dict], title_key: str, url_key: str) -> dict[str, tuple[str, dict]]:
    """
    ソースデータを正規化キー -> (url, 元エントリ) マップに変換。
    """
    norm_map: dict[str, tuple[str, dict]] = {}
    for item in data:
        title = item.get(title_key, "")
        url = item.get(url_key, "")
        for variant in expand_variants(title):
            if variant and variant not in norm_map:
                norm_map[variant] = (url, item)
    return norm_map


def rematch_source(
    src_config: dict,
    unmatched_jilpt: list[str],
    already_matched: set[str],
) -> list[dict]:
    """
    1ソースに対してリマッチングを行い、新規追加エントリリストを返す。
    既マッチ済み (already_matched) は除外。
    """
    json_path = src_config["json"]
    title_key = src_config["title_key"]
    url_key = src_config["url_key"]
    src_name = src_config["name"]

    if not json_path.exists():
        print(f"  [SKIP] {src_name}: JSON が存在しない ({json_path})")
        return []

    data: list[dict] = json.loads(json_path.read_text(encoding="utf-8"))
    existing_jilpt: set[str] = {item.get("jilpt_name", "") for item in data}

    # 正規化マップ
    norm_map = build_source_norm_map(data, title_key, url_key)

    fetched_at = datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")
    new_entries: list[dict] = []

    for jname in unmatched_jilpt:
        if jname in already_matched:
            continue
        if jname in existing_jilpt:
            continue

        variants = expand_variants(jname)
        matched_entry = None
        matched_url = ""

        for variant in variants:
            if variant in norm_map:
                matched_url, matched_entry = norm_map[variant]
                break

        if matched_entry is None:
            # 部分一致 (バリアント展開後)
            for variant in variants:
                if len(variant) < 3:
                    continue
                for nk, (nu, ne) in norm_map.items():
                    if len(nk) >= 3 and (variant in nk or nk in variant):
                        matched_url = nu
                        matched_entry = ne
                        break
                if matched_entry:
                    break

        if matched_entry is None:
            continue

        # 新エントリを構築 (元エントリのセクション等を流用)
        new_item = dict(matched_entry)
        new_item["jilpt_name"] = jname
        new_item["match_type"] = "kana_variant"
        new_item["fetched_at"] = fetched_at
        new_entries.append(new_item)

    return new_entries


# ---------------------------------------------------------------------------
# 特殊補完: jpsk jidosya.html を 免許系 JILPT 全種に適用
# ---------------------------------------------------------------------------
JPSK_JIDOSYA_JILPT_TARGETS = [
    "牽引免許",
    "大型自動車第二種免許",
    "普通自動車第二種免許",
    "自動二輪車免許",
    "大型自動車免許",
    "中型自動車免許",
    "普通自動車免許",
    "準中型自動車免許",
    "大型特殊自動車免許",
    "小型特殊自動車運転免許",
]

SHIKAKUDE_UNTENMEN_JILPT_TARGETS = [
    "牽引免許",
    "大型自動車第二種免許",
    "普通自動車第二種免許",
    "自動二輪車免許",
    "クレーン・デリック運転士(限定なし)",
    "クレーン運転特別教育",
    "小型移動式クレーン運転特別教育",
]


def apply_explicit_supplements(
    already_matched: set[str],
) -> dict[str, list[dict]]:
    """
    明示的な URL 割り当てで補完できないケースを直接追加する。
    返値: {ソース名: [新エントリ]}
    """
    fetched_at = datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")
    supplements: dict[str, list[dict]] = {s["name"]: [] for s in SOURCES}

    # --- jpsk: jidosya.html から牽引免許等を補完 ---
    jpsk_path = GENERATED / "jpsk_qualifications.json"
    if jpsk_path.exists():
        jpsk_data: list[dict] = json.loads(jpsk_path.read_text(encoding="utf-8"))
        jidosya_entry = next(
            (item for item in jpsk_data if "jidosya" in item.get("jpsk_url", "")),
            None,
        )
        existing_jpsk: set[str] = {item.get("jilpt_name", "") for item in jpsk_data}

        if jidosya_entry:
            for jname in JPSK_JIDOSYA_JILPT_TARGETS:
                if jname in already_matched:
                    continue
                if jname in existing_jpsk:
                    continue
                new_item = dict(jidosya_entry)
                new_item["jilpt_name"] = jname
                new_item["match_type"] = "explicit_supplement"
                new_item["fetched_at"] = fetched_at
                supplements["jpsk"].append(new_item)
                print(f"  [jpsk explicit] {jname} -> jidosya.html")

    # --- shikakude: 運転免許ページから補完 ---
    shika_path = GENERATED / "shikakude_qualifications.json"
    if shika_path.exists():
        shika_data: list[dict] = json.loads(shika_path.read_text(encoding="utf-8"))
        unten_entry = next(
            (item for item in shika_data if "untenmen" in item.get("shikakude_url", "")),
            None,
        )
        existing_shika: set[str] = {item.get("jilpt_name", "") for item in shika_data}

        if unten_entry:
            for jname in SHIKAKUDE_UNTENMEN_JILPT_TARGETS:
                if jname in already_matched:
                    continue
                if jname in existing_shika:
                    continue
                new_item = dict(unten_entry)
                new_item["jilpt_name"] = jname
                new_item["match_type"] = "explicit_supplement"
                new_item["fetched_at"] = fetched_at
                supplements["shikakude"].append(new_item)
                print(f"  [shikakude explicit] {jname} -> untenmen.html")

    return supplements


# ---------------------------------------------------------------------------
# メイン
# ---------------------------------------------------------------------------
def main() -> None:
    print("=" * 60)
    print("rematch_with_kana_variants.py 開始")
    print("=" * 60)

    # 1. 現状把握
    already_matched = load_all_matched()
    unmatched_list = load_jilpt_all()
    truly_unmatched = [n for n in unmatched_list if n not in already_matched]

    print(f"\n[現状]")
    print(f"  全JSON既マッチ: {len(already_matched)} 件")
    print(f"  unmatched_qualifications.json: {len(unmatched_list)} 件")
    print(f"  真の未マッチ: {len(truly_unmatched)} 件")

    # 2. 表記揺れリマッチング
    print(f"\n[ステップ1] バリアント展開リマッチング")
    total_new = 0
    source_new_counts: dict[str, int] = {}

    for src in SOURCES:
        new_entries = rematch_source(src, truly_unmatched, already_matched)
        source_new_counts[src["name"]] = len(new_entries)

        if new_entries:
            # JSONに追記
            json_path = src["json"]
            data = json.loads(json_path.read_text(encoding="utf-8"))
            data.extend(new_entries)
            json_path.write_text(json.dumps(data, ensure_ascii=False, indent=2), encoding="utf-8")
            for entry in new_entries:
                already_matched.add(entry["jilpt_name"])
            total_new += len(new_entries)
            print(f"  [{src['name']}] +{len(new_entries)} 件")
            for e in new_entries:
                print(f"    {e['jilpt_name']}")

    # 3. 明示的補完
    print(f"\n[ステップ2] 明示的URL補完 (jpsk jidosya / shikakude 運転免許)")
    supplements = apply_explicit_supplements(already_matched)

    for src in SOURCES:
        s_name = src["name"]
        new_entries = supplements.get(s_name, [])
        if new_entries:
            json_path = src["json"]
            data = json.loads(json_path.read_text(encoding="utf-8"))
            data.extend(new_entries)
            json_path.write_text(json.dumps(data, ensure_ascii=False, indent=2), encoding="utf-8")
            for entry in new_entries:
                already_matched.add(entry["jilpt_name"])
            n = source_new_counts.get(s_name, 0) + len(new_entries)
            source_new_counts[s_name] = n
            total_new += len(new_entries)

    # 4. build_*_sql.py 再実行
    print(f"\n[ステップ3] SQL 再生成")
    build_results: dict[str, bool] = {}
    for src in SOURCES:
        script = src["build_script"]
        if not script.exists():
            print(f"  [SKIP] {src['name']}: build script なし ({script})")
            build_results[src["name"]] = False
            continue

        # 変更があったソースのみ再実行
        if source_new_counts.get(src["name"], 0) == 0:
            print(f"  [SKIP] {src['name']}: 新規追加なし")
            build_results[src["name"]] = True
            continue

        result = subprocess.run(
            [sys.executable, str(script)],
            capture_output=True,
            text=True,
            encoding="utf-8",
            errors="replace",
            cwd=str(BASE_DIR),
        )
        if result.returncode == 0:
            print(f"  [OK] {src['name']}: {script.name}")
            build_results[src["name"]] = True
        else:
            print(f"  [FAIL] {src['name']}: exit={result.returncode}")
            if result.stderr:
                print(f"    stderr: {result.stderr[:200]}")
            build_results[src["name"]] = False

    # 5. 必須資格カバー確認
    print(f"\n[ステップ4] 必須資格カバー確認")
    priority_names = [
        "牽引免許",
        "自動二輪車免許",
        "大型自動車第二種免許",
        "普通自動車第二種免許",
        "クレーン・デリック運転士(限定なし)",
        "クレーン運転特別教育",
        "小型移動式クレーン運転特別教育",
        "大型自動車免許",
        "中型自動車免許",
        "普通自動車免許",
    ]
    # 全JSONから再ロード
    now_matched = load_all_matched()
    for pname in priority_names:
        status = "OK" if pname in now_matched else "MISS"
        # どのJSONに入ったか
        sources_hit = []
        for src in SOURCES:
            if src["json"].exists():
                d = json.loads(src["json"].read_text(encoding="utf-8"))
                if any(item.get("jilpt_name") == pname for item in d):
                    sources_hit.append(src["name"])
        print(f"  [{status}] {pname}: {sources_hit if sources_hit else '-'}")

    # 6. サマリー
    print(f"\n{'=' * 60}")
    print(f"[結果サマリー]")
    print(f"  新規マッチ合計: {total_new} 件")
    print(f"\n  ソース別:")
    for src in SOURCES:
        n = source_new_counts.get(src["name"], 0)
        if n > 0:
            print(f"    {src['name']}: +{n} 件")

    truly_unmatched_after = [n for n in unmatched_list if n not in now_matched]
    print(f"\n  真の未マッチ (補完後): {len(truly_unmatched_after)} 件 (補完前: {len(truly_unmatched)} 件)")


if __name__ == "__main__":
    main()
