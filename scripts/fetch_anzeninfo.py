"""
安全衛生情報センター (anzeninfo.mhlw.go.jp) 技能講習一覧スクレイピング
meishou.html から全技能講習名を収集し、JILPT未マッチ資格とマッチングする。

出力: data/generated/anzeninfo_qualifications.json
"""
import json
import re
import sys
import time
import unicodedata
import urllib.request
from datetime import datetime, timezone
from pathlib import Path

sys.stdout.reconfigure(encoding="utf-8")

BASE_URL = "https://anzeninfo.mhlw.go.jp/gino/meishou.html"
OUTPUT_PATH = Path(__file__).parent.parent / "data" / "generated" / "anzeninfo_qualifications.json"
UNMATCHED_PATH = Path(__file__).parent.parent / "data" / "generated" / "unmatched_qualifications.json"

# ---------------------------------------------------------------------------
# 正規化ヘルパー (_tmp_brush_match2.py の strip_level / normalize を流用)
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
    (re.compile(r"[\s　]+"), ""),
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
    s = re.sub(r"[\s（）()【】「」『』，、,.・/／・]+", "", s)
    return s.lower()


# ---------------------------------------------------------------------------
# Step 1: meishou.html を取得して現行技能講習名を収集
# ---------------------------------------------------------------------------

def fetch_meishou() -> tuple[str, list[dict]]:
    """meishou.html を取得し、現行技能講習リストを返す。

    Returns
    -------
    tuple[str, list[dict]]
        (raw_html_utf8, list of {no, abbr, title})
    """
    print(f"[fetch] GET {BASE_URL}")
    req = urllib.request.Request(
        BASE_URL,
        headers={"User-Agent": "Mozilla/5.0 (compatible; research-bot/1.0)"},
    )
    resp = urllib.request.urlopen(req, timeout=20)
    raw = resp.read()
    html = raw.decode("shift_jis", errors="replace")
    print(f"[fetch] OK  {len(html)} chars")

    # 現行技能講習テーブルのみ抽出 (最初の <TABLE> ... </TABLE>)
    # 各行: <TR><TD NOWRAP>01</TD><TD NOWRAP>整地</TD><TD>...講習名...</TD></TR>
    pattern = re.compile(
        r"<TR><TD NOWRAP>(\d+)</TD><TD NOWRAP>([^<]+)</TD><TD>([^<]+)</TD></TR>"
    )
    entries = []
    for m in pattern.finditer(html):
        no = m.group(1).strip()
        abbr = m.group(2).strip()
        title = m.group(3).strip()
        # 特例講習テーブル（4列: No. [ref] 略称 名称）はパターンが異なるのでスキップされる
        entries.append({"no": no, "abbr": abbr, "title": title})

    print(f"[parse] 現行技能講習 {len(entries)} 件")
    return html, entries


# ---------------------------------------------------------------------------
# Step 2: JILPT未マッチ資格とマッチング
# ---------------------------------------------------------------------------

# 安全衛生側の講習名から検索キーワードを生成する際に除去するサフィックス
SUFFIX_STRIP = re.compile(r"(技能講習|作業主任者|運転技能講習|主任者技能講習)$")


def phonetic_normalize(s: str) -> str:
    """カタカナの表記ゆれを吸収する。
    例: フオークリフト → フォークリフト (大文字小文字カタカナ統一)
    """
    # 小書き文字への変換テーブル (大書き → 小書き)
    # ※サイトの古いHTML が半角カナ表記ゆれを持つ場合の対策
    table = str.maketrans(
        "アイウエオツヤユヨワカケ",  # 大書き (変換元)
        "ァィゥェォッャュョヮヵヶ",  # 小書き (変換先)
    )
    # 「フオ」「シヨ」など大書きカナが混在するケースを小書きに正規化
    phonetic_fixes = {
        "フオ": "フォ",
        "シヨ": "ショ",
        "ヘリ": "ヘリ",
    }
    for old, new in phonetic_fixes.items():
        s = s.replace(old, new)
    return s


