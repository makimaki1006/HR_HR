#!/usr/bin/env node
// Write-path load test for the CRM (PATCH /api/crm/deals/{id}): N virtual operators edit their own fake deals
// against the real rust_dashboard (debug build), a fake HubSpot (--writable) and a fake Turso (ledger).
// Start it through `node scripts/loadtest/run.mjs --scenario write ...` (same options style). No dependencies (Node >= 23.11
// because fake_turso uses node:sqlite). Never talks to a real HubSpot / Turso. See scripts/loadtest/README.md.
//
// Variants (--variant):
//   v1  normal      healthy fake HubSpot
//   v2  hiccups     5% of PATCHes answered 503 for the whole run + one 30 s total outage (t=120..150 s)
//   v3  429 storm   every HubSpot call answered 429 (Retry-After 2) for 20 s (t=120..140 s)
//   v5  same-deal race: every operator saves the same property of the same 5 shared deals at once, identical base (expected: 1 saved + rest 409 per deal)
//   v4  rush        all operators save 10 properties inside the same 10 s, then normal behaviour for --duration (default 60 s)

import { spawn } from 'node:child_process';
import crypto from 'node:crypto';
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
const VARIANT = A.variant ?? 'v1';
const OPT = {
  variant: VARIANT,
  users: num('users', VARIANT === 'v5' ? 20 : 100),
  dealsPerUser: num('deals-per-user', 3),
  duration: num('duration', VARIANT === 'v4' ? 60 : 300),
  drainMax: num('drain-max', 300), // extra seconds to keep observing until the ledger has no pending / in_progress rows
  thinkMin: num('think-min', 10),
  thinkMax: num('think-max', 40),
  burstMin: num('burst-min', 5),
  burstMax: num('burst-max', 15),
  burstGapMin: num('burst-gap-min', 0.15), // seconds between the saves of a burst (one per property edited)
  burstGapMax: num('burst-gap-max', 0.5),
  stageMoveProb: num('stage-move-prob', 0.25),
  rushProps: num('rush-props', 10),
  rushSeconds: num('rush-seconds', 10),
  retries: num('retries', VARIANT === 'v5' ? 0 : 2), // client retries (same operation_id) after 429 / 503 / network; 0 = give up at once
  retryMin: num('retry-min', 3),
  retryMax: num('retry-max', 8),
  clientTimeoutMs: num('client-timeout-ms', 35_000),
  backoffSecs: num('backoff-secs', 5), // CRM_PENDING_BACKOFF_SECS_DEBUG (debug build only; the override is one flat value)
  workerDelaySecs: num('worker-delay-secs', 1),
  writeRatePerMin: A['write-rate'] ?? null, // CRM_WRITE_RATE_PER_MIN (only if the binary reads it)
  hsLatencyMs: num('latency-ms', 450),
  hsJitterMs: num('jitter-ms', 150),
  limit10s: num('limit-10s', 190),
  limit1s: num('limit-1s', 19),
  tursoLatencyMs: num('turso-latency-ms', 40),
  outageAt: num('outage-at', 120), outageSecs: num('outage-secs', 30),
  stormAt: num('storm-at', 120), stormSecs: num('storm-secs', 20),
  rate503: num('rate-503', 0.05),
  appPort: num('app-port', 9438), hsPort: num('fake-port', 9430), tursoPort: num('turso-port', 9431),
  bin: A.bin ?? path.join(process.env.CARGO_TARGET_DIR ?? path.join(ROOT, 'target'), 'debug', 'rust_dashboard'),
  db: A.db ?? null,
  label: A.label ?? VARIANT,
  outDir: A.out ?? path.join(HERE, 'results'),
  seed: num('seed', 42),
  appEnv: Object.fromEntries((A['app-env'] ?? '').split(',').filter(Boolean).map(kv => [kv.slice(0, kv.indexOf('=')), kv.slice(kv.indexOf('=') + 1)])),
};
const APP = `http://localhost:${OPT.appPort}`;
const HS = `http://127.0.0.1:${OPT.hsPort}`;
const TURSO = `http://127.0.0.1:${OPT.tursoPort}`;
const DEAL_BASE = 9_000_000_000;
const PIPELINE = '753186575';
const STAGE_A = '1095387442'; // 未済 (no required properties)
const STAGE_B = '1095387443'; // 不通 (bpo_10 required)
// plain text properties of deals, writable through the catalog (one edit = one PATCH, as the property panel does)
const PROP_POOL = ['bpo_31', 'bpo_57', 'bpo_14', ...Array.from({ length: 15 }, (_, k) => `bpo_${61 + k}`)];

let rngState = OPT.seed >>> 0;
function rnd() {
  rngState = (rngState + 0x6d2b79f5) | 0;
  let t = Math.imul(rngState ^ (rngState >>> 15), 1 | rngState);
  t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
  return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
}
const uniform = (a, b) => a + (b - a) * rnd();
const sleep = ms => new Promise(r => setTimeout(r, Math.max(0, ms)));
const pct = (sorted, p) => (sorted.length ? sorted[Math.min(sorted.length - 1, Math.ceil((p / 100) * sorted.length) - 1)] : NaN);
const fmt = v => (Number.isFinite(v) ? (v >= 100 ? v.toFixed(0) : v.toFixed(1)) : '-');
const log = (...xs) => console.log(`[write ${new Date().toISOString().slice(11, 19)}]`, ...xs);
const jget = async (url) => (await fetch(url)).json();
const jpost = async (url, body = {}) => (await fetch(url, { method: 'POST', body: JSON.stringify(body) })).json();

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
const vuEmail = n => `lt-write${String(n).padStart(3, '0')}@f-a-c.co.jp`;

