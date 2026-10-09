#!/usr/bin/env node
// Fake Turso (libSQL HTTP "v2/pipeline") backed by an in-memory node:sqlite database, for the CRM write live E2E.
// Implements only what src/db/turso_http.rs sends: one `execute` + one `close` per request.
// Never talks to a real Turso. Node >= 23.11 (StatementSync.columns()).
//
// Controls (no auth): POST /_down {down: true|false}  -> pipeline answers 500 while down (audit unavailable)
//                     GET  /_health
//                     POST /_query {sql}  -> rows as objects (test inspection of the ledger)
import http from 'node:http';
import { DatabaseSync } from 'node:sqlite';

const portArg = process.argv.indexOf('--port');
const PORT = portArg >= 0 ? Number(process.argv[portArg + 1]) : 9411;
const db = new DatabaseSync(':memory:');
let down = false;

const toArg = a => {
  switch (a?.type) {
    case 'integer': return Number(a.value);
    case 'float': return Number(a.value);
    case 'text': return String(a.value);
    default: return null;
  }
};
const cell = v => {
  if (v === null || v === undefined) return { type: 'null' };
  if (typeof v === 'bigint' || Number.isInteger(v)) return { type: 'integer', value: String(v) };
  if (typeof v === 'number') return { type: 'float', value: v };
  return { type: 'text', value: String(v) };
};
function run(sql, args) {
  const stmt = db.prepare(sql);
  const cols = stmt.columns();
  if (cols.length > 0) {
    const rows = stmt.all(...args);
    return { cols: cols.map(c => ({ name: c.name })), rows: rows.map(r => cols.map(c => cell(r[c.name]))) };
  }
  const info = stmt.run(...args);
  return { cols: [], rows: [], affected_row_count: Number(info.changes) };
}
const readBody = req => new Promise(r => { const c = []; req.on('data', x => c.push(x)); req.on('end', () => r(Buffer.concat(c).toString('utf8'))); });
const send = (res, status, body) => {
  const b = Buffer.from(JSON.stringify(body));
  res.writeHead(status, { 'content-type': 'application/json', 'content-length': b.length });
  res.end(b);
};

http.createServer(async (req, res) => {
  try {
    if (req.url === '/_health') return send(res, 200, { ok: true });
    if (req.url === '/_down' && req.method === 'POST') { down = JSON.parse(await readBody(req)).down === true; return send(res, 200, { down }); }
    if (req.url === '/_query' && req.method === 'POST') {
      const { sql } = JSON.parse(await readBody(req));
      return send(res, 200, { rows: db.prepare(sql).all() });
    }
    if (req.url === '/v2/pipeline' && req.method === 'POST') {
      const body = JSON.parse(await readBody(req));
      if (down) return send(res, 500, { error: 'fake turso is down' });
      const stmt = body.requests?.[0]?.stmt ?? {};
      try {
        const result = run(stmt.sql ?? '', (stmt.args ?? []).map(toArg));
        return send(res, 200, { results: [{ type: 'ok', response: { type: 'execute', result } }, { type: 'ok', response: { type: 'close' } }] });
      } catch (e) {
        return send(res, 200, { results: [{ type: 'error', error: { message: String(e.message) } }, { type: 'ok', response: { type: 'close' } }] });
      }
    }
    return send(res, 404, { error: 'not found' });
  } catch (e) {
    return send(res, 500, { error: String(e.message) });
  }
}).listen(PORT, '127.0.0.1', () => console.log(`[fake_turso] listening on http://127.0.0.1:${PORT}`));