def build_anzen_index(entries: list[dict]) -> list[dict]:
    """各技能講習エントリに正規化キーを付与する。"""
    result = []
    for e in entries:
        title = e["title"]
        # 注釈除去 (例: 「（※１）」)
        clean_title = re.sub(r"[（(]※[０-９\d]+[）)]", "", title).strip()
        # 表記ゆれを正規化してから clean_title に使う
        display_title = phonetic_normalize(clean_title)
        base = strip_level(display_title)
        key = normalize(base)
        result.append({
            **e,
            "clean_title": display_title,
            "norm_key": key,
        })
    return result


def match_jilpt_to_anzen(
    jilpt_names: list[str],
    anzen_entries: list[dict],
) -> list[dict]:
    """JILPT未マッチ資格と安全衛生センター講習名を突合する。

    マッチ戦略:
    1. 完全一致 (normalize同士)
    2. 安全衛生センター側の講習名が jilpt_name を含む (部分一致)
    3. jilpt_name 側の主要語が安全衛生センター講習名を含む

    Returns
    -------
    list[dict]  マッチした結果リスト
    """
    # anzen: norm_key -> entry
    anzen_by_key = {e["norm_key"]: e for e in anzen_entries}

    # anzeninfo は技能講習のみ対象。以下は除外する:
    # - 「特別教育」 (技能講習と別制度)
    # - 「運転士」「技術者」「検定」「試験」 (免許・資格系)
    EXCLUDE_SUFFIXES = re.compile(
        r"(特別教育|運転士|技術士|技術者|検定|試験|認定|修了証|研修|パスポート)"
    )

    results = []
    for jilpt_name in jilpt_names:
        # 特別教育・免許系はスキップ
        if EXCLUDE_SUFFIXES.search(jilpt_name):
            continue

        jilpt_norm = normalize(strip_level(jilpt_name))

        matched_entry = None
        match_type = None

        # 1. 完全一致
        if jilpt_norm in anzen_by_key:
            matched_entry = anzen_by_key[jilpt_norm]
            match_type = "exact"

        # 2. 安全衛生センター講習名 (norm_key) が jilpt_name を含む
        if matched_entry is None:
            for e in anzen_entries:
                if jilpt_norm in e["norm_key"] or e["norm_key"] in jilpt_norm:
                    matched_entry = e
                    match_type = "substring"
                    break

        # 3. 主要語マッチ: jilpt_name から特徴的な語を取り出し安全衛生側検索
        if matched_entry is None:
            # 「技能講習」「作業主任者」「技能者」「特別教育」などのサフィックスを除去して核語を取得
            core = re.sub(r"(技能講習|作業主任者|技能者|特別教育|修了|者|講習修了)$", "", jilpt_name)
            core = strip_level(core)
            core_norm = normalize(core)
            if len(core_norm) >= 3:
                for e in anzen_entries:
                    if core_norm in e["norm_key"]:
                        matched_entry = e
                        match_type = "core_word"
                        break

        # 4. 表記ゆれ対応 (「両」「車」等の字違いを許容するトークンマッチ)
        #    例: 「不整地運搬車両運転」→ anzen側「不整地運搬車運転」との共通部分を計算
        #    「技能」「講習」「作業」等の汎用語はスコアから除外して精度を高める。
        if matched_entry is None:
            # 汎用語: 技能系・工事系の共通サフィックスを除外
            GENERIC_NGRAMS = {
                "技能者", "技能講", "技能講習", "技能運", "転技能",
                "作業主", "作業主任", "主任者技", "主任者技能",
                "者技能", "者技能講", "能講習",
                "運転技", "運転技能",
            }
            jilpt_chars = re.sub(r"[^一-鿿]", "", jilpt_name)
            ngrams: set[str] = set()
            for length in (3, 4, 5):
                for i in range(len(jilpt_chars) - length + 1):
                    ng = jilpt_chars[i : i + length]
                    if ng not in GENERIC_NGRAMS:
                        ngrams.add(ng)
            if ngrams:
                best_score = 0
                best_entry = None
                for e in anzen_entries:
                    anzen_chars = re.sub(r"[^一-鿿]", "", e["clean_title"])
                    score = sum(1 for ng in ngrams if ng in anzen_chars)
                    if score > best_score:
                        best_score = score
                        best_entry = e
                # 特異的 n-gram が 5 つ以上一致した場合のみマッチとみなす
                if best_score >= 5:
                    matched_entry = best_entry
                    match_type = "ngram_overlap"

        if matched_entry is not None:
            results.append({
                "jilpt_name": jilpt_name,
                "match_type": match_type,
                "anzen_no": matched_entry["no"],
                "anzen_abbr": matched_entry["abbr"],
                "anzen_title": matched_entry["clean_title"],
            })

    return results


