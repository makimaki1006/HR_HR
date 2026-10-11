#!/usr/bin/env python3
"""求人の案用プロパティ定義だけを確認・作成する。レコード用APIは呼ばない。"""
import argparse
import json
import os
from pathlib import Path
import sys
import urllib.error
import urllib.request

ROOT = Path(__file__).resolve().parents[1]
API = 'https://api.hubapi.com/crm/v3/properties/0-420'

def token():
    value = os.environ.get('HUBSPOT_ACCESS_TOKEN', '').strip()
    if value:
        return value
    path = Path('/Users/s_fujimaki/Downloads/env')
    if path.is_file():
        for line in path.read_text().splitlines():
            name, sep, value = line.removeprefix('export ').partition('=')
            if sep and name.strip() == 'HUBSPOT_ACCESS_TOKEN':
                return value.strip().strip('\"\'')
    raise RuntimeError('HubSpotのトークンが未取得です')

def request(key, method, suffix='', body=None):
    req = urllib.request.Request(API + suffix, method=method,
        headers={'Authorization': 'Bearer ' + key, 'Content-Type': 'application/json'},
        data=None if body is None else json.dumps(body, ensure_ascii=False).encode())
    try:
        with urllib.request.urlopen(req, timeout=30) as response:
            return json.load(response)
    except urllib.error.HTTPError as e:
        # 認証ヘッダーや応答本文をログに出さない。
        raise RuntimeError(f'プロパティ定義のAPIが失敗しました（応答番号 {e.code}）') from None
    except (urllib.error.URLError, TimeoutError):
        raise RuntimeError('プロパティ定義のAPIへ接続できませんでした') from None

def compatible(found, wanted):
    if any(found.get(k) != wanted[k] for k in ['type','fieldType','groupName']):
        return False
    return sorted((o['value'],o['label']) for o in found.get('options',[])) == sorted((o['value'],o['label']) for o in wanted.get('options',[]))

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--apply',action='store_true',help='不足しているプロパティの定義のみを作成')
    args=parser.parse_args()
    key=token()
    desired=json.loads((ROOT/'src/job_gen/draft_properties.json').read_text())
    group=request(key,'GET','/groups/tab_p_1_kyuujinjouhou')
    if group.get('name') != 'tab_p_1_kyuujinjouhou':
        raise RuntimeError('求人情報のグループを確認できませんでした')
    existing={p['name']:p for p in request(key,'GET')['results']}
    archived={p['name']:p for p in request(key,'GET','?archived=true')['results']}
    # 全項目を検査してから作る。既存項目の変更や復元は行わない。
    for p in desired:
        name=p['name']
        if name in archived or (name in existing and not compatible(existing[name],p)):
            raise RuntimeError(f'{name} に同名の異なる定義があります。変更せず停止します')
    for p in desired:
        name=p['name']; action='既存の定義と一致' if name in existing else '未作成'
        if name not in existing and args.apply:
            created=request(key,'POST',body=p)
            if not compatible(created,p):
                raise RuntimeError(f'{name} の作成結果を確認できませんでした')
            action='作成済み'
        print(json.dumps({'name':name,'type':p['type'],'fieldType':p['fieldType'],'options':p.get('options',[]),'result':action},ensure_ascii=False))

if __name__=='__main__':
    try:main()
    except (RuntimeError, OSError, ValueError, KeyError) as e:
        # この範囲に資格情報を含む例外の本文は渡さない。
        if isinstance(e,RuntimeError):print(str(e),file=sys.stderr)
        else:print('定義または設定ファイルを確認できませんでした',file=sys.stderr)
        sys.exit(1)
