#!/usr/bin/env python3
"""アポ取りの電話を、アポ取りトーク(apo-talk-v1.html)の項目で評価する試作。

HubSpot の取引・通話を渡すと、純正連携の通話記録の本文にある録音 URL から
Zoom の通話記録 ID を取り、Zoom の文字起こしを読んで、checks.json の項目ごとに
MiniMax で判定する。結果は複数の電話を並べた HTML で出す。

使い方:
  python3 scripts/call_advice/call_advice.py <取引ID|通話ID|HubSpotのURL> ...
  python3 scripts/call_advice/call_advice.py --deal 123 --deal 456 --call 789

HubSpot・Zoom は読み取りだけ。文字起こしと結果は --out (既定 ~/call_advice_reports) に置き、
リポジトリには入れない。鍵は --env (既定 ~/Downloads/env) から読む。
"""
import argparse, base64, datetime, html, json, os, pathlib, re, sys, time
import urllib.error, urllib.parse, urllib.request

HERE = pathlib.Path(__file__).parent
JST = datetime.timezone(datetime.timedelta(hours=9))
REC_RE = re.compile(r'/audio/play/\d+/(\d{8}-[0-9a-f-]{36})')


def load_env(path):
    env = {}
    for line in open(path, encoding='utf-8'):
        line = line.strip().replace('\r', '')
        if '=' in line and not line.startswith('#'):
            k, v = line.split('=', 1)
            env[k.strip()] = v.strip().strip('"').strip("'")
    return env


def http(url, headers=None, body=None, method=None, tries=4):
    data = json.dumps(body).encode() if isinstance(body, (dict, list)) else body
    h = dict(headers or {})
    if isinstance(body, (dict, list)):
        h['Content-Type'] = 'application/json'
    for i in range(tries):
        last = i == tries - 1
        try:
            with urllib.request.urlopen(urllib.request.Request(url, data=data, headers=h, method=method), timeout=300) as r:
                return json.loads(r.read() or b'{}')
        except urllib.error.HTTPError as e:
            if e.code not in (429, 500, 502, 503, 504) or last:
                raise RuntimeError(f'{e.code} {url.split("?")[0]}: {e.read()[:300].decode("utf-8", "replace")}')
        except (TimeoutError, urllib.error.URLError) as e:
            if last:
                raise RuntimeError(f'通信できない {url.split("?")[0]}: {e}')
        time.sleep(2 ** i + 1)


class HubSpot:
    def __init__(self, token):
        self.h = {'Authorization': 'Bearer ' + token}

    def get(self, path, **q):
        return http('https://api.hubapi.com' + path + ('?' + urllib.parse.urlencode(q, doseq=True) if q else ''), self.h)

    def assoc(self, frm, oid, to):
        out, after = [], None
        while True:
            q = {'limit': 500}
            if after:
                q['after'] = after
            r = self.get(f'/crm/v4/objects/{frm}/{oid}/associations/{to}', **q)
            out += [str(x['toObjectId']) for x in r.get('results', [])]
            after = r.get('paging', {}).get('next', {}).get('after')
            if not after:
                return out

    def calls(self, ids):
        props = ['hs_timestamp', 'hs_call_duration', 'hs_call_direction', 'hs_call_source', 'hs_call_body', 'hs_call_title', 'hubspot_owner_id']
        out = []
        for i in range(0, len(ids), 100):
            r = http('https://api.hubapi.com/crm/v3/objects/calls/batch/read', self.h,
                     {'inputs': [{'id': x} for x in ids[i:i + 100]], 'properties': props}, 'POST')
            out += r.get('results', [])
        return out

    def deal_name(self, did):
        try:
            return self.get(f'/crm/v3/objects/deals/{did}', properties='dealname')['properties'].get('dealname') or ''
        except RuntimeError:
            return ''


class Zoom:
    def __init__(self, env):
        b = base64.b64encode(f"{env['ZOOM_CLIENT_ID']}:{env['ZOOM_CLIENT_SECRET']}".encode()).decode()
        r = http('https://zoom.us/oauth/token?' + urllib.parse.urlencode({'grant_type': 'account_credentials', 'account_id': env['ZOOM_ACCOUNT_ID']}),
                 {'Authorization': 'Basic ' + b}, b'', 'POST')
        self.h = {'Authorization': 'Bearer ' + r['access_token']}

    def transcript(self, call_log_id):
        rec = http(f'https://api.zoom.us/v2/phone/call_logs/{call_log_id}/recordings', self.h)
        tx = http(f'https://api.zoom.us/v2/phone/recording_transcript/download/{rec["id"]}', self.h)
        return rec, tx