async function startProcesses(workDir) {
  const spawnLogged = (name, cmd, args, env = process.env) => {
    const fd = fs.openSync(path.join(workDir, name), 'w');
    const c = spawn(cmd, args, { cwd: ROOT, stdio: ['ignore', fd, fd], env });
    children.push(c);
    return c;
  };
  spawnLogged('fake_hubspot.log', process.execPath, [path.join(HERE, 'fake_hubspot.mjs'), '--port', String(OPT.hsPort), '--writable',
    '--deals', String(OPT.users * OPT.dealsPerUser + 10), '--latency-ms', String(OPT.hsLatencyMs), '--jitter-ms', String(OPT.hsJitterMs),
    '--limit-10s', String(OPT.limit10s), '--limit-1s', String(OPT.limit1s)]);
  spawnLogged('fake_turso.log', process.execPath, [path.join(ROOT, 'tests/e2e/crm_write_live/fake_turso.mjs'), '--port', String(OPT.tursoPort), '--latency-ms', String(OPT.tursoLatencyMs)]);
  await waitFor(`${HS}/_health`, 60_000, 'fake_hubspot');
  await waitFor(`${TURSO}/_health`, 30_000, 'fake_turso');
  if (!fs.existsSync(OPT.bin)) throw new Error(`app binary not found: ${OPT.bin} (--bin)`);
  let db = OPT.db;
  if (!db) {
    db = path.join(workDir, 'hellowork.db');
    await new Promise((res, rej) => {
      const p = spawn('python3', [path.join(ROOT, 'scripts/e2e/make_fixture_db.py'), db], { stdio: 'inherit' });
      p.on('exit', c => (c === 0 ? res() : rej(new Error(`make_fixture_db.py exit ${c}`))));
    });
  }
  const env = { ...process.env };
  for (const k of Object.keys(env)) {
    if (/^(GEMINI_|GOOGLE_|OPENAI_|ANTHROPIC_|HUBSPOT_|SLACK_|ZOOM_|CRM_|ADMIN_EMAILS|AUDIT_|.*TURSO.*)/.test(k)) delete env[k];
  }
  spawnLogged('app.log', OPT.bin, [], {
    ...env,
    PORT: String(OPT.appPort),
    AUTH_PASSWORD: `loadtest-${process.pid}`,
    ALLOWED_DOMAINS: 'f-a-c.co.jp',
    HELLOWORK_DB_PATH: db,
    HUBSPOT_ACCESS_TOKEN: 'fake-loadtest-token',
    HUBSPOT_BASE_URL: HS,
    GOOGLE_OIDC_CLIENT_ID: 'test-client',
    GOOGLE_OIDC_CLIENT_SECRET: 'test-secret',
    GOOGLE_OIDC_REDIRECT_URL: `${APP}/auth/google/callback`,
    GOOGLE_OIDC_HOSTED_DOMAIN: 'f-a-c.co.jp',
    GOOGLE_OIDC_DISCOVERY_URL_DEBUG: `${HS}/oidc/.well-known/openid-configuration`,
    CSRF_EXTRA_ORIGINS_DEBUG: APP,
    AUDIT_TURSO_URL: TURSO,
    AUDIT_TURSO_TOKEN: 'fake-loadtest-token',
    CRM_WRITES_ENABLED: '1',
    CRM_WORKER_START_DELAY_SECS_DEBUG: String(OPT.workerDelaySecs),
    CRM_PENDING_BACKOFF_SECS_DEBUG: String(OPT.backoffSecs),
    ...(OPT.writeRatePerMin ? { CRM_WRITE_RATE_PER_MIN: OPT.writeRatePerMin } : {}),
    RUST_LOG: process.env.RUST_LOG ?? 'info',
    ...OPT.appEnv,
  });
  await waitFor(`${APP}/health`, 120_000, 'rust_dashboard');
  return { appLogPath: path.join(workDir, 'app.log') };
}

