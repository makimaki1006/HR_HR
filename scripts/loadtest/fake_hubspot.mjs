#!/usr/bin/env node
// Fake HubSpot (CRM API v3/v4) + fake Google OIDC for the CRM call-screen load test.
// No dependencies (Node >= 18). Never talks to the real HubSpot / Google.
//
// HubSpot side: serves the endpoints src/hubspot/client.rs uses for the CRM screen
// (search, batch read, objects, associations v3/v4, owners, pipelines, properties, property groups,
// association labels, access-token-info) with synthetic data, emulated latency and rate limits:
//   - general limit: --limit-10s (190) per rolling 10 s and --limit-1s (19) per rolling 1 s
//   - Search: --search-per-sec (5) per rolling 1 s, separate from the general limit (as HubSpot documents)
//   - 429 with Retry-After and x-hubspot-ratelimit-* headers (headers are also sent on success, except Search)
//   - optional background consumer (--background-rps) that eats general budget like the external batch
// Google side (prefix /oidc): discovery, authorize (auto-approves; email from login_hint), token (RS256
// signed with tests/fixtures/oidc/test_key_1.pem), jwks. Works only with the debug build's
// GOOGLE_OIDC_DISCOVERY_URL_DEBUG override.
//
// Control: GET /_stats (counters + per-second timeline), POST /_reset, GET /_health.
//
// Usage: node scripts/loadtest/fake_hubspot.mjs [--port 9400] [--latency-ms 400] [--jitter-ms 150]
//        [--limit-10s 190] [--limit-1s 19] [--search-per-sec 5] [--background-rps 0] [--deals 20000] [--seed 1]

import http from 'node:http';
import crypto from 'node:crypto';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(HERE, '../..');

// ---------------------------------------------------------------------------
// options
// ---------------------------------------------------------------------------
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
const args = parseArgs(process.argv.slice(2));
const num = (k, d) => (args[k] !== undefined ? Number(args[k]) : d);
const OPT = {
  port: num('port', 9400),
  latencyMs: num('latency-ms', 400),
  jitterMs: num('jitter-ms', 150),
  limit10s: num('limit-10s', 190),
  limit1s: num('limit-1s', 19),
  searchPerSec: num('search-per-sec', 5),
  backgroundRps: num('background-rps', 0),
  rateLimitedLatencyMs: num('429-latency-ms', 40),
  deals: num('deals', 20000),
  owners: num('owners', 150),
  seed: num('seed', 1),
  searchCountsGeneral: args['search-counts-general'] === 'true',
};
const BASE = `http://127.0.0.1:${OPT.port}`;

