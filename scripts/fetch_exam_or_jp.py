"""
fetch_exam_or_jp.py
安全衛生技術試験協会 (exam.or.jp) の全試験ページをスクレイピングし、
JILPT未マッチ資格509件と正規化マッチングして構造化JSONを生成する。

出力: data/generated/exam_or_jp_qualifications.json
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

BASE_URL = "https://www.exam.or.jp"
UA = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0 Safari/537.36"
INTERVAL_SEC = 1.0

# ---------------------------------------------------------------------------
# パス設定
# ---------------------------------------------------------------------------
HERE = Path(__file__).parent.parent  # hellowork-deploy/
UNMATCHED_PATH = HERE / "data" / "generated" / "unmatched_qualifications.json"
OUT_PATH = HERE / "data" / "generated" / "exam_or_jp_qualifications.json"


# ---------------------------------------------------------------------------
# 正規化ユーティリティ (_tmp_brush_match2.py から流用)
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
# HTML 取得
# ---------------------------------------------------------------------------
def fetch_html(url: str) -> str:
    req = urllib.request.Request(url, headers={"User-Agent": UA})
    with urllib.request.urlopen(req, timeout=30) as resp:
        raw = resp.read()
        content_type = resp.headers.get("Content-Type", "")

    # Content-Type または meta charset から文字コードを判定
    enc_m = re.search(r"charset=([A-Za-z0-9_-]+)", content_type)
    if enc_m:
        declared_enc = enc_m.group(1).lower()
    else:
        meta_m = re.search(rb"charset=[\"']*([A-Za-z0-9_-]+)", raw[:2000])
        declared_enc = meta_m.group(1).decode("ascii").lower() if meta_m else "utf-8"

    # UTF-8系
    if declared_enc in ("utf-8", "utf8"):
        return raw.decode("utf-8", errors="replace")
    # Shift-JIS系
    if declared_enc in ("shift_jis", "shift-jis", "sjis", "cp932", "ms932", "windows-31j", "x-sjis"):
        return raw.decode("cp932", errors="replace")
    # EUC-JP
    if declared_enc in ("euc-jp", "euc_jp"):
        return raw.decode("euc_jp", errors="replace")
    # フォールバック: UTF-8
    try:
        return raw.decode("utf-8", errors="replace")
    except Exception:
        return raw.decode("cp932", errors="replace")


# ---------------------------------------------------------------------------
# TOPページからh_shokaiリンクを収集
# ---------------------------------------------------------------------------
def collect_exam_links(top_html: str) -> dict[str, str]:
    """
    Returns: {url: title}
    """
    pattern = re.compile(
        r'href="(https://www\.exam\.or\.jp/introduction/h_shokai\d+)"[^>]*>\s*([^<]+)\s*</a>'
    )
    result: dict[str, str] = {}
    for url, text in pattern.findall(top_html):
        text = text.strip()
        if text and url not in result:
            result[url] = text
    # テキストなしリンクも収集
    all_urls = re.findall(
        r'href="(https://www\.exam\.or\.jp/introduction/h_shokai\d+)"', top_html
    )
    for url in all_urls:
        if url not in result:
            result[url] = ""
    return result


# ---------------------------------------------------------------------------
# 個別ページからセクション抽出
# ---------------------------------------------------------------------------
def parse_exam_page(html: str, url: str, title_fallback: str) -> dict:
    """
    H2/H3 セクション + テーブル本文を構造化して返す。
    """
    # <main> タグを優先、なければ全体
    main_m = re.search(r"<main[^>]*>(.*?)</main>", html, re.DOTALL)
    body = main_m.group(1) if main_m else html

    # ページタイトル (h1 or h2 最初)
    h1_m = re.search(r"<h1[^>]*>(.*?)</h1>", body, re.DOTALL)
    h2_first_m = re.search(r"<h2[^>]*>(.*?)</h2>", body, re.DOTALL)
    page_title = ""
    if h1_m:
        page_title = re.sub(r"<[^>]+>", "", h1_m.group(1)).strip()
    elif h2_first_m:
        page_title = re.sub(r"<[^>]+>", "", h2_first_m.group(1)).strip()
    if not page_title:
        page_title = title_fallback

    # H2単位でセクション分割
    # セクション区切り: <h2...>...</h2> の後に続くHTMLをbodyとする
    section_pattern = re.compile(r"<h2[^>]*>(.*?)</h2>(.*?)(?=<h2|$)", re.DOTALL)
    sections = []
    for m in section_pattern.finditer(body):
        h2_text = re.sub(r"<[^>]+>", "", m.group(1)).strip()
        section_html = m.group(2)

        # テーブル内容をテキスト化
        section_text = extract_text_from_html(section_html)
        if h2_text or section_text:
            sections.append({"h2": h2_text, "body": section_text})

    # セクションがない場合は本文全体を1セクションとして扱う
    if not sections:
        full_text = extract_text_from_html(body)
        if full_text:
            sections.append({"h2": page_title, "body": full_text})

    return {
        "exam_url": url,
        "exam_title": page_title,
        "sections": sections,
    }


def extract_text_from_html(html: str) -> str:
    """
    テーブルのセル、リスト、段落をセミコロン区切りのテキストに変換する。
    """
    # テーブル: <tr> 内の <th>/<td> をタブ区切り、行は改行
    def table_to_text(m: re.Match) -> str:
        rows = []
        for row_m in re.finditer(r"<tr[^>]*>(.*?)</tr>", m.group(0), re.DOTALL):
            cells = re.findall(r"<t[hd][^>]*>(.*?)</t[hd]>", row_m.group(1), re.DOTALL)
            cell_texts = [re.sub(r"<[^>]+>", "", c).strip() for c in cells]
            cell_texts = [re.sub(r"\s+", " ", t) for t in cell_texts]
            if any(cell_texts):
                rows.append("\t".join(cell_texts))
        return "\n".join(rows)

    html = re.sub(r"<table[^>]*>.*?</table>", table_to_text, html, flags=re.DOTALL)

    # リスト: <li> → "・item"
    html = re.sub(r"<li[^>]*>(.*?)</li>", lambda m: "・" + re.sub(r"<[^>]+>", "", m.group(1)).strip() + "\n", html, flags=re.DOTALL)

    # 残タグ除去
    text = re.sub(r"<[^>]+>", " ", html)
    # 空白正規化
    text = re.sub(r"[ \t]+", " ", text)
    text = re.sub(r"\n{3,}", "\n\n", text)
    text = text.strip()
    return text


# ---------------------------------------------------------------------------
# JILPT未マッチ資格とのマッチング
# ---------------------------------------------------------------------------
def match_jilpt(
    exam_title: str,
    jilpt_names: list[str],
    jilpt_norm_map: dict[str, str],
) -> str | None:
    """
    試験名をJILPT未マッチ509件とマッチ。
    完全一致 → 部分一致の順。マッチすればjilpt_nameを返す。
    """
    base = strip_level(exam_title)
    key = normalize(base)

    # 完全一致
    if key in jilpt_norm_map:
        return jilpt_norm_map[key]

    # 部分一致 (3文字以上)
    if len(key) >= 3:
        for jkey, jname in jilpt_norm_map.items():
            if len(jkey) >= 3 and (key in jkey or jkey in key):
                return jname
    return None


# ---------------------------------------------------------------------------
# メイン処理
# ---------------------------------------------------------------------------
def main() -> None:
    print("=== exam.or.jp スクレイパー 開始 ===")

    # JILPT未マッチ資格 読み込み
    print(f"JILPT未マッチ資格読み込み: {UNMATCHED_PATH}")
    with open(UNMATCHED_PATH, encoding="utf-8") as f:
        jilpt_names: list[str] = json.load(f)
    print(f"  -> {len(jilpt_names)} 件")

    # 正規化マップ構築
    jilpt_norm_map: dict[str, str] = {}
    for name in jilpt_names:
        key = normalize(strip_level(name))
        if key:
            jilpt_norm_map[key] = name

    # TOPページ取得
    print(f"TOPページ取得: {BASE_URL}")
    top_html = fetch_html(BASE_URL)
    time.sleep(INTERVAL_SEC)

    # リンク収集
    exam_links = collect_exam_links(top_html)
    print(f"収集リンク数: {len(exam_links)} 件")
    for url, title in exam_links.items():
        print(f"  {url}  {title}")

    # 各ページ取得・パース
    results = []
    fetched_at = datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")
    match_count = 0
    no_match_urls = []

    for i, (url, title_hint) in enumerate(exam_links.items(), 1):
        print(f"\n[{i:02d}/{len(exam_links):02d}] {url}")
        try:
            html = fetch_html(url)
        except Exception as e:
            print(f"  ERROR: {e}")
            no_match_urls.append(url)
            time.sleep(INTERVAL_SEC)
            continue

        page_data = parse_exam_page(html, url, title_hint)
        exam_title = page_data["exam_title"]
        print(f"  タイトル: {exam_title}")
        print(f"  セクション数: {len(page_data['sections'])}")

        # JILPT マッチング
        jilpt_name = match_jilpt(exam_title, jilpt_names, jilpt_norm_map)
        if jilpt_name is None:
            # セクションタイトルでも試みる
            for section in page_data["sections"]:
                jilpt_name = match_jilpt(section["h2"], jilpt_names, jilpt_norm_map)
                if jilpt_name:
                    break
        if jilpt_name:
            print(f"  JILPT マッチ: {jilpt_name}")
            match_count += 1
        else:
            print(f"  JILPT マッチ: (なし)")
            no_match_urls.append(url)

        record = {
            "jilpt_name": jilpt_name or "",
            "exam_url": url,
            "exam_title": exam_title,
            "sections": page_data["sections"],
            "fetched_at": fetched_at,
        }
        results.append(record)
        time.sleep(INTERVAL_SEC)

    # 出力
    OUT_PATH.parent.mkdir(parents=True, exist_ok=True)
    with open(OUT_PATH, "w", encoding="utf-8") as f:
        json.dump(results, f, ensure_ascii=False, indent=2)

    # サマリー
    print("\n=== 完了 ===")
    print(f"取得URL数:         {len(results)}")
    print(f"JILPTマッチ数:     {match_count}")
    print(f"JILPTマッチなし:   {len(no_match_urls)}")
    print(f"出力: {OUT_PATH}")

    # 必須カバー候補確認
    required = [
        "クレーン・デリック運転士",
        "移動式クレーン運転士",
        "揚貨装置運転士",
        "ガス溶接作業主任者",
        "ボイラー整備士",
        "エックス線作業主任者",
        "発破技士",
        "衛生管理者",
        "作業環境測定士",
        "労働安全コンサルタント",
        "労働衛生コンサルタント",
    ]
    titles_found = [r["exam_title"] for r in results]
    print("\n=== 必須カバー候補チェック ===")
    hit_count = 0
    for req_name in required:
        found = any(req_name in t for t in titles_found)
        status = "HIT " if found else "miss"
        if found:
            hit_count += 1
        print(f"  [{status}] {req_name}")
    print(f"  必須カバー: {hit_count}/{len(required)}")

    # サンプル3件表示
    print("\n=== サンプル 3 件 ===")
    for r in results[:3]:
        print(f"  jilpt_name : {r['jilpt_name'] or '(未マッチ)'}")
        print(f"  exam_title : {r['exam_title']}")
        print(f"  exam_url   : {r['exam_url']}")
        print(f"  sections   : {len(r['sections'])} 件")
        if r["sections"]:
            first_sec = r["sections"][0]
            preview = first_sec["body"][:100].replace("\n", " ")
            print(f"  [0] h2={first_sec['h2']!r}  body={preview!r}...")
        print()


if __name__ == "__main__":
    main()