def is_rep(u):
    return bool(u) and not re.fullmatch(r'[\d+\-\s()]*', u) and u != 'Unknown Speaker'


def turns_from(tx):
    out = []
    for t in tx.get('timeline', []):
        u = (t.get('users') or [{}])[0].get('username') or t.get('username') or ''
        text = (t.get('text') or '').strip()
        if not text:
            continue
        sp = '営' if is_rep(u) else '先'
        if out and out[-1]['sp'] == sp:
            out[-1]['text'] += text
        else:
            out.append({'sp': sp, 'ts': t.get('ts', ''), 'text': text})
    for i, t in enumerate(out, 1):
        t['n'] = i
    return out


def build_prompt(checks, turns):
    items = '\n'.join(
        f'- id: {c["id"]}\n  項目: {c["title"]}\n  判定: {c["rule"]}\n  該当なし: {c["na_when"]}' for c in checks['checks'])
    tr = '\n'.join(f'{t["n"]}. {t["sp"]}: {t["text"]}' for t in turns)
    return f"""あなたは新規営業のアポ取り電話を評価する担当です。下の文字起こし(営=営業、先=先方。番号は発言の順番)を、項目ごとに判定してください。

決まり:
- アポが決まった後の会話(日時の復唱、Zoom・メールアドレスの確認など)は判定に使わない。決まったかどうかは outcome に書くだけにする。
- 判定は「できた」「できていない」「該当なし」のどれか。文字起こしが崩れていて判断できないときは「該当なし」にして note に理由を書く。
- evidence には判定の根拠になった発言の番号を入れる。quote にはその発言をそのまま短く引用する。
- advice は「できていない」ときだけ、その電話の流れに合わせて、営業が次に言うとよい一言を具体的に書く。資料の言い方を参考にする。
- 平易な日本語で書く。

項目:
{items}

文字起こし:
{tr}

次の JSON だけを返してください:
{{"reached_person": true/false (担当者と話せたか),
 "reception": true/false (受付を通したか),
 "first_date_turn": 営業が最初に具体的な日付を出した発言の番号 または null,
 "outcome": "日程が決まった" / "決まらなかった" / "担当者不在" のどれか,
 "checks": [{{"id": "...", "status": "...", "evidence": [番号], "quote": "...", "advice": "...", "note": "..."}}],
 "summary": "この電話で一番直すとよい点を1文で"}}"""


def extract_json(text):
    """<think> やコードブロックが付いていても JSON を取り出す。"""
    t = re.sub(r'<think>.*?</think>', '', str(text or ''), flags=re.S).strip()
    m = re.search(r'```(?:json)?\s*(.*?)```', t, re.S)
    if m:
        t = m.group(1).strip()
    i, j = t.find('{'), t.rfind('}')
    if i < 0 or j <= i:
        return None
    try:
        return json.loads(t[i:j + 1])
    except json.JSONDecodeError:
        return None


def judge(env, model, checks, turns):
    base = env.get('MINIMAX_BASE_URL') or 'https://api.minimax.io/v1'
    body = {'model': model, 'temperature': 0, 'max_tokens': 24000,
            'messages': [{'role': 'user', 'content': build_prompt(checks, turns) + '\n\n説明やコードブロックは付けず、JSON だけを出力してください。'}]}
    for i in range(4):
        r = http(base + '/chat/completions', {'Authorization': 'Bearer ' + env['MINIMAX_API_KEY']}, body=body, method='POST')
        obj = extract_json(r['choices'][0]['message'].get('content'))
        if obj is not None:
            return obj
    raise RuntimeError('判定の応答から JSON を取り出せない')


def parse_targets(args):
    deals, calls = list(args.deal), list(args.call)
    for x in args.targets:
        m = re.search(r'/record/0-3/(\d+)', x) or re.search(r'/deal/(\d+)', x)
        if m:
            deals.append(m.group(1)); continue
        m = re.search(r'/calls/\d+/review/(\d+)', x) or re.search(r'/record/0-48/(\d+)', x)
        if m:
            calls.append(m.group(1)); continue
        if x.isdigit():
            (calls if args.ids_are == 'call' else deals).append(x)
        else:
            sys.exit(f'読めない指定: {x}')
    return deals, calls


