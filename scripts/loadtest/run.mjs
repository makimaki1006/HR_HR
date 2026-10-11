#!/usr/bin/env node
// Load test driver for the CRM call screen (/app/crm) backend: N virtual users against the real
// rust_dashboard binary, with HubSpot and Google replaced by scripts/loadtest/fake_hubspot.mjs.
// No dependencies (Node >= 18). See scripts/loadtest/README.md.

import { execFileSync, spawn } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(HERE, '../..');

function parseArgs(argv) {
  const o = {};
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    if (!a.startsWith('--')) continue;
    const k = a.slice(2);
    const v = argv[i + 1] && !argv[i + 1].startsWith('--') ? argv[++i] : 'true';
    o[k] = v;
  }
  return o;
}
const A = parseArgs(process.argv.slice(2));
const num = (k, d) => (A[k] !== undefined ? Number(A[k]) : d);
const OPT = {
  users: num('users', 100),
  admins: num('admins', 10),
  duration: num('duration', 300), // seconds of scenario (rush + steady)
  rushSeconds: num('rush-seconds', 10),
  thinkMin: num('think-min', 30),
  thinkMax: num('think-max', 90),
  clientTimeoutMs: num('client-timeout-ms', 35_000), // same as the React screen (apiGet timeoutMs)
  appPort: num('app-port', 9418),
  fakePort: num('fake-port', 9400),
  spawn: A.spawn !== 'false',
  bin: A.bin ?? path.join(process.env.CARGO_TARGET_DIR ?? path.join(ROOT, 'target'), 'debug', 'rust_dashboard'),
  db: A.db ?? null,
  label: A.label ?? 'run',
  outDir: A.out ?? path.join(HERE, 'results'),
  seed: num('seed', 42),
  // 503 hubspot_busy (gateway): the user retries after busyRetryMin–Max s, at most busyRetries times per action
  busyRetries: num('busy-retries', 5),
  busyRetryMin: num('busy-retry-min', 3),
  busyRetryMax: num('busy-retry-max', 8),
  // extra env for the app, e.g. --app-env HUBSPOT_APP_SEARCH_PER_SEC=1,HUBSPOT_APP_RATE_PER_SEC=8
  appEnv: Object.fromEntries((A['app-env'] ?? '').split(',').filter(Boolean).map(kv => [kv.slice(0, kv.indexOf('=')), kv.slice(kv.indexOf('=') + 1)])),
  // passthrough to fake_hubspot
  fake: ['latency-ms', 'jitter-ms', 'limit-10s', 'limit-1s', 'search-per-sec', 'background-rps', 'deals', 'search-counts-general']
    .filter(k => A[k] !== undefined).flatMap(k => [`--${k}`, A[k]]),
};
const APP = `http://localhost:${OPT.appPort}`;
const FAKE = `http://127.0.0.1:${OPT.fakePort}`;

let rngState = OPT.seed >>> 0;
function rnd() { // mulberry32 (repeatable scenario)
  rngState = (rngState + 0x6d2b79f5) | 0;
  let t = Math.imul(rngState ^ (rngState >>> 15), 1 | rngState);
  t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
  return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
}
const uniform = (a, b) => a + (b - a) * rnd();
const sleep = ms => new Promise(r => setTimeout(r, Math.max(0, ms)));
const pct = (sorted, p) => (sorted.length ? sorted[Math.min(sorted.length - 1, Math.ceil((p / 100) * sorted.length) - 1)] : NaN);
const fmt = v => (Number.isFinite(v) ? (v >= 100 ? v.toFixed(0) : v.toFixed(1)) : '-');
const log = (...xs) => console.log(`[run ${new Date().toISOString().slice(11, 19)}]`, ...xs);

const pipelines = JSON.parse(fs.readFileSync(path.join(ROOT, 'frontend/src/generated/call_queue_pipelines.json'), 'utf8'));
const DEFAULT_PIPELINE = pipelines.default;
const DEFAULT_PROPS = 'deal_props=hubspot_owner_id%2Cbpo_13%2Cbpo_14%2Cbpo_20%2Cbpo_10%2Cbpo_3%2Cbpo_4%2Cbpo_32&contact_props=lastname%2Cfirstname%2Cphone&company_props=website';

