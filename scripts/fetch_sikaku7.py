"""
sikaku7.com の全資格ページをスクレイピングし、JILPT 未マッチ資格とマッチングして
data/generated/sikaku7_qualifications.json に保存する。

- 1 秒インターバル (商業サイト配慮)
- User-Agent: Mozilla/5.0 (compatible; LicenseKarteResearcher/1.0)
- strip_level() + normalize() は fetch_brushup_qualifications.py から流用
"""

from __future__ import annotations

import json
import re
import sys
import time
import unicodedata
import urllib.request
from datetime import datetime, timezone
from pathlib import Path

try:
    from bs4 import BeautifulSoup

    USE_BS4 = True
except ImportError:
    USE_BS4 = False
    print("[WARNING] beautifulsoup4 not found; falling back to regex parser", file=sys.stderr)

# ---------------------------------------------------------------------------
# パス定義
# ---------------------------------------------------------------------------
BASE_DIR = Path(__file__).parent.parent  # hellowork-deploy/
DATA_DIR = BASE_DIR / "data" / "generated"
UNMATCHED_PATH = DATA_DIR / "unmatched_qualifications.json"
OUTPUT_PATH = DATA_DIR / "sikaku7_qualifications.json"

BASE_URL = "https://www.sikaku7.com"
SITEMAP_URL = f"{BASE_URL}/sitemap"
UA = "Mozilla/5.0 (compatible; LicenseKarteResearcher/1.0)"
INTERVAL_SEC = 1.0

# ---------------------------------------------------------------------------
# 正規化ロジック (fetch_brushup_qualifications.py と同一)
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
    s = re.sub(r"[\s（）()【】「」『』，、,.・/／・]+", "", s)
    return s.lower()


# ---------------------------------------------------------------------------
# サイトマップから URL 取得
# ---------------------------------------------------------------------------
EXCLUDE_SLUGS = {
    "sitemap", "info", "feed", "xmlrpc.php", "wp-json",
    # ガイド記事 (資格ページではない)
    "job-hunting", "career-it", "career-est", "career-acc", "career-law",
    "career-wel", "mama", "fukeiki", "koshunyu", "toeic-toefl",
    "itami-hatarako", "cn-cns", "seitai-chiro", "ahaki", "toschool",
    "youngest", "yumeuta", "boki", "food", "koreisha",
}

EXCLUDE_PATH_PREFIXES = {
    "category/", "wp-content/", "wp-json/", "wp-admin/",
    "comments/", "xmlrpc",
}


def is_qualification_url(url: str) -> bool:
    """資格個別ページか判定する。"""
    if not url.startswith(BASE_URL):
        return False
    path = url[len(BASE_URL):].strip("/")
    if not path:
        return False  # トップページ
    # パス階層が 2 以上 (category/* 等) は除外
    if "/" in path:
        return False
    # 除外スラッグ
    if path in EXCLUDE_SLUGS:
        return False
    # プレフィックス除外
    for pfx in EXCLUDE_PATH_PREFIXES:
        if path.startswith(pfx):
            return False
    # クエリ・アンカー除外
    if "?" in path or "#" in path:
        return False
    return True


def fetch_sitemap_urls() -> list[str]:
    """サイトマップから全資格 URL を取得する。"""
    print(f"[sitemap] {SITEMAP_URL} を取得中...")
    req = urllib.request.Request(SITEMAP_URL, headers={"User-Agent": UA})
    raw = urllib.request.urlopen(req, timeout=30).read()
    html = raw.decode("utf-8")

    # href="..." から全 URL 抽出
    urls_raw = re.findall(r'href=["\']([^"\']+)["\']', html)
    seen: set[str] = set()
    qual_urls: list[str] = []
    for u in urls_raw:
        # 相対パスを絶対化
        if u.startswith("/"):
            u = BASE_URL + u
        if u not in seen and is_qualification_url(u):
            seen.add(u)
            qual_urls.append(u)

    print(f"[sitemap] 全 href 数: {len(urls_raw)}, 資格ページ候補: {len(qual_urls)}")
    return qual_urls


# ---------------------------------------------------------------------------
# 個別ページ取得・フィールド抽出
# ---------------------------------------------------------------------------

