"""
u-can.co.jp の全資格講座ページをスクレイピングして
JILPT 未マッチ資格とマッチングし、
data/generated/ucan_qualifications.json に保存する。

サイト構造:
  - 全コースは /course/data/in_html/{id}/ で管理
  - 試験情報は /course/data/in_html/{id}/exam/index.html
  - サイトマップ + カテゴリページから 164 件のコースIDを収集

マッチング:
  - JILPT 未マッチリスト (509件) x u-can 全コース (164件)
  - fetch_brushup_qualifications.py の strip_level() + normalize() を流用
"""

import json
import re
import time
import unicodedata
import urllib.request
from datetime import datetime, timezone
from pathlib import Path

# ---------------------------------------------------------------------------
# マッチングロジック (fetch_brushup_qualifications.py と同一)
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
# コースID収集
# ---------------------------------------------------------------------------
UA = 'Mozilla/5.0 (compatible; LicenseKarteResearcher/1.0)'
BASE = 'https://www.u-can.co.jp'


def _get(url: str, timeout: int = 30) -> str:
    req = urllib.request.Request(url, headers={'User-Agent': UA})
    resp = urllib.request.urlopen(req, timeout=timeout)
    return resp.read().decode('utf-8', errors='replace')


def collect_course_ids() -> set[str]:
    """サイトマップ + カテゴリページから全コースIDを収集する。"""
    ids: set[str] = set()

    # 1. サイトマップ
    content = _get(f'{BASE}/sitemap.xml')
    sm_urls = re.findall(r'<loc>(.*?)</loc>', content)
    for u in sm_urls:
        m = re.match(r'https://www\.u-can\.co\.jp/course/data/in_html/(\d+)/', u.strip())
        if m:
            ids.add(m.group(1))
    print(f'[sitemap] {len(ids)} 件')

    # 2. カテゴリ・一覧ページ (追加ID)
    list_pages = [
        f'{BASE}/course/kouza/shikaku.html',
        f'{BASE}/course/category/welfare.html',
        f'{BASE}/course/category/medical.html',
        f'{BASE}/course/category/law.html',
        f'{BASE}/course/category/cooking.html',
        f'{BASE}/course/category/beauty.html',
        f'{BASE}/course/category/pc.html',
        f'{BASE}/course/category/design.html',
        f'{BASE}/course/category/handwriting.html',
    ]
    for lp in list_pages:
        time.sleep(1)
        try:
            html = _get(lp)
            new_ids = set(re.findall(r'/course/data/in_html/(\d+)/', html))
            before = len(ids)
            ids |= new_ids
            added = len(ids) - before
            if added:
                print(f'[list] {lp}: +{added}件')
        except Exception as e:
            print(f'[list] {lp}: ERROR {e}')

    print(f'[collect] 合計コースID: {len(ids)} 件')
    return ids


# ---------------------------------------------------------------------------
# 個別コースページ取得
# ---------------------------------------------------------------------------
def strip_tags(s: str) -> str:
    return re.sub(r'<[^>]+>', '', s)


def _clean(s: str) -> str:
    return re.sub(r'\s+', ' ', strip_tags(s)).strip()


def fetch_course_info(course_id: str) -> dict | None:
    """
    コースのメインページと exam/index.html から資格名・セクションを取得する。
    戻り値: {"title": str, "exam_url": str, "sections": [{"h2": str, "body": str}]}
    """
    main_url = f'{BASE}/course/data/in_html/{course_id}/'
    exam_url = f'{BASE}/course/data/in_html/{course_id}/exam/index.html'

    # メインページで資格タイトル取得
    try:
        html_main = _get(main_url)
    except Exception as e:
        print(f'  [ERROR] main {main_url}: {e}')
        return None

    # H1 からタイトル
    h1_m = re.search(r'<h1[^>]*>(.*?)</h1>', html_main, re.S)
    title = _clean(h1_m.group(1)) if h1_m else ''
    # "講座" を除去して資格名のみにする
    title_clean = re.sub(r'\s*講座$', '', title).strip()

    # title タグからも補完
    title_tag_m = re.search(r'<title>(.*?)</title>', html_main)
    title_tag = _clean(title_tag_m.group(1)) if title_tag_m else ''

    if not title_clean:
        # title タグから「通信講座」前後を取得
        m2 = re.match(r'^(.+?)(?:通信講座|の資格|の通信|講座)', title_tag)
        if m2:
            title_clean = m2.group(1).strip()
        else:
            title_clean = title_tag[:50]

    # exam ページからセクション取得
    sections: list[dict] = []
    try:
        html_exam = _get(exam_url)
        sections = _parse_sections(html_exam)
    except Exception:
        # exam ページがないコースはメインページをパース
        sections = _parse_sections(html_main)

    return {
        'title': title_clean,
        'main_url': main_url,
        'exam_url': exam_url,
        'sections': sections,
    }


