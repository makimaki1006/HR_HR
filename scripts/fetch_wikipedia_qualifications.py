"""
Wikipedia 日本語版から JILPT 資格の記事を取得するスクリプト。

対象: brush-up.jp にヒットしなかった残り 509 件
出力: data/generated/wikipedia_qualifications.json

ライセンス注意:
  Wikipedia の記事テキストは CC BY-SA 4.0 ライセンス。
  表示時は「出典: Wikipedia」「ライセンス: CC BY-SA 4.0」を明記すること。
"""

import sys
import re
import json
import time
import unicodedata
import urllib.request
import urllib.parse
from pathlib import Path

sys.stdout.reconfigure(encoding="utf-8")

BASE_DIR = Path(__file__).parent.parent
OUTPUT_PATH = BASE_DIR / "data" / "generated" / "wikipedia_qualifications.json"
OUTPUT_PATH.parent.mkdir(parents=True, exist_ok=True)

HEADERS = {
    "User-Agent": "LicenseKarteResearcher/1.0 (testing internal tool; contact: internal)",
    "Accept-Language": "ja",
}

RATE_LIMIT = 0.5  # seconds between requests


# ---------------------------------------------------------------------------
# Step 1: brush-up.jp ヒット済み資格を再算出して 509 件の未マッチを特定
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


def fetch_brushup_names() -> set[str]:
    """brush-up.jp の資格名一覧を取得して正規化済み名前セットを返す"""
    url = "https://www.brush-up.jp/genre/50index"
    req = urllib.request.Request(url, headers={"User-Agent": "Mozilla/5.0"})
    html = urllib.request.urlopen(req, timeout=30).read().decode("utf-8")
    pattern = re.compile(
        r'<a[^>]+href="(https://www\.brush-up\.jp/theme/[a-z_]+/[^"#?]+)"[^>]*>([^<]+)</a>'
    )
    brush_norm: set[str] = set()
    seen_urls: set[str] = set()
    for m in pattern.finditer(html):
        u = m.group(1)
        name = m.group(2).strip()
        rel = u.replace("https://www.brush-up.jp", "")
        if rel.count("/") != 3:
            continue
        if u in seen_urls:
            continue
        seen_urls.add(u)
        key = normalize(strip_level(name))
        if key:
            brush_norm.add(key)
    return brush_norm


def fetch_jilpt_names() -> list[str]:
    """Turso から JILPT 682 資格名を取得"""
    from libsql_client import create_client_sync

    env_path = BASE_DIR / ".env"
    e = {
        k.strip(): v.strip()
        for line in env_path.read_text(encoding="utf-8").splitlines()
        for k, _, v in [line.partition("=")]
        if k and not line.startswith("#") and "=" in line
    }
    url = e["TURSO_EXTERNAL_URL"].replace("libsql://", "https://")
    token = e["TURSO_EXTERNAL_TOKEN"]
    client = create_client_sync(url=url, auth_token=token)
    rows = client.execute("SELECT DISTINCT name FROM v2_external_jobtag_qualifications ORDER BY name").rows
    return [r[0] for r in rows]


def compute_unmatched(jilpt_names: list[str], brush_norm: set[str]) -> list[str]:
    """brush-up.jp にマッチしなかった資格名リストを返す"""
    unmatched = []
    for jname in jilpt_names:
        base = strip_level(jname)
        key = normalize(base)
        if key in brush_norm:
            continue
        # 部分一致チェック
        matched = False
        if len(key) >= 3:
            for bk in brush_norm:
                if len(bk) >= 3 and (key in bk or bk in key):
                    matched = True
                    break
        if not matched:
            unmatched.append(jname)
    return unmatched


# ---------------------------------------------------------------------------
# Step 2: Wikipedia MediaWiki API で記事を取得
# ---------------------------------------------------------------------------

WIKI_OPENSEARCH = "https://ja.wikipedia.org/w/api.php"
WIKI_EXTRACT = "https://ja.wikipedia.org/w/api.php"


def wiki_opensearch(query: str) -> list[tuple[str, str]]:
    """opensearch で候補 (title, url) リストを返す。最大 3 件"""
    params = urllib.parse.urlencode({
        "action": "opensearch",
        "search": query,
        "limit": "3",
        "format": "json",
        "redirects": "resolve",
    })
    req = urllib.request.Request(f"{WIKI_OPENSEARCH}?{params}", headers=HEADERS)
    try:
        with urllib.request.urlopen(req, timeout=15) as res:
            data = json.loads(res.read().decode("utf-8"))
        titles = data[1]
        urls = data[3]
        return list(zip(titles, urls))
    except Exception:
        return []


