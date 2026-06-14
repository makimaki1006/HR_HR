"""
jpsk.jp (日本の資格・検定) 横断ポータルから資格情報を取得し、
JILPT 未マッチ資格とマッチングして
data/generated/jpsk_qualifications.json に保存する。

手順:
  1. syllabary.html から全資格 URL を収集（重複除去後 ~1,300 件）
  2. JILPT 未マッチ 509 件と正規化マッチング (strip_level / normalize 流用)
  3. マッチした URL のみ個別ページ取得 (1秒間隔)
  4. 各ページから基本情報・詳細情報セクション（dt/dd）を抽出
  5. data/generated/jpsk_qualifications.json を出力

規約: 1 秒インターバル、User-Agent: Mozilla/5.0 (compatible; LicenseKarteResearcher/1.0)
"""

import json
import re
import sys
import time
import unicodedata
import urllib.request
from datetime import datetime, timezone
from pathlib import Path

# ---------------------------------------------------------------------------
# 定数
# ---------------------------------------------------------------------------
BASE_URL = "https://jpsk.jp"
SYLLABARY_URL = f"{BASE_URL}/examinations/syllabary.html"
UA = "Mozilla/5.0 (compatible; LicenseKarteResearcher/1.0)"
INTERVAL = 1.0  # seconds

# ---------------------------------------------------------------------------
# strip_level / normalize (_tmp_brush_match2.py より流用)
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
    # 全角数字を半角に（両トランスレート適用で念押し）
    s = s.translate(str.maketrans("０１2３４５６７８９", "0123456789"))
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
# HTTP ヘルパー
# ---------------------------------------------------------------------------
def fetch_html(url: str) -> str:
    req = urllib.request.Request(url, headers={"User-Agent": UA})
    with urllib.request.urlopen(req, timeout=30) as resp:
        raw = resp.read()
    # charset を検出して decode
    try:
        return raw.decode("utf-8")
    except UnicodeDecodeError:
        return raw.decode("cp932", errors="replace")


def strip_tags(html_fragment: str) -> str:
    """HTML タグを除去してテキスト化する。"""
    text = re.sub(r"<br\s*/?>", "\n", html_fragment, flags=re.IGNORECASE)
    text = re.sub(r"<[^>]+>", "", text)
    # 連続する空白・改行を整理
    text = re.sub(r"[ \t]+", " ", text)
    text = re.sub(r"\n{3,}", "\n\n", text)
    return text.strip()


# ---------------------------------------------------------------------------
# Step 1: syllabary.html から全資格 URL を収集
# ---------------------------------------------------------------------------
EXCLUDE_PATH = re.compile(
    r"/examinations/(browse|tag/|calendar|syllabary|initial/|safety|transport|food|realestate|genre)"
)


def collect_all_urls() -> dict[str, str]:
    """
    syllabary.html から全資格 URL を収集し {url: title} を返す。
    title は syllabary ページ上のリンクテキストを利用。
    個別ページ名が欲しいため、ここでは URL だけを確定させる。
    """
    print(f"[Step1] {SYLLABARY_URL} を取得中...")
    html = fetch_html(SYLLABARY_URL)
    time.sleep(INTERVAL)

    # href + リンクテキストを抽出
    pattern = re.compile(
        r'href="(/examinations/[^"]+\.html)"[^>]*>\s*([^<]+?)\s*</a>',
        re.S,
    )
    seen: set[str] = set()
    url_to_title: dict[str, str] = {}

    for slug, raw_name in pattern.findall(html):
        if EXCLUDE_PATH.search(slug):
            continue
        if slug in seen:
            continue
        seen.add(slug)
        name = re.sub(r"<[^>]+>", "", raw_name).strip()
        if name:
            url_to_title[f"{BASE_URL}{slug}"] = name

    print(f"[Step1] 収集完了: {len(url_to_title)} 件")
    return url_to_title


