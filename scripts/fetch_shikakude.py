"""
shikakude.com (資格の王道) の全資格ページをスクレイピングし、
JILPT 未マッチ資格とマッチングして JSON を生成する。

入力: data/generated/unmatched_qualifications.json (509件)
出力: data/generated/shikakude_qualifications.json

マッチングロジック: fetch_brushup_qualifications.py の strip_level() / normalize() を流用
"""
import html as html_module
import json
import re
import sys
import time
import unicodedata
import urllib.request
from datetime import datetime, timezone
from pathlib import Path

# ---------------------------------------------------------------------------
# マッチングロジック (_tmp_brush_match2.py / fetch_brushup_qualifications.py 流用)
# ---------------------------------------------------------------------------
LEVEL_PATTERNS = [
    (re.compile(r'^[特1-9一二三四五六七八九十]+級'), ''),
    (re.compile(r'^第[1-9一二三四五六七八九十]+[種類部]'), ''),
    (re.compile(r'[1-9一二三四五六七八九十]+級$'), ''),
    (re.compile(r'第[1-9一二三四五六七八九十]+[種類部]$'), ''),
    (re.compile(r'補$'), ''),
    (re.compile(r'（[^）]+部門）$'), ''),
    (re.compile(r'\([^)]+部門\)$'), ''),
    (re.compile(r'（[^）]+科目）$'), ''),
    (re.compile(r'^(専門|上級|中級|初級|高等|基礎)'), ''),
]


def strip_level(s: str) -> str:
    s = unicodedata.normalize('NFKC', s)
    s = re.sub(r'\s+', '', s)
    s = s.translate(str.maketrans('０１２３４５６７８９', '0123456789'))
    prev = ''
    while prev != s:
        prev = s
        for pat, rep in LEVEL_PATTERNS:
            s = pat.sub(rep, s)
    return s


def normalize(s: str) -> str:
    s = unicodedata.normalize('NFKC', s)
    s = re.sub(r'[\s（）()【】「」『』，、,.・/／・]+', '', s)
    return s.lower()


# ---------------------------------------------------------------------------
# sitemap から全資格 URL を収集
# ---------------------------------------------------------------------------
SITEMAP_URL = 'http://www.shikakude.com/sitemap.xml'
# 資格ページのディレクトリ (minsikakupaje=民間, sikakupaje=国家/公的, kosikakupaje=公的)
QUAL_DIRS = {'minsikakupaje', 'sikakupaje', 'kosikakupaje'}
USER_AGENT = 'Mozilla/5.0 (compatible; LicenseKarteResearcher/1.0)'


def fetch_sitemap_urls() -> list[str]:
    """sitemap.xml から資格ページ URL を取得する。"""
    req = urllib.request.Request(
        SITEMAP_URL,
        headers={'User-Agent': USER_AGENT},
    )
    raw = urllib.request.urlopen(req, timeout=30).read()
    # Shift_JIS エンコーディング
    content = raw.decode('shift_jis', errors='replace')
    all_urls = re.findall(r'<loc>(.*?)</loc>', content)

    qual_urls = []
    for url in all_urls:
        path = url.replace('http://www.shikakude.com', '').replace('https://www.shikakude.com', '')
        parts = path.strip('/').split('/')
        if parts and parts[0] in QUAL_DIRS:
            qual_urls.append(url)

    print(f'[sitemap] 総URL: {len(all_urls)} / 資格ページ: {len(qual_urls)}', flush=True)
    return qual_urls


# ---------------------------------------------------------------------------
# 個別ページ取得 + H1 タイトル + H2 セクション抽出
# ---------------------------------------------------------------------------
_TAG_RE = re.compile(r'<[^>]+>')
_ENTITY_NBSP = re.compile(r'&nbsp;')


def _strip_tags(s: str) -> str:
    s = _ENTITY_NBSP.sub(' ', s)
    s = _TAG_RE.sub('', s)
    return html_module.unescape(s).strip()


