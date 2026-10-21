# 顧客向け応募者画面（/portal）の UI 試作を、本物の Google スプレッドシートにつなぐ橋渡しサーバー。
# この Mac の中だけで使うテスト用。Render や本番には出さない。
#
# 起動:
#   PORTAL_SA_KEY=/path/to/service_account.json  <google-auth と requests の入った python>  scripts/portal_dev/sheet_bridge.py
#   （サービスアカウントの鍵のパスは環境変数 PORTAL_SA_KEY で渡す。既定値は無い）
# 開き方:
#   http://localhost:5179/static/app/portal-preview.html?live=1#list
# 止め方:
#   起動したターミナルで Ctrl+C。バックグラウンドで動かしたときは
#   lsof -ti tcp:5180 | xargs kill
#
# 待ち受け: 127.0.0.1:5180。CORS は http://localhost:5179 だけ許す。
#   GET  /sheet?sid=&gid=   見出し（見出しの名前 → 列の位置）、応募者の行、プルダウンの選択肢、読んだ時刻、Drive の更新時刻
#   GET  /version?sid=      Drive の更新時刻（同じシートは 20 秒使い回す）
#   POST /cell {sid, gid, rowNumber, header, base, next}
#        その 1 マスを読み直し、base と違えば書かずに {kind:'conflict', latest}、同じなら書いて {kind:'saved', value}
#
# 鍵の中身・トークン・シートの値は、画面のログにもファイルにも出さない。ターミナルには「いつ・どの見出しを・保存/競合」だけを出す。
import json
import os
import sys
import threading
import time
from datetime import datetime, timezone
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.parse import parse_qs, quote, urlparse

import google.auth.transport.requests as gtr
import google.oauth2.service_account as gsa
import requests

PORT = 5180
ALLOWED_ORIGIN = 'http://localhost:5179'
SCOPES = ['https://www.googleapis.com/auth/spreadsheets', 'https://www.googleapis.com/auth/drive.metadata.readonly']
HEADER_ROW = 1  # 1 行目が見出し
FIRST_APPLICANT_ROW = 3  # 2 行目は記入例
WRITABLE_HEADERS = {'状況', '面接予定日', '時間', '備考', '次回アクション', '入社予定日'}
VERSION_TTL = 20.0

KEY_PATH = os.environ.get('PORTAL_SA_KEY', '')
if KEY_PATH == '' or not os.path.isfile(KEY_PATH):
    print('PORTAL_SA_KEY にサービスアカウントの鍵のパスを入れてください。', file=sys.stderr)
    sys.exit(2)

_creds = gsa.Credentials.from_service_account_file(KEY_PATH, scopes=SCOPES)
_creds_lock = threading.Lock()
_write_lock = threading.Lock()
_version_cache: dict[str, tuple[float, str]] = {}
_title_cache: dict[tuple[str, int], str] = {}


def now_iso() -> str:
    return datetime.now(timezone.utc).isoformat()


def log(msg: str) -> None:
    print(datetime.now().strftime('%H:%M:%S'), msg, flush=True)


def auth_headers() -> dict[str, str]:
    with _creds_lock:
        if not _creds.valid:
            _creds.refresh(gtr.Request())
        return {'Authorization': 'Bearer ' + str(_creds.token)}


class ApiError(Exception):
    def __init__(self, status: int, message: str):
        super().__init__(message)
        self.status = status
        self.message = message


def google_get(url: str, params: dict | None = None) -> dict:
    r = requests.get(url, params=params, headers=auth_headers(), timeout=30)
    if r.status_code != 200:
        raise ApiError(502, f'Google の読み取りに失敗しました（{r.status_code}）')
    return r.json()


def col_letter(idx: int) -> str:
    s = ''
    n = idx + 1
    while n > 0:
        n, rem = divmod(n - 1, 26)
        s = chr(65 + rem) + s
    return s


def drive_modified(sid: str, fresh: bool = False) -> str:
    hit = _version_cache.get(sid)
    if hit and not fresh and time.time() - hit[0] < VERSION_TTL:
        return hit[1]
    j = google_get(f'https://www.googleapis.com/drive/v3/files/{sid}', {'fields': 'modifiedTime', 'supportsAllDrives': 'true'})
    m = str(j.get('modifiedTime', ''))
    _version_cache[sid] = (time.time(), m)
    return m


