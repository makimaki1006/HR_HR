import { createServer } from 'node:http';
import { fileURLToPath } from 'node:url';
import { createServer as createViteServer } from '../../frontend/node_modules/vite/dist/node/index.js';
import { fixtureJobs, fixtureHistory } from './job-copy-listings-fixture.mjs';
const polls = new Map();
const api = createServer((request, response) => {
  const url = new URL(request.url, 'http://localhost');
  response.setHeader('Content-Type', 'application/json');
  const send = (data, status = 200) => { response.writeHead(status); response.end(JSON.stringify(data)); };
  if (url.pathname === '/api/nav') return send({ user_email: 'review@example.invalid', is_admin: false, header_links: [], groups: [], items: [{ id: 'job-copy', label: '求人文面管理', title: null, kind: 'app', href: '/app/job-copy', group: null, hidden: false, hidden_reason: null, hidden_since: null }] });
  if (url.pathname.endsWith('/market')) return send({ titles: ['ドライバー', '看護師'], prefectures: ['大分県', '沖縄県', '東京都'], series: null });
  if (url.pathname.endsWith('/listings')) {
    const cookie = request.headers.cookie ?? '';
    if (cookie.includes('jc_preparing=')) {
      const count = polls.get(cookie) ?? 0; polls.set(cookie, count + 1);
      if (count < 2) return send({ status: 'preparing', total: null, index_built_at: null, listings: [], titles: [], offset: 0, next_offset: null, refreshing: true, refresh_failed: false });
    }
    let rows = fixtureJobs.filter(row => ['media', 'prefecture'].every(key => !url.searchParams.get(key) || row[key] === url.searchParams.get(key)) && (!url.searchParams.get('title') || row.category === url.searchParams.get('title')));
    if (url.searchParams.get('sort') === 'media') rows = rows.toSorted((a, b) => a.media.localeCompare(b.media));
    const offset = Number(url.searchParams.get('offset') ?? 0);
    return send({ status: 'ready', total: rows.length, index_built_at: '2026-10-10T00:00:00Z', listings: rows.slice(offset, offset + 50), titles: ['ドライバー', '看護師'], offset, next_offset: offset + 50 < rows.length ? offset + 50 : null, refreshing: false, refresh_failed: false });
  }
  const match = url.pathname.match(/\/listings\/(\d+)\/versions$/);
  if (match) return match[1] === '38' ? send({ code: 'synthetic_unavailable' }, 503) : send(fixtureHistory(match[1]));
  return send({ code: 'synthetic_unavailable' }, 503);
});
await new Promise(resolve => api.listen(5198, '127.0.0.1', resolve));
const vite = await createViteServer({ plugins: [{ name: 'review-shell', configureServer(server) { server.middlewares.use(async (request, response, next) => { if (!request.url?.startsWith('/app/job-copy')) return next(); const html = await server.transformIndexHtml(request.url, '<!doctype html><html lang="ja"><head><meta charset="UTF-8"><meta name="viewport" content="width=device-width, initial-scale=1.0"><title>求人文面管理・操作確認</title></head><body><div id="app-root"></div><script type="module" src="/src/entries/job-copy.tsx"></script></body></html>'); response.setHeader('Content-Type', 'text/html'); response.end(html); }); } }], root: fileURLToPath(new URL('../../frontend', import.meta.url)), server: { host: '127.0.0.1', port: 5197, strictPort: true, proxy: { '/api/nav': 'http://127.0.0.1:5198', '/api/job-copy': 'http://127.0.0.1:5198' } } });
await vite.listen();
const close = async () => { await vite.close(); api.close(); process.exit(); };
process.on('SIGTERM', close); process.on('SIGINT', close);