// ---------------------------------------------------------------------------
// processes
// ---------------------------------------------------------------------------
const children = [];
function stopChildren() { for (const c of children) { try { c.kill('SIGTERM'); } catch { /* gone */ } } }
process.on('exit', stopChildren);
process.on('SIGINT', () => { stopChildren(); process.exit(130); });

async function waitFor(url, ms, what) {
  const deadline = Date.now() + ms;
  for (;;) {
    try { const r = await fetch(url); if (r.ok) return; } catch { /* starting */ }
    if (Date.now() > deadline) throw new Error(`${what} did not become ready: ${url}`);
    await sleep(300);
  }
}

const vuEmail = n => `lt-user${String(n).padStart(3, '0')}@f-a-c.co.jp`;

async function startProcesses(workDir) {
  const fakeLog = fs.openSync(path.join(workDir, 'fake_hubspot.log'), 'w');
  const fake = spawn(process.execPath, [path.join(HERE, 'fake_hubspot.mjs'), '--port', String(OPT.fakePort), ...OPT.fake], { stdio: ['ignore', fakeLog, fakeLog] });
  children.push(fake);
  await waitFor(`${FAKE}/_health`, 30_000, 'fake_hubspot');

  if (!fs.existsSync(OPT.bin)) throw new Error(`app binary not found: ${OPT.bin} (--bin)`);
  let db = OPT.db;
  if (!db) {
    db = path.join(workDir, 'hellowork.db');
    await new Promise((res, rej) => {
      const p = spawn(process.platform === 'win32' ? 'python' : 'python3', [path.join(ROOT, 'scripts/e2e/make_fixture_db.py'), db], { stdio: 'inherit' });
      p.on('exit', c => (c === 0 ? res() : rej(new Error(`make_fixture_db.py exit ${c}`))));
    });
  }
  const env = { ...process.env };
  for (const k of Object.keys(env)) {
    if (/^(GEMINI_|GOOGLE_|OPENAI_|ANTHROPIC_|HUBSPOT_|SLACK_|ZOOM_|CRM_|ADMIN_EMAILS|AUDIT_|.*TURSO.*)/.test(k)) delete env[k];
  }
  const admins = Array.from({ length: OPT.admins }, (_, n) => vuEmail(n)).join(',');
  const appLogPath = path.join(workDir, 'app.log');
  const appLog = fs.openSync(appLogPath, 'w');
  const app = spawn(OPT.bin, [], {
    cwd: ROOT,
    stdio: ['ignore', appLog, appLog],
    env: {
      ...env,
      PORT: String(OPT.appPort),
      AUTH_PASSWORD: `loadtest-${process.pid}`,
      ALLOWED_DOMAINS: 'f-a-c.co.jp',
      HELLOWORK_DB_PATH: db,
      ADMIN_EMAILS: admins,
      HUBSPOT_ACCESS_TOKEN: 'fake-loadtest-token',
      // main: needs the uncommitted before-main-base-url.patch; the gateway branch reads it natively
      HUBSPOT_BASE_URL: FAKE,
      GOOGLE_OIDC_CLIENT_ID: 'test-client',
      GOOGLE_OIDC_CLIENT_SECRET: 'test-secret',
      GOOGLE_OIDC_REDIRECT_URL: `${APP}/auth/google/callback`,
      GOOGLE_OIDC_HOSTED_DOMAIN: 'f-a-c.co.jp',
      GOOGLE_OIDC_DISCOVERY_URL_DEBUG: `${FAKE}/oidc/.well-known/openid-configuration`,
      RUST_LOG: process.env.RUST_LOG ?? 'info',
      ...OPT.appEnv,
    },
  });
  children.push(app);
  await waitFor(`${APP}/health`, 120_000, 'rust_dashboard');
  return { appLogPath };
}

// ---------------------------------------------------------------------------
// HTTP with a per-user cookie jar
// ---------------------------------------------------------------------------
class Jar {
  constructor() { this.c = new Map(); }
  take(res) {
    for (const sc of res.headers.getSetCookie?.() ?? []) {
      const [pair, ...attrs] = sc.split(';');
      const i = pair.indexOf('=');
      const k = pair.slice(0, i).trim(), v = pair.slice(i + 1).trim();
      const expired = attrs.some(a => /max-age=0\b/i.test(a.trim())) || v === '';
      if (expired) this.c.delete(k); else this.c.set(k, v);
    }
  }
  header() { return [...this.c].map(([k, v]) => `${k}=${v}`).join('; '); }
}

