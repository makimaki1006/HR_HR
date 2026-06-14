"""
Wikipedia 日本語版から JILPT 資格の記事を取得するスクリプト (v2)。

改良点:
  1. 親資格ベースの多段階クエリ (元名 → strip_level → +技能講習/試験/免許)
  2. opensearch + search API の併用
  3. リダイレクト追跡 (redirects=1)
  4. ヒット判定の緩和 (親キーワードがWPタイトルに含まれれば採用)

対象: data/generated/unmatched_qualifications.json (509 件)
出力: data/generated/wikipedia_qualifications_v2.json
"""

import sys
import re
import json
import time
import unicodedata
import urllib.request
import urllib.parse
import urllib.error
from pathlib import Path

sys.stdout.reconfigure(encoding="utf-8")

BASE_DIR = Path(__file__).parent.parent
INPUT_PATH = BASE_DIR / "data" / "generated" / "unmatched_qualifications.json"
OUTPUT_PATH = BASE_DIR / "data" / "generated" / "wikipedia_qualifications_v2.json"
OUTPUT_PATH.parent.mkdir(parents=True, exist_ok=True)

# ログファイルも同時に書き出す (バックグラウンド実行時の進捗確認用)
_LOG_PATH = BASE_DIR / "data" / "generated" / "wiki_v2_progress.log"
_log_fh = _LOG_PATH.open("w", encoding="utf-8", buffering=1)


def _log(msg: str) -> None:
    print(msg)
    _log_fh.write(msg + "\n")
    _log_fh.flush()

HEADERS = {
    # Wikipedia bot policy: メールアドレスを含む識別可能な User-Agent を推奨
    "User-Agent": "LicenseKarteResearcher/2.0 (https://github.com/makimaki1006; internal qualification research tool)",
    "Accept-Language": "ja",
}

RATE_LIMIT = 2.0  # seconds between API calls (Wikipedia 429対策)
WIKI_API = "https://ja.wikipedia.org/w/api.php"

# 429エラー時のウェイト: 1回だけ10秒待ってリトライ、それでもNGなら諦める
BACKOFF_ON_429 = 10


# ---------------------------------------------------------------------------
# strip_level: 級別・種別接頭辞・接尾辞を除去 (_tmp_brush_match2.py から流用)
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
# クエリ候補生成: 5段階
# ---------------------------------------------------------------------------

def build_queries(jilpt_name: str) -> list[str]:
    """
    検索クエリを優先順で返す (重複排除)。
    速度最適化のため最大 3 クエリ: 完全一致 / 親資格 / +技能講習。
    搜索APIフォールバックは別途処理。
    """
    base = strip_level(jilpt_name)
    seen: set[str] = set()
    queries: list[str] = []

    for q in [
        jilpt_name,           # 1. 完全一致
        base,                  # 2. 親資格
        base + " 技能講習",    # 3. +技能講習 (現場系で有効)
    ]:
        q = q.strip()
        if q and q not in seen:
            seen.add(q)
            queries.append(q)
    return queries


# ---------------------------------------------------------------------------
# Wikipedia API ラッパー
# ---------------------------------------------------------------------------

def _api_get(params: dict) -> dict | list:
    """Wikipedia API GET リクエスト (429時に短いバックオフ後1回リトライ)

    opensearch は JSON 配列を返すため戻り値が list になる場合がある。
    エラー時は dict ({"_429": True} または {}) を返す。
    """
    url = WIKI_API + "?" + urllib.parse.urlencode(params)
    req = urllib.request.Request(url, headers=HEADERS)
    for attempt in range(2):
        try:
            with urllib.request.urlopen(req, timeout=25) as res:
                return json.loads(res.read().decode("utf-8"))
        except urllib.error.HTTPError as e:
            if e.code == 429:
                if attempt == 0:
                    time.sleep(BACKOFF_ON_429)
                    continue
                # 2回目も429なら諦める
                return {"_429": True}
            return {}
        except Exception:
            return {}
    return {"_429": True}


def wiki_opensearch(query: str) -> list[tuple[str, str]]:
    """opensearch で (title, url) リストを返す (最大 5 件)

    opensearch のレスポンスは JSON 配列形式: [query, [titles], [descs], [urls]]
    """
    data = _api_get({
        "action": "opensearch",
        "search": query,
        "limit": "5",
        "format": "json",
        "redirects": "resolve",
    })
    # エラー時は dict が返る。正常時はリスト。
    if isinstance(data, dict):
        return []  # {"_429": True} または {}
    if not data or len(data) < 4:
        return []
    return list(zip(data[1], data[3]))


def wiki_search(query: str) -> list[str]:
    """search API で title リストを返す (最大 5 件)"""
    data = _api_get({
        "action": "query",
        "list": "search",
        "srsearch": query,
        "srlimit": "5",
        "format": "json",
    })
    if data.get("_429"):
        return []
    hits = data.get("query", {}).get("search", [])
    return [h["title"] for h in hits]


