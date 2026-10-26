// 架空のデータだけを使う。実在する顧客の fixture は参照しない。
export function consultingFixture(count: number, months: number, long: boolean) {
  const name = (i: number) => long ? `架空の検証専用法人とても長い拠点名と担当者名ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789営業所${i}` : i % 2 ? 'あ' : `架空の拠点${i}`;
  const views = ['top', 'silent', 'no_mtg', 'mtg_risk', 'stay_esc', 'stay_long', 'nps_low'];
  const meta = { today: '2026-10-18', source_as_of: '2026-10-18 09:00:00', source_age_days: 0, n_active: count, n_deals: count, n_consultant: count, n_hit: count, n_shown: count, n_not_started: 0, act_views: views.map((key, i) => ({ key, label: ['最優先', '電話での接触なし', '初回のMTGなし', 'MTGでリスク高', '満了後も稼働中', '同じステージに45日超', '満足度が4以下'][i], rule: '架空の確認用の条件', n: count })), threshold_sec: 60, not_counted: '担当者の評価ではありません。仕事量を見る一覧です。' };
  const flags = ['採用目標の半分に届いていない', '満了まで60日以内', 'MTGが60日以上空いている', '接触の記録が無い', '満足度が4以下', '満了90日前でMTGが30日以上空いている'];
  const rows = Array.from({ length: count }, (_, i) => ({ deal_id: `sample-${i}`, houjin: `sample-${i}`, name: name(i), site: name(i), consultant: name(i), flags, views, mtg_risk: '高', n_flags: flags.length, amount: [1234567890, 0, null, -123456789][i % 4], days_left: i % 2 ? -7 : 7, is_active: true, start: months > 24 ? '2024-01-01' : '2026-10-12', expiration: '2026-10-31', expiry: '2026-10-31', renewal_no: i % 3, stage: name(i), n_active: 1, unattended: 1, n_flags2: 1, deals: 1, sites: 1, active: 1, ltv: 1234567890, spread: 2, prev: 100, last: 200, ratio: 2 }));
  const monthly = Array.from({ length: months }, (_, i) => ({ month: `${2024 + Math.floor(i / 12)}-${String(i % 12 + 1).padStart(2, '0')}`, calls: i % 3 ? 123456789 : 0, contacts: i % 3 ? 30 : 0, n: i % 3 ? 40 : 0, denom: 40, keep: 30, cancel: 5, fill: 5, pending: 0, rate: 75 }));
  const contactUnit = {
    periods: monthly.map(m => ({ key: m.month, label: m.month, provisional: false, calls_missing: false })),
    rows: rows.map(r => ({ consultant: r.consultant, cells: monthly.map((_, i) => ({ deals: count, contacts: count * (i % 3), avg: i % 3, small_n: count < 3 })) })),
    team: monthly.map((_, i) => ({ deals: count, contacts: count * (i % 3), avg: count ? i % 3 : null, small_n: count < 3 })),
    undetermined: monthly.map(() => ({ deals: 0, contacts: 0, avg: null })), shared: monthly.map(() => 0), moved: monthly.map(() => 0),
  };
  const contact = { meta: { ...meta, min_deals: 3 }, month: contactUnit, week: contactUnit };
  const box = { n: 80, median: 12345678, q1: 10000000, q3: 123456789, min: 0, max: 1234567890, mean: 123456789 };
  const focus = { meta, nps_low: { threshold: 4, n: count, n_have_nps: count, n_act: count, coverage: count ? 100 : null, dist: count ? [{ v: 0, n: count }] : [], rows, note: '' }, cpa: { worse: count, judged: count, skipped_censored: 0, rows, note: '' }, mtg_layers: { both: 0, only_recording: 0, only_mail: 0, neither: count, n_act: count, fact_recording: {}, estimated_mail: {}, note: '' }, shape: { ltv: count >= 30 ? box : null, display_label: '', n_all: count, n_display: count, multi_site: 0, multi_site_note: '' } };
  const renewal = { meta, by_renewal: count ? [{ renewal_no: 0, denom: 40, n: 40, cancel_rate: 25, cancel_rate_excl_fill: 12.5 }] : [], monthly_retention: { rows: monthly }, survival: { rows: [] } };
  const outcome = { meta, goal_act: { bands: [], fill_rate: 0, has_goal: 0, pop: count, median: null }, goal_all: {}, efficiency: { groups: count >= 30 ? [{ label: '継続済', box }] : [], caveat: '', has_keisaisu_act: 0, n_act: count }, risk: { bands: [], n_act: count, ax3: {}, ax4: {}, top: [], order_note: '' }, contact_source: {} };
  const rampup = { meta, phase: { rows: [], rule: '' }, first_mtg: { n: 0, pre_contract: 0, stats: null, buckets: [] }, no_mtg: { n: count, first_active: count, rate: 100, note: '', rows } };
  const dq = { meta, houjin_source: { rows: [], note: '' }, outcome_bias: { rows: [], note: '' }, missing: count ? [{ label: name(0), n: count, rate: 100, note: '' }] : [], sheets: rows.map(r => ({ name: r.name, rows: count })) };
  const mtg = { meta: { ...meta, n_mtg: count > 1 ? count : 0 }, linked: { n: 0, rate: null }, filled: count > 1 ? [{ field: name(0), n: count, rate: 100 }] : [], risk_dist: [], hosts: count > 1 ? rows.map(r => ({ host: r.consultant, n: count })) : [], monthly: count > 1 ? monthly : [] };
  const customer = { meta: { ...meta, found: true, houjin: 'sample-0' }, customer: { ...rows[0], name: name(0) }, focus: { n_focus: count, n_display: count }, deals: rows, mtgs: [], cpa3: [], cpa_by_site: [], monthly: rows.slice(0, 6).map(r => ({ deal_id: r.deal_id, name: r.name, start: r.start, period: months, series: { oubo: monthly.map((m, i) => ({ m: i + 1, v: m.calls, month: m.month })) }, nps: {} })), handover: [], contacts: [], funnel: { oubo: null, mensetu: 0, syoudaku: null }, events: [] };
  const events = count > 1 ? rows.map((r, i) => ({ kind: i % 2 ? 'mtg' : 'call', date: i % 3 ? '2026-10-15' : '2024-01-01', time: '09:00', contact: true, duration_sec: 120, host: r.consultant, handler: r.consultant, extracted: true, todo: r.name, concern: r.name, minutes: 60, summary: { summary: r.name, next_action: r.name, concern: r.name }, source_label: '架空の確認用' })) : [];
  return { rows, name, api: (path: string, params: URLSearchParams): unknown => {
    switch (path.split('/').pop()) {
      case 'team': return { meta, consultants: { rows: [], meta: {}, contact_rule: '' }, status: { rows, meta: { n_active: count, n_flags2: count, min_flags: 2, week_days: 7 } }, contact };
      case 'today': return { meta, rows, candidates: rows, consultants: rows.map(r => r.consultant), expiring_this_week: rows, started_this_week: rows };
      case 'deals': return { meta, rows };
      case 'deal-detail': return params.get('id') ? { meta: { ...meta, found: true, deal_id: 'sample-0', summary_sheet: 'ok' }, deal: rows[0], counts: {}, chain: { rows }, events } : { meta, search: { q: params.get('q') || '', rows, n_match: count } };
      case 'customer': return params.has('houjin') ? customer : { meta, index: rows, focus: customer.focus };
      case 'focus': return focus;
      case 'results': return { meta, handover: { meta: { n: count, n_active: count }, rows: rows.map(r => ({ ...r, date: '2026-10-01', from: r.consultant, to: r.consultant })), reflected_dist: [] }, pending_recordings: { rows: [] }, money: { active_total: { n: count, amount: count ? 1234567890 : null }, window: { sum: { n: count, amount: count ? 1234567890 : 0 }, months: ['2026-10', '2026-11', '2026-12'] } }, renewal, outcome, rampup, focus, headquarters: { meta, rows: rows.map(r => ({ ...r, rows: rows.slice(0, 3).map(x => ({ ...x, cpa: 123456789, syoudaku: 1, denom: 40, cancel_rate: 25 })) })), multi_site: count } };
      case 'renewal-pipe': return { meta: { ...meta, window: ['2026-10', '2026-11', '2026-12'] }, rows, months: [0, 1, 2].map(i => ({ n: i ? 0 : count, amount: 1234567890, amount_missing: 0 })), stages: rows.map(r => r.stage), window_total: { n: count, amount: 1234567890 }, active_total: { n: count, amount: 1234567890 } };
      case 'phone': return { meta, reach: { no_call: count, no_contact: count, no_call_rate: 100, no_contact_rate: 100, note: '' }, days_since: null, transcript: { rate: null, n: 0, rows: 0, note: '' }, silent: { n: count, rule: '', rows }, monthly };
      case 'data-quality': return dq;
      case 'mtg-quality': return mtg;
      case 'contact-trend': return contact;
      default: throw new Error(`未定義のAPI: ${path}`);
    }
  } };
}