async function loginVu(n) {
  const jar = new Jar();
  const email = vuEmail(n);
  let r = await fetch(`${APP}/auth/google/login`, { redirect: 'manual' });
  jar.take(r);
  const authz = r.headers.get('location');
  if (r.status !== 303 && r.status !== 302) throw new Error(`login start ${r.status}`);
  r = await fetch(`${authz}&login_hint=${encodeURIComponent(email)}`, { redirect: 'manual' });
  const cb = r.headers.get('location');
  r = await fetch(cb, { redirect: 'manual', headers: { cookie: jar.header() } });
  jar.take(r);
  if (r.status !== 200) throw new Error(`callback ${r.status}: ${(await r.text()).slice(0, 200)}`);
  await r.text();
  r = await fetch(`${APP}/api/nav`, { headers: { cookie: jar.header() } });
  if (r.status !== 200) throw new Error(`/api/nav after login ${r.status}`);
  await r.text();
  return { n, email, jar, isAdmin: n < OPT.admins };
}

const records = []; // {vu, ep, phase, start, dur, status, kind, attempt, final, userDur}
let T0 = 0;
const isBusy = r => r.status === 503 && r.body?.error_kind === 'hubspot_busy';
/**
 * One user action. On 503 hubspot_busy the screen says "混み合っています。少し待ってから再試行" — the user waits a few
 * seconds and presses 再試行 (`retryPath`, e.g. the queue's reload sends fresh=1). Every attempt is recorded;
 * the last one carries `final: true` and `userDur` (first attempt start -> last attempt end).
 */
async function action(vu, ep, phase, urlPath, retryPath = urlPath) {
  const first = Date.now();
  for (let attempt = 0; ; attempt++) {
    const r = await callOnce(vu, ep, phase, attempt === 0 ? urlPath : retryPath, attempt);
    if (!isBusy(r) || attempt >= OPT.busyRetries) {
      r.rec.final = true;
      r.rec.userDur = Date.now() - first;
      return r;
    }
    await sleep(uniform(OPT.busyRetryMin, OPT.busyRetryMax) * 1000);
  }
}
const call = (vu, ep, phase, urlPath, retryPath) => action(vu, ep, phase, urlPath, retryPath);
async function callOnce(vu, ep, phase, urlPath, attempt) {
  const start = Date.now();
  const ctl = new AbortController();
  const timer = setTimeout(() => ctl.abort(), OPT.clientTimeoutMs);
  let status, body = null;
  try {
    const r = await fetch(`${APP}${urlPath}`, { headers: { cookie: vu.jar.header(), accept: 'application/json' }, signal: ctl.signal, redirect: 'manual' });
    status = r.status;
    const text = await r.text();
    if (text && (r.headers.get('content-type') ?? '').includes('json')) { try { body = JSON.parse(text); } catch { /* keep null */ } }
  } catch (e) {
    status = e.name === 'AbortError' ? 'client_timeout' : 'net_error';
  } finally {
    clearTimeout(timer);
  }
  const rec = { vu: vu.n, ep, phase, start: start - T0, dur: Date.now() - start, status, kind: body?.error_kind ?? null, attempt, final: false };
  records.push(rec);
  return { status, body, rec };
}

// ---------------------------------------------------------------------------
// scenario
// ---------------------------------------------------------------------------
const shared = { hot: [] }; // popular deals (first rows of the first queue page someone loaded)

function queuePath(f, cursor, fresh = false) {
  const p = new URLSearchParams();
  if (f.pipeline !== DEFAULT_PIPELINE) p.set('pipeline', f.pipeline);
  for (const s of f.stages) p.append('stage', s);
  if (f.owner) p.set('owner', f.owner);
  p.set('limit', '50');
  if (cursor) p.set('cursor', cursor);
  if (fresh) p.set('fresh', '1');
  return `/api/crm/call-queue?${p}`;
}
function randomFilters(vu) {
  const pl = rnd() < 0.8 ? DEFAULT_PIPELINE : 'default';
  const def = pipelines.pipelines.find(p => p.id === pl);
  const eligible = def.stages.filter(s => s.rule !== 'exclude').map(s => s.id);
  const stages = rnd() < 0.3 ? eligible.filter(() => rnd() < 0.3).slice(0, 3) : [];
  const owner = vu.isAdmin ? (rnd() < 0.3 ? 'me' : null) : (rnd() < 0.3 ? 'all' : null);
  return { pipeline: pl, stages, owner };
}