def wiki_extract(title: str) -> tuple[str, str]:
    """
    記事の intro テキストとリダイレクト後の実タイトルを返す。
    リダイレクトがある場合も redirects=1 で追跡。
    戻り値: (extract_text, resolved_title)
    """
    data = _api_get({
        "action": "query",
        "prop": "extracts",
        "exintro": "1",
        "explaintext": "1",
        "format": "json",
        "titles": title,
        "redirects": "1",
    })
    pages = data.get("query", {}).get("pages", {})
    redirects = data.get("query", {}).get("redirects", [])
    resolved_title = title
    if redirects:
        resolved_title = redirects[-1].get("to", title)

    for page_id, page in pages.items():
        if page_id == "-1":
            return "", resolved_title
        extract = page.get("extract", "")[:2000]
        actual_title = page.get("title", resolved_title)
        return extract, actual_title
    return "", resolved_title


# ---------------------------------------------------------------------------
# ヒット判定 (緩和版)
# ---------------------------------------------------------------------------

def title_matches(jilpt_name: str, wp_title: str) -> bool:
    """
    JILPT 資格名と Wikipedia タイトルが対応するか判定。
    緩和戦略:
      1. 完全一致 (正規化後)
      2. 親資格 (strip_level) が WP タイトルに含まれる
      3. WP タイトルが親資格に含まれる (最小 4 文字)
      4. 括弧除去後の比較
    """
    jn = normalize(jilpt_name)
    jn_base = normalize(strip_level(jilpt_name))
    tn = normalize(wp_title)
    tn_no_paren = normalize(re.sub(r"（[^）]*）|\([^)]*\)", "", wp_title))

    # 1. 完全一致
    if jn == tn or jn_base == tn:
        return True

    # 2. 親資格が WP タイトルに含まれる (最小 4 文字で誤検出防止)
    if len(jn_base) >= 4 and jn_base in tn:
        return True

    # 3. WP タイトルが親資格に含まれる (最小 4 文字)
    if len(tn) >= 4 and tn in jn_base:
        return True

    # 4. 括弧除去後
    if len(jn_base) >= 4 and len(tn_no_paren) >= 4:
        if jn_base in tn_no_paren or tn_no_paren in jn_base:
            return True

    return False


# ---------------------------------------------------------------------------
# 1 件の資格に対して検索 → ヒット判定 → テキスト取得
# ---------------------------------------------------------------------------

def _try_titles(jilpt_name: str, titles_with_urls: list[tuple[str, str]], tried: set[str], query: str) -> dict | None:
    """候補タイトルリストからマッチするものを探してヒット情報を返す"""
    for title, url in titles_with_urls:
        if title in tried:
            continue
        tried.add(title)
        if title_matches(jilpt_name, title):
            extract, resolved_title = wiki_extract(title)
            time.sleep(RATE_LIMIT)
            if extract and len(extract) >= 20:
                wp_url = f"https://ja.wikipedia.org/wiki/{urllib.parse.quote(resolved_title)}"
                return {
                    "jilpt_name": jilpt_name,
                    "wikipedia_title": resolved_title,
                    "wikipedia_url": wp_url,
                    "extract": extract,
                    "license": "CC BY-SA 4.0",
                    "source": "Wikipedia 日本語版",
                    "matched_query": query,
                }
    return None


def fetch_for_qualification(jilpt_name: str) -> dict | None:
    """
    多段階クエリで Wikipedia を検索し、ヒットしたら情報 dict を返す。

    最適化方針:
    - まず全クエリで opensearch のみ実行してタイトル候補を収集
    - マッチ候補があれば extract を1回だけ取得
    - opensearch で見つからなかった場合のみ search API にフォールバック
    """
    queries = build_queries(jilpt_name)
    tried_titles: set[str] = set()

    # Phase 1: 全クエリで opensearch 試行 (軽量)
    for query in queries:
        candidates = wiki_opensearch(query)
        time.sleep(RATE_LIMIT)
        result = _try_titles(jilpt_name, candidates, tried_titles, query)
        if result:
            return result

    # Phase 2: search API フォールバック (親資格クエリのみ)
    base_query = strip_level(jilpt_name)
    if base_query and base_query != jilpt_name:
        search_titles = wiki_search(base_query)
        time.sleep(RATE_LIMIT)
        candidates2 = [(t, f"https://ja.wikipedia.org/wiki/{urllib.parse.quote(t)}") for t in search_titles]
        result = _try_titles(jilpt_name, candidates2, tried_titles, base_query + " (search)")
        if result:
            return result

    return None


# ---------------------------------------------------------------------------
# main
# ---------------------------------------------------------------------------

