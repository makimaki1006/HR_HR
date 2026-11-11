#!/usr/bin/env python3
"""トークの振り返り (headless-crm-design §22) が HubSpot に書くプロパティの定義を確認・作成する。

- 取引: アポの取引ごとのランク (talk_apo_rank_*)。アプリを再起動しても消えないように HubSpot に残す。
- 通話: 文字起こしの全文 (hrhr_transcript)。Zoom の録音が消えた後も読めるように HubSpot に残す。
- 既定は dry-run (何も作らない。あるか・定義が合っているかを表示するだけ)。--apply で不足分だけ作る。
- 既にある同名のプロパティは変更しない。定義が違えば、何も作らずに止まる (冪等)。
- レコードは読まない・書かない。必要な権限: crm.schemas.deals.read / write と通話のプロパティ定義の読み書き
"""
import argparse
import json
import os
import sys
import urllib.error
import urllib.request
from pathlib import Path

API = 'https://api.hubapi.com/crm/v3/properties/'

# 内部値は src/crm/talk_review/hs_store.rs の rank_value() と同じ
RANK_OPTIONS = [
    ('S', 'S'),
    ('A', 'A'),
    ('B', 'B'),
    ('C', 'C'),
    ('no_call', '電話なし'),
    ('unavailable', '取れない'),
]

DESIRED = {
    'deals': [
        {
            'name': 'talk_apo_rank', 'label': 'アポのランク', 'type': 'enumeration',
            'fieldType': 'select', 'groupName': 'dealinformation',
            'description': 'アポを取った電話のヒアリングの答えから決めたランク。電話が無い・録音が取れない取引も残す (HR_HR が書く)',
            'options': [
                {'label': label, 'value': value, 'displayOrder': i, 'hidden': False}
                for i, (value, label) in enumerate(RANK_OPTIONS)
            ],
        },
        {
            'name': 'talk_apo_rank_entered_at', 'label': 'アポのランク 対象のアポ日確定', 'type': 'datetime',
            'fieldType': 'date', 'groupName': 'dealinformation',
            'description': 'ランクを決めたアポ (アポ日確定に入った時刻)。この時刻が今のアポと違えば、HR_HR が決め直す',
        },
        {
            'name': 'talk_apo_rank_call_id', 'label': 'アポのランク アポを取った通話', 'type': 'string',
            'fieldType': 'text', 'groupName': 'dealinformation',
            'description': 'ランクを決めたアポを取った HubSpot の通話 (Call) の ID。電話なし・取れないときは空',
        },
    ],
    'calls': [
        {
            'name': 'hrhr_transcript', 'label': '文字起こし (HR_HR)', 'type': 'string',
            'fieldType': 'textarea', 'groupName': 'callinformation',
            'description': 'Zoom の録音の文字起こし。1 行 1 発言で「営業: 」「先方: 」から始まる (HR_HR が書く)',
        },
    ],
}


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


def request(key, obj, method, suffix='', body=None):
    req = urllib.request.Request(
        API + obj + suffix, method=method,
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
    existing, archived = {}, {}
    # 全部を検査してから作る。既存の項目は変更・復元しない
    for obj, props in DESIRED.items():
        existing[obj] = {p['name']: p for p in request(key, obj, 'GET')['results']}
        archived[obj] = {p['name'] for p in request(key, obj, 'GET', '?archived=true')['results']}
        for p in props:
            name = p['name']
            if name in archived[obj] or (name in existing[obj] and not compatible(existing[obj][name], p)):
                raise RuntimeError(f'{obj}.{name} に同名の異なる定義 (またはアーカイブ済み) があります。何も作らず停止します')
    for obj, props in DESIRED.items():
        for p in props:
            name = p['name']
            action = '既存の定義と一致' if name in existing[obj] else ('未作成 (dry-run)' if not args.apply else '作成')
            if name not in existing[obj] and args.apply:
                created = request(key, obj, 'POST', body=p)
                if not compatible(created, p):
                    raise RuntimeError(f'{obj}.{name} の作成結果を確認できませんでした')
                action = '作成済み'
            print(json.dumps({'object': obj, 'name': name, 'type': p['type'], 'fieldType': p['fieldType'],
                              'result': action}, ensure_ascii=False))
    if not args.apply:
        print('dry-run です。作成するには --apply を付けて再実行してください')


if __name__ == '__main__':
    try:
        main()
    except RuntimeError as e:
        print(str(e), file=sys.stderr)
        sys.exit(1)