def fetch_page_data(url: str) -> dict | None:
    """1ページを取得し {title, sections, official_url} を返す。エラー時は None。"""
    req = urllib.request.Request(url, headers={'User-Agent': USER_AGENT})
    try:
        raw = urllib.request.urlopen(req, timeout=30).read()
        html = raw.decode('shift_jis', errors='replace')
    except Exception as exc:
        print(f'  [ERROR] {url} -> {exc}', flush=True)
        return None

    # H1 タイトル取得
    h1_m = re.search(r'<h1[^>]*>(.*?)</h1>', html, re.S)
    title = _strip_tags(h1_m.group(1)) if h1_m else ''
    title = re.sub(r'\s+', ' ', title).strip()

    # 公式 URL 抽出 (外部リンクを探す)
    official_url = ''
    official_pat = re.compile(
        r'href="(https?://(?!(?:www\.shikakude\.com|www\.google\.|www\.amazon\.|www\.rakuten\.|ad\.|affiliate\.|track\.)[^"]*)[^"]{10,})"',
        re.I
    )
    # main コンテンツ内のみ
    main_start = html.find('<div id="main">')
    main_end = html.find('</div><!-- main -->', main_start) if main_start >= 0 else len(html)
    main_html = html[main_start:main_end] if main_start >= 0 else html
    for m in official_pat.finditer(main_html):
        href = m.group(1)
        # 省庁・試験機関系を優先
        if re.search(r'\.(go\.jp|or\.jp|ac\.jp|org)', href):
            official_url = href
            break
    if not official_url:
        # 最初の外部リンク
        m = official_pat.search(main_html)
        if m:
            official_url = m.group(1)

    # H2 セクション抽出 (main コンテンツ内)
    sections = _extract_sections(main_html)

    return {
        'title': title,
        'sections': sections,
        'official_url': official_url,
    }


def _extract_sections(html: str) -> list[dict]:
    """HTML から H2 単位のセクションを抽出する。"""
    # タグレベルで分割
    block_pat = re.compile(
        r'(<h2[^>]*>.*?</h2>|<h3[^>]*>.*?</h3>|<p[^>]*>.*?</p>|<li[^>]*>.*?</li>|<td[^>]*>.*?</td>)',
        re.S
    )

    sections: list[dict] = []
    current_h2: str | None = None
    body_parts: list[str] = []

    for part in block_pat.split(html):
        part = part.strip()
        if not part:
            continue
        if re.match(r'<h2', part):
            # 前セクションを確定
            if current_h2 is not None:
                body = '\n'.join(body_parts).strip()
                if body:
                    sections.append({'h2': current_h2, 'body': body})
            current_h2 = _strip_tags(part)
            current_h2 = re.sub(r'\s+', ' ', current_h2).strip()
            body_parts = []
        elif re.match(r'<h3', part):
            t = _strip_tags(part)
            t = re.sub(r'\s+', ' ', t).strip()
            if t:
                body_parts.append(f'### {t}')
        else:
            t = _strip_tags(part)
            t = re.sub(r'\s+', ' ', t).strip()
            if t and len(t) > 1:
                body_parts.append(t)

    # 最後のセクション
    if current_h2 is not None:
        body = '\n'.join(body_parts).strip()
        if body:
            sections.append({'h2': current_h2, 'body': body})

    return sections


# ---------------------------------------------------------------------------
# マッチング: shikakude 全資格 x JILPT 未マッチリスト
# ---------------------------------------------------------------------------
def build_title_map(url_title_pairs: list[tuple[str, str]]) -> dict[str, tuple[str, str]]:
    """正規化キー -> (url, original_title) マップを構築。"""
    result: dict[str, tuple[str, str]] = {}
    for url, title in url_title_pairs:
        base = strip_level(title)
        key = normalize(base)
        if key and key not in result:
            result[key] = (url, title)
    return result