def wait_for_api_ready() -> None:
    """スクリプト開始時に Wikipedia API が使えるか確認し、429なら解除まで待つ"""
    test_params = {
        "action": "opensearch",
        "search": "フォークリフト",
        "limit": "1",
        "format": "json",
    }
    url = WIKI_API + "?" + urllib.parse.urlencode(test_params)
    req = urllib.request.Request(url, headers=HEADERS)
    for i in range(20):
        try:
            with urllib.request.urlopen(req, timeout=20) as res:
                res.read()
                _log("  API接続: OK")
                return
        except urllib.error.HTTPError as e:
            if e.code == 429:
                wait = 30  # 固定30秒ずつ待機
                _log(f"  API 429 受信。{wait}秒待機中 ({i+1}/20)...")
                time.sleep(wait)
            else:
                _log(f"  API エラー: {e}")
                return
        except Exception as e:
            _log(f"  API エラー: {e}")
            return
    _log("  警告: API接続を確認できませんでした。処理を続行します。")


def main():
    _log("=== Wikipedia 資格情報取得スクリプト v2 ===\n")

    if not INPUT_PATH.exists():
        _log(f"ERROR: 入力ファイルが存在しません: {INPUT_PATH}")
        raise SystemExit(1)

    _log("API接続確認中...")
    wait_for_api_ready()
    time.sleep(2)  # API安定化のために追加待機

    unmatched: list[str] = json.loads(INPUT_PATH.read_text(encoding="utf-8"))
    _log(f"対象: {len(unmatched)} 件")
    _log(f"出力: {OUTPUT_PATH}\n")

    # 既存 v2 JSON がある場合は読み込んでスキップ (中断再開対応)
    existing_results: dict[str, dict] = {}
    if OUTPUT_PATH.exists():
        try:
            existing = json.loads(OUTPUT_PATH.read_text(encoding="utf-8"))
            for r in existing:
                existing_results[r["jilpt_name"]] = r
            _log(f"  既存ヒット読み込み: {len(existing_results)} 件 (スキップ)")
        except Exception:
            pass

    results: list[dict] = list(existing_results.values())
    hit_count = len(results)
    miss_list: list[str] = []

    for i, jname in enumerate(unmatched, 1):
        # 既処理はスキップ
        if jname in existing_results:
            continue
        result = fetch_for_qualification(jname)
        if result:
            results.append(result)
            hit_count += 1
            status = f"HIT  [{result['wikipedia_title']}] (query={result['matched_query']})"
        else:
            miss_list.append(jname)
            status = "miss"

        # 進捗表示: 毎50件、最初5件、ヒット時
        if i % 50 == 0 or i <= 5 or result is not None:
            pct = hit_count / i * 100
            _log(f"  [{i:3d}/{len(unmatched)}] {jname[:35]:<35} -> {status}  (累計{hit_count}件/{pct:.0f}%)")

    # 出力保存
    OUTPUT_PATH.write_text(
        json.dumps(results, ensure_ascii=False, indent=2), encoding="utf-8"
    )

    _log(f"\n=== 結果 ===")
    _log(f"対象件数          : {len(unmatched)} 件")
    _log(f"Wikipedia ヒット   : {hit_count} 件 ({hit_count / len(unmatched) * 100:.1f}%)")
    _log(f"ミス              : {len(miss_list)} 件")
    _log(f"出力ファイル      : {OUTPUT_PATH}")

    # 必須ヒット候補確認
    REQUIRED = [
        "フォークリフト運転技能者",
        "玉掛技能者",
        "ガス溶接技能者",
        "移動式クレーン運転士",
        "公害防止管理者",
        "毒物劇物取扱責任者",
        "第一種電気主任技術者",
        "第二種電気主任技術者",
        "第三種電気主任技術者",
        "放射線取扱主任者",
        "大型自動車第一種運転免許",
        "食品衛生責任者",
        "エネルギー管理士",
        "危険物取扱者乙種第4類",
        "有機溶剤作業主任者",
        "運行管理者（貨物）",
        "運行管理者（旅客）",
    ]
    hit_names = {r["jilpt_name"] for r in results}
    _log("\n--- 必須ヒット候補確認 ---")
    required_hit = 0
    for req in REQUIRED:
        if req in hit_names:
            matched = next(r for r in results if r["jilpt_name"] == req)
            _log(f"  OK  {req} -> {matched['wikipedia_title']}")
            required_hit += 1
        else:
            _log(f"  NG  {req} (未ヒット)")
    _log(f"必須候補カバー: {required_hit}/{len(REQUIRED)}")

    _log("\n--- サンプル (最大 5 件) ---")
    for r in results[:5]:
        _log(f"  JILPT名    : {r['jilpt_name']}")
        _log(f"  WP タイトル: {r['wikipedia_title']}")
        _log(f"  本文冒頭   : {r['extract'][:80]}")
        _log("")

    # ミスリスト (製造・建築・運送系を優先表示)
    keywords = ["クレーン", "フォークリフト", "玉掛", "溶接", "電気", "危険物", "運行"]
    _log("--- 主要ミス候補 (製造・建築・運送系) ---")
    for nm in miss_list:
        if any(k in nm for k in keywords):
            _log(f"  MISS: {nm}")

    _log("\n=== 完了 ===")
    _log_fh.close()


if __name__ == "__main__":
    main()