def _parse_sections(html: str) -> list[dict]:
    """
    h2 を区切りにセクションリストを構築する。
    nav/header 等ノイズ H2 を除外する。

    注意: <p[^>]*> は <picture> にもマッチするため、タグ名の後に
    空白か > が続く形式 <p(?:\\s[^>]*)?> を使用する。
    """
    # ノイズ H2 パターン (ナビゲーション・フッター系)
    NOISE_H2 = re.compile(
        r'^(INDEX|おすすめコンテンツ|おすすめコラム|よくある質問|'
        r'(?:.+)おすすめ.*$|(?:.+)コラム.*$)',
        re.I,
    )

    # タグ名の後に必ず空白か > が続くパターンで誤マッチを防ぐ
    block_pat = re.compile(
        r'(<h2(?:\s[^>]*)?>.*?</h2>|<h3(?:\s[^>]*)?>.*?</h3>|'
        r'<p(?:\s[^>]*)?>.*?</p>|<li(?:\s[^>]*)?>.*?</li>|'
        r'<td(?:\s[^>]*)?>.*?</td>|<th(?:\s[^>]*)?>.*?</th>)',
        re.S,
    )
    parts = block_pat.split(html)

    sections: list[dict] = []
    current_h2: str | None = None
    body_parts: list[str] = []

    for part in parts:
        part = part.strip()
        if not part:
            continue
        if re.match(r'<h2', part):
            # 前のセクションを確定
            if current_h2 is not None and not NOISE_H2.match(current_h2):
                body_text = '\n'.join(body_parts).strip()
                if body_text and len(body_text) > 10:
                    sections.append({'h2': current_h2, 'body': body_text})
            current_h2 = _clean(part)
            body_parts = []
        elif re.match(r'<h3', part):
            t = _clean(part)
            if t:
                body_parts.append(f'### {t}')
        else:
            t = _clean(part)
            if t and len(t) > 2:
                body_parts.append(t)

    # 最後のセクション
    if current_h2 is not None and not NOISE_H2.match(current_h2):
        body_text = '\n'.join(body_parts).strip()
        if body_text and len(body_text) > 10:
            sections.append({'h2': current_h2, 'body': body_text})

    return sections


# ---------------------------------------------------------------------------
# マッチング
# ---------------------------------------------------------------------------
def build_ucan_name_map(course_list: list[dict]) -> dict[str, dict]:
    """
    正規化キー -> コース情報 のマップを構築する。
    """
    name_map: dict[str, dict] = {}
    for course in course_list:
        title = course['title']
        base = strip_level(title)
        key = normalize(base)
        if key and key not in name_map:
            name_map[key] = course
    return name_map


def match_jilpt(
    unmatched: list[str],
    name_map: dict[str, dict],
) -> list[dict]:
    """
    JILPT 未マッチ資格名と u-can コース名を突き合わせる。
    完全一致 > 部分一致 の順で評価。
    """
    exact: list[dict] = []
    substr: list[dict] = []
    fetched_at = datetime.now(timezone.utc).strftime('%Y-%m-%dT%H:%M:%SZ')

    for jname in unmatched:
        base = strip_level(jname)
        key = normalize(base)

        if key in name_map:
            c = name_map[key]
            exact.append({
                'jilpt_name': jname,
                'ucan_url': c['exam_url'],
                'ucan_title': c['title'],
                'match_type': 'exact',
                'sections': c['sections'],
                'fetched_at': fetched_at,
            })
        else:
            hits = [
                c for k, c in name_map.items()
                if len(key) >= 3 and len(k) >= 3 and (key in k or k in key)
            ]
            if hits:
                c = hits[0]
                substr.append({
                    'jilpt_name': jname,
                    'ucan_url': c['exam_url'],
                    'ucan_title': c['title'],
                    'match_type': 'partial',
                    'sections': c['sections'],
                    'fetched_at': fetched_at,
                })

    print(f'[match] 完全一致: {len(exact)}, 部分一致: {len(substr)}, 合計: {len(exact) + len(substr)}')
    return exact + substr