def collect_calls(hs, deals, calls, min_sec):
    found = {}  # call id -> deal id (or '')
    for d in deals:
        ids = set(hs.assoc('deals', d, 'calls'))
        for c in hs.assoc('deals', d, 'contacts'):
            ids |= set(hs.assoc('contacts', c, 'calls'))
        for c in ids:
            found.setdefault(c, d)
    for c in calls:
        found.setdefault(c, '')
    rows = []
    for r in hs.calls(list(found)):
        p = r['properties']
        dur = int(p.get('hs_call_duration') or 0) / 1000
        m = REC_RE.search(p.get('hs_call_body') or '')
        rows.append({'call_id': r['id'], 'deal_id': found.get(r['id'], ''), 'ts': p.get('hs_timestamp'), 'sec': dur,
                     'direction': p.get('hs_call_direction'), 'rec': m.group(1) if m else None,
                     'skip': None if (p.get('hs_call_direction') == 'OUTBOUND' and dur >= min_sec and m) else
                     ('着信' if p.get('hs_call_direction') != 'OUTBOUND' else f'{min_sec}秒未満' if dur < min_sec else '録音の URL が無い')})
    rows.sort(key=lambda x: x['ts'] or '')
    return rows


CSS = """:root{--bg:#f7f4ee;--ink:#1c1c1a;--sub:#5d5a54;--rule:#d8d2c6;--acc:#a8432a;--ok:#3b3a37}
body{margin:0;background:var(--bg);color:var(--ink);font:15px/1.75 "Hiragino Sans","Noto Sans JP",sans-serif}
main{max-width:1080px;margin:0 auto;padding:40px 16px 80px}
h1{font-family:"Hiragino Mincho ProN","Noto Serif JP",serif;font-size:26px;margin:0 0 6px;letter-spacing:.04em}
h2{font-family:"Hiragino Mincho ProN","Noto Serif JP",serif;font-size:19px;margin:44px 0 10px;padding-top:12px;border-top:1px solid var(--ink)}
.meta{color:var(--sub);font-size:13px}
.tbl{overflow-x:auto;margin:12px 0}
table{border-collapse:collapse;width:100%;font-size:13.5px;background:#fff}
th,td{border-bottom:1px solid var(--rule);padding:6px 8px;text-align:left;vertical-align:top}
th{font-weight:700;border-bottom:1px solid var(--ink);white-space:nowrap}
td.c{text-align:center;white-space:nowrap}
.ng{color:var(--acc);font-weight:700}.ok{color:var(--ok)}.na{color:#9a958c}
.base{color:var(--sub);font-size:12.5px}
.sum{margin:8px 0;padding-left:12px;border-left:3px solid var(--acc)}
details{margin:10px 0}summary{cursor:pointer;color:var(--sub);font-size:13px}
.tx{font-size:13px;background:#fff;border:1px solid var(--rule);padding:8px 12px;max-height:420px;overflow:auto}
.tx p{margin:2px 0}.tx .me{color:var(--acc)}.tx mark{background:#f1dfd6}
.skip{color:var(--sub);font-size:13px}"""

MARK = {'できた': ('○', 'ok'), 'できていない': ('×', 'ng'), '該当なし': ('−', 'na')}


def esc(s):
    return html.escape(str(s or ''))