# ---------------------------------------------------------------------------
# Step 2: JILPT 未マッチ 509 件と正規化マッチング
# ---------------------------------------------------------------------------
def build_match_table(
    jpsk_url_to_title: dict[str, str],
    jilpt_unmatched: list[str],
) -> dict[str, list[tuple[str, str, str]]]:
    """
    JILPT 未マッチ名 → [(url, jpsk_title, match_type), ...]
    match_type: 'exact' | 'substr'
    """
    # jpsk 側を正規化: normalized_base -> (url, original_title)
    jpsk_norm: dict[str, tuple[str, str]] = {}
    for url, title in jpsk_url_to_title.items():
        base = strip_level(title)
        key = normalize(base)
        if key and key not in jpsk_norm:
            jpsk_norm[key] = (url, title)

    result: dict[str, list[tuple[str, str, str]]] = {}

    for jname in jilpt_unmatched:
        base = strip_level(jname)
        key = normalize(base)

        hits: list[tuple[str, str, str]] = []

        # 完全一致（級別正規化後）
        if key in jpsk_norm:
            url, jtitle = jpsk_norm[key]
            hits.append((url, jtitle, "exact"))
        else:
            # 部分一致（最低3文字以上）
            for bk, (bu, bo) in jpsk_norm.items():
                if len(key) >= 3 and len(bk) >= 3 and (key in bk or bk in key):
                    hits.append((bu, bo, "substr"))
                    if len(hits) >= 3:
                        break

        if hits:
            result[jname] = hits

    return result


# ---------------------------------------------------------------------------
# Step 3: 個別ページ取得・セクション抽出
# ---------------------------------------------------------------------------
def extract_page_sections(html: str) -> list[dict]:
    """
    examination-item div（基本情報・詳細情報）から
    [{"h2": "...", "body": "DT: DD\nDT: DD\n..."}, ...] を返す。
    og:description も先頭セクションとして追加。
    """
    sections: list[dict] = []

    # og:description を概要セクションとして追加
    og_m = re.search(
        r'<meta[^>]+property="og:description"[^>]+content="([^"]+)"',
        html,
    )
    if og_m:
        sections.append({"h2": "概要", "body": og_m.group(1).strip()})

    # examination-item positions
    item_positions = [m.start() for m in re.finditer(r'<div[^>]+class="examination-item"', html)]
    if not item_positions:
        return sections

    # 各 item の終端を次 item または sidebar で区切る
    item_positions.append(len(html))
    for i, pos in enumerate(item_positions[:-1]):
        chunk = html[pos : item_positions[i + 1]]

        # h2
        h2_m = re.search(r"<h2[^>]*>(.*?)</h2>", chunk, re.S)
        h2_text = strip_tags(h2_m.group(1)) if h2_m else ""

        # dt/dd ペア
        pairs = re.findall(r"<dt[^>]*>(.*?)</dt>\s*<dd[^>]*>(.*?)</dd>", chunk, re.S)
        body_parts: list[str] = []
        for dt_raw, dd_raw in pairs:
            dt_text = strip_tags(dt_raw)
            dd_text = strip_tags(dd_raw)
            if dt_text or dd_text:
                body_parts.append(f"{dt_text}: {dd_text}")

        body = "\n".join(body_parts)
        if h2_text or body:
            sections.append({"h2": h2_text, "body": body})

    return sections


def fetch_page_data(
    jilpt_name: str,
    url: str,
    jpsk_title: str,
    fetched_at: str,
) -> dict | None:
    """
    個別 URL から情報を取得してエントリ辞書を返す。
    失敗時は None を返す（呼び出し側でリトライなし）。
    """
    try:
        html = fetch_html(url)
    except Exception as exc:
        print(f"  [WARN] 取得失敗: {url} ({exc})", file=sys.stderr)
        return None

    # H1 タイトル
    h1_m = re.search(r"<h1[^>]*>(.*?)</h1>", html, re.S)
    page_title = strip_tags(h1_m.group(1)) if h1_m else jpsk_title

    sections = extract_page_sections(html)

    return {
        "jilpt_name": jilpt_name,
        "jpsk_url": url,
        "jpsk_title": page_title,
        "sections": sections,
        "fetched_at": fetched_at,
    }