# 資格ページに特有のフィールドキー (H2 または段落ラベル)
FIELD_KEYS = [
    "受験資格", "試験日", "申込期間", "申込方法",
    "合格発表日", "受験料", "試験地", "試験内容",
    "受験者数", "合格率", "問合せ先", "住所", "電話番号",
    "公式サイト", "過去問・サンプル問題",
    # 資格区分ラベル (記述がある場合)
    "資格区分", "実務制限", "学歴制限", "年齢制限",
]

# 資格ページ判定キーワード (これらが含まれない場合はガイド記事とみなす)
QUAL_PAGE_SIGNALS = {"受験資格", "試験日", "受験料", "合格率", "問合せ先"}


def _extract_fields_bs4(html: str) -> tuple[str, dict[str, str]]:
    """BeautifulSoup でフィールドを抽出。(title, fields) を返す。"""
    soup = BeautifulSoup(html, "html.parser")

    title_tag = soup.find("title")
    page_title = title_tag.get_text(strip=True) if title_tag else ""
    # "看護師（医療の資格）| 日本の資格ガイド" → 前半部分
    page_title = page_title.split("|")[0].strip()

    main = (
        soup.find("article")
        or soup.find("main")
        or soup.find(id=re.compile(r"content", re.I))
        or soup.body
    )
    if main is None:
        return page_title, {}

    text = main.get_text(separator="\n", strip=True)
    lines = [l.strip() for l in text.splitlines() if l.strip()]

    fields: dict[str, str] = {}

    # --- 資格区分ブロック (ページ冒頭に固定パターンで現れる) ---
    # "国家資格", "（業務独占）" など
    QUAL_TYPES = ["国家資格", "公的資格", "民間資格", "民間検定"]
    RESTRICTION_TYPES = ["業務独占", "名称独占", "設置義務", "必置資格"]

    for line in lines[:20]:
        for qt in QUAL_TYPES:
            if qt in line and "資格区分" not in fields:
                restriction = next((r for r in RESTRICTION_TYPES if r in line), None)
                fields["資格区分"] = f"{qt}（{restriction}）" if restriction else qt
        for label in ["実務制限", "学歴制限", "年齢制限"]:
            if line.startswith(f"{label}：") and label not in fields:
                fields[label] = line[len(label) + 1:]

    # --- キーバリュー形式のフィールド抽出 ---
    # 行を順番に走査し、フィールドキーが見つかったら次の行を値とする
    i = 0
    while i < len(lines):
        line = lines[i]
        matched = False
        for key in FIELD_KEYS:
            if line == key or line.startswith(key + "\n"):
                # 次の行(s)を値として収集
                val_parts: list[str] = []
                j = i + 1
                while j < len(lines) and lines[j] not in FIELD_KEYS:
                    val_parts.append(lines[j])
                    j += 1
                    # 5行以上になったら打ち切り
                    if len(val_parts) >= 5:
                        break
                if val_parts and key not in fields:
                    fields[key] = " ".join(val_parts)
                matched = True
                i = j
                break
        if not matched:
            i += 1

    return page_title, fields


def _extract_fields_regex(html: str) -> tuple[str, dict[str, str]]:
    """regex フォールバック。"""
    title_m = re.search(r"<title>([^<]+)</title>", html)
    page_title = title_m.group(1).split("|")[0].strip() if title_m else ""
    text = re.sub(r"<[^>]+>", "\n", html)
    text = re.sub(r"\n{2,}", "\n", text)
    lines = [l.strip() for l in text.splitlines() if l.strip()]

    fields: dict[str, str] = {}
    i = 0
    while i < len(lines):
        for key in FIELD_KEYS:
            if lines[i] == key:
                val_parts = []
                j = i + 1
                while j < len(lines) and lines[j] not in FIELD_KEYS:
                    val_parts.append(lines[j])
                    j += 1
                    if len(val_parts) >= 5:
                        break
                if val_parts:
                    fields[key] = " ".join(val_parts)
                i = j
                break
        else:
            i += 1
    return page_title, fields


def fetch_page(url: str) -> tuple[str, dict[str, str], bool]:
    """
    1 ページを取得してフィールドを抽出する。
    戻り値: (page_title, fields, is_qual_page)
    is_qual_page=False の場合はガイド記事。
    """
    req = urllib.request.Request(url, headers={"User-Agent": UA})
    try:
        raw = urllib.request.urlopen(req, timeout=30).read()
        html = raw.decode("utf-8", errors="replace")
    except Exception as exc:
        print(f"  [ERROR] GET {url} -> {exc}")
        return "", {}, False

    if USE_BS4:
        page_title, fields = _extract_fields_bs4(html)
    else:
        page_title, fields = _extract_fields_regex(html)

    # 資格ページかどうかの判定
    is_qual = bool(QUAL_PAGE_SIGNALS & set(fields.keys()))
    return page_title, fields, is_qual


