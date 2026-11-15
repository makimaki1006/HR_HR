#!/usr/bin/env python3
"""競合調査の記録を求人 (0-420) に残すためのプロパティ competitor_* の定義を確認・作成する。

- 既定は dry-run (何も作らない。あるか・定義が合っているかを表示するだけ)。--apply で不足分だけ作る。
- 既にある同名のプロパティは変更しない。定義が違えば、何も作らずに止まる (冪等)。
- グループは 0-420 の既存のグループを読み、「システム情報」「改善管理」に当たるものを使う。無ければ止まる。
- 求人のレコードは読まない・書かない。必要な権限: 求人のプロパティ定義の読み取り (--apply は書き込みも)。
- 項目の名前と形は src/handlers/job_copy_live/competitor_record.rs と同じ。
"""
import argparse
import json
import os
import sys
import urllib.error
import urllib.request
from pathlib import Path

API = 'https://api.hubapi.com/crm/v3/properties/0-420'
# グループの内部名の終わり (表示名はローマ字のため内部名で探す)。「システム情報」「改善管理」のタブに当たる。
# 前例の項目 (Indeed の課金) が同じグループにあることも確かめる。違えば止まる。
SYSTEM_GROUP_WORD = 'shisutemujouhou'
READABLE_GROUP_WORD = 'kaizenkanri'
SYSTEM_GROUP_SAMPLE = 'indeed_weekly_perf'
READABLE_GROUP_SAMPLE = 'indeed_cost_last4w'

SYSTEM = [
    {
        'name': 'competitor_monthly', 'label': '競合調査 月ごとの記録(システム用)', 'type': 'string',
        'fieldType': 'textarea',
        'description': '競合調査の月ごとの記録 (JSON の配列)。案件管理が読む。同じ月は置き換え (HR_HR が書く。手で直さない)',
    },
]
READABLE = [
    {
        'name': 'competitor_last_date', 'label': '競合調査 最後に調べた日', 'type': 'date',
        'fieldType': 'date',
        'description': '最後に競合調査を記録した日 (HR_HR が書く)',
    },
    {
        'name': 'competitor_count', 'label': '競合調査 競合の件数', 'type': 'number',
        'fieldType': 'number',
        'description': '最後の競合調査の、競合の求人の件数 (HR_HR が書く)',
    },
    {
        'name': 'competitor_salary_median', 'label': '競合調査 競合の給与の中央値(下限)', 'type': 'number',
        'fieldType': 'number',
        'description': '最後の競合調査の、競合の給与の下限の中央値。月給は万円、時給は円 (HR_HR が書く)',
    },
    {
        'name': 'competitor_salary_gap', 'label': '競合調査 自社の給与との差', 'type': 'number',
        'fieldType': 'number',
        'description': '自社の給与(下限) − 競合の給与の中央値(下限)。自社の給与が読めないときは空 (HR_HR が書く)',
    },
    {
        'name': 'competitor_summary', 'label': '競合調査 要約', 'type': 'string',
        'fieldType': 'text',
        'description': '最後の競合調査の要約 (件数・給与の中央値・自社の給与・人気の件数) (HR_HR が書く)',
    },
]


def token():
    value = os.environ.get('HUBSPOT_ACCESS_TOKEN', '').strip()
    if value:
        return value
    path = Path('/Users/s_fujimaki/Downloads/env')
    if path.is_file():
        for line in path.read_text().splitlines():
            name, sep, val = line.removeprefix('export ').partition('=')
            if sep and name.strip() == 'HUBSPOT_ACCESS_TOKEN':
                return val.strip().strip('"\'')
    raise RuntimeError('HUBSPOT_ACCESS_TOKEN が未設定です')


def request(key, method, suffix='', body=None):
    req = urllib.request.Request(
        API + suffix, method=method,
        headers={'Authorization': 'Bearer ' + key, 'Content-Type': 'application/json'},
        data=None if body is None else json.dumps(body, ensure_ascii=False).encode())
    try:
        with urllib.request.urlopen(req, timeout=30) as response:
            return json.load(response)
    except urllib.error.HTTPError as e:
        # 認証ヘッダーや応答本文は出さない
        raise RuntimeError(f'プロパティ定義の API が失敗しました (応答番号 {e.code})') from None
    except (urllib.error.URLError, TimeoutError):
        raise RuntimeError('プロパティ定義の API へ接続できませんでした') from None


def pick_group(groups, word):
    """内部名が word で終わるグループがちょうど 1 つならその内部名。0 個・2 個以上は止まる。"""
    found = [g for g in groups if (g.get('name') or '').endswith(word) and not g.get('archived')]
    if len(found) != 1:
        labels = '、'.join(sorted(g.get('name') or '' for g in found)) or 'なし'
        raise RuntimeError(f'「{word}」に当たるグループを 1 つに決められません (候補: {labels})。何も作らず停止します')
    return found[0]['name']


def compatible(found, wanted):
    return all(found.get(k) == wanted[k] for k in ('type', 'fieldType', 'groupName'))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--apply', action='store_true', help='不足しているプロパティだけを作成する (既定は dry-run)')
    args = parser.parse_args()
    key = token()
    groups = request(key, 'GET', '/groups')['results']
    existing = {p['name']: p for p in request(key, 'GET')['results']}
    archived = {p['name'] for p in request(key, 'GET', '?archived=true')['results']}
    system_group = pick_group(groups, SYSTEM_GROUP_WORD)
    readable_group = pick_group(groups, READABLE_GROUP_WORD)
    for sample, group in ((SYSTEM_GROUP_SAMPLE, system_group), (READABLE_GROUP_SAMPLE, readable_group)):
        if existing.get(sample, {}).get('groupName') != group:
            raise RuntimeError(f'前例の項目 {sample} がグループ {group} にありません。何も作らず停止します')
    print(json.dumps({'group_system': system_group, 'group_readable': readable_group}, ensure_ascii=False))
    desired = [dict(p, groupName=system_group) for p in SYSTEM] + [dict(p, groupName=readable_group) for p in READABLE]
    # 全部を検査してから作る。既存の項目は変更・復元しない
    for p in desired:
        name = p['name']
        if name in archived or (name in existing and not compatible(existing[name], p)):
            raise RuntimeError(f'{name} に同名の異なる定義 (またはアーカイブ済み) があります。何も作らず停止します')
    for p in desired:
        name = p['name']
        action = '既存の定義と一致' if name in existing else ('未作成 (dry-run)' if not args.apply else '作成')
        if name not in existing and args.apply:
            created = request(key, 'POST', body=p)
            if not compatible(created, p):
                raise RuntimeError(f'{name} の作成結果を確認できませんでした')
            action = '作成済み'
        print(json.dumps({'name': name, 'type': p['type'], 'fieldType': p['fieldType'],
                          'groupName': p['groupName'], 'result': action}, ensure_ascii=False))
    if not args.apply:
        print('dry-run です。作成するには --apply を付けて再実行してください')


if __name__ == '__main__':
    try:
        main()
    except RuntimeError as e:
        print(str(e), file=sys.stderr)
        sys.exit(1)