async function loadQueue(vu, st, phase, page2, reload = false) {
  // page 1: the screen's 再試行 / reload skips the server's 30 s cache (fresh=1); load-more does not
  const path1 = queuePath(st.filters, page2 ? st.cursor : null, !page2 && reload);
  const retry = queuePath(st.filters, page2 ? st.cursor : null, !page2);
  const r = await call(vu, page2 ? 'queue_page2+' : 'queue_page1', phase, path1, retry);
  if (r.status === 200 && r.body) {
    const ids = (r.body.items ?? []).map(x => x.deal_id);
    st.rows = page2 ? st.rows.concat(ids) : ids;
    st.cursor = r.body.next_cursor ?? null;
    if (!page2) st.idx = 0;
    if (shared.hot.length === 0 && ids.length >= 5) shared.hot = ids.slice(0, 5);
  } else if (r.status === 409 && !page2) {
    // owner_not_resolved: the screen asks to pick an owner; emulate choosing "all"
    st.filters.owner = 'all';
  }
  return r;
}

async function openDeal(vu, st, phase, id, fresh = false) {
  if (!id) return;
  st.lastDeal = { id, at: Date.now() };
  await call(vu, fresh ? 'workspace_fresh' : 'workspace', phase, `/api/crm/workspace/deals/${id}?${DEFAULT_PROPS}${fresh ? '&fresh=1' : ''}`);
}

function nextDealId(st) {
  if (!st.rows.length) return null; // no queue on screen: nothing to click
  if (shared.hot.length && rnd() < 0.2) return shared.hot[Math.floor(rnd() * shared.hot.length)];
  const id = st.rows[st.idx % st.rows.length];
  st.idx++;
  return id;
}

async function screenOpen(vu, st, phase) {
  // the React screen fires these in parallel on mount
  await Promise.all([
    call(vu, 'nav', phase, '/api/nav'),
    call(vu, 'metadata', phase, '/api/crm/metadata'),
    call(vu, 'property_catalog', phase, '/api/crm/property-catalog'),
    call(vu, 'queue_pipelines', phase, '/api/crm/call-queue/pipelines'),
    call(vu, 'owners', phase, '/api/crm/owners'),
    loadQueue(vu, st, phase, false),
  ]);
}

async function runVu(vu, endAt) {
  const st = { filters: randomFilters(vu), rows: [], idx: 0, cursor: null, lastDeal: null };
  // morning rush: everyone opens the screen within rushSeconds
  await sleep(uniform(0, OPT.rushSeconds * 1000));
  await screenOpen(vu, st, 'rush');
  if (st.filters.owner === 'all' && st.rows.length === 0) await loadQueue(vu, st, 'rush', false); // after 409
  await sleep(uniform(2000, 6000));
  await openDeal(vu, st, 'rush', nextDealId(st));
  if (rnd() < 0.3 && st.cursor) { await sleep(uniform(3000, 15000)); await loadQueue(vu, st, 'rush', true); }

  while (Date.now() < endAt) {
    const think = uniform(OPT.thinkMin, OPT.thinkMax) * 1000;
    if (Date.now() + think >= endAt) break; // no burst of actions at the end
    await sleep(think);
    const roll = rnd();
    if (st.rows.length === 0) {
      // the queue never loaded (e.g. 504): the user presses reload (再試行 = fresh=1) instead of opening a deal
      await loadQueue(vu, st, 'steady', false, true);
      continue;
    }
    if (roll < 0.1) {
      st.filters = randomFilters(vu);
      st.cursor = null;
      await loadQueue(vu, st, 'steady', false);
      continue;
    }
    // scroll: load the next page when close to the end of what is loaded
    if (st.cursor && st.idx >= st.rows.length - 3) await loadQueue(vu, st, 'steady', true);
    if (st.lastDeal && Date.now() - st.lastDeal.at < 60_000 && roll < 0.25) {
      // reopen the same deal within 60 s (half of them after a call ended: fresh=1)
      await openDeal(vu, st, 'steady', st.lastDeal.id, rnd() < 0.5);
      continue;
    }
    const id = nextDealId(st);
    await openDeal(vu, st, 'steady', id);
    if (rnd() < 0.25) {
      // call ended shortly after: the screen re-reads the deal (fresh=1) 10–60 s later
      const wait = uniform(10_000, 60_000);
      if (Date.now() + wait >= endAt) break;
      await sleep(wait);
      await openDeal(vu, st, 'steady', id, true);
    }
  }
}

