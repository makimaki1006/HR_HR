"""
brush-up.jp の個別資格ページから情報を取得し、
data/generated/brushup_qualifications.json に保存する。

マッチング: JILPT 682件 x brush-up 432件
  完全一致 + 部分一致 = 173件のみ対象
"""
import asyncio
import json
import re
import time
import unicodedata
import urllib.request
from pathlib import Path

try:
    from bs4 import BeautifulSoup
    USE_BS4 = True
except ImportError:
    USE_BS4 = False

from libsql_client import create_client

# ---------------------------------------------------------------------------
# マッチングロジック (_tmp_brush_match2.py から流用)
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
# brush-up.jp 一覧取得 (432件)
# ---------------------------------------------------------------------------
def fetch_brushup_list() -> dict[str, str]:
    """brush-up.jp の資格一覧を取得し {url: name} を返す。"""
    INDEX_URL = 'https://www.brush-up.jp/genre/50index'
    req = urllib.request.Request(
        INDEX_URL,
        headers={'User-Agent': 'Mozilla/5.0 (compatible; LicenseKarteResearcher/1.0)'},
    )
    html = urllib.request.urlopen(req, timeout=30).read().decode('utf-8')
    pattern = re.compile(
        r'<a[^>]+href="(https://www\.brush-up\.jp/theme/[a-z_]+/[^"#?]+)"[^>]*>([^<]+)</a>'
    )
    brush_qual: dict[str, str] = {}
    for m in pattern.finditer(html):
        url = m.group(1)
        name = m.group(2).strip()
        rel = url.replace('https://www.brush-up.jp', '')
        if rel.count('/') != 3:
            continue
        if name and url not in brush_qual:
            brush_qual[url] = name
    print(f'[fetch_brushup_list] {len(brush_qual)} 件取得')
    return brush_qual


# ---------------------------------------------------------------------------
# JILPT 682件取得 (Turso)
# ---------------------------------------------------------------------------
async def fetch_jilpt_qualifications(env_path: str = '.env') -> dict[str, int]:
    """JILPT 資格名 -> 関連職業数 を Turso から取得する。"""
    e = dict(
        l.strip().split('=', 1)
        for l in open(env_path, encoding='utf-8')
        if '=' in l and not l.startswith('#')
    )
    url = e['TURSO_EXTERNAL_URL'].replace('libsql://', 'https://')
    async with create_client(url=url, auth_token=e['TURSO_EXTERNAL_TOKEN']) as c:
        rs = await c.execute(
            'SELECT name, COUNT(DISTINCT jobtag_id) AS n '
            'FROM v2_external_jobtag_qualifications GROUP BY name'
        )
    result = {r[0]: r[1] for r in rs.rows}
    print(f'[fetch_jilpt] {len(result)} 件取得')
    return result


# ---------------------------------------------------------------------------
# マッチング
# ---------------------------------------------------------------------------
def build_matched_pairs(
    brush_qual: dict[str, str],
    jilpt: dict[str, int],
) -> list[dict]:
    """完全一致 + 部分一致した (jilpt_name, brushup_url, brushup_name) リストを返す。"""
    # brush-up 側を正規化
    brush_norm: dict[str, tuple[str, str]] = {}  # key -> (url, original_name)
    for url, name in brush_qual.items():
        base = strip_level(name)
        key = normalize(base)
        if key and key not in brush_norm:
            brush_norm[key] = (url, name)

    exact: list[dict] = []
    substr: list[dict] = []
    for jname in jilpt:
        base = strip_level(jname)
        key = normalize(base)
        if key in brush_norm:
            bu, bo = brush_norm[key]
            exact.append({'jilpt_name': jname, 'brushup_url': bu, 'brushup_name': bo, 'match_type': 'exact'})
        else:
            hits = [
                (bu, bo)
                for bk, (bu, bo) in brush_norm.items()
                if len(key) >= 3 and len(bk) >= 3 and (key in bk or bk in key)
            ]
            if hits:
                bu, bo = hits[0]
                substr.append({'jilpt_name': jname, 'brushup_url': bu, 'brushup_name': bo, 'match_type': 'partial'})

    print(f'[match] 完全一致: {len(exact)}, 部分一致: {len(substr)}, 合計: {len(exact) + len(substr)}')
    return exact + substr


# ---------------------------------------------------------------------------
# 個別ページ取得 + セクション抽出
# ---------------------------------------------------------------------------
def fetch_page_sections(url: str) -> list[dict]:
    """1ページを取得し h2 単位のセクション一覧を返す。"""
    req = urllib.request.Request(
        url,
        headers={'User-Agent': 'Mozilla/5.0 (compatible; LicenseKarteResearcher/1.0)'},
    )
    try:
        html_bytes = urllib.request.urlopen(req, timeout=30).read()
        html = html_bytes.decode('utf-8', errors='replace')
    except Exception as exc:
        print(f'  [ERROR] GET {url} -> {exc}')
        return []

    if USE_BS4:
        return _parse_sections_bs4(html)
    else:
        return _parse_sections_regex(html)