def tab_title(sid: str, gid: int) -> str:
    key = (sid, gid)
    if key in _title_cache:
        return _title_cache[key]
    j = google_get(f'https://sheets.googleapis.com/v4/spreadsheets/{sid}', {'fields': 'sheets(properties(sheetId,title))'})
    for s in j.get('sheets', []):
        p = s.get('properties', {})
        _title_cache[(sid, int(p.get('sheetId', -1)))] = str(p.get('title', ''))
    if key not in _title_cache:
        raise ApiError(404, 'そのタブが見つかりません')
    return _title_cache[key]


def a1(title: str, rng: str) -> str:
    return "'" + title.replace("'", "''") + "'!" + rng


def read_header_map(sid: str, gid: int) -> tuple[str, dict[str, int], dict[str, int]]:
    """見出しの名前 → 列の位置（結合の左端）と、結合の幅。"""
    title = tab_title(sid, gid)
    j = google_get(
        f'https://sheets.googleapis.com/v4/spreadsheets/{sid}',
        {'ranges': a1(title, f'A{HEADER_ROW}:ZZ{HEADER_ROW}'), 'includeGridData': 'true',
         'fields': 'sheets(properties(sheetId),merges,data(rowData(values(formattedValue))))'},
    )
    sheet = next((s for s in j.get('sheets', []) if int(s['properties']['sheetId']) == gid), None)
    if sheet is None:
        raise ApiError(404, 'そのタブが見つかりません')
    cells = ((sheet.get('data') or [{}])[0].get('rowData') or [{}])[0].get('values') or []
    spans: dict[int, int] = {}
    for m in sheet.get('merges', []):
        if m.get('startRowIndex', 0) <= HEADER_ROW - 1 < m.get('endRowIndex', 0):
            spans[int(m.get('startColumnIndex', 0))] = int(m.get('endColumnIndex', 0)) - int(m.get('startColumnIndex', 0))
    positions: dict[str, int] = {}
    widths: dict[str, int] = {}
    for i, c in enumerate(cells):
        name = str(c.get('formattedValue', '') or '').strip()
        if name == '' or name in positions:
            continue
        positions[name] = i
        widths[name] = spans.get(i, 1)
    return title, positions, widths


def read_sheet(sid: str, gid: int) -> dict:
    title, positions, widths = read_header_map(sid, gid)
    j = google_get(
        f'https://sheets.googleapis.com/v4/spreadsheets/{sid}',
        {'ranges': a1(title, f'A{FIRST_APPLICANT_ROW}:ZZ'), 'includeGridData': 'true',
         'fields': 'sheets(properties(sheetId),data(startRow,rowData(values(formattedValue,dataValidation))))'},
    )
    sheet = next((s for s in j.get('sheets', []) if int(s['properties']['sheetId']) == gid), None)
    data = ((sheet or {}).get('data') or [{}])[0]
    start = int(data.get('startRow', FIRST_APPLICANT_ROW - 1))
    rows_out = []
    options: dict[str, list[str]] = {}
    for ri, rd in enumerate(data.get('rowData') or []):
        cells = rd.get('values') or []
        values: dict[str, str] = {}
        for name, pos in positions.items():
            v = ''
            for k in range(pos, pos + widths.get(name, 1)):
                if k < len(cells):
                    t = str(cells[k].get('formattedValue', '') or '')
                    if t != '':
                        v = t
                        break
            values[name] = v
            if pos < len(cells):
                dv = cells[pos].get('dataValidation')
                cond = (dv or {}).get('condition') or {}
                if cond.get('type') == 'ONE_OF_LIST':
                    lst = options.setdefault(name, [])
                    for x in cond.get('values', []):
                        u = str(x.get('userEnteredValue', ''))
                        if u not in lst:
                            lst.append(u)
        if any(v.strip() != '' for v in values.values()):
            rows_out.append({'rowNumber': start + ri + 1, 'values': values})
    return {
        'headers': positions,
        'rows': rows_out,
        'options': options,
        'readAt': now_iso(),
        'modifiedTime': drive_modified(sid, fresh=True),
    }


def read_cell(sid: str, title: str, col: int, row: int) -> str:
    j = google_get(f'https://sheets.googleapis.com/v4/spreadsheets/{sid}/values/{quote(a1(title, col_letter(col) + str(row)), safe="")}')
    vals = j.get('values') or [[]]
    return str((vals[0] or [''])[0]) if vals and vals[0] else ''