// ---------------------------------------------------------------------------
// deterministic synthetic data
// ---------------------------------------------------------------------------
function mulberry32(a) {
  return function () {
    a |= 0; a = (a + 0x6d2b79f5) | 0;
    let t = Math.imul(a ^ (a >>> 15), 1 | a);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}
/** stable pseudo-random in [0,1) from integers */
function h01(...xs) {
  let a = OPT.seed * 2654435761;
  for (const x of xs) a = Math.imul(a ^ (x >>> 0), 2246822519) ^ Math.floor(x / 4294967296);
  return mulberry32(a)();
}
const pick = (arr, r) => arr[Math.floor(r * arr.length) % arr.length];

const pipelinesJson = JSON.parse(
  fs.readFileSync(path.join(ROOT, 'frontend/src/generated/call_queue_pipelines.json'), 'utf8'),
);
const QUEUE_PIPELINES = pipelinesJson.pipelines; // [{id, fallback_name, stages:[{id, rule}]}]

const DAY = 86_400_000;
function jstTodayUtcMidnightMs() {
  const jst = new Date(Date.now() + 9 * 3600_000);
  return Date.UTC(jst.getUTCFullYear(), jst.getUTCMonth(), jst.getUTCDate());
}
const TODAY = jstTodayUtcMidnightMs();
const ymd = ms => new Date(ms).toISOString().slice(0, 10);

const DEAL_BASE = 9_000_000_000;
const CONTACT_BASE = 7_000_000_000;
const CONTACT2_BASE = 7_100_000_000;
const COMPANY_BASE = 8_000_000_000;
const CALL_BASE = 5_000_000_000; // deal i: + i*32 + k (k<16 deal calls, 16..31 contact-only calls)
const NOTE_BASE = 5_100_000_000; // + i*8 + k
const EMAIL_BASE = 5_200_000_000; // + i*4 + k
const MEETING_BASE = 5_300_000_000; // + i*2 + k
const N_COMPANIES = Math.max(1, Math.floor(OPT.deals * 0.4));
const OWNER_BASE = 80_000_000;

const PREFS = ['東京', '大阪', '愛知', '福岡', '北海道', '宮城', '広島', '神奈川', '埼玉', '千葉'];
const KINDS = ['建設', '運輸', '介護', '物流', '製造', '食品', '警備', '清掃', '設備', '不動産'];
const LAST = ['佐藤', '鈴木', '高橋', '田中', '伊藤', '渡辺', '山本', '中村', '小林', '加藤'];
const FIRST = ['太郎', '花子', '一郎', '美咲', '健', '陽子', '翔', '優子', '大輔', '直子'];

// owners: VU emails lt-user000..lt-user119@f-a-c.co.jp resolve to owners 0..119
const owners = [];
for (let n = 0; n < OPT.owners + 20; n++) {
  const archived = n >= OPT.owners;
  owners.push({
    id: String(OWNER_BASE + n),
    email: n < 120 ? `lt-user${String(n).padStart(3, '0')}@f-a-c.co.jp` : `owner${n}@f-a-c.co.jp`,
    firstName: pick(FIRST, h01(n, 1)),
    lastName: pick(LAST, h01(n, 2)),
    userId: 1_000_000 + n,
    archived,
    teams: [{ id: String(500 + (n % 6)), name: `チーム${(n % 6) + 1}`, primary: true }],
    createdAt: '2024-01-01T00:00:00Z',
    updatedAt: '2026-01-01T00:00:00Z',
  });
}

// deals (kept in memory; filters/sorts evaluate over these)
const deals = new Array(OPT.deals);
for (let i = 0; i < OPT.deals; i++) {
  const p = h01(i, 10) < 0.7 ? QUEUE_PIPELINES[0] : QUEUE_PIPELINES.find(x => x.id === 'default') ?? QUEUE_PIPELINES[1];
  // weight the "all" stage (未済) heavier
  const allStage = p.stages.find(s => s.rule === 'all');
  const stage = allStage && h01(i, 11) < 0.35 ? allStage.id : pick(p.stages, h01(i, 12)).id;
  const ownerN = Math.floor(h01(i, 13) * 120);
  const props = {
    dealname: `株式会社${pick(PREFS, h01(i, 14))}${pick(KINDS, h01(i, 15))}${i} 求人掲載`,
    dealstage: stage,
    pipeline: p.id,
    hubspot_owner_id: h01(i, 16) < 0.05 ? null : String(OWNER_BASE + ownerN),
    bpo_13: h01(i, 17) < 0.3 ? null : ymd(TODAY + Math.floor((h01(i, 18) - 0.5) * 60) * DAY),
    bpo_14: h01(i, 19) < 0.5 ? null : `${10 + Math.floor(h01(i, 20) * 8)}:00`,
    bpo_20: h01(i, 21) < 0.4 ? null : ymd(TODAY - Math.floor(h01(i, 22) * 60) * DAY),
    bpo_3: h01(i, 23) < 0.03 ? '先方NG' : null,
    bpo_4: h01(i, 24) < 0.03 ? '受付ブロック' : null,
    bpo_10: h01(i, 25) < 0.1 ? 'true' : null,
    bpo_29: h01(i, 26) < 0.5 ? `03-${String(1000 + (i % 9000)).padStart(4, '0')}-${String(i % 10000).padStart(4, '0')}` : null,
    bpo_32: `https://example.invalid/jobs/${i}`,
    amount: String(Math.floor(h01(i, 27) * 50) * 10000),
    closedate: ymd(TODAY + Math.floor(h01(i, 28) * 90) * DAY),
    hs_object_id: String(DEAL_BASE + i),
    createdate: '2026-01-15T00:00:00.000Z',
    hs_lastmodifieddate: '2026-10-01T00:00:00.000Z',
  };
  deals[i] = { id: String(DEAL_BASE + i), i, props };
}
const DATE_PROPS = new Set(['bpo_13', 'bpo_20', 'closedate', 'createdate', 'hs_lastmodifieddate', 'hs_timestamp']);
function propNumeric(name, v) {
  if (v === null || v === undefined || v === '') return null;
  if (DATE_PROPS.has(name) || /^\d{4}-\d{2}-\d{2}/.test(v)) {
    const t = Date.parse(v.length === 10 ? `${v}T00:00:00Z` : v);
    if (!Number.isNaN(t)) return t;
  }
  const n = Number(v);
  return Number.isNaN(n) ? null : n;
}

const nCalls = i => Math.floor(h01(i, 30) * 15); // 0..14
const nContactCalls = i => Math.floor(h01(i, 31) * 5); // 0..4
const nNotes = i => Math.floor(h01(i, 32) * 6);
const nEmails = i => Math.floor(h01(i, 33) * 3);
const nMeetings = i => Math.floor(h01(i, 34) * 2);
const hasContact2 = i => h01(i, 35) < 0.3;
const contactsOf = i => (hasContact2(i) ? [CONTACT_BASE + i, CONTACT2_BASE + i] : [CONTACT_BASE + i]);
const companyOf = i => COMPANY_BASE + (i % N_COMPANIES);
const range = (n, f) => Array.from({ length: n }, (_, k) => f(k));
const dealCallIds = i => range(nCalls(i), k => CALL_BASE + i * 32 + k);
const contactOnlyCallIds = i => range(nContactCalls(i), k => CALL_BASE + i * 32 + 16 + k);

function contactProps(id) {
  const secondary = id >= CONTACT2_BASE;
  const i = id - (secondary ? CONTACT2_BASE : CONTACT_BASE);
  return {
    firstname: pick(FIRST, h01(i, secondary ? 41 : 40)),
    lastname: pick(LAST, h01(i, secondary ? 43 : 42)),
    phone: h01(i, 44) < 0.7 ? `090-${String(i % 10000).padStart(4, '0')}-${String((i * 7) % 10000).padStart(4, '0')}` : null,
    mobilephone: h01(i, 45) < 0.3 ? `080-${String(i % 10000).padStart(4, '0')}-0000` : null,
    jobtitle: pick(['総務部長', '人事担当', '代表取締役', '採用担当'], h01(i, 46)),
    email: `contact${i}@example.invalid`,
    hubspot_owner_id: deals[i]?.props.hubspot_owner_id ?? null,
    hs_object_id: String(id),
  };
}
function companyProps(id) {
  const c = id - COMPANY_BASE;
  return {
    name: `株式会社${pick(PREFS, h01(c, 50))}${pick(KINDS, h01(c, 51))}${c}`,
    phone: `06-${String(c % 10000).padStart(4, '0')}-1111`,
    address: `${pick(PREFS, h01(c, 50))}市1-2-3`,
    city: pick(PREFS, h01(c, 50)),
    state: pick(PREFS, h01(c, 50)),
    zip: '100-0001',
    industry: 'STAFFING_AND_RECRUITING',
    domain: `company${c}.example.invalid`,
    website: `https://company${c}.example.invalid`,
    hs_object_id: String(id),
  };
}
function engagementProps(object, id) {
  const ts = new Date(Date.now() - Math.floor(h01(id, 60) * 90) * DAY).toISOString();
  const owner = String(OWNER_BASE + Math.floor(h01(id, 61) * 120));
  switch (object) {
    case 'calls': return {
      hs_timestamp: ts, hs_call_title: '架電', hs_call_body: '<p>不在。折り返し依頼</p>', hs_call_direction: 'OUTBOUND',
      hs_call_status: 'COMPLETED', hs_call_duration: String(Math.floor(h01(id, 62) * 300000)), hs_call_source: 'INTEGRATIONS_PLATFORM',
      hubspot_owner_id: owner, hs_object_id: String(id),
    };
    case 'notes': return { hs_timestamp: ts, hs_note_body: '<p>メモ: 次回 10 時に再架電</p>', hubspot_owner_id: owner, hs_object_id: String(id) };
    case 'emails': return {
      hs_timestamp: ts, hs_email_subject: 'ご案内', hs_email_text: '資料をお送りします', hs_email_direction: 'EMAIL',
      hs_email_status: 'SENT', hubspot_owner_id: owner, hs_object_id: String(id),
    };
    case 'meetings': return {
      hs_timestamp: ts, hs_meeting_title: '商談', hs_meeting_body: 'オンライン', hs_meeting_outcome: 'COMPLETED',
      hubspot_owner_id: owner, hs_object_id: String(id),
    };
    default: return { hs_object_id: String(id) };
  }
}
function dealIndexOf(id) {
  const i = Number(id) - DEAL_BASE;
  return Number.isInteger(i) && i >= 0 && i < OPT.deals ? i : null;
}
function recordExists(object, id) {
  const n = Number(id);
  switch (object) {
    case 'deals': return dealIndexOf(id) !== null;
    case 'contacts': {
      if (n >= CONTACT2_BASE && n < CONTACT2_BASE + OPT.deals) return hasContact2(n - CONTACT2_BASE);
      return n >= CONTACT_BASE && n < CONTACT_BASE + OPT.deals;
    }
    case 'companies': return n >= COMPANY_BASE && n < COMPANY_BASE + N_COMPANIES;
    case 'calls': {
      const r = n - CALL_BASE; if (r < 0 || r >= OPT.deals * 32) return false;
      const i = Math.floor(r / 32), k = r % 32;
      return k < 16 ? k < nCalls(i) : k - 16 < nContactCalls(i);
    }
    case 'notes': { const r = n - NOTE_BASE; return r >= 0 && r < OPT.deals * 8 && r % 8 < nNotes(Math.floor(r / 8)); }
    case 'emails': { const r = n - EMAIL_BASE; return r >= 0 && r < OPT.deals * 4 && r % 4 < nEmails(Math.floor(r / 4)); }
    case 'meetings': { const r = n - MEETING_BASE; return r >= 0 && r < OPT.deals * 2 && r % 2 < nMeetings(Math.floor(r / 2)); }
    default: return false;
  }
}
function allProps(object, id) {
  switch (object) {
    case 'deals': return deals[dealIndexOf(id)].props;
    case 'contacts': return contactProps(Number(id));
    case 'companies': return companyProps(Number(id));
    default: return engagementProps(object, Number(id));
  }
}
function recordJson(object, id, wanted) {
  const all = allProps(object, id);
  const props = {};
  const names = wanted && wanted.length ? wanted : Object.keys(all).slice(0, 6);
  for (const n of names) props[n] = all[n] ?? null;
  props.hs_object_id = String(id);
  if (object === 'deals' || object === 'contacts' || object === 'companies') {
    props.createdate = '2026-01-15T00:00:00.000Z';
    props.hs_lastmodifieddate = '2026-10-01T00:00:00.000Z';
  }
  return { id: String(id), properties: props, createdAt: '2026-01-15T00:00:00.000Z', updatedAt: '2026-10-01T00:00:00.000Z', archived: false };
}

/** to-ids of object `from`/`id` towards `to` (with v4 association types) */
function associationsOf(from, id, to) {
  const n = Number(id);
  const t = (typeId, label = null, category = 'HUBSPOT_DEFINED') => ({ category, typeId, label });
  if (from === 'deals') {
    const i = dealIndexOf(id); if (i === null) return [];
    switch (to) {
      case 'contacts': return contactsOf(i).map(c => ({ id: c, types: [t(3)], v3: 'deal_to_contact' }));
      case 'companies': return [{ id: companyOf(i), types: [t(5, 'Primary'), t(341)], v3: ['deal_to_company', 'deal_to_company_unlabeled'] }];
      case 'calls': return dealCallIds(i).map(c => ({ id: c, types: [t(205)], v3: 'deal_to_call' }));
      case 'notes': return range(nNotes(i), k => ({ id: NOTE_BASE + i * 8 + k, types: [t(213)], v3: 'deal_to_note' }));
      case 'emails': return range(nEmails(i), k => ({ id: EMAIL_BASE + i * 4 + k, types: [t(209)], v3: 'deal_to_email' }));
      case 'meetings': return range(nMeetings(i), k => ({ id: MEETING_BASE + i * 2 + k, types: [t(211)], v3: 'deal_to_meeting' }));
      default: return [];
    }
  }
  if (from === 'contacts') {
    const secondary = n >= CONTACT2_BASE;
    const i = n - (secondary ? CONTACT2_BASE : CONTACT_BASE);
    if (i < 0 || i >= OPT.deals) return [];
    if (to === 'calls') {
      const ids = secondary ? contactOnlyCallIds(i) : [...dealCallIds(i), ...contactOnlyCallIds(i)];
      return ids.map(c => ({ id: c, types: [t(193)], v3: 'contact_to_call' }));
    }
    if (to === 'deals') return [{ id: DEAL_BASE + i, types: [t(4)], v3: 'contact_to_deal' }];
    if (to === 'companies') return [{ id: companyOf(i), types: [t(1, 'Primary'), t(279)], v3: 'contact_to_company' }];
    return [];
  }
  if (from === 'companies' && to === 'deals') {
    const c = n - COMPANY_BASE;
    const out = [];
    for (let i = c; i < OPT.deals && out.length < 20; i += N_COMPANIES) out.push({ id: DEAL_BASE + i, types: [t(6)], v3: 'company_to_deal' });
    return out;
  }
  return [];
}

// properties / groups / pipelines definitions
const PROPERTY_DEFS = {
  deals: [
    ['dealname', '取引名', 'string', 'text', 'dealinformation'],
    ['dealstage', '取引ステージ', 'enumeration', 'radio', 'dealinformation'],
    ['pipeline', 'パイプライン', 'enumeration', 'select', 'dealinformation'],
    ['hubspot_owner_id', '取引担当者', 'enumeration', 'select', 'dealinformation'],
    ['amount', '金額', 'number', 'number', 'dealinformation'],
    ['closedate', '完了予定日', 'date', 'date', 'dealinformation'],
    ['bpo_13', '次回架電日', 'date', 'date', 'bpo'],
    ['bpo_14', '次回架電時間', 'string', 'text', 'bpo'],
    ['bpo_20', '最終架電日', 'date', 'date', 'bpo'],
    ['bpo_3', '架電禁止理由', 'enumeration', 'select', 'bpo', ['先方NG', '番号不使用']],
    ['bpo_4', 'ブロック理由', 'enumeration', 'select', 'bpo', ['受付ブロック', '担当者ブロック']],
    ['bpo_10', '不通時チェック', 'enumeration', 'booleancheckbox', 'bpo', ['true', 'false']],
    ['bpo_29', '架電先電話番号', 'string', 'phonenumber', 'bpo'],
    ['bpo_32', 'URL_求人検索', 'string', 'text', 'bpo'],
    ['bpo_57', 'その他理由', 'string', 'text', 'bpo'],
    ...range(30, k => [`bpo_${60 + k}`, `BPO 項目 ${60 + k}`, 'string', 'text', 'bpo']),
    ...['bpo_16', 'bpo_40', 'bpo_42', 'bpo_45', 'bpo_21', 'bpo_22', 'bpo_50', 'bpo_24', 'bpo_49', 'bpo_34', 'bpo_25', 'bpo_8', 'bpo_23', 'bpo__', 'bpo_33', 'bpo_18', 'bpo_19']
      .map(n => [n, `BPO ${n}`, 'enumeration', 'select', 'bpo', ['A', 'B', 'C']]),
  ],
  contacts: [
    ['firstname', '名', 'string', 'text', 'contactinformation'],
    ['lastname', '姓', 'string', 'text', 'contactinformation'],
    ['email', 'Eメール', 'string', 'text', 'contactinformation'],
    ['phone', '電話番号', 'string', 'phonenumber', 'contactinformation'],
    ['mobilephone', '携帯電話番号', 'string', 'phonenumber', 'contactinformation'],
    ['jobtitle', '役職', 'string', 'text', 'contactinformation'],
    ['hubspot_owner_id', 'コンタクト担当者', 'enumeration', 'select', 'contactinformation'],
    ['hs_lead_status', 'リードステータス', 'enumeration', 'select', 'sales_properties', ['NEW', 'OPEN']],
    ['lifecyclestage', 'ライフサイクルステージ', 'enumeration', 'select', 'contactinformation', ['lead', 'customer']],
    ['notes_last_contacted', '最終コンタクト日', 'datetime', 'date', 'sales_properties'],
  ],
  companies: [
    ['name', '会社名', 'string', 'text', 'companyinformation'],
    ['domain', 'ドメイン', 'string', 'text', 'companyinformation'],
    ['phone', '電話番号', 'string', 'phonenumber', 'companyinformation'],
    ['website', 'ウェブサイト', 'string', 'text', 'companyinformation'],
    ['address', '住所', 'string', 'text', 'companyinformation'],
    ['city', '市区町村', 'string', 'text', 'companyinformation'],
    ['state', '都道府県', 'string', 'text', 'companyinformation'],
    ['zip', '郵便番号', 'string', 'text', 'companyinformation'],
    ['industry', '業種', 'enumeration', 'select', 'companyinformation', ['STAFFING_AND_RECRUITING']],
    ['numberofemployees', '従業員数', 'number', 'number', 'companyinformation'],
    ['hubspot_owner_id', '会社担当者', 'enumeration', 'select', 'companyinformation'],
    ['lifecyclestage', 'ライフサイクルステージ', 'enumeration', 'select', 'companyinformation', ['lead', 'customer']],
  ],
};
function propertiesResponse(object) {
  return {
    results: (PROPERTY_DEFS[object] ?? []).map(([name, label, type, fieldType, groupName, opts], k) => ({
      name, label, type, fieldType, groupName, description: '', displayOrder: k, hidden: false, archived: false,
      calculated: false, hasUniqueValue: false, formField: true, dataSensitivity: 'non_sensitive',
      options: (opts ?? []).map((v, j) => ({ label: v, value: v, displayOrder: j, hidden: false })),
      createdAt: '2024-01-01T00:00:00Z', updatedAt: '2026-01-01T00:00:00Z',
    })),
  };
}
function groupsResponse(object) {
  const names = [...new Set((PROPERTY_DEFS[object] ?? []).map(d => d[4]))];
  return { results: names.map((name, k) => ({ name, label: `${name} グループ`, displayOrder: k, archived: false })) };
}
const STAGE_LABEL = id => `ステージ ${id}`;
function pipelinesResponse() {
  const results = QUEUE_PIPELINES.map((p, k) => ({
    id: p.id, label: p.fallback_name, displayOrder: k, archived: false,
    stages: p.stages.map((s, j) => ({ id: s.id, label: STAGE_LABEL(s.id), displayOrder: j, archived: false, metadata: { probability: '0.1' } })),
  }));
  results.push({ id: '999000001', label: '納品管理', displayOrder: 50, archived: false, stages: [{ id: '999000101', label: '納品済', displayOrder: 0, archived: false, metadata: {} }] });
  return { results };
}

// ---------------------------------------------------------------------------
// search
// ---------------------------------------------------------------------------
function matchFilter(d, f) {
  const v = d.props[f.propertyName] ?? (f.propertyName === 'hs_object_id' ? d.id : null);
  const has = v !== null && v !== undefined && v !== '';
  switch (f.operator) {
    case 'HAS_PROPERTY': return has;
    case 'NOT_HAS_PROPERTY': return !has;
    case 'EQ': return has && String(v) === String(f.value);
    case 'NEQ': return !has || String(v) !== String(f.value);
    case 'IN': return has && (f.values ?? []).map(String).includes(String(v));
    case 'NOT_IN': return !has || !(f.values ?? []).map(String).includes(String(v));
    case 'GT': case 'GTE': case 'LT': case 'LTE': {
      const a = propNumeric(f.propertyName, v); const b = Number(f.value);
      if (a === null || Number.isNaN(b)) return false;
      return f.operator === 'GT' ? a > b : f.operator === 'GTE' ? a >= b : f.operator === 'LT' ? a < b : a <= b;
    }
    case 'BETWEEN': {
      const a = propNumeric(f.propertyName, v);
      return a !== null && a >= Number(f.value) && a <= Number(f.highValue);
    }
    case 'CONTAINS_TOKEN': return has && String(v).toLowerCase().includes(String(f.value).replace(/\*/g, '').toLowerCase());
    default: return false;
  }
}
function runSearch(object, body) {
  if (object !== 'deals') return { status: 200, json: { total: 0, results: [] } };
  const groups = body.filterGroups ?? [];
  const q = typeof body.query === 'string' ? body.query.toLowerCase() : null;
  const limit = Math.min(Number(body.limit ?? 10), 200);
  const after = Number(body.after ?? 0);
  if (after + limit > 10_000) return { status: 400, json: { status: 'error', message: 'paging beyond 10000', category: 'VALIDATION_ERROR' } };
  let rows = deals.filter(d =>
    (groups.length === 0 || groups.some(g => (g.filters ?? []).every(f => matchFilter(d, f)))) &&
    (!q || d.props.dealname.toLowerCase().includes(q)));
  const s = (body.sorts ?? [])[0];
  if (s) {
    const name = typeof s === 'string' ? s : s.propertyName;
    const desc = typeof s === 'object' && s.direction === 'DESCENDING';
    const key = d => (name === 'hs_object_id' ? Number(d.id) : propNumeric(name, d.props[name]));
    rows = rows.slice().sort((a, b) => {
      const x = key(a), y = key(b);
      if (x === null && y === null) return a.i - b.i;
      if (x === null) return 1;
      if (y === null) return -1;
      return (desc ? y - x : x - y) || a.i - b.i;
    });
  }
  const page = rows.slice(after, after + limit);
  const props = body.properties ?? [];
  const json = {
    total: rows.length,
    results: page.map(d => {
      const p = {}; for (const n of props) p[n] = d.props[n] ?? null;
      p.hs_object_id = d.id; p.createdate = d.props.createdate; p.hs_lastmodifieddate = d.props.hs_lastmodifieddate;
      return { id: d.id, properties: p, createdAt: d.props.createdate, updatedAt: d.props.hs_lastmodifieddate, archived: false };
    }),
  };
  if (after + limit < rows.length) json.paging = { next: { after: String(after + limit), link: '' } };
  return { status: 200, json };
}

// ---------------------------------------------------------------------------
// rate limiting + stats
// ---------------------------------------------------------------------------
let t0 = Date.now();
const generalHits = []; // timestamps (ms) of admitted general requests (incl. background)
const searchHits = [];
function prune(arr, now, windowMs) { while (arr.length && arr[0] <= now - windowMs) arr.shift(); }
function countSince(arr, since) {
  // arr sorted ascending; binary search
  let lo = 0, hi = arr.length;
  while (lo < hi) { const m = (lo + hi) >> 1; if (arr[m] > since) hi = m; else lo = m + 1; }
  return arr.length - lo;
}

const stats = newStats();
function newStats() { return { byType: {}, status: {}, perSec: [], inFlight: 0, peakInFlight: 0, background: 0, policy429: {} }; }
function bump(type, outcome) {
  const sec = Math.floor((Date.now() - t0) / 1000);
  while (stats.perSec.length <= sec) stats.perSec.push({});
  const b = stats.perSec[sec];
  const key = `${type}|${outcome}`;
  b[key] = (b[key] ?? 0) + 1;
  const t = (stats.byType[type] ??= { ok: 0, r429: 0, other: 0 });
  if (outcome === '429') t.r429++; else if (outcome === 'ok') t.ok++; else t.other++;
}

/** admit or reject. returns null if admitted, else { retryAfterS, policy } */
function admit(isSearch) {
  const now = Date.now();
  prune(generalHits, now, 10_000);
  prune(searchHits, now, 1_000);
  if (isSearch) {
    if (searchHits.length >= OPT.searchPerSec) {
      return { retryAfterS: Math.max(1, Math.ceil((searchHits[0] + 1000 - now) / 1000)), policy: 'SEARCH_SECONDLY' };
    }
    if (OPT.searchCountsGeneral && generalHits.length >= OPT.limit10s) {
      return { retryAfterS: Math.max(1, Math.ceil((generalHits[0] + 10_000 - now) / 1000)), policy: 'TEN_SECONDLY_ROLLING' };
    }
    searchHits.push(now);
    if (OPT.searchCountsGeneral) generalHits.push(now);
    return null;
  }
  if (generalHits.length >= OPT.limit10s) {
    return { retryAfterS: Math.max(1, Math.ceil((generalHits[0] + 10_000 - now) / 1000)), policy: 'TEN_SECONDLY_ROLLING' };
  }
  if (countSince(generalHits, now - 1000) >= OPT.limit1s) {
    return { retryAfterS: 1, policy: 'SECONDLY' };
  }
  generalHits.push(now);
  return null;
}
function rateHeaders() {
  const now = Date.now();
  prune(generalHits, now, 10_000);
  return {
    'x-hubspot-ratelimit-max': String(OPT.limit10s),
    'x-hubspot-ratelimit-remaining': String(Math.max(0, OPT.limit10s - generalHits.length)),
    'x-hubspot-ratelimit-interval-milliseconds': '10000',
    'x-hubspot-ratelimit-secondly': String(OPT.limit1s),
    'x-hubspot-ratelimit-secondly-remaining': String(Math.max(0, OPT.limit1s - countSince(generalHits, now - 1000))),
    'x-hubspot-ratelimit-daily': '1000000',
    'x-hubspot-ratelimit-daily-remaining': '900000',
  };
}
if (OPT.backgroundRps > 0) {
  // spread evenly; a background request that would be rejected is counted as background_429
  setInterval(() => {
    const r = admit(false);
    stats.background++;
    bump('background', r ? '429' : 'ok');
  }, 1000 / OPT.backgroundRps);
}

// ---------------------------------------------------------------------------
// HTTP
// ---------------------------------------------------------------------------
const sleep = ms => new Promise(r => setTimeout(r, ms));
const latency = () => Math.max(0, OPT.latencyMs + (Math.random() * 2 - 1) * OPT.jitterMs);
function send(res, status, body, headers = {}) {
  const b = Buffer.from(typeof body === 'string' ? body : JSON.stringify(body));
  res.writeHead(status, { 'content-type': typeof body === 'string' ? 'text/html; charset=utf-8' : 'application/json;charset=utf-8', 'content-length': b.length, ...headers });
  res.end(b);
}
function readBody(req) {
  return new Promise(resolve => {
    const chunks = [];
    req.on('data', c => chunks.push(c));
    req.on('end', () => resolve(Buffer.concat(chunks).toString('utf8')));
  });
}

/** classify + handle HubSpot routes. returns { type, isSearch, run: () => {status, json} } or null */
function route(method, url, body) {
  const p = url.pathname;
  let m;
  if (method === 'POST' && p === '/oauth/v2/private-apps/get/access-token-info') {
    return { type: 'token_info', run: () => ({ status: 200, json: { hubId: 12345678, appId: 1, scopes: ['crm.objects.deals.read', 'crm.objects.contacts.read', 'crm.objects.companies.read', 'crm.objects.owners.read', 'crm.schemas.deals.read'], userId: 1, tokenType: 'APP' } }) };
  }
  if (method === 'POST' && (m = p.match(/^\/crm\/v3\/objects\/(\w+)\/search$/))) {
    return { type: 'search', isSearch: true, run: () => runSearch(m[1], JSON.parse(body || '{}')) };
  }
  if (method === 'POST' && (m = p.match(/^\/crm\/v3\/objects\/(\w+)\/batch\/read$/))) {
    const object = m[1];
    return {
      type: `batch_read:${object}`, run: () => {
        const b = JSON.parse(body || '{}');
        const ids = (b.inputs ?? []).map(x => String(x.id));
        if (ids.length > 100) return { status: 400, json: { status: 'error', message: 'too many inputs' } };
        const found = ids.filter(id => recordExists(object, id));
        const missing = ids.filter(id => !recordExists(object, id));
        const json = { status: 'COMPLETE', results: found.map(id => recordJson(object, id, b.properties)), startedAt: new Date().toISOString(), completedAt: new Date().toISOString() };
        if (missing.length) { json.numErrors = 1; json.errors = [{ status: 'error', category: 'OBJECT_NOT_FOUND', message: 'not found', context: { ids: missing } }]; }
        return { status: missing.length ? 207 : 200, json };
      },
    };
  }
  if (method === 'POST' && (m = p.match(/^\/crm\/v4\/associations\/(\w+)\/(\w+)\/batch\/read$/))) {
    const [, from, to] = m;
    return {
      type: `assoc_batch:${from}->${to}`, run: () => {
        const b = JSON.parse(body || '{}');
        const results = (b.inputs ?? []).map(x => ({
          from: { id: String(x.id) },
          to: associationsOf(from, x.id, to).map(a => ({ toObjectId: a.id, associationTypes: a.types })),
        })).filter(r => r.to.length);
        return { status: 200, json: { status: 'COMPLETE', results } };
      },
    };
  }
  if (method === 'GET' && (m = p.match(/^\/crm\/v4\/associations\/(\w+)\/(\w+)\/labels$/))) {
    const [, from, to] = m;
    const labels = from === 'deals' && to === 'companies'
      ? [{ category: 'HUBSPOT_DEFINED', typeId: 5, label: 'Primary' }, { category: 'HUBSPOT_DEFINED', typeId: 341, label: null }]
      : [{ category: 'HUBSPOT_DEFINED', typeId: 3, label: null }];
    return { type: 'assoc_labels', run: () => ({ status: 200, json: { results: labels } }) };
  }
  if (method === 'GET' && (m = p.match(/^\/crm\/v4\/objects\/(\w+)\/(\d+)\/associations\/(\w+)$/))) {
    const [, from, id, to] = m;
    return { type: `assoc_list:${from}->${to}`, run: () => ({ status: 200, json: { results: associationsOf(from, id, to).map(a => ({ toObjectId: a.id, associationTypes: a.types })) } }) };
  }
  if (method === 'GET' && (m = p.match(/^\/crm\/v3\/objects\/(\w+)\/(\d+)$/))) {
    const [, object, id] = m;
    const assocTypes = (url.searchParams.get('associations') ?? '').split(',').filter(Boolean);
    return {
      type: assocTypes.length ? `object_get+assoc:${object}` : `object_get:${object}`, run: () => {
        if (!recordExists(object, id)) return { status: 404, json: { status: 'error', message: 'Object not found', category: 'OBJECT_NOT_FOUND' } };
        const wanted = (url.searchParams.get('properties') ?? '').split(',').filter(Boolean);
        const rec = recordJson(object, id, wanted);
        if (assocTypes.length) {
          rec.associations = {};
          for (const t of assocTypes) {
            const list = associationsOf(object, id, t);
            if (!list.length) continue;
            rec.associations[t] = { results: list.flatMap(a => (Array.isArray(a.v3) ? a.v3 : [a.v3]).map(v3 => ({ id: String(a.id), type: v3 }))) };
          }
        }
        return { status: 200, json: rec };
      },
    };
  }
  if (method === 'GET' && p === '/crm/v3/owners') {
    const email = url.searchParams.get('email');
    if (email) {
      return { type: 'owners_by_email', run: () => ({ status: 200, json: { results: owners.filter(o => o.email.toLowerCase() === email.toLowerCase()) } }) };
    }
    return {
      type: 'owners_page', run: () => {
        const archived = url.searchParams.get('archived') === 'true';
        const list = owners.filter(o => o.archived === archived);
        const limit = Number(url.searchParams.get('limit') ?? 100);
        const after = Number(url.searchParams.get('after') ?? 0);
        const json = { results: list.slice(after, after + limit) };
        if (after + limit < list.length) json.paging = { next: { after: String(after + limit) } };
        return { status: 200, json };
      },
    };
  }
  if (method === 'GET' && p === '/crm/v3/pipelines/deals') return { type: 'pipelines', run: () => ({ status: 200, json: pipelinesResponse() }) };
  if (method === 'GET' && (m = p.match(/^\/crm\/v3\/properties\/(\w+)\/groups$/))) return { type: `property_groups:${m[1]}`, run: () => ({ status: 200, json: groupsResponse(m[1]) }) };
  if (method === 'GET' && (m = p.match(/^\/crm\/v3\/properties\/(\w+)$/))) return { type: `properties:${m[1]}`, run: () => ({ status: 200, json: propertiesResponse(m[1]) }) };
  return null;
}

// --- fake Google OIDC (/oidc/*) ---
const OIDC_ISSUER = `${BASE}/oidc`;
const OIDC_KEY = fs.readFileSync(path.join(ROOT, 'tests/fixtures/oidc/test_key_1.pem'), 'utf8');
const OIDC_JWKS = fs.readFileSync(path.join(ROOT, 'tests/fixtures/oidc/jwks.json'), 'utf8');
const oidcCodes = new Map();
const b64u = b => Buffer.from(b).toString('base64url');
function signIdToken(claims) {
  const head = b64u(JSON.stringify({ alg: 'RS256', typ: 'JWT', kid: 'test-key-1' }));
  const payload = b64u(JSON.stringify(claims));
  const sig = crypto.createSign('RSA-SHA256').update(`${head}.${payload}`).sign(OIDC_KEY).toString('base64url');
  return `${head}.${payload}.${sig}`;
}
async function handleOidc(req, res, url) {
  const p = url.pathname.slice('/oidc'.length);
  if (p === '/.well-known/openid-configuration') {
    return send(res, 200, { issuer: OIDC_ISSUER, authorization_endpoint: `${OIDC_ISSUER}/authorize`, token_endpoint: `${OIDC_ISSUER}/token`, jwks_uri: `${OIDC_ISSUER}/jwks` });
  }
  if (p === '/jwks') return send(res, 200, JSON.parse(OIDC_JWKS));
  if (p === '/authorize') {
    const q = url.searchParams;
    const email = (q.get('login_hint') || 'taro@f-a-c.co.jp').toLowerCase();
    const code = crypto.randomBytes(16).toString('base64url');
    oidcCodes.set(code, { email, nonce: q.get('nonce'), clientId: q.get('client_id') });
    const ru = new URL(q.get('redirect_uri'));
    ru.searchParams.set('code', code);
    ru.searchParams.set('state', q.get('state'));
    res.writeHead(302, { location: ru.toString() });
    return res.end();
  }
  if (p === '/token' && req.method === 'POST') {
    const form = new URLSearchParams(await readBody(req));
    const c = oidcCodes.get(form.get('code'));
    oidcCodes.delete(form.get('code'));
    if (!c) return send(res, 400, { error: 'invalid_grant' });
    const now = Math.floor(Date.now() / 1000);
    const id_token = signIdToken({ iss: OIDC_ISSUER, aud: c.clientId || form.get('client_id'), sub: crypto.createHash('sha256').update(c.email).digest('hex').slice(0, 20), email: c.email, email_verified: true, hd: 'f-a-c.co.jp', nonce: c.nonce, iat: now, exp: now + 3600 });
    return send(res, 200, { access_token: 'x', token_type: 'Bearer', expires_in: 3600, id_token });
  }
  return send(res, 404, { error: 'not found' });
}

const server = http.createServer(async (req, res) => {
  const url = new URL(req.url, BASE);
  try {
    if (url.pathname.startsWith('/oidc/')) return await handleOidc(req, res, url);
    if (url.pathname === '/_health') return send(res, 200, { ok: true });
    if (url.pathname === '/_reset') {
      t0 = Date.now();
      Object.assign(stats, newStats());
      return send(res, 200, { ok: true });
    }
    if (url.pathname === '/_stats') {
      return send(res, 200, { t0, now: Date.now(), options: OPT, deals: OPT.deals, ...stats });
    }
    const body = req.method === 'POST' ? await readBody(req) : '';
    if (!/^Bearer \S+/.test(req.headers.authorization ?? '')) {
      bump('unauthenticated', 'other');
      return send(res, 401, { status: 'error', message: 'Authentication credentials not found.', category: 'INVALID_AUTHENTICATION' });
    }
    const r = route(req.method, url, body);
    if (!r) {
      bump(`unknown:${req.method} ${url.pathname.replace(/\d+/g, ':id')}`, 'other');
      return send(res, 404, { status: 'error', message: 'unknown endpoint (fake)', category: 'OBJECT_NOT_FOUND' });
    }
    stats.inFlight++;
    stats.peakInFlight = Math.max(stats.peakInFlight, stats.inFlight);
    try {
      const rejected = admit(Boolean(r.isSearch));
      if (rejected) {
        bump(r.type, '429');
        stats.policy429[rejected.policy] = (stats.policy429[rejected.policy] ?? 0) + 1;
        await sleep(OPT.rateLimitedLatencyMs);
        const h = { 'retry-after': String(rejected.retryAfterS), ...(r.isSearch ? {} : rateHeaders()) };
        return send(res, 429, { status: 'error', message: `You have reached your ${rejected.policy === 'SECONDLY' ? 'secondly' : 'ten secondly rolling'} limit.`, errorType: 'RATE_LIMIT', correlationId: crypto.randomUUID(), policyName: rejected.policy }, h);
      }
      const out = r.run();
      await sleep(latency());
      bump(r.type, out.status < 300 ? 'ok' : String(out.status));
      return send(res, out.status, out.json, r.isSearch ? {} : rateHeaders());
    } finally {
      stats.inFlight--;
    }
  } catch (e) {
    bump('fake_error', 'other');
    return send(res, 500, { status: 'error', message: `fake server error: ${e.message}` });
  }
});
server.keepAliveTimeout = 65_000;
server.listen(OPT.port, '127.0.0.1', () => {
  console.log(`[fake_hubspot] listening on ${BASE} (deals=${OPT.deals}, latency=${OPT.latencyMs}±${OPT.jitterMs}ms, ` +
    `limits=${OPT.limit10s}/10s & ${OPT.limit1s}/s, search=${OPT.searchPerSec}/s, background=${OPT.backgroundRps} rps)`);
});