# ---------------------------------------------------------------------------
# JILPT 未マッチリストの読み込みとマッチング
# ---------------------------------------------------------------------------

def load_unmatched() -> list[str]:
    with open(UNMATCHED_PATH, encoding="utf-8") as f:
        return json.load(f)


def build_jilpt_norm_map(jilpt_names: list[str]) -> dict[str, str]:
    """正規化キー -> jilpt_name のマップ。"""
    result: dict[str, str] = {}
    for name in jilpt_names:
        key = normalize(strip_level(name))
        if key and key not in result:
            result[key] = name
    return result


def match_jilpt(page_title: str, url: str, jilpt_norm: dict[str, str]) -> str | None:
    """sikaku7 のページタイトルが JILPT リストにマッチするか確認し、
    マッチした jilpt_name を返す。None = 未マッチ。"""
    # タイトルから括弧内の補足を除去して正規化
    # 例: "看護師（医療の資格）" -> "看護師"
    cleaned = re.sub(r"[（(][^）)]+[）)]", "", page_title).strip()
    key = normalize(strip_level(cleaned))

    # 完全一致
    if key in jilpt_norm:
        return jilpt_norm[key]

    # 部分一致 (key が jilpt キーを含む、またはその逆)
    if len(key) >= 3:
        for jk, jname in jilpt_norm.items():
            if len(jk) >= 3 and (key in jk or jk in key):
                return jname

    return None


# ---------------------------------------------------------------------------
# メイン
# ---------------------------------------------------------------------------

def main() -> None:
    print("=== fetch_sikaku7.py 開始 ===")

    # JILPT 未マッチリスト読み込み
    unmatched = load_unmatched()
    print(f"[JILPT] 未マッチ資格: {len(unmatched)} 件")
    jilpt_norm = build_jilpt_norm_map(unmatched)

    # サイトマップ取得
    qual_urls = fetch_sitemap_urls()
    time.sleep(INTERVAL_SEC)

    all_pages: list[dict] = []   # 資格ページ全件
    matched: list[dict] = []     # JILPT ヒット分
    skipped_guide = 0

    total = len(qual_urls)
    for idx, url in enumerate(qual_urls, 1):
        print(f"[{idx}/{total}] {url}")
        page_title, fields, is_qual = fetch_page(url)
        time.sleep(INTERVAL_SEC)

        if not is_qual:
            print(f"  -> ガイド記事とみなしてスキップ (fields={list(fields.keys())[:3]})")
            skipped_guide += 1
            continue

        # JILPT マッチング
        jilpt_name = match_jilpt(page_title, url, jilpt_norm)
        print(f"  title={page_title!r}, fields={len(fields)}, jilpt={jilpt_name!r}")

        record = {
            "jilpt_name": jilpt_name,
            "sikaku7_url": url,
            "sikaku7_title": page_title,
            "fields": fields,
            "fetched_at": datetime.now(timezone.utc).isoformat(),
        }
        all_pages.append(record)

        if jilpt_name:
            matched.append(record)

    print()
    print(f"=== 結果 ===")
    print(f"  サイトマップ URL 数: {total}")
    print(f"  ガイド記事スキップ: {skipped_guide}")
    print(f"  資格ページ: {len(all_pages)}")
    print(f"  JILPT ヒット: {len(matched)}")

    # JSON 保存 (ヒット分のみ)
    DATA_DIR.mkdir(parents=True, exist_ok=True)
    with open(OUTPUT_PATH, "w", encoding="utf-8") as f:
        json.dump(matched, f, ensure_ascii=False, indent=2)
    print(f"  保存先: {OUTPUT_PATH}")

    # サンプル出力
    print()
    print("=== サンプル 3 件 ===")
    for rec in matched[:3]:
        print(f"  jilpt_name: {rec['jilpt_name']}")
        print(f"  url: {rec['sikaku7_url']}")
        print(f"  title: {rec['sikaku7_title']}")
        print(f"  fields ({len(rec['fields'])} 件): {list(rec['fields'].keys())}")
        print()


if __name__ == "__main__":
    main()