def write_cell(sid: str, gid: int, row: int, header: str, base: str, nxt: str) -> dict:
    if header not in WRITABLE_HEADERS:
        raise ApiError(403, 'この項目は画面から書けません')
    if row < FIRST_APPLICANT_ROW:
        raise ApiError(403, 'この行には書けません')
    title, positions, _ = read_header_map(sid, gid)
    if header not in positions:
        raise ApiError(404, 'その見出しがシートにありません')
    col = positions[header]
    with _write_lock:
        current = read_cell(sid, title, col, row)
        if current != base:
            log(f'「{header}」 競合（書かなかった）')
            return {'kind': 'conflict', 'latest': current}
        rng = a1(title, col_letter(col) + str(row))
        r = requests.put(
            f'https://sheets.googleapis.com/v4/spreadsheets/{sid}/values/{quote(rng, safe="")}',
            params={'valueInputOption': 'RAW'}, headers=auth_headers(), json={'range': rng, 'values': [[nxt]]}, timeout=30,
        )
        if r.status_code != 200:
            log(f'「{header}」 保存の失敗（{r.status_code}）')
            raise ApiError(502, f'シートに保存できませんでした（{r.status_code}）')
        _version_cache.pop(sid, None)
        log(f'「{header}」 保存')
        return {'kind': 'saved', 'value': nxt}


class Handler(BaseHTTPRequestHandler):
    def log_message(self, format: str, *args) -> None:  # noqa: A002 - 標準のアクセスログは出さない
        return

    def cors(self) -> None:
        if self.headers.get('Origin') == ALLOWED_ORIGIN:
            self.send_header('Access-Control-Allow-Origin', ALLOWED_ORIGIN)
            self.send_header('Vary', 'Origin')
            self.send_header('Access-Control-Allow-Methods', 'GET, POST, OPTIONS')
            self.send_header('Access-Control-Allow-Headers', 'Content-Type')

    def reply(self, status: int, body: dict) -> None:
        raw = json.dumps(body, ensure_ascii=False).encode('utf-8')
        self.send_response(status)
        self.cors()
        self.send_header('Content-Type', 'application/json; charset=utf-8')
        self.send_header('Cache-Control', 'no-store')
        self.send_header('Content-Length', str(len(raw)))
        self.end_headers()
        self.wfile.write(raw)

    def origin_ok(self) -> bool:
        o = self.headers.get('Origin')
        return o is None or o == ALLOWED_ORIGIN

    def do_OPTIONS(self) -> None:  # noqa: N802
        self.send_response(204 if self.origin_ok() else 403)
        self.cors()
        self.end_headers()

    def do_GET(self) -> None:  # noqa: N802
        if not self.origin_ok():
            return self.reply(403, {'error': 'origin'})
        u = urlparse(self.path)
        q = {k: v[0] for k, v in parse_qs(u.query).items()}
        try:
            if u.path == '/sheet':
                return self.reply(200, read_sheet(q['sid'], int(q['gid'])))
            if u.path == '/version':
                return self.reply(200, {'modifiedTime': drive_modified(q['sid'])})
            return self.reply(404, {'error': 'not_found'})
        except (KeyError, ValueError):
            return self.reply(400, {'error': 'sid と gid が必要です'})
        except ApiError as e:
            return self.reply(e.status, {'error': e.message})
        except requests.RequestException:
            return self.reply(502, {'error': 'Google につながりません'})

    def do_POST(self) -> None:  # noqa: N802
        if not self.origin_ok():
            return self.reply(403, {'error': 'origin'})
        if urlparse(self.path).path != '/cell':
            return self.reply(404, {'error': 'not_found'})
        try:
            n = int(self.headers.get('Content-Length') or '0')
            b = json.loads(self.rfile.read(min(n, 100_000)).decode('utf-8'))
            res = write_cell(str(b['sid']), int(b['gid']), int(b['rowNumber']), str(b['header']), str(b['base']), str(b['next']))
            return self.reply(200, res)
        except (KeyError, ValueError, json.JSONDecodeError):
            return self.reply(400, {'error': '入力の形が違います'})
        except ApiError as e:
            return self.reply(e.status, {'error': e.message})
        except requests.RequestException:
            return self.reply(502, {'error': 'Google につながりません'})


if __name__ == '__main__':
    srv = ThreadingHTTPServer(('127.0.0.1', PORT), Handler)
    log(f'橋渡しサーバーを起動しました（http://127.0.0.1:{PORT}、止めるのは Ctrl+C）')
    try:
        srv.serve_forever()
    except KeyboardInterrupt:
        pass