def wiki_extract(title: str) -> str:
    """記事の intro テキストを取得 (最大 1000 字)"""
    params = urllib.parse.urlencode({
        "action": "query",
        "prop": "extracts",
        "exintro": "1",
        "explaintext": "1",
        "format": "json",
        "titles": title,
        "redirects": "1",
    })
    req = urllib.request.Request(f"{WIKI_EXTRACT}?{params}", headers=HEADERS)
    try:
        with urllib.request.urlopen(req, timeout=15) as res:
            data = json.loads(res.read().decode("utf-8"))
        pages = data.get("query", {}).get("pages", {})
        for page_id, page in pages.items():
            if page_id == "-1":
                return ""
            return page.get("extract", "")[:1500]
    except Exception:
        return ""


def title_matches(jilpt_name: str, title: str) -> bool:
    """Wikipedia タイトルが資格名に対応するか判定"""
    jn = normalize(jilpt_name)
    jn_base = normalize(strip_level(jilpt_name))
    tn = normalize(title)
    # 完全一致
    if jn == tn or jn_base == tn:
        return True
    # タイトルが資格名を含む、または資格名がタイトルを含む (最小 3 文字)
    if len(jn_base) >= 3 and len(tn) >= 3:
        if jn_base in tn or tn in jn_base:
            return True
    # 括弧除去後の比較
    tn_no_paren = re.sub(r"（[^）]*）|\([^)]*\)", "", tn)
    if len(jn_base) >= 3 and len(tn_no_paren) >= 3:
        if jn_base in tn_no_paren or tn_no_paren in jn_base:
            return True
    return False


def fetch_wikipedia_for_qualification(jilpt_name: str) -> dict | None:
    """
    1 件の資格について Wikipedia を検索し、ヒットしたら情報を返す。
    ヒットしなかったら None を返す。
    """
    candidates = wiki_opensearch(jilpt_name)
    for title, url in candidates:
        if title_matches(jilpt_name, title):
            extract = wiki_extract(title)
            if extract and len(extract) >= 20:
                return {
                    "jilpt_name": jilpt_name,
                    "wikipedia_title": title,
                    "wikipedia_url": url,
                    "extract": extract,
                    "license": "CC BY-SA 4.0",
                    "source": "Wikipedia 日本語版",
                }
    return None


# ---------------------------------------------------------------------------
# main
# ---------------------------------------------------------------------------

def main():
    print("=== Wikipedia 資格情報取得スクリプト ===\n")

    unmatched_path = BASE_DIR / "data" / "generated" / "unmatched_qualifications.json"

    if unmatched_path.exists():
        # 既存の未マッチリストを使用
        unmatched = json.loads(unmatched_path.read_text(encoding="utf-8"))
        print(f"[1/2] 既存の未マッチリストを使用: {unmatched_path}")
        print(f"  未マッチ件数: {len(unmatched)} 件\n")
    else:
        print("[1/4] brush-up.jp 資格名を取得中...")
        brush_norm = fetch_brushup_names()
        print(f"  brush-up.jp 正規化済み資格: {len(brush_norm)} 件")

        print("[2/4] Turso から JILPT 682 件を取得中...")
        jilpt_names = fetch_jilpt_names()
        print(f"  JILPT 資格: {len(jilpt_names)} 件")

        print("[3/4] 未マッチ資格を算出中...")
        unmatched = compute_unmatched(jilpt_names, brush_norm)
        matched_count = len(jilpt_names) - len(unmatched)
        print(f"  brush-up.jp マッチ: {matched_count} 件")
        print(f"  未マッチ (Wikipedia 対象): {len(unmatched)} 件\n")

        # 未マッチリストを保存
        unmatched_path.write_text(
            json.dumps(unmatched, ensure_ascii=False, indent=2), encoding="utf-8"
        )
        print(f"  未マッチリスト保存: {unmatched_path}\n")

    print(f"[4/4] Wikipedia API で {len(unmatched)} 件を検索中 (0.5s/件)...")
    results = []
    hit_count = 0

    for i, jname in enumerate(unmatched, 1):
        result = fetch_wikipedia_for_qualification(jname)
        if result:
            results.append(result)
            hit_count += 1
            status = f"HIT  [{result['wikipedia_title']}]"
        else:
            status = "miss"

        if i % 50 == 0 or i <= 5 or result is not None:
            print(f"  [{i:3d}/{len(unmatched)}] {jname[:30]:<30} -> {status}")

        time.sleep(RATE_LIMIT)

    # 出力
    OUTPUT_PATH.write_text(
        json.dumps(results, ensure_ascii=False, indent=2), encoding="utf-8"
    )

    print(f"\n=== 結果 ===")
    print(f"対象件数    : {len(unmatched)} 件")
    print(f"Wikipedia ヒット: {hit_count} 件 ({hit_count/len(unmatched)*100:.1f}%)")
    print(f"出力ファイル: {OUTPUT_PATH}")

    print("\n--- サンプル (最大 5 件) ---")
    for r in results[:5]:
        print(f"  JILPT名: {r['jilpt_name']}")
        print(f"  WP タイトル: {r['wikipedia_title']}")
        print(f"  本文冒頭: {r['extract'][:100]}")
        print()


if __name__ == "__main__":
    main()