def _parse_sections_bs4(html: str) -> list[dict]:
    soup = BeautifulSoup(html, 'html.parser')
    # 主コンテンツを探す (article, main, #contents 等)
    main = (
        soup.find('article')
        or soup.find('main')
        or soup.find(id=re.compile(r'content', re.I))
        or soup.body
    )
    if main is None:
        return []

    sections: list[dict] = []
    current_h2: str | None = None
    body_parts: list[str] = []

    for tag in main.find_all(['h2', 'h3', 'p', 'li', 'ul', 'ol', 'table']):
        if tag.name == 'h2':
            if current_h2 is not None:
                text = '\n'.join(body_parts).strip()
                if text:
                    sections.append({'h2': current_h2, 'body': text})
            current_h2 = tag.get_text(separator=' ', strip=True)
            body_parts = []
        elif tag.name == 'h3':
            t = tag.get_text(separator=' ', strip=True)
            if t:
                body_parts.append(f'### {t}')
        else:
            t = tag.get_text(separator=' ', strip=True)
            if t and len(t) > 1:
                body_parts.append(t)

    if current_h2 is not None:
        text = '\n'.join(body_parts).strip()
        if text:
            sections.append({'h2': current_h2, 'body': text})

    return sections


def _parse_sections_regex(html: str) -> list[dict]:
    """BeautifulSoup が使えない場合の簡易正規表現実装。"""
    # タグ除去
    def strip_tags(s: str) -> str:
        return re.sub(r'<[^>]+>', '', s)

    h2_pat = re.compile(r'<h2[^>]*>(.*?)</h2>', re.S)
    block_pat = re.compile(r'(<h2[^>]*>.*?</h2>|<h3[^>]*>.*?</h3>|<p[^>]*>.*?</p>|<li[^>]*>.*?</li>)', re.S)

    sections: list[dict] = []
    parts = block_pat.split(html)
    current_h2: str | None = None
    body_parts: list[str] = []
    for part in parts:
        if re.match(r'<h2', part):
            if current_h2 is not None:
                text = '\n'.join(body_parts).strip()
                if text:
                    sections.append({'h2': current_h2, 'body': text})
            current_h2 = strip_tags(part).strip()
            body_parts = []
        elif re.match(r'<h3', part):
            t = strip_tags(part).strip()
            if t:
                body_parts.append(f'### {t}')
        else:
            t = strip_tags(part).strip()
            if t and len(t) > 1:
                body_parts.append(t)
    if current_h2 is not None:
        text = '\n'.join(body_parts).strip()
        if text:
            sections.append({'h2': current_h2, 'body': text})
    return sections


# ---------------------------------------------------------------------------
# メイン処理
# ---------------------------------------------------------------------------
async def main() -> None:
    base_dir = Path(__file__).resolve().parent.parent
    out_path = base_dir / 'data' / 'generated' / 'brushup_qualifications.json'
    out_path.parent.mkdir(parents=True, exist_ok=True)

    # 1. 一覧取得
    brush_qual = fetch_brushup_list()

    # 2. JILPT 取得
    jilpt = await fetch_jilpt_qualifications(str(base_dir / '.env'))

    # 3. マッチング
    pairs = build_matched_pairs(brush_qual, jilpt)

    # 4. 既存 JSON があれば読み込み (再実行時のスキップ用)
    existing: dict[str, dict] = {}
    if out_path.exists():
        with open(out_path, encoding='utf-8') as f:
            existing_list = json.load(f)
        existing = {item['brushup_url']: item for item in existing_list}
        print(f'[cache] 既存 {len(existing)} 件ロード済み')

    # 5. 個別ページ取得 (1秒インターバル)
    results: list[dict] = []
    failed_urls: list[dict] = []

    for i, pair in enumerate(pairs):
        jname = pair['jilpt_name']
        url = pair['brushup_url']
        bname = pair['brushup_name']

        # キャッシュヒット
        if url in existing:
            cached = existing[url]
            results.append({
                'jilpt_name': jname,
                'brushup_url': url,
                'brushup_name': bname,
                'match_type': pair['match_type'],
                'sections': cached.get('sections', []),
            })
            print(f'[{i+1}/{len(pairs)}] CACHE {jname}')
            continue

        print(f'[{i+1}/{len(pairs)}] GET {jname} <- {url}')
        sections = fetch_page_sections(url)

        if not sections:
            failed_urls.append(pair)
            results.append({
                'jilpt_name': jname,
                'brushup_url': url,
                'brushup_name': bname,
                'match_type': pair['match_type'],
                'sections': [],
            })
        else:
            results.append({
                'jilpt_name': jname,
                'brushup_url': url,
                'brushup_name': bname,
                'match_type': pair['match_type'],
                'sections': sections,
            })

        time.sleep(1.0)

    # 6. 失敗URLをリトライ (1回)
    if failed_urls:
        print(f'\n[retry] {len(failed_urls)} 件リトライ中...')
        retry_again: list[dict] = []
        for pair in failed_urls:
            time.sleep(2.0)
            sections = fetch_page_sections(pair['brushup_url'])
            # results 内の対応エントリを更新
            for r in results:
                if r['brushup_url'] == pair['brushup_url'] and r['jilpt_name'] == pair['jilpt_name']:
                    if sections:
                        r['sections'] = sections
                    else:
                        retry_again.append(pair)
                    break
        print(f'[retry] リトライ後も失敗: {len(retry_again)} 件')
        for p in retry_again:
            print(f'  FAIL: {p["jilpt_name"]} {p["brushup_url"]}')

    # 7. 保存
    with open(out_path, 'w', encoding='utf-8') as f:
        json.dump(results, f, ensure_ascii=False, indent=2)

    success_count = sum(1 for r in results if r['sections'])
    print(f'\n[done] 保存先: {out_path}')
    print(f'  総エントリ: {len(results)} 件')
    print(f'  セクション取得成功: {success_count} 件')
    print(f'  セクションなし: {len(results) - success_count} 件')


if __name__ == '__main__':
    asyncio.run(main())