# ---------------------------------------------------------------------------
# メイン
# ---------------------------------------------------------------------------
def main() -> None:
    sys.stdout.reconfigure(encoding="utf-8")

    base_dir = Path(__file__).resolve().parent.parent
    unmatched_path = base_dir / "data" / "generated" / "unmatched_qualifications.json"
    out_path = base_dir / "data" / "generated" / "jpsk_qualifications.json"

    if not unmatched_path.exists():
        raise FileNotFoundError(f"未マッチファイルが見つかりません: {unmatched_path}")

    with open(unmatched_path, encoding="utf-8") as f:
        jilpt_unmatched: list[str] = json.load(f)
    print(f"[load] JILPT 未マッチ: {len(jilpt_unmatched)} 件")

    # Step 1: URL 収集
    jpsk_url_to_title = collect_all_urls()

    # Step 2: マッチング
    match_table = build_match_table(jpsk_url_to_title, jilpt_unmatched)
    exact_count = sum(1 for hits in match_table.values() if hits and hits[0][2] == "exact")
    substr_count = sum(1 for hits in match_table.values() if hits and hits[0][2] == "substr")
    print(f"[Step2] マッチング結果: 完全一致={exact_count}, 部分一致={substr_count}, 合計={len(match_table)} 件")

    # 取得対象 URL（重複除去。先頭 hit のみ使用）
    target_pairs: list[tuple[str, str, str]] = []  # (jilpt_name, url, jpsk_title)
    seen_urls: set[str] = set()
    for jname, hits in match_table.items():
        url, jtitle, _mtype = hits[0]
        if url not in seen_urls:
            seen_urls.add(url)
            target_pairs.append((jname, url, jtitle))

    print(f"[Step3] 個別ページ取得対象: {len(target_pairs)} 件")

    # Step 3: 個別ページ取得
    fetched_at = datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")
    results: list[dict] = []
    success = 0
    failure = 0

    for i, (jname, url, jtitle) in enumerate(target_pairs, 1):
        print(f"  [{i:3d}/{len(target_pairs)}] {jname} <- {url}")
        entry = fetch_page_data(jname, url, jtitle, fetched_at)
        if entry is not None:
            results.append(entry)
            success += 1
        else:
            failure += 1
        time.sleep(INTERVAL)

    # Step 4: JSON 出力
    out_path.parent.mkdir(parents=True, exist_ok=True)
    with open(out_path, "w", encoding="utf-8") as f:
        json.dump(results, f, ensure_ascii=False, indent=2)

    print()
    print("=" * 60)
    print(f"[完了] jpsk 取得結果")
    print(f"  総 URL 数 (syllabary):      {len(jpsk_url_to_title)}")
    print(f"  JILPT 未マッチ入力:         {len(jilpt_unmatched)}")
    print(f"  マッチ数 (完全 + 部分):     {len(match_table)}")
    print(f"  個別ページ取得 成功:         {success}")
    print(f"  個別ページ取得 失敗:         {failure}")
    print(f"  出力 JSON:                  {out_path}")
    print("=" * 60)

    # 必須サンプル確認
    samples = [
        "エネルギー管理士",
        "電気主任技術者",
        "公害防止管理者",
        "毒物劇物取扱責任者",
        "危険物取扱者",
        "運行管理者",
        "大型自動車免許",
        "食品衛生責任者",
    ]
    print("\n[必須サンプル カバー確認]")
    for s in samples:
        if s in match_table:
            url, jtitle, mtype = match_table[s][0]
            print(f"  ✓ {s} ({mtype}) -> {jtitle} / {url}")
        else:
            # 部分一致でも探す
            found = any(s in jn or jn in s for jn in match_table)
            print(f"  {'△' if found else '✗'} {s} (未マッチ)")


if __name__ == "__main__":
    main()