# ---------------------------------------------------------------------------
# メイン処理
# ---------------------------------------------------------------------------
def main() -> None:
    import sys
    sys.stdout.reconfigure(encoding='utf-8')

    base_dir = Path(__file__).resolve().parent.parent
    unmatched_path = base_dir / 'data' / 'generated' / 'unmatched_qualifications.json'
    out_path = base_dir / 'data' / 'generated' / 'ucan_qualifications.json'
    cache_path = base_dir / 'data' / 'generated' / '_ucan_course_cache.json'
    out_path.parent.mkdir(parents=True, exist_ok=True)

    # 1. JILPT 未マッチリスト読み込み
    with open(unmatched_path, encoding='utf-8') as f:
        unmatched: list[str] = json.load(f)
    print(f'[input] JILPT 未マッチ資格: {len(unmatched)} 件')

    # 2. 全コースID収集
    course_ids = collect_course_ids()

    # 3. 既存キャッシュ読み込み
    course_cache: dict[str, dict] = {}
    if cache_path.exists():
        with open(cache_path, encoding='utf-8') as f:
            course_cache = {c['course_id']: c for c in json.load(f)}
        print(f'[cache] 既存キャッシュ {len(course_cache)} 件ロード')

    # 4. 各コースページを取得 (1秒インターバル、キャッシュヒット時はスキップ)
    course_list: list[dict] = []
    failed_ids: list[str] = []
    sorted_ids = sorted(course_ids, key=int)

    for i, cid in enumerate(sorted_ids):
        if cid in course_cache:
            course_list.append(course_cache[cid])
            print(f'[{i+1}/{len(sorted_ids)}] CACHE id={cid}: {course_cache[cid]["title"]}')
            continue

        print(f'[{i+1}/{len(sorted_ids)}] GET id={cid} ...', end=' ', flush=True)
        info = fetch_course_info(cid)
        if info is None:
            failed_ids.append(cid)
            print('FAILED')
        else:
            info['course_id'] = cid
            course_list.append(info)
            course_cache[cid] = info
            print(f'OK: {info["title"]} ({len(info["sections"])} セクション)')
            # キャッシュを逐次保存
            with open(cache_path, 'w', encoding='utf-8') as f:
                json.dump(list(course_cache.values()), f, ensure_ascii=False, indent=2)

        time.sleep(1.0)

    print(f'\n[fetch] 取得成功: {len(course_list)}, 失敗: {len(failed_ids)}')
    if failed_ids:
        print(f'  失敗ID: {failed_ids}')

    # 5. マッチング
    name_map = build_ucan_name_map(course_list)
    print(f'[name_map] ユニーク正規化キー: {len(name_map)} 件')

    results = match_jilpt(unmatched, name_map)

    # 6. 保存
    with open(out_path, 'w', encoding='utf-8') as f:
        json.dump(results, f, ensure_ascii=False, indent=2)

    matched_with_sections = sum(1 for r in results if r['sections'])
    print(f'\n[done] 出力: {out_path}')
    print(f'  マッチ件数: {len(results)} / {len(unmatched)} '
          f'(マッチ率: {len(results)/len(unmatched)*100:.1f}%)')
    print(f'  セクション取得成功: {matched_with_sections} 件')
    print(f'  セクションなし: {len(results) - matched_with_sections} 件')

    # サンプル表示
    print('\n--- サンプル (最初の3件) ---')
    shown = 0
    for r in results:
        if not r['sections']:
            continue
        print(f"  jilpt_name : {r['jilpt_name']}")
        print(f"  ucan_url   : {r['ucan_url']}")
        print(f"  ucan_title : {r['ucan_title']}")
        print(f"  match_type : {r['match_type']}")
        h2s = [s['h2'] for s in r['sections']]
        print(f"  h2 一覧    : {h2s}")
        print()
        shown += 1
        if shown >= 3:
            break


if __name__ == '__main__':
    main()
