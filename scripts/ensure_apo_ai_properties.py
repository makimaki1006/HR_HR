#!/usr/bin/env python3
"""アポの自動判定 (ADR-035) が書く取引のプロパティ apo_ai_* の定義を確認・作成する。

- 既定は dry-run (何も作らない。あるか・定義が合っているかを表示するだけ)。--apply で不足分だけ作る。
- 既にある同名のプロパティは変更しない。定義が違えば、何も作らずに止まる (冪等)。
- 取引のレコードは読まない・書かない。必要な権限: crm.schemas.deals.read / crm.schemas.deals.write
"""
import argparse
import json
import os
import sys
import urllib.error
import urllib.request
from pathlib import Path

API = 'https://api.hubapi.com/crm/v3/properties/deals'
GROUP = 'dealinformation'

# 内部値は src/crm/apo_judge/judge.rs の Grade::value() と worker.rs の GRADE_* と同じ
GRADE_OPTIONS = [
    ('warning', '警告'),
    ('normal', '通常'),
    ('attend', '同席推奨'),
    ('no_transcript', '文字起こしなし'),
    ('judge_failed', '判定失敗'),
]

DESIRED = [
    {
        'name': 'apo_ai_score', 'label': 'アポ判定 点数(0-5)', 'type': 'number',
        'fieldType': 'number', 'groupName': GROUP,
        'description': 'AI によるアポの受注確度の点数。5 条件のうち該当した数 (HR_HR が書く)',
    },
    {
        'name': 'apo_ai_grade', 'label': 'アポ判定 区分', 'type': 'enumeration',
        'fieldType': 'select', 'groupName': GROUP,
        'description': 'AI によるアポの区分。ワークフローの通知条件に使う (HR_HR が書く)',
        'options': [
            {'label': label, 'value': value, 'displayOrder': i, 'hidden': False}
            for i, (value, label) in enumerate(GRADE_OPTIONS)
        ],
    },
    {
        'name': 'apo_ai_reason', 'label': 'アポ判定 理由', 'type': 'string',
        'fieldType': 'textarea', 'groupName': GROUP,
        'description': '条件ごとの有無と根拠の引用、アポ化しない層、理由の短文 (HR_HR が書く)',
    },
    {
        'name': 'apo_ai_judged_at', 'label': 'アポ判定 日時', 'type': 'datetime',
        'fieldType': 'date', 'groupName': GROUP,
        'description': '判定した日時。空なら未判定 (HR_HR はこの有無で判定済みかを決める)',
    },
    {
        'name': 'apo_ai_call_id', 'label': 'アポ判定 対象の通話', 'type': 'string',
        'fieldType': 'text', 'groupName': GROUP,
        'description': '判定に使った HubSpot の通話 (Call) の ID',
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


def compatible(found, wanted):
    if any(found.get(k) != wanted[k] for k in ('type', 'fieldType')):
        return False
    have = sorted((o['value'], o['label']) for o in found.get('options', []))
    want = sorted((o['value'], o['label']) for o in wanted.get('options', []))
    return have == want


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--apply', action='store_true', help='不足しているプロパティだけを作成する (既定は dry-run)')
    args = parser.parse_args()
    key = token()
    existing = {p['name']: p for p in request(key, 'GET')['results']}
    archived = {p['name'] for p in request(key, 'GET', '?archived=true')['results']}
    # 全部を検査してから作る。既存の項目は変更・復元しない
    for p in DESIRED:
        name = p['name']
        if name in archived or (name in existing and not compatible(existing[name], p)):
            raise RuntimeError(f'{name} に同名の異なる定義 (またはアーカイブ済み) があります。何も作らず停止します')
    for p in DESIRED:
        name = p['name']
        action = '既存の定義と一致' if name in existing else ('未作成 (dry-run)' if not args.apply else '作成')
        if name not in existing and args.apply:
            created = request(key, 'POST', body=p)
            if not compatible(created, p):
                raise RuntimeError(f'{name} の作成結果を確認できませんでした')
            action = '作成済み'
        print(json.dumps({'name': name, 'type': p['type'], 'fieldType': p['fieldType'], 'result': action},
                         ensure_ascii=False))
    if not args.apply:
        print('dry-run です。作成するには --apply を付けて再実行してください')


if __name__ == '__main__':
    try:
        main()
    except RuntimeError as e:
        print(str(e), file=sys.stderr)
        sys.exit(1)