def match_qualifications(
    shika_map: dict[str, tuple[str, str]],
    unmatched: list[str],
) -> list[dict]:
    """完全一致 + 部分一致でマッチング結果リストを返す。"""
    exact: list[dict] = []
    substr: list[dict] = []

    for jname in unmatched:
        base = strip_level(jname)
        key = normalize(base)

        if key in shika_map:
            url, stitle = shika_map[key]
            exact.append({
                'jilpt_name': jname,
                'shikakude_url': url,
                'shikakude_title': stitle,
                'match_type': 'exact',
            })
        else:
            hits = [
                (url, stitle)
                for sk, (url, stitle) in shika_map.items()
                if len(key) >= 3 and len(sk) >= 3 and (key in sk or sk in key)
            ]
            if hits:
                url, stitle = hits[0]
                substr.append({
                    'jilpt_name': jname,
                    'shikakude_url': url,
                    'shikakude_title': stitle,
                    'match_type': 'partial',
                })

    print(
        f'[match] 完全一致: {len(exact)}, 部分一致: {len(substr)}, 合計: {len(exact) + len(substr)}',
        flush=True
    )
    return exact + substr


# ---------------------------------------------------------------------------
# メイン処理
# ---------------------------------------------------------------------------
def main() -> None:
    sys.stdout.reconfigure(encoding='utf-8')

    base_dir = Path(__file__).resolve().parent.parent
    unmatched_path = base_dir / 'data' / 'generated' / 'unmatched_qualifications.json'
    out_path = base_dir / 'data' / 'generated' / 'shikakude_qualifications.json'
    out_path.parent.mkdir(parents=True, exist_ok=True)

    # 1. 未マッチリスト読み込み
    with open(unmatched_path, encoding='utf-8') as f:
        unmatched: list[str] = json.load(f)
    print(f'[input] JILPT 未マッチ: {len(unmatched)} 件', flush=True)

    # 2. sitemap から全資格 URL 収集
    qual_urls = fetch_sitemap_urls()
    time.sleep(1)

    # 3. 各ページの H1 タイトルを取得して title_map を構築
    #    (まず全タイトルを取得 -> マッチング -> 対象ページだけ全セクション取得)
    print(f'\n[phase1] 全 {len(qual_urls)} ページのタイトル取得中...', flush=True)

    # キャッシュ読み込み (再実行対応)
    cache_path = base_dir / 'data' / 'generated' / '_shikakude_title_cache.json'
    title_cache: dict[str, str] = {}
    if cache_path.exists():
        with open(cache_path, encoding='utf-8') as f:
            title_cache = json.load(f)
        print(f'[cache] タイトルキャッシュ: {len(title_cache)} 件', flush=True)

    url_title_pairs: list[tuple[str, str]] = []
    for i, url in enumerate(qual_urls):
        if url in title_cache:
            url_title_pairs.append((url, title_cache[url]))
            continue

        req = urllib.request.Request(url, headers={'User-Agent': USER_AGENT})
        try:
            raw = urllib.request.urlopen(req, timeout=30).read()
            html = raw.decode('shift_jis', errors='replace')
            h1_m = re.search(r'<h1[^>]*>(.*?)</h1>', html, re.S)
            title = _strip_tags(h1_m.group(1)) if h1_m else ''
            title = re.sub(r'\s+', ' ', title).strip()
        except Exception as exc:
            print(f'  [ERROR title] {url} -> {exc}', flush=True)
            title = ''

        title_cache[url] = title
        url_title_pairs.append((url, title))

        if (i + 1) % 50 == 0:
            print(f'  [{i+1}/{len(qual_urls)}] タイトル取得中...', flush=True)
            # キャッシュ保存 (途中経過)
            with open(cache_path, 'w', encoding='utf-8') as f:
                json.dump(title_cache, f, ensure_ascii=False, indent=2)

        time.sleep(1.0)

    # キャッシュ最終保存
    with open(cache_path, 'w', encoding='utf-8') as f:
        json.dump(title_cache, f, ensure_ascii=False, indent=2)
    print(f'[phase1] タイトル取得完了: {len(url_title_pairs)} 件', flush=True)

    # 4. マッチング
    shika_map = build_title_map(url_title_pairs)
    pairs = match_qualifications(shika_map, unmatched)

    if not pairs:
        print('[warn] マッチなし。JSON を空で出力。', flush=True)
        with open(out_path, 'w', encoding='utf-8') as f:
            json.dump([], f, ensure_ascii=False)
        return

    # 5. マッチしたページの全セクションを取得
    print(f'\n[phase2] {len(pairs)} 件のセクション取得中...', flush=True)

    # 既存 JSON キャッシュ
    existing: dict[str, dict] = {}
    if out_path.exists():
        with open(out_path, encoding='utf-8') as f:
            existing_list = json.load(f)
        existing = {item['shikakude_url']: item for item in existing_list}
        print(f'[cache] 既存セクションキャッシュ: {len(existing)} 件', flush=True)

    fetched_at = datetime.now(timezone.utc).strftime('%Y-%m-%dT%H:%M:%SZ')
    results: list[dict] = []
    failed: list[dict] = []

    for i, pair in enumerate(pairs):
        jname = pair['jilpt_name']
        url = pair['shikakude_url']
        stitle = pair['shikakude_title']

        if url in existing:
            cached = existing[url]
            results.append({
                'jilpt_name': jname,
                'shikakude_url': url,
                'shikakude_title': stitle,
                'match_type': pair['match_type'],
                'official_url': cached.get('official_url', ''),
                'sections': cached.get('sections', []),
                'fetched_at': cached.get('fetched_at', fetched_at),
            })
            print(f'[{i+1}/{len(pairs)}] CACHE {jname}', flush=True)
            continue

        print(f'[{i+1}/{len(pairs)}] GET {jname} <- {url}', flush=True)
        page_data = fetch_page_data(url)

        if page_data is None or not page_data['sections']:
            failed.append(pair)
            results.append({
                'jilpt_name': jname,
                'shikakude_url': url,
                'shikakude_title': stitle,
                'match_type': pair['match_type'],
                'official_url': page_data['official_url'] if page_data else '',
                'sections': [],
                'fetched_at': fetched_at,
            })
        else:
            results.append({
                'jilpt_name': jname,
                'shikakude_url': url,
                'shikakude_title': stitle,
                'match_type': pair['match_type'],
                'official_url': page_data['official_url'],
                'sections': page_data['sections'],
                'fetched_at': fetched_at,
            })

        time.sleep(1.0)

    # 6. 失敗分リトライ
    if failed:
        print(f'\n[retry] {len(failed)} 件リトライ中...', flush=True)
        still_failed: list[dict] = []
        for pair in failed:
            time.sleep(2.0)
            page_data = fetch_page_data(pair['shikakude_url'])
            for r in results:
                if r['shikakude_url'] == pair['shikakude_url'] and r['jilpt_name'] == pair['jilpt_name']:
                    if page_data and page_data['sections']:
                        r['sections'] = page_data['sections']
                        r['official_url'] = page_data['official_url']
                    else:
                        still_failed.append(pair)
                    break
        if still_failed:
            print(f'[retry] 最終失敗: {len(still_failed)} 件', flush=True)
            for p in still_failed:
                print(f'  FAIL: {p["jilpt_name"]} {p["shikakude_url"]}', flush=True)

    # 7. 保存
    with open(out_path, 'w', encoding='utf-8') as f:
        json.dump(results, f, ensure_ascii=False, indent=2)

    success_count = sum(1 for r in results if r['sections'])
    print(f'\n[done] 保存先: {out_path}', flush=True)
    print(f'  総マッチ: {len(results)} 件', flush=True)
    print(f'  セクション取得成功: {success_count} 件', flush=True)
    print(f'  セクションなし: {len(results) - success_count} 件', flush=True)

    # サンプル表示
    print('\n--- サンプル (最初の3件) ---', flush=True)
    shown = 0
    for r in results:
        if not r['sections']:
            continue
        print(f"  jilpt_name      : {r['jilpt_name']}", flush=True)
        print(f"  shikakude_url   : {r['shikakude_url']}", flush=True)
        print(f"  shikakude_title : {r['shikakude_title']}", flush=True)
        print(f"  match_type      : {r['match_type']}", flush=True)
        print(f"  sections        : {[s['h2'] for s in r['sections']]}", flush=True)
        print(flush=True)
        shown += 1
        if shown >= 3:
            break


if __name__ == '__main__':
    main()