# ---------------------------------------------------------------------------
# Step 3: 出力 JSON 生成
# ---------------------------------------------------------------------------

def build_output(matches: list[dict], fetched_at: str) -> list[dict]:
    """anzeninfo_qualifications.json の形式に変換する。"""
    records = []
    for m in matches:
        records.append({
            "jilpt_name": m["jilpt_name"],
            "anzeninfo_url": BASE_URL,
            "anzeninfo_title": m["anzen_title"],
            "match_type": m["match_type"],
            "sections": [
                {
                    "h2": "技能講習概要",
                    "body": (
                        f"No.{m['anzen_no']} [{m['anzen_abbr']}] "
                        f"{m['anzen_title']}。"
                        "詳細は労働安全衛生法第61条・第76条に基づく技能講習。"
                        "登録教習機関が実施。修了者は当該業務に従事できる。"
                    ),
                }
            ],
            "fetched_at": fetched_at,
        })
    return records


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------

def main() -> None:
    fetched_at = datetime.now(tz=timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")

    # Step 1: meishou.html 取得
    _, anzen_entries = fetch_meishou()
    time.sleep(1)  # 礼儀的スリープ

    anzen_with_keys = build_anzen_index(anzen_entries)

    # Step 2: JILPT未マッチ資格 読み込み
    with open(UNMATCHED_PATH, encoding="utf-8") as f:
        unmatched: list[str] = json.load(f)
    print(f"[input] JILPT未マッチ {len(unmatched)} 件")

    # Step 3: マッチング
    matches = match_jilpt_to_anzen(unmatched, anzen_with_keys)
    print(f"[match] マッチ {len(matches)} 件")

    # 必須カバー候補のヒット確認
    required_keywords = [
        "フォークリフト", "玉掛", "ガス溶接", "有機溶剤",
        "クレーン", "高所作業", "不整地", "はい作業",
    ]
    hit_required = []
    for kw in required_keywords:
        hits = [m for m in matches if kw in m["jilpt_name"] or kw in m["anzen_title"]]
        if hits:
            hit_required.append(kw)
    print(f"[check] 必須カバー候補ヒット: {len(hit_required)}/{len(required_keywords)} 件")
    for kw in required_keywords:
        status = "HIT" if kw in hit_required else "MISS"
        print(f"        [{status}] {kw}")

    # Step 4: 出力 JSON
    output = build_output(matches, fetched_at)
    OUTPUT_PATH.parent.mkdir(parents=True, exist_ok=True)
    with open(OUTPUT_PATH, "w", encoding="utf-8") as f:
        json.dump(output, f, ensure_ascii=False, indent=2)
    print(f"\n[output] {OUTPUT_PATH}")
    print(f"[output] {len(output)} 件書き込み完了")

    # サンプル表示
    print("\n--- サンプル3件 ---")
    for rec in output[:3]:
        print(f"  jilpt_name   : {rec['jilpt_name']}")
        print(f"  anzen_title  : {rec['anzeninfo_title']}")
        print(f"  match_type   : {rec['match_type']}")
        print()


if __name__ == "__main__":
    main()