// ---------------------------------------------------------------------------
// report
// ---------------------------------------------------------------------------
function summarize(recs) {
  const durs = recs.map(r => r.dur).sort((a, b) => a - b);
  const byStatus = {};
  for (const r of recs) {
    const k = r.status === 200 ? '200' : `${r.status}${r.kind ? ` ${r.kind}` : ''}`;
    byStatus[k] = (byStatus[k] ?? 0) + 1;
  }
  const ok = recs.filter(r => r.status === 200).length;
  return { n: recs.length, ok, okRate: recs.length ? ok / recs.length : NaN, p50: pct(durs, 50), p95: pct(durs, 95), p99: pct(durs, 99), max: durs[durs.length - 1] ?? NaN, byStatus };
}
function epTable(recs) {
  const eps = [...new Set(recs.map(r => r.ep))].sort();
  const rows = ['| endpoint | n | 200 % | p50 ms | p95 ms | p99 ms | max ms | non-200 |', '|---|---:|---:|---:|---:|---:|---:|---|'];
  for (const ep of [...eps, '(all)']) {
    const s = summarize(ep === '(all)' ? recs : recs.filter(r => r.ep === ep));
    const errs = Object.entries(s.byStatus).filter(([k]) => k !== '200').map(([k, v]) => `${k}: ${v}`).join(', ') || '-';
    rows.push(`| ${ep} | ${s.n} | ${fmt(s.okRate * 100)} | ${fmt(s.p50)} | ${fmt(s.p95)} | ${fmt(s.p99)} | ${fmt(s.max)} | ${errs} |`);
  }
  return rows.join('\n');
}
function hubspotWindowStats(perSec, fromS, toS) {
  const types = new Set();
  const sec = [];
  for (let s = fromS; s < Math.min(toS, perSec.length); s++) {
    const b = perSec[s] ?? {};
    const row = { app_ok: 0, app_429: 0, bg_ok: 0, bg_429: 0, search_ok: 0, search_429: 0 };
    for (const [k, v] of Object.entries(b)) {
      const [type, outcome] = k.split('|');
      types.add(type);
      const bg = type === 'background';
      if (outcome === '429') { if (bg) row.bg_429 += v; else row.app_429 += v; } else if (bg) row.bg_ok += v; else row.app_ok += v;
      if (type === 'search') { if (outcome === '429') row.search_429 += v; else row.search_ok += v; }
    }
    sec.push(row);
  }
  const len = sec.length || 1;
  const sum = k => sec.reduce((a, r) => a + r[k], 0);
  let peak10 = 0, peak10bg = 0;
  for (let i = 0; i < sec.length; i++) {
    let a = 0, b = 0;
    for (let j = i; j < Math.min(sec.length, i + 10); j++) { a += sec[j].app_ok; b += sec[j].app_ok + sec[j].bg_ok; }
    peak10 = Math.max(peak10, a); peak10bg = Math.max(peak10bg, b);
  }
  return {
    seconds: sec.length,
    appCalls: sum('app_ok'), app429: sum('app_429'), bgCalls: sum('bg_ok'), bg429: sum('bg_429'),
    avgPerSec: sum('app_ok') / len,
    peak1s: Math.max(0, ...sec.map(r => r.app_ok)),
    peak10s: peak10, peak10sInclBg: peak10bg,
    searchPerSecAvg: sum('search_ok') / len,
    searchPeak1s: Math.max(0, ...sec.map(r => r.search_ok)),
    search429: sum('search_429'),
  };
}