// ---------------------------------------------------------------------------
// HTTP with a per-user cookie jar (same login as run.mjs)
// ---------------------------------------------------------------------------
class Jar {
  constructor() { this.c = new Map(); }
  take(res) {
    for (const sc of res.headers.getSetCookie?.() ?? []) {
      const [pair, ...attrs] = sc.split(';');
      const i = pair.indexOf('=');
      const k = pair.slice(0, i).trim(), v = pair.slice(i + 1).trim();
      if (attrs.some(a => /max-age=0\b/i.test(a.trim())) || v === '') this.c.delete(k); else this.c.set(k, v);
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
  return { n, email, jar };
}

// ---------------------------------------------------------------------------
// scenario
// ---------------------------------------------------------------------------
const attempts = []; // one row per HTTP request: {vu, deal, op, kind, attempt, start, dur, status, tag}
const saves = []; // one row per save action (final outcome after client retries): {vu, deal, op, kind, start, end, outcome, attempts, props}
let T0 = 0;
const tNow = () => Date.now() - T0;

const tagOf = (status, body) => {
  if (status === 200) return 'saved_200';
  if (status === 202) return 'queued_202';
  if (status === 409) return 'conflict_409';
  if (status === 422) return 'invalid_422';
  if (status === 429) return `rate_limited_429${body?.error_kind && body.error_kind !== 'rate_limited' ? `:${body.error_kind}` : ''}`;
  if (status === 503) return `${body?.error_kind ?? 'unavailable'}_503`;
  if (status === 504) return `${body?.error_kind ?? 'timeout'}_504`;
  if (typeof status === 'number' && status >= 500) return `${body?.error_kind ?? 'error'}_${status}`;
  if (status === 'client_timeout' || status === 'net_error') return String(status);
  return `other_${status}${body?.error_kind ? `:${body.error_kind}` : ''}`;
};
// the screen shows "もう一度保存してください" for everything but 409 / 422 / 403 and keeps the operation_id, so a user retry = same id
const isRetryable = status => status === 429 || status === 'client_timeout' || status === 'net_error' || (typeof status === 'number' && status >= 500);

/** One save (one PATCH, possibly retried with the same operation_id). Returns {outcome, body}. */
async function save(vu, st, dealIdx, body, kind) {
  const deal = String(DEAL_BASE + dealIdx);
  const op = body.operation_id;
  const first = tNow();
  let last = null;
  for (let attempt = 0; attempt <= OPT.retries; attempt++) {
    const start = Date.now();
    const ctl = new AbortController();
    const timer = setTimeout(() => ctl.abort(), OPT.clientTimeoutMs);
    let status, json = null, retryAfter = null;
    try {
      const r = await fetch(`${APP}/api/crm/deals/${deal}`, {
        method: 'PATCH',
        headers: { cookie: vu.jar.header(), 'content-type': 'application/json', 'x-requested-with': 'fetch', accept: 'application/json', origin: APP },
        body: JSON.stringify(body), signal: ctl.signal,
      });
      status = r.status;
      retryAfter = r.headers.get('retry-after');
      const text = await r.text();
      if (text) { try { json = JSON.parse(text); } catch { /* keep null */ } }
    } catch (e) {
      status = e.name === 'AbortError' ? 'client_timeout' : 'net_error';
    } finally { clearTimeout(timer); }
    const tag = tagOf(status, json);
    attempts.push({ vu: vu.n, deal, op, kind, attempt, start: start - T0, dur: Date.now() - start, status, tag });
    last = { status, json, tag };
    if (!isRetryable(status) || attempt >= OPT.retries) break;
    await sleep(Math.max(retryAfter ? Number(retryAfter) * 1000 : 0, uniform(OPT.retryMin, OPT.retryMax) * 1000));
  }
  const end = tNow();
  const final = last.tag;
  if (last.status === 202) pollers.push(pollOperation(vu, op, end));
  saves.push({ vu: vu.n, deal, op, kind, start: first, end, outcome: final, attempts: attempts.filter(a => a.op === op).length, props: Object.keys(body.set) });
  return { ...last, op };
}

const polls = []; // GET /api/crm/operations/{id} as the screen does after a 202: every 10 s, for at most 10 min
const confirmed = new Map(); // op -> {status, at (ms from T0 when the screen would show green/red), queuedAt}
const pollers = [];
const POLL_INTERVAL_MS = 10_000, POLL_MAX_MS = 10 * 60_000;
async function pollOperation(vu, op, queuedAt) {
  for (let n = 0; n < POLL_MAX_MS / POLL_INTERVAL_MS && !stopPolling; n++) {
    await sleep(POLL_INTERVAL_MS);
    const start = Date.now();
    let status = 'net_error', body = null;
    try {
      const r = await fetch(`${APP}/api/crm/operations/${op}`, { headers: { cookie: vu.jar.header(), accept: 'application/json' }, signal: AbortSignal.timeout(15_000) });
      status = r.status;
      const t = await r.text();
      try { body = JSON.parse(t); } catch { /* keep null */ }
    } catch (e) { status = e.name === 'TimeoutError' ? 'client_timeout' : 'net_error'; }
    polls.push({ start: start - T0, dur: Date.now() - start, status, op: body?.status ?? null });
    if (status === 200 && (body?.status === 'saved' || body?.status === 'failed')) { confirmed.set(op, { status: body.status, at: tNow(), queuedAt }); return; }
  }
  confirmed.set(op, { status: 'unconfirmed', at: tNow(), queuedAt });
}
let stopPolling = false;

/** apply a finished save to the operator's beliefs: base = last known HubSpot value, expected = last accepted value */
function applyResult(st, dealIdx, setProps, res, stageTo) {
  const d = st.deals[dealIdx];
  if (res.status === 200 || res.status === 202) {
    for (const [k, v] of Object.entries(setProps)) { d.base[k] = v; d.expected[k] = v; d.expectedOp[k] = res.op; }
    if (stageTo) { d.stage = stageTo; d.expectedStage = stageTo; }
  } else if (res.status === 409 && res.json?.current) {
    for (const [k, v] of Object.entries(res.json.current)) d.base[k] = v ?? null; // re-base on what HubSpot has now
  } else {
    for (const k of Object.keys(setProps)) d.attempted[k] = (d.attempted[k] ?? []).concat(setProps[k]);
  }
}

let valueSeq = 0;
const newValue = vu => `L${vu.n}-${++valueSeq}`;

async function saveProp(vu, st, dealIdx, prop) {
  const d = st.deals[dealIdx];
  const value = newValue(vu);
  const body = { operation_id: crypto.randomUUID(), base: { [prop]: d.base[prop] ?? null }, set: { [prop]: value } };
  const res = await save(vu, st, dealIdx, body, 'prop');
  applyResult(st, dealIdx, { [prop]: value }, res);
  return res;
}
async function moveStage(vu, st, dealIdx) {
  const d = st.deals[dealIdx];
  const to = d.stage === STAGE_A ? STAGE_B : STAGE_A;
  const set = to === STAGE_B ? { bpo_10: 'true' } : { bpo_10: 'false' };
  const body = { operation_id: crypto.randomUUID(), base: { bpo_10: d.base.bpo_10 ?? null }, set, stage: { pipeline_id: PIPELINE, stage_id: to } };
  const res = await save(vu, st, dealIdx, body, 'stage');
  applyResult(st, dealIdx, set, res, to);
  return res;
}

function pickProps(n) {
  const pool = [...PROP_POOL];
  for (let i = pool.length - 1; i > 0; i--) { const j = Math.floor(rnd() * (i + 1)); [pool[i], pool[j]] = [pool[j], pool[i]]; }
  return pool.slice(0, Math.min(n, pool.length));
}
/** n property saves on one deal, started `gap` seconds apart without waiting for the previous answer (the user keeps typing) */
async function burst(vu, st, dealIdx, n, gapMin, gapMax) {
  const props = pickProps(n);
  const ps = [];
  for (let i = 0; i < props.length; i++) {
    ps.push(saveProp(vu, st, dealIdx, props[i]));
    if (i < props.length - 1) await sleep(uniform(gapMin, gapMax) * 1000);
  }
  await Promise.all(ps);
}
/** v4: n saves spread randomly over `seconds` */
async function rushBurst(vu, st, n, seconds) {
  const props = pickProps(n);
  const dealIdx = vu.dealIdxs[0];
  await Promise.all(props.map(async p => { await sleep(uniform(0, seconds * 1000)); return saveProp(vu, st, dealIdx, p); }));
}

async function runVu(vu, endAt) {
  const st = vu.st;
  if (OPT.variant === 'v4') { await rushBurst(vu, st, OPT.rushProps, OPT.rushSeconds); }
  let k = 0;
  for (;;) {
    const think = uniform(OPT.thinkMin, OPT.thinkMax) * 1000;
    if (Date.now() + think >= endAt) break;
    await sleep(think);
    const dealIdx = vu.dealIdxs[k++ % vu.dealIdxs.length];
    const n = OPT.burstMin + Math.floor(rnd() * (OPT.burstMax - OPT.burstMin + 1));
    await burst(vu, st, dealIdx, n, OPT.burstGapMin, OPT.burstGapMax);
    if (rnd() < OPT.stageMoveProb) { await sleep(uniform(1000, 3000)); await moveStage(vu, st, dealIdx); }
  }
}

// v5: all operators save property bpo_31 of the same SHARED_DEALS deals at the same instant, all with the same base
const SHARED_DEALS = 5;
const v5res = []; // {vu, deal, idx, status, tag, value}
async function runV5(vus) {
  const idxs = Array.from({ length: SHARED_DEALS }, (_, k) => OPT.users * OPT.dealsPerUser + k);
  const bases = {};
  for (const i of idxs) bases[i] = ((await jget(`${HS}/_record?object=deals&id=${DEAL_BASE + i}`)).properties ?? {}).bpo_31 ?? null;
  await Promise.all(vus.flatMap(vu => idxs.map(async i => {
    const value = newValue(vu);
    const body = { operation_id: crypto.randomUUID(), base: { bpo_31: bases[i] }, set: { bpo_31: value } };
    const r = await save(vu, vu.st, i, body, 'prop');
    v5res.push({ vu: vu.n, deal: DEAL_BASE + i, idx: i, status: r.status, tag: r.tag, value, op: r.op });
  })));
}
function v5Section(patches, ledgerOps) {
  if (OPT.variant !== 'v5') return { md: '', lost: 0 };
  const rows = ['| deal | 200 | 202 | 409 | other | HubSpot PATCH applied | final value (operator) | accepted (200+202) | lost updates (HubSpot PATCH applied - 1) | expected |', '|---|---:|---:|---:|---:|---:|---|---:|---:|---|'];
  let lostTotal = 0, acceptedTotal = 0;
  const finals = [];
  for (const [i, list] of Object.entries(Object.groupBy(v5res, r => r.idx))) {
    const c = k => list.filter(r => r.status === k).length;
    const accepted = c(200) + c(202);
    const applied = patches.filter(p => p.status === 200 && String(p.id) === String(DEAL_BASE + Number(i)) && 'bpo_31' in p.properties).length;
    const lost = Math.max(0, applied - 1); // each extra applied PATCH overwrote another accepted write
    lostTotal += lost; acceptedTotal += accepted;
    finals.push(i);
    rows.push(`| ${DEAL_BASE + Number(i)} | ${c(200)} | ${c(202)} | ${c(409)} | ${list.length - accepted - c(409)} | ${applied} | @@${i}@@ | ${accepted} | ${lost} | 1 saved + ${list.length - 1} x 409 |`);
  }
  return { rows, lost: lostTotal, accepted: acceptedTotal };
}

// ---------------------------------------------------------------------------
// observation
// ---------------------------------------------------------------------------
const series = []; // {t, pending, in_progress, failed, saved}
let monitorOn = true;
async function monitor() {
  while (monitorOn) {
    const t = tNow();
    try {
      const r = await jpost(`${TURSO}/_query`, { sql: 'SELECT status, COUNT(*) AS n FROM crm_pending_operations GROUP BY status' });
      const row = { t, pending: 0, in_progress: 0, failed: 0, saved: 0 };
      for (const x of r.rows ?? []) row[x.status] = Number(x.n);
      series.push(row);
    } catch { /* table not there yet */ }
    await sleep(1000);
  }
}
const faultWindows = []; // {name, from, to} (ms from T0)
async function chaosScript() {
  const at = async (sec, body, name) => { await sleep(T0 + sec * 1000 - Date.now()); await jpost(`${HS}/_chaos`, body); log(`chaos ${name}`); };
  if (OPT.variant === 'v2') {
    await jpost(`${HS}/_chaos`, { patch503Rate: OPT.rate503 });
    faultWindows.push({ name: `PATCH 503 ${OPT.rate503 * 100}%`, from: 0, to: OPT.duration * 1000 });
    faultWindows.push({ name: 'outage', from: OPT.outageAt * 1000, to: (OPT.outageAt + OPT.outageSecs) * 1000 });
    await at(OPT.outageAt, { outage: true }, 'outage on');
    await at(OPT.outageAt + OPT.outageSecs, { outage: false }, 'outage off');
  } else if (OPT.variant === 'v3') {
    faultWindows.push({ name: '429 storm', from: OPT.stormAt * 1000, to: (OPT.stormAt + OPT.stormSecs) * 1000 });
    await at(OPT.stormAt, { storm429: true, retryAfter: 2 }, 'storm on');
    await at(OPT.stormAt + OPT.stormSecs, { storm429: false }, 'storm off');
  }
}

// ---------------------------------------------------------------------------
// report
// ---------------------------------------------------------------------------
const latStats = recs => {
  const d = recs.map(r => r.dur).sort((a, b) => a - b);
  return { n: d.length, p50: pct(d, 50), p95: pct(d, 95), p99: pct(d, 99), max: d[d.length - 1] ?? NaN };
};
const count = (arr, f) => { const o = {}; for (const x of arr) { const k = f(x); o[k] = (o[k] ?? 0) + 1; } return o; };

async function main() {
  const runId = new Date().toISOString().replace(/[:.]/g, '-').slice(0, 19);
  const workDir = fs.mkdtempSync(path.join(os.tmpdir(), 'crm_writetest_'));
  log(`variant ${OPT.variant}, workdir ${workDir}`);
  const { appLogPath } = await startProcesses(workDir);

  log(`logging in ${OPT.users} operators`);
  const vus = [], loginErrors = [];
  for (let i = 0; i < OPT.users; i += 10) {
    const batch = await Promise.allSettled(Array.from({ length: Math.min(10, OPT.users - i) }, (_, k) => loginVu(i + k)));
    for (const b of batch) (b.status === 'fulfilled' ? vus.push(b.value) : loginErrors.push(String(b.reason?.message ?? b.reason)));
  }
  log(`logged in ${vus.length}, errors ${loginErrors.length}`);
  if (!vus.length) throw new Error(`no operator could log in: ${loginErrors[0]}`);

  // each operator owns OPT.dealsPerUser deals; read their starting values straight from the fake HubSpot (not through the app)
  for (const vu of vus) {
    vu.dealIdxs = Array.from({ length: OPT.dealsPerUser }, (_, k) => vu.n * OPT.dealsPerUser + k);
    vu.st = { deals: {} };
    for (const i of vu.dealIdxs) {
      const rec = await jget(`${HS}/_record?object=deals&id=${DEAL_BASE + i}`);
      const p = rec.properties ?? {};
      const base = {};
      for (const k of [...PROP_POOL, 'bpo_10']) base[k] = p[k] ?? null;
      vu.st.deals[i] = { base, expected: {}, expectedOp: {}, attempted: {}, stage: p.dealstage, expectedStage: p.dealstage, initial: { ...base } };
    }
  }
  // warm the property catalog as the screen does on mount (6 h cache); otherwise the first save would pay for 6 HubSpot calls,
  // and a failure there is remembered for 60 s (FAILURE_TTL) which would swamp the fault variants
  {
    const r = await fetch(`${APP}/api/crm/property-catalog`, { headers: { cookie: vus[0].jar.header(), accept: 'application/json' } });
    await r.text();
    if (r.status !== 200) throw new Error(`property-catalog warm-up failed: ${r.status}`);
  }
  await jpost(`${HS}/_reset`);
  await jpost(`${TURSO}/_reset_stats`);
  T0 = Date.now();
  const endAt = T0 + OPT.duration * 1000;
  const progress = setInterval(() => {
    const s = series[series.length - 1] ?? {};
    log(`t=${Math.round(tNow() / 1000)}s attempts=${attempts.length} non2xx=${attempts.filter(a => !String(a.status).startsWith('20')).length} ledger p/ip/f/s=${s.pending}/${s.in_progress}/${s.failed}/${s.saved}`);
  }, 15_000);
  const mon = monitor();
  const chaos = chaosScript();
  if (OPT.variant === 'v5') await runV5(vus); else await Promise.all(vus.map(vu => runVu(vu, endAt)));
  const loadEnd = tNow();
  log(`load finished at t=${fmt(loadEnd / 1000)}s; waiting for the ledger to drain (max ${OPT.drainMax}s)`);
  await chaos;
  await jpost(`${HS}/_chaos`, { outage: false, storm429: false, patch503Rate: 0 });
  let drainedAt = null;
  const drainDeadline = Date.now() + OPT.drainMax * 1000;
  while (Date.now() < drainDeadline) {
    const s = series[series.length - 1];
    if (s && s.pending + s.in_progress === 0 && s.t >= loadEnd) { drainedAt = s.t; break; }
    await sleep(1000);
  }
  await sleep(1500);
  const finishedAt = tNow();
  clearInterval(progress);
  stopPolling = true;
  await Promise.all(pollers);
  monitorOn = false;
  await mon;

  // ---- gather ----
  const hsStats = await jget(`${HS}/_stats`);
  const patches = (await jget(`${HS}/_patches`)).patches;
  const tursoStats = await jget(`${TURSO}/_stats`);
  const ledger = (await jpost(`${TURSO}/_query`, { sql: 'SELECT status, COUNT(*) AS n, SUM(attempts) AS att FROM crm_pending_operations GROUP BY status' })).rows;
  const failedBy = (await jpost(`${TURSO}/_query`, { sql: "SELECT last_error_code AS code, http_status AS http, COUNT(*) AS n FROM crm_pending_operations WHERE status = 'failed' GROUP BY 1, 2" })).rows;
  const savedAtt = (await jpost(`${TURSO}/_query`, { sql: "SELECT attempts, COUNT(*) AS n FROM crm_pending_operations WHERE status = 'saved' GROUP BY attempts ORDER BY attempts" })).rows;
  const ledgerOps = new Map((await jpost(`${TURSO}/_query`, { sql: 'SELECT operation_id, status FROM crm_pending_operations' })).rows.map(r => [r.operation_id, r.status]));
  const appLog = fs.readFileSync(appLogPath, 'utf8');

  // consistency: every (deal, prop) the operator edited. Expected = the last value the app accepted (200 / 202).
  // A mismatch whose accepted operation is still pending / in_progress in the ledger is "not yet applied" (the queue has not drained),
  // not a loss; 'lost' = the ledger says saved (or has no row) but HubSpot holds something else; 'ledger_failed' = the ledger gave up.
  const mism = { notYet: [], ledgerFailed: [], lost: [], phantom: [], total: 0, checked: 0, stage: 0, stageNotYet: 0, stageChecked: 0 };
  for (const vu of vus) {
    for (const i of vu.dealIdxs) {
      const d = vu.st.deals[i];
      const rec = (await jget(`${HS}/_record?object=deals&id=${DEAL_BASE + i}`)).properties ?? {};
      const touched = new Set([...Object.keys(d.expected), ...Object.keys(d.attempted)]);
      for (const k of touched) {
        const finalV = rec[k] ?? null;
        const expected = k in d.expected ? d.expected[k] : d.initial[k];
        mism.checked++;
        if (finalV === expected) continue;
        mism.total++;
        const item = { vu: vu.n, deal: DEAL_BASE + i, prop: k, expected, final: finalV };
        if (!(k in d.expected)) { mism.phantom.push(item); continue; }
        const lst = ledgerOps.get(d.expectedOp[k]);
        if ((d.attempted[k] ?? []).includes(finalV)) mism.phantom.push(item);
        else if (lst === 'pending' || lst === 'in_progress') mism.notYet.push(item);
        else if (lst === 'failed') mism.ledgerFailed.push(item);
        else mism.lost.push({ ...item, ledger: lst ?? 'no row' });
      }
      mism.stageChecked++;
      if ((rec.dealstage ?? null) !== d.expectedStage) mism.stage++;
    }
  }
  // duplicates: a unique value applied by HubSpot (status 200) more than once
  const applied = patches.filter(p => p.status === 200);
  const seen = new Map();
  for (const p of applied) for (const [k, v] of Object.entries(p.properties)) if (/^L\d+-\d+$/.test(v)) seen.set(`${p.id}|${k}|${v}`, (seen.get(`${p.id}|${k}|${v}`) ?? 0) + 1);
  const duplicates = [...seen.values()].filter(n => n > 1).length;

  let v5md = '';
  if (OPT.variant === 'v5') {
    const sec = v5Section(patches, ledgerOps);
    let table = sec.rows.join('\n');
    for (const m of table.matchAll(/@@(\d+)@@/g)) {
      const rec = (await jget(`${HS}/_record?object=deals&id=${DEAL_BASE + Number(m[1])}`)).properties ?? {};
      const who = v5res.find(r => r.value === rec.bpo_31)?.vu;
      table = table.replace(m[0], `${rec.bpo_31 ?? 'null'} (${who === undefined ? '-' : 'lt-write' + String(who).padStart(3, '0')})`);
    }
    v5md = `\n## 7. V5 same-deal race (${OPT.users} operators x ${SHARED_DEALS} shared deals, identical base, one property)\n\n${table}\n\n- Total requests ${v5res.length}; accepted (200/202) ${sec.accepted}; **lost updates (writes HubSpot applied and a later write silently overwrote, same base) ${sec.lost}**. 202s that the worker later turned into failed/409 are not counted as lost (the screen shows red). With the conflict check working: ${SHARED_DEALS} applied, 0 lost, the rest 409.\n`;
  }
  const md0 = buildReport({ runId, vus, loginErrors, loadEnd, finishedAt, drainedAt, hsStats, patches, tursoStats, ledger, failedBy, savedAtt, ledgerOps, appLog, mism, duplicates });
  const md = md0 + v5md;
  fs.mkdirSync(OPT.outDir, { recursive: true });
  const base = path.join(OPT.outDir, `${runId}-write-${OPT.label}`);
  fs.writeFileSync(`${base}.md`, md);
  fs.writeFileSync(`${base}.json`, JSON.stringify({ options: OPT, attempts, saves, series, faultWindows, hsStats: { ...hsStats, perSec: undefined }, tursoStats, ledger, failedBy, savedAtt, mism, duplicates, loadEnd, drainedAt, finishedAt }));
  fs.copyFileSync(appLogPath, `${base}.app.log`);
  log(`report: ${base}.md`);
  console.log(md);
  stopChildren();
  fs.rmSync(workDir, { recursive: true, force: true });
}

function buildReport({ runId, vus, loginErrors, loadEnd, finishedAt, drainedAt, hsStats, patches, tursoStats, ledger, failedBy, savedAtt, ledgerOps, appLog, mism, duplicates }) {
  const nSaves = saves.length;
  const byTag = count(attempts, a => a.tag);
  const byFinal = count(saves, s => s.outcome);
  const okFinal = (byFinal.saved_200 ?? 0) + (byFinal.queued_202 ?? 0);
  const lat = latStats(attempts);
  const latSaved = latStats(attempts.filter(a => a.tag === 'saved_200'));
  const latQueued = latStats(attempts.filter(a => a.tag === 'queued_202'));
  const userDur = saves.map(s => s.end - s.start).sort((a, b) => a - b);
  const tagRows = Object.entries(byTag).sort((a, b) => b[1] - a[1]).map(([k, v]) => `| ${k} | ${v} | ${fmt(v / attempts.length * 100)} % |`).join('\n');
  const finalRows = Object.entries(byFinal).sort((a, b) => b[1] - a[1]).map(([k, v]) => `| ${k} | ${v} | ${fmt(v / nSaves * 100)} % |`).join('\n');
  const rejectedFinal = nSaves - okFinal;
  const conflictInvalid = (byFinal.conflict_409 ?? 0) + (byFinal.invalid_422 ?? 0);
  // ledger
  const peak = k => Math.max(0, ...series.map(s => s[k]));
  const peakActive = Math.max(0, ...series.map(s => s.pending + s.in_progress));
  const peakAt = series.find(s => s.pending + s.in_progress === peakActive)?.t ?? 0;
  const lastActive = [...series].reverse().find(s => s.pending + s.in_progress > 0)?.t ?? null;
  const faultEnd = faultWindows.length ? Math.max(...faultWindows.map(w => w.to)) : null;
  const ledgerFinal = Object.fromEntries(ledger.map(r => [r.status, Number(r.n)]));
  const savedOps = ledgerFinal.saved ?? 0;
  // turso writes
  const tw = Object.entries(tursoStats.counts).filter(([k]) => /^(INSERT|UPDATE|DELETE|REPLACE|CREATE|DROP|ALTER) /.test(k));
  const twTotal = tw.reduce((a, [, n]) => a + n, 0);
  const twLedger = tw.filter(([k]) => k.endsWith('crm_pending_operations')).reduce((a, [, n]) => a + n, 0);
  const trRows = Object.entries(tursoStats.counts).sort((a, b) => b[1] - a[1]).map(([k, v]) => `| ${k} | ${v} |`).join('\n');
  // hubspot calls
  const types = Object.entries(hsStats.byType);
  const hsTotal = types.reduce((a, [, v]) => a + v.ok + v.r429 + v.other, 0);
  const perSec = hsStats.perSec ?? [];
  const sec = Array.from({ length: Math.ceil(finishedAt / 1000) + 1 }, (_, s) => Object.values(perSec[s] ?? {}).reduce((a, n) => a + n, 0));
  const secAdm = Array.from({ length: sec.length }, (_, s) => Object.entries(perSec[s] ?? {}).filter(([k]) => !k.endsWith('|429') && !k.endsWith('|503')).reduce((a, [, n]) => a + n, 0));
  const peakWin = arr => { let m = 0; for (let i = 0; i < arr.length; i++) { let a = 0; for (let j = i; j < Math.min(arr.length, i + 10); j++) a += arr[j]; m = Math.max(m, a); } return m; };
  const peak1s = Math.max(0, ...sec);
  const patchSent = types.filter(([k]) => k.startsWith('patch:')).reduce((a, [, v]) => a + v.ok + v.r429 + v.other, 0);
  const appliedPatch = patches.filter(p => p.status === 200).length;
  const getsSent = types.filter(([k]) => /object|batch|deal|contact|compan/.test(k) && !k.startsWith('patch:')).reduce((a, [, v]) => a + v.ok + v.r429 + v.other, 0);
  // series table (10 s)
  const tl = ['| t (s) | PATCH sent by users | non-2xx | pending | in_progress | failed | saved |', '|---:|---:|---:|---:|---:|---:|---:|'];
  const lastT = series.length ? series[series.length - 1].t : 0;
  for (let s = 0; s <= lastT / 1000; s += 10) {
    const rs = attempts.filter(a => a.start >= s * 1000 && a.start < (s + 10) * 1000);
    const win = series.filter(x => x.t >= s * 1000 && x.t < (s + 10) * 1000);
    const mx = k => Math.max(0, ...win.map(x => x[k]));
    if (!rs.length && !win.length) continue;
    tl.push(`| ${s} | ${rs.length} | ${rs.filter(a => !String(a.status).startsWith('20')).length} | ${mx('pending')} | ${mx('in_progress')} | ${mx('failed')} | ${mx('saved')} |`);
  }
  const lg = appLog.replace(/\x1b\[[0-9;]*m/g, '').split('\n');
  const gc = re => lg.filter(l => re.test(l)).length;
  // drain rate over the last 60 s of the observation, extrapolated
  const tail = series.filter(x => x.t >= series[series.length - 1].t - 60_000);
  const act = x => x.pending + x.in_progress;
  const drainRate = tail.length > 1 ? (act(tail[0]) - act(tail[tail.length - 1])) / ((tail[tail.length - 1].t - tail[0].t) / 1000) : NaN;
  const remaining = act(series[series.length - 1]);
  const eta = remaining > 0 && drainRate > 0 ? remaining / drainRate : null;
  const conf = [...confirmed.values()];
  const confSaved = conf.filter(c => c.status === 'saved').map(c => c.at - c.queuedAt).sort((a, b) => a - b);
  const pollByStatus = count(polls, p => `${p.status}${p.op ? ' ' + p.op : ''}`);
  const getReads = (hsStats.byType['object_get:deals']?.ok ?? 0);
  const failedTxt = failedBy.length ? failedBy.map(r => `${r.code || '-'}/${r.http}: ${r.n}`).join(', ') : 'none';
  return `# CRM write load test — ${OPT.variant} ${OPT.label} (${runId})

- binary: \`${path.relative(ROOT, OPT.bin)}\` (debug), operators: ${OPT.users} (logged in ${vus.length}, login errors ${loginErrors.length}), deals per operator: ${OPT.dealsPerUser}
- scenario: ${OPT.variant === 'v4' ? `rush of ${OPT.rushProps} saves per operator inside ${OPT.rushSeconds} s, then ` : ''}bursts of ${OPT.burstMin}–${OPT.burstMax} property saves (gap ${OPT.burstGapMin}–${OPT.burstGapMax} s), stage move after a burst with p=${OPT.stageMoveProb}, think ${OPT.thinkMin}–${OPT.thinkMax} s, load ${OPT.duration} s (ended at t=${fmt(loadEnd / 1000)} s), client retries ${OPT.retries}x on 429/503/timeout (same operation_id)
- fake HubSpot: latency ${OPT.hsLatencyMs} ± ${OPT.hsJitterMs} ms, limits ${OPT.limit10s}/10 s & ${OPT.limit1s}/s; fake Turso latency ${OPT.tursoLatencyMs} ms/statement; retry backoff ${OPT.backoffSecs} s flat (debug override), worker start delay ${OPT.workerDelaySecs} s
- fault: ${faultWindows.length ? faultWindows.map(w => `${w.name} t=${w.from / 1000}–${w.to / 1000} s`).join('; ') : 'none'}
- observation ended at t=${fmt(finishedAt / 1000)} s; ledger drained: ${drainedAt === null ? 'NOT within the observation window' : `yes, at t=${fmt(drainedAt / 1000)} s`}

## 1. Requests and outcomes (every HTTP attempt of the users, ${attempts.length} PATCH requests for ${nSaves} saves)

| outcome (HTTP) | count | share |
|---|---:|---:|
${tagRows}

Per-request latency (all outcomes): n ${lat.n}, p50 ${fmt(lat.p50)} ms, p95 ${fmt(lat.p95)} ms, p99 ${fmt(lat.p99)} ms, max ${fmt(lat.max)} ms.
200 saved: p50 ${fmt(latSaved.p50)} / p95 ${fmt(latSaved.p95)} / p99 ${fmt(latSaved.p99)} ms (n ${latSaved.n}). 202 queued: p50 ${fmt(latQueued.p50)} / p95 ${fmt(latQueued.p95)} / p99 ${fmt(latQueued.p99)} ms (n ${latQueued.n}).

### Per save (final outcome after the client's retries)

| final outcome | saves | share |
|---|---:|---:|
${finalRows}

- Accepted (200 or 202): ${okFinal} / ${nSaves} = ${fmt(okFinal / nSaves * 100)} %. **Error rate (not accepted at the end): ${fmt(rejectedFinal / nSaves * 100)} %** (of which 409/422: ${conflictInvalid}).
- Error rate per request (non-2xx attempts / all attempts): ${fmt(attempts.filter(a => !String(a.status).startsWith('20')).length / attempts.length * 100)} %.
- Time the user waited per save (first request -> final answer): p50 ${fmt(pct(userDur, 50))} ms, p95 ${fmt(pct(userDur, 95))} ms, p99 ${fmt(pct(userDur, 99))} ms.

## 2. Ledger (crm_pending_operations in the fake Turso)

- Peak rows by status (1 s samples): pending ${peak('pending')}, in_progress ${peak('in_progress')}, failed ${peak('failed')}, saved ${peak('saved')}; peak pending+in_progress ${peakActive} at t=${fmt(peakAt / 1000)} s.
- Last time pending+in_progress > 0: ${lastActive === null ? 'never' : `t=${fmt(lastActive / 1000)} s`}${faultEnd !== null && lastActive !== null ? ` (${fmt((lastActive - faultEnd) / 1000)} s after the fault window ended at t=${faultEnd / 1000} s)` : ''}; time from the end of the load to pending == 0: ${drainedAt === null ? 'not drained' : `${fmt(Math.max(0, drainedAt - loadEnd) / 1000)} s`}.
- Final ledger: ${ledger.map(r => `${r.status} ${r.n} (attempts sum ${r.att})`).join(', ') || 'empty'}. failed rows by code/http: ${failedTxt}.
- Drain rate in the last 60 s of observation: ${fmt(drainRate)} rows/s; remaining ${remaining}; ${remaining === 0 ? 'drained' : eta === null ? 'cannot extrapolate' : `extrapolated time to empty: ${fmt(eta / 60)} min (${fmt(eta)} s) after the observation ended`}.
- What the screen shows after a 202 (GET /api/crm/operations/{id} every 10 s, up to 10 min; ${polls.length} polls): ${conf.length} queued operations; confirmed saved ${confSaved.length} (queued -> green: p50 ${fmt(pct(confSaved, 50) / 1000)} s, p95 ${fmt(pct(confSaved, 95) / 1000)} s, max ${fmt((confSaved[confSaved.length - 1] ?? NaN) / 1000)} s), shown as failed ${conf.filter(c => c.status === 'failed').length}, still yellow when the observation ended ${conf.filter(c => c.status === 'unconfirmed').length}. Poll answers: ${Object.entries(pollByStatus).map(([k, v]) => `${k} ${v}`).join(', ') || '-'}.
- Saved rows by number of attempts: ${savedAtt.map(r => `${r.attempts} attempt(s): ${r.n}`).join(', ') || '-'}.

### Time series (10 s buckets: max of the 1 s samples)

${tl.join('\n')}

## 3. Consistency (fake HubSpot final state vs what each operator believes was saved)

- Checked (deal, property) pairs the operators touched: ${mism.checked}; mismatches: ${mism.total}.
- Not yet applied (accepted with 202, the ledger row is still pending/in_progress when observation ended): ${mism.notYet.length}
- Ledger gave up (accepted with 202, ledger row ended failed): ${mism.ledgerFailed.length}${mism.ledgerFailed.length ? ` e.g. ${JSON.stringify(mism.ledgerFailed.slice(0, 2))}` : ''}
- **Lost (accepted with 200/202, ledger says saved or has no row, but HubSpot holds another value): ${mism.lost.length}**${mism.lost.length ? ` e.g. ${JSON.stringify(mism.lost.slice(0, 3))}` : ''}
- Phantom (reported as failed to the user, but the value is in HubSpot): ${mism.phantom.length}${mism.phantom.length ? ` e.g. ${JSON.stringify(mism.phantom.slice(0, 3))}` : ''}
- Duplicated writes (the same unique value applied twice by HubSpot): ${duplicates}.
- Stage differs from what the operator believes: ${mism.stage} / ${mism.stageChecked} deals (includes stage moves still queued).
- Accepted saves with no ledger row at the end: ${saves.filter(s => (s.outcome === 'saved_200' || s.outcome === 'queued_202') && !ledgerOps.has(s.op)).length}; accepted-202 saves whose ledger row ended in failed: ${saves.filter(s => s.outcome === 'queued_202' && ledgerOps.get(s.op) === 'failed').length}; ledger rows still pending/in_progress at the end: ${(ledgerFinal.pending ?? 0) + (ledgerFinal.in_progress ?? 0)}.

## 4. Turso statements (counted at the fake, from the start of the load)

- Write statements (INSERT/UPDATE/DELETE/DDL): ${twTotal}, of which on crm_pending_operations ${twLedger}. Ledger rows that ended 'saved': ${savedOps}.
- Write statements per saved operation: ${savedOps ? fmt(twTotal / savedOps) : '-'} (ledger only: ${savedOps ? fmt(twLedger / savedOps) : '-'}). Per accepted save: ${okFinal ? fmt(twTotal / okFinal) : '-'}.
- Write statements per user request (PATCH attempt, accepted or not): ${fmt(twTotal / attempts.length)} — every request is logged to activity_logs, including the rejected ones.
- Reads: ${Object.entries(tursoStats.counts).filter(([k]) => k.startsWith('SELECT')).reduce((a, [, n]) => a + n, 0)} SELECT statements.
- Failed statements while the fake was down: ${tursoStats.failedWhileDown}

| statement | count |
|---|---:|
${trRows}

## 5. HubSpot side (fake)

- Requests received: ${hsTotal} (PATCH ${patchSent}, applied PATCH ${appliedPatch}, other (reads) ${hsTotal - patchSent}). Per saved operation: ${savedOps ? fmt(hsTotal / savedOps) : '-'} calls; per save action ${fmt(hsTotal / nSaves)}.
- Peak requests in 1 s: ${peak1s}; peak in any 10 s window: ${peakWin(sec)} (excluding 429/503 rejections: ${peakWin(secAdm)}); peak in-flight at the fake: ${hsStats.peakInFlight}.
- Reads of the deal (prepare step) ${getReads} vs PATCH applied ${appliedPatch}: ${appliedPatch ? fmt(getReads / appliedPatch) : '-'} reads per applied PATCH (reads whose PATCH was then refused by the app's gateway or by HubSpot are wasted calls).
- 429 issued by the fake: ${Object.entries(hsStats.policy429 ?? {}).map(([k, v]) => `${k} ${v}`).join(', ') || 'none'}; injected 503/504: ${hsStats.chaos503 ?? 0}.

| type | ok | 429 | other |
|---|---:|---:|---:|
${types.map(([k, v]) => `| ${k} | ${v.ok} | ${v.r429} | ${v.other} |`).join('\n')}

## 6. App log

- \`HubSpot API を retry します\`: ${gc(/HubSpot API を retry します/)}, crm_timeout: ${gc(/crm_timeout/)}, WARN lines: ${gc(/ WARN /)}, ERROR lines: ${gc(/ ERROR /)}
`;
}

main().catch(e => { console.error(e); stopChildren(); process.exit(1); });