def render(results, skipped, checks, model):
    cs = checks['checks']
    now = datetime.datetime.now(JST).strftime('%Y-%m-%d %H:%M')
    o = [f'<!doctype html><html lang="ja"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1"><title>架電の振り返り</title><style>{CSS}</style></head><body><main>',
         f'<h1>架電の振り返り</h1><p class="meta">{now} 作成 ｜ 判定: {esc(model)} ｜ 基準: アポ取りトーク(apo-talk-v1.html) ｜ {len(results)}本</p>',
         '<p>○ できた　× できていない　− 該当なし。「全体」は、分析した電話でできていなかった割合(07章 図5)。</p>']
    # 一覧
    o.append('<h2>電話の一覧</h2><div class="tbl"><table><tr><th>日時</th><th>秒</th><th>結果</th>' +
             ''.join(f'<th title="{esc(c["rule"])}">{esc(c["title"])}</th>' for c in cs) + '</tr>')
    o.append('<tr class="base"><td>全体でできていない</td><td></td><td></td>' + ''.join(
        f'<td class="c">{c["baseline"]["pct"]}%</td>' if c['baseline'] else '<td class="c">−</td>' for c in cs) + '</tr>')
    for r in results:
        j = {x['id']: x for x in r['judge'].get('checks', [])}
        cells = ''
        for c in cs:
            s = j.get(c['id'], {}).get('status', '該当なし')
            m, k = MARK.get(s, ('?', 'na'))
            cells += f'<td class="c {k}">{m}</td>'
        o.append(f'<tr><td><a href="#c{r["call_id"]}">{esc(r["when"])}</a></td><td class="c">{int(r["sec"])}</td><td>{esc(r["judge"].get("outcome"))}</td>{cells}</tr>')
    ng = {c['id']: sum(1 for r in results if any(x.get('id') == c['id'] and x.get('status') == 'できていない' for x in r['judge'].get('checks', []))) for c in cs}
    den = {c['id']: sum(1 for r in results if any(x.get('id') == c['id'] and x.get('status') in ('できた', 'できていない') for x in r['judge'].get('checks', []))) for c in cs}
    o.append('<tr><td><b>この中でできていない</b></td><td></td><td></td>' + ''.join(
        f'<td class="c">{ng[c["id"]]}/{den[c["id"]]}</td>' if den[c['id']] else '<td class="c">−</td>' for c in cs) + '</tr></table></div>')
    # 1本ずつ
    for r in results:
        jd = r['judge']
        hr = r['hour']
        tod = checks['time_of_day']
        tnote = '9時台' if hr in tod['good_hours'] else ('17時台。新規は9時台にかけたほうが話せる電話が多い' if hr in tod['bad_hours'] else f'{hr}時台')
        deal = f' ｜ 取引 {esc(r["deal_id"])} {esc(r["deal_name"])}' if r['deal_id'] else ''
        o.append(f'<h2 id="c{r["call_id"]}">{esc(r["when"])}　{int(r["sec"])}秒</h2>'
                 f'<p class="meta">通話 {esc(r["call_id"])}{deal} ｜ 営業 {esc(r["rep"])} ｜ かけた時間帯 {esc(tnote)} ｜ 結果 {esc(jd.get("outcome"))}</p>')
        if jd.get('summary'):
            o.append(f'<p class="sum">{esc(jd["summary"])}</p>')
        o.append('<div class="tbl"><table><tr><th>場面</th><th>項目</th><th>判定</th><th>根拠の発言</th><th>次に言うとよいこと</th></tr>')
        j = {x['id']: x for x in jd.get('checks', [])}
        for c in cs:
            x = j.get(c['id'], {})
            s = x.get('status', '該当なし')
            m, k = MARK.get(s, ('?', 'na'))
            ev = ', '.join(str(n) for n in x.get('evidence') or [])
            q = (f'{esc(ev)}: 「{esc(x.get("quote"))}」' if x.get('quote') else esc(ev)) + (f'<br><span class="base">{esc(x.get("note"))}</span>' if x.get('note') else '')
            adv = esc(x.get('advice')) if s == 'できていない' else ''
            if s == 'できていない' and c['baseline']:
                adv += f'<br><span class="base">全体でも{c["baseline"]["basis"]} {c["baseline"]["pct"]}% ができていない</span>'
            o.append(f'<tr><td>{esc(c["stage"])}</td><td>{esc(c["title"])}</td><td class="c {k}">{m} {esc(s)}</td><td>{q}</td><td>{adv}</td></tr>')
        o.append('</table></div>')
        marks = {n for x in jd.get('checks', []) if x.get('status') == 'できていない' for n in (x.get('evidence') or [])}
        lines = ''.join(f'<p class="{"me" if t["sp"] == "営" else ""}">{"<mark>" if t["n"] in marks else ""}{t["n"]}. {t["sp"]}: {esc(t["text"])}{"</mark>" if t["n"] in marks else ""}</p>' for t in r['turns'])
        o.append(f'<details><summary>文字起こし({len(r["turns"])}発言。できていない根拠に色)</summary><div class="tx">{lines}</div></details>')
    if skipped:
        o.append('<h2>評価しなかった通話</h2><div class="tbl"><table><tr><th>日時</th><th>通話</th><th>秒</th><th>理由</th></tr>')
        for s in skipped:
            o.append(f'<tr><td>{esc(s["when"])}</td><td>{esc(s["call_id"])}</td><td class="c">{int(s["sec"])}</td><td class="skip">{esc(s["skip"])}</td></tr>')
        o.append('</table></div>')
    o.append('</main></body></html>')
    return '\n'.join(o)


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument('targets', nargs='*', help='取引ID・通話ID・HubSpot の URL')
    ap.add_argument('--deal', action='append', default=[])
    ap.add_argument('--call', action='append', default=[])
    ap.add_argument('--ids-are', choices=['deal', 'call'], default='deal', help='数字だけの指定を取引と通話のどちらとみなすか')
    ap.add_argument('--since', help='この日以降の通話だけ (YYYY-MM-DD)')
    ap.add_argument('--env', default=os.path.expanduser('~/Downloads/env'))
    ap.add_argument('--out', default=os.path.expanduser('~/call_advice_reports'))
    ap.add_argument('--model', default='MiniMax-M3')
    ap.add_argument('--checks', default=str(HERE.parent.parent / 'src/crm/talk_review/checks.json'))
    ap.add_argument('--refresh', action='store_true', help='判定のキャッシュを使わずに判定し直す')
    a = ap.parse_args()
    deals, calls = parse_targets(a)
    if not deals and not calls:
        ap.error('取引か通話を1つ以上指定してください')
    env = load_env(a.env)
    checks = json.load(open(a.checks, encoding='utf-8'))
    out = pathlib.Path(a.out); (out / 'cache').mkdir(parents=True, exist_ok=True)
    hs = HubSpot(env['HUBSPOT_ACCESS_TOKEN'])
    rows = collect_calls(hs, deals, calls, checks['min_talk_sec'])
    if a.since:
        rows = [r for r in rows if (r['ts'] or '') >= a.since]
    names = {d: hs.deal_name(d) for d in deals}
    zoom = None
    results, skipped = [], []
    ver = hashlib_short(json.dumps(checks, ensure_ascii=False) + a.model)
    for r in rows:
        t = datetime.datetime.fromisoformat(r['ts'].replace('Z', '+00:00')).astimezone(JST) if r['ts'] else None
        r['when'] = t.strftime('%Y-%m-%d %H:%M') if t else ''
        r['hour'] = t.hour if t else None
        if r['skip']:
            skipped.append(r); continue
        txf = out / 'cache' / f'{r["rec"]}.tx.json'
        if txf.exists():
            rec, tx = json.load(open(txf, encoding='utf-8'))
        else:
            zoom = zoom or Zoom(env)
            try:
                rec, tx = zoom.transcript(r['rec'])
            except RuntimeError as e:
                r['skip'] = '録音・文字起こしを取れない(古くて消えた可能性)' if re.match(r'(404|400)', str(e)) else f'Zoom: {e}'
                skipped.append(r); continue
            json.dump([rec, tx], open(txf, 'w', encoding='utf-8'), ensure_ascii=False)
        r['rep'] = rec.get('caller_name') or ''
        r['turns'] = turns_from(tx)
        r['deal_name'] = names.get(r['deal_id'], '')
        if not any(t['sp'] == '先' for t in r['turns']) or not any(t['sp'] == '営' for t in r['turns']):
            r['skip'] = '文字起こしで営業と先方を分けられない'; skipped.append(r); continue
        jf = out / 'cache' / f'{r["rec"]}.{ver}.judge.json'
        if jf.exists() and not a.refresh:
            r['judge'] = json.load(open(jf, encoding='utf-8'))
        else:
            print(f'判定中 {r["when"]} {int(r["sec"])}秒', file=sys.stderr)
            r['judge'] = judge(env, a.model, checks, r['turns'])
            json.dump(r['judge'], open(jf, 'w', encoding='utf-8'), ensure_ascii=False, indent=1)
        results.append(r)
    name = datetime.datetime.now(JST).strftime('%Y%m%d-%H%M%S') + '.html'
    (out / name).write_text(render(results, skipped, checks, a.model), encoding='utf-8')
    print(out / name)


def hashlib_short(s):
    import hashlib
    return hashlib.sha1(s.encode()).hexdigest()[:8]


if __name__ == '__main__':
    main()
