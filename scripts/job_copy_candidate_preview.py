"""Loopback-only preview of the existing built React job screen; never production auth.

The real dataset stays outside public assets. Rust /api/job-copy/moc owns deployed
authorization. This preview makes the same screen reviewable before integration.
"""
import argparse
from functools import partial
import html
import hashlib
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
import mimetypes
from pathlib import Path
from urllib.parse import unquote, urlsplit

ROOT = Path(__file__).resolve().parents[1]
PRIVATE_ROOT = ROOT
ASSETS = ROOT / 'static/app'
MAX_BYTES = 32 * 1024 * 1024


class PreviewHandler(BaseHTTPRequestHandler):
    def __init__(self, *args, dataset, image_results=None, **kwargs):
        self.dataset = Path(dataset)
        self.image_results = Path(image_results).resolve() if image_results else None
        super().__init__(*args, **kwargs)

    def log_message(self, *_):
        pass  # Do not log record selection/query values.

    def reply(self, status, content, mime='application/json'):
        self.send_response(status)
        self.send_header('Content-Type', mime)
        self.send_header('Content-Length', str(len(content)))
        self.send_header('Cache-Control', 'no-store')
        self.send_header('Referrer-Policy', 'no-referrer')
        self.send_header('X-Content-Type-Options', 'nosniff')
        self.end_headers()
        self.wfile.write(content)

    def do_GET(self):
        expected = f'127.0.0.1:{self.server.server_port}'
        if self.headers.get('Host') != expected or self.headers.get('Sec-Fetch-Site') == 'cross-site':
            self.reply(403, b'{"code":"preview_origin_denied"}')
            return
        path = unquote(urlsplit(self.path).path)
        if path == '/api/job-copy/image' and self.image_results is not None:
            # Preview only: exact URLs backed by files already downloaded/verified by Rust.
            # Deployed requests use Rust authorization and live HubSpot/Drive resolution.
            try:
                raw = self.image_results.read_bytes()
                if len(raw) > 128 * 1024:
                    raise ValueError()
                images = [image for row in json.loads(raw)['results'] for image in row['images']]
                matches = [image for image in images if image['url'] == self.path]
                if len(matches) != 1:
                    self.reply(404, b'{"code":"preview_image_not_mapped"}')
                    return
                image = matches[0]
                target = (PRIVATE_ROOT / image['path']).resolve()
                if not target.is_relative_to(self.image_results.parent) or image['mime'] not in ('image/jpeg', 'image/png', 'image/webp'):
                    raise ValueError()
                with target.open('rb') as source:
                    content = source.read(5 * 1024 * 1024 + 1)
                if len(content) > 5 * 1024 * 1024 or len(content) != image['bytes'] or hashlib.sha256(content).hexdigest() != image['sha256']:
                    raise ValueError()
                self.reply(200, content, image['mime'])
            except (OSError, ValueError, KeyError, TypeError):
                self.reply(503, b'{"code":"preview_image_unavailable"}')
        elif path == '/api/nav':
            # Synthetic navigation only. This harness does not provide production auth.
            item = {'id': 'job-copy', 'label': '求人文面', 'title': None, 'kind': 'app',
                    'href': '/app/job-copy', 'group': None, 'hidden': False,
                    'hidden_reason': None, 'hidden_since': None}
            nav = {'user_email': 'fixture@example.invalid', 'is_admin': False,
                   'header_links': [], 'groups': [], 'items': [item]}
            self.reply(200, json.dumps(nav, ensure_ascii=False).encode('utf-8'))
        elif path == '/api/job-copy/moc':
            try:
                with self.dataset.open('rb') as source:
                    raw = source.read(MAX_BYTES + 1)
                value = json.loads(raw)
                if len(raw) > MAX_BYTES or value.get('schemaVersion') != 1:
                    raise ValueError()
                self.reply(200, raw)
            except (OSError, ValueError, AttributeError):
                self.reply(503, b'{"code":"preview_dataset_unavailable"}')
        elif path in ('/', '/app/job-copy'):
            try:
                manifest = json.loads((ASSETS / '.vite/manifest.json').read_text(encoding='utf-8'))
                entry = manifest['src/entries/job-copy.tsx']
                def asset(name):
                    target = (ASSETS / name).resolve()
                    if not target.is_relative_to(ASSETS.resolve()) or not target.is_file():
                        raise ValueError()
                    return '/static/app/' + html.escape(name, quote=True)
                css_names = []
                visited = set()
                def collect(key):
                    if key in visited:
                        return
                    visited.add(key)
                    chunk = manifest[key]
                    for imported in chunk.get('imports', []):
                        collect(imported)
                    for name in chunk.get('css', []):
                        if name not in css_names:
                            css_names.append(name)
                collect('src/entries/job-copy.tsx')
                css = ''.join(f'<link rel="stylesheet" href="{asset(name)}">' for name in css_names)
                page = ('<!doctype html><html lang="ja"><head><meta charset="UTF-8">'
                        '<meta name="viewport" content="width=device-width,initial-scale=1">'
                        f'<title>実データ 求人管理 MOC</title>{css}</head><body>'
                        f'<div id="app-root"></div><script type="module" src="{asset(entry["file"])}"></script>'
                        '</body></html>').encode('utf-8')
                self.reply(200, page, 'text/html; charset=utf-8')
            except (OSError, ValueError, KeyError, TypeError):
                self.reply(503, b'{"code":"preview_build_required"}')
        elif path.startswith('/static/app/assets/'):
            target = (ASSETS / path.removeprefix('/static/app/')).resolve()
            if (not target.is_relative_to((ASSETS / 'assets').resolve())
                    or target.suffix not in ('.js', '.css', '.woff', '.woff2') or not target.is_file()):
                self.reply(404, b'{"code":"not_found"}')
                return
            self.reply(200, target.read_bytes(), mimetypes.guess_type(target.name)[0] or 'application/octet-stream')
        else:
            self.reply(404, b'{"code":"preview_route_unavailable"}')


def make_server(dataset, port=5184, image_results=None):
    return ThreadingHTTPServer(('127.0.0.1', port), partial(PreviewHandler, dataset=dataset, image_results=image_results))


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--dataset', type=Path, default=ROOT / 'data/job-copy-local/real-moc.json')
    parser.add_argument('--port', type=int, default=5184)
    parser.add_argument('--image-results', type=Path, help='Private Rust-verified image results for offline preview only')
    parser.add_argument('--private-root', type=Path, required=True, help='Read-only source workspace for private original paths')
    args = parser.parse_args()
    PRIVATE_ROOT = args.private_root.resolve()
    server = make_server(args.dataset, args.port, args.image_results)
    print(f'Preview: http://127.0.0.1:{server.server_port}/app/job-copy?data=actual', flush=True)
    server.serve_forever()