function report({ fakeStats, appLog, started, finished, loginErrors, vus, runId, git }) {
  const rush = records.filter(r => r.phase === 'rush');
  const steady = records.filter(r => r.phase === 'steady');
  const rushEndS = Math.ceil(Math.max(0, ...rush.map(r => r.start + r.dur)) / 1000);
  const steadyFromS = Math.max(rushEndS, 60);
  const totalS = Math.ceil((finished - T0) / 1000);
  const hsRush = hubspotWindowStats(fakeStats.perSec, 0, rushEndS);
  const hsSteady = hubspotWindowStats(fakeStats.perSec, steadyFromS, totalS);
  const hsAll = hubspotWindowStats(fakeStats.perSec, 0, totalS);
  // time until each user saw the queue (first queue_page1 in rush)
  // (first queue_page1 action in rush; with busy retries: from the first attempt to the final answer)
  const isFinal = r => r.final !== false; // records from before the busy-retry change have no flag
  const uDur = r => r.userDur ?? r.dur;
  const firstQueue = new Map();
  for (const r of rush.filter(x => x.ep === 'queue_page1' && isFinal(x))) if (!firstQueue.has(r.vu)) firstQueue.set(r.vu, r);
  const tq = [...firstQueue.values()];
  const tqOk = tq.filter(r => r.status === 200).map(uDur).sort((a, b) => a - b);
  const within = s => tq.filter(r => r.status === 200 && uDur(r) <= s * 1000).length;
  const busySummary = recs => {
    const busy = recs.filter(r => r.status === 503 && r.kind === 'hubspot_busy');
    const fin = recs.filter(isFinal);
    const gaveUp = fin.filter(r => r.status === 503 && r.kind === 'hubspot_busy').length;
    const retried = fin.filter(r => (r.attempt ?? 0) > 0);
    const byEp = {};
    for (const r of busy) byEp[r.ep] = (byEp[r.ep] ?? 0) + 1;
    const epTxt = Object.entries(byEp).map(([k, v]) => `${k} ${v}`).join(', ') || '-';
    const actionOk = fin.length ? fin.filter(r => r.status === 200).length / fin.length : NaN;
    return `503 hubspot_busy responses: ${busy.length} (${epTxt}); user actions: ${fin.length}, of which retried after busy: ${retried.length} (succeeded in the end: ${retried.filter(r => r.status === 200).length}), gave up still busy: ${gaveUp}; action success rate (after busy retries) ${fmt(actionOk * 100)} %.`;
  };
  const gotQueue = new Set(records.filter(r => r.ep === 'queue_page1' && r.status === 200).map(r => r.vu));
  const firstOk = new Map();
  for (const r of records) if (r.ep === 'queue_page1' && r.status === 200 && !firstOk.has(r.vu)) firstOk.set(r.vu, r.start + r.dur);
  const firstOkS = [...firstOk.values()].sort((a, b) => a - b);
  const typeRows = Object.entries(fakeStats.byType).sort((a, b) => (b[1].ok + b[1].r429) - (a[1].ok + a[1].r429))
    .map(([t, v]) => `| ${t} | ${v.ok} | ${v.r429} | ${v.other} |`).join('\n');
  // 10-second timeline
  const tl = ['| t (s) | app requests started | app non-200 | HubSpot calls (app) | HubSpot 429 (app) | Search ok | background ok/429 |', '|---:|---:|---:|---:|---:|---:|---|'];
  for (let s = 0; s < totalS; s += 10) {
    const rs = records.filter(r => r.start >= s * 1000 && r.start < (s + 10) * 1000);
    const w = hubspotWindowStats(fakeStats.perSec, s, s + 10);
    tl.push(`| ${s} | ${rs.length} | ${rs.filter(r => r.status !== 200).length} | ${w.appCalls} | ${w.app429} | ${w.searchPerSecAvg * w.seconds} | ${w.bgCalls}/${w.bg429} |`);
  }
  const logLines = appLog.replace(/\x1b\[[0-9;]*m/g, '').split('\n');
  const grepCount = re => logLines.filter(l => re.test(l)).length;
  const timeoutKinds = {};
  for (const l of logLines) {
    const m = l.match(/rust_dashboard::([\w:]+): ([^=]*?(?:timed out|too long)[^=]*?)\s+error_kind="crm_timeout"/);
    if (m) timeoutKinds[`${m[1]}: ${m[2].trim()}`] = (timeoutKinds[`${m[1]}: ${m[2].trim()}`] ?? 0) + 1;
  }
  const o = fakeStats.options;
  const hsRow = (name, w) => `| ${name} | ${w.seconds} | ${w.appCalls} | ${fmt(w.avgPerSec)} | ${w.peak1s} | ${w.peak10s} | ${w.peak10sInclBg} | ${fmt(w.searchPerSecAvg)} | ${w.searchPeak1s} | ${w.app429} (search ${w.search429}) |`;
  return `# CRM call screen load test — ${OPT.label} (${runId})

- git: \`${git ?? gitHead()}\`  binary: \`${path.relative(ROOT, OPT.bin)}\` (debug build: Google OIDC test override is debug-only)
- users: ${OPT.users} (admins ${OPT.admins}), logged in: ${vus.length}, login errors: ${loginErrors.length}${loginErrors.length ? ` (${loginErrors.slice(0, 3).join('; ')})` : ''}
- scenario: rush within ${OPT.rushSeconds} s, then think ${OPT.thinkMin}–${OPT.thinkMax} s between actions; total ${fmt((finished - T0) / 1000)} s; client timeout ${OPT.clientTimeoutMs} ms; seed ${OPT.seed}
- fake HubSpot: latency ${o.latencyMs} ± ${o.jitterMs} ms, limits ${o.limit10s}/10 s & ${o.limit1s}/s (general), Search ${o.searchPerSec}/s (separate${o.searchCountsGeneral ? ', also counted in general' : ''}), background ${o.backgroundRps || 0} req/s, ${o.deals} deals
- started ${new Date(started).toISOString()}

## Morning rush (each user's first screen open + first deal, ${rush.length} requests, until t=${rushEndS} s)

Time until the queue (page 1) answered, per user: ${tq.length} users, 200 for ${tqOk.length}; within 5 s: ${within(5)}, 10 s: ${within(10)}, 20 s: ${within(20)}, 35 s: ${within(35)}; p50 ${fmt(pct(tqOk, 50))} ms, p95 ${fmt(pct(tqOk, 95))} ms, max ${fmt(tqOk[tqOk.length - 1])} ms.

Users who saw a queue at least once during the whole run: ${gotQueue.size} / ${vus.length}${firstOkS.length ? ` (the last of them at t=${fmt(firstOkS[firstOkS.length - 1] / 1000)} s; 90% of them by t=${fmt(pct(firstOkS, 90) / 1000)} s)` : ''}.

${epTable(rush)}

${busySummary(rush)}

## Steady state (${steady.length} requests)

${epTable(steady)}

${busySummary(steady)}

## HubSpot side (from the fake server; "app" = calls made by rust_dashboard, background = simulated external batch)

| window | s | calls | avg/s | peak 1 s | peak 10 s | peak 10 s incl. background | Search avg/s | Search peak 1 s | 429 issued to app |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---|
${hsRow(`rush (0–${rushEndS} s)`, hsRush)}
${hsRow(`steady (${steadyFromS}–${totalS} s)`, hsSteady)}
${hsRow('whole run', hsAll)}

Peak in-flight requests at the fake: ${fakeStats.peakInFlight}. 429 by limit (app calls): ${Object.entries(fakeStats.policy429 ?? {}).map(([k, v]) => `${k} ${v}`).join(', ') || 'none'}.

| HubSpot call type | ok | 429 | other |
|---|---:|---:|---:|
${typeRows}

## Timeline (10 s buckets)

${tl.join('\n')}

## App log counts

- \`HubSpot API を retry します\` (client retry): ${grepCount(/HubSpot API を retry します/)}
- \`crm_timeout\` log lines: ${grepCount(/crm_timeout/)}${Object.keys(timeoutKinds).length ? ` (${Object.entries(timeoutKinds).map(([k, v]) => `${k}: ${v}`).join('; ')})` : ''}
- rate limit warnings (\`残りが 10% 未満\`): ${grepCount(/残りが 10% 未満/)}
- WARN lines: ${grepCount(/ WARN /)}, ERROR lines: ${grepCount(/ ERROR /)}
`;
}
function gitHead() {
  try {
    const sha = execFileSync('git', ['-C', ROOT, 'rev-parse', '--short', 'HEAD']).toString().trim();
    const dirty = execFileSync('git', ['-C', ROOT, 'status', '--porcelain', '--', 'src']).toString().trim();
    return `${sha}${dirty ? ' + uncommitted changes in src/ (e.g. before-main-base-url.patch)' : ''}`;
  } catch { return 'unknown'; }
}

// ---------------------------------------------------------------------------
// main
// ---------------------------------------------------------------------------
/** --report <file.json>: rebuild the markdown from a saved run (and <base>.app.log next to it) */
function reportOnly(jsonPath) {
  const j = JSON.parse(fs.readFileSync(jsonPath, 'utf8'));
  Object.assign(OPT, { ...j.options, outDir: OPT.outDir });
  if (A.label) OPT.label = A.label;
  records.push(...j.records);
  const meta = j.meta ?? { runId: path.basename(jsonPath).slice(0, 19), T0: j.fakeStats.t0, started: j.fakeStats.t0, finished: j.fakeStats.now, loginErrors: [], vus: OPT.users };
  T0 = meta.T0;
  const logPath = jsonPath.replace(/\.json$/, '.app.log');
  const appLog = fs.existsSync(logPath) ? fs.readFileSync(logPath, 'utf8') : '';
  const md = report({ fakeStats: j.fakeStats, appLog, started: meta.started, finished: meta.finished, loginErrors: meta.loginErrors, vus: { length: meta.vus }, runId: meta.runId, git: meta.git });
  const out = jsonPath.replace(/\.json$/, '.md');
  fs.writeFileSync(out, md);
  console.log(md);
}

async function main() {
  if (A.scenario === 'write') {
    // write scenario (PATCH /api/crm/deals/{id}): separate driver, same options style. See write_run.mjs
    const r = spawn(process.execPath, [path.join(HERE, 'write_run.mjs'), ...process.argv.slice(2)], { stdio: 'inherit' });
    return new Promise(res => r.on('exit', code => { process.exitCode = code ?? 1; res(); }));
  }
  if (A.report) return reportOnly(A.report);
  const runId = new Date().toISOString().replace(/[:.]/g, '-').slice(0, 19);
  const workDir = fs.mkdtempSync(path.join(os.tmpdir(), 'crm_loadtest_'));
  log(`workdir ${workDir}`);
  let appLogPath = null;
  if (OPT.spawn) ({ appLogPath } = await startProcesses(workDir));
  else await waitFor(`${APP}/health`, 10_000, 'rust_dashboard');

  log(`logging in ${OPT.users} users via the fake Google OIDC`);
  const vus = [], loginErrors = [];
  for (let i = 0; i < OPT.users; i += 10) {
    const batch = await Promise.allSettled(Array.from({ length: Math.min(10, OPT.users - i) }, (_, k) => loginVu(i + k)));
    for (const b of batch) (b.status === 'fulfilled' ? vus.push(b.value) : loginErrors.push(String(b.reason?.message ?? b.reason)));
  }
  log(`logged in ${vus.length}, errors ${loginErrors.length}`);
  if (!vus.length) throw new Error(`no user could log in: ${loginErrors[0]}`);

  await fetch(`${FAKE}/_reset`, { method: 'POST' });
  T0 = Date.now();
  const started = T0;
  const endAt = T0 + OPT.duration * 1000;
  const progress = setInterval(() => {
    const done = records.length, bad = records.filter(r => r.status !== 200).length;
    log(`t=${Math.round((Date.now() - T0) / 1000)}s requests=${done} non200=${bad}`);
  }, 15_000);
  await Promise.all(vus.map(vu => runVu(vu, endAt)));
  clearInterval(progress);
  const finished = Date.now();
  const fakeStats = await (await fetch(`${FAKE}/_stats`)).json();
  const appLog = appLogPath ? fs.readFileSync(appLogPath, 'utf8') : '';

  fs.mkdirSync(OPT.outDir, { recursive: true });
  const md = report({ fakeStats, appLog, started, finished, loginErrors, vus, runId });
  const base = path.join(OPT.outDir, `${runId}-${OPT.label}`);
  fs.writeFileSync(`${base}.md`, md);
  fs.writeFileSync(`${base}.json`, JSON.stringify({ options: OPT, meta: { runId, T0, started, finished, loginErrors, vus: vus.length, git: gitHead() }, records, fakeStats }, null, 0));
  if (appLogPath) fs.copyFileSync(appLogPath, `${base}.app.log`);
  log(`report: ${base}.md`);
  console.log(md);
  stopChildren();
}

main().catch(e => { console.error(e); stopChildren(); process.exit(1); });
