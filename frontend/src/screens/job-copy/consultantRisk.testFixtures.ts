import type { RiskResponse, RiskRow } from './ConsultantRisk';
export const riskRow: RiskRow = { deal_id: '101', name: '架空の採用支援A', company: '架空の運輸A', consultant: '架空の担当A', flags: ['NPSが4以下', '満了まで60日以内'], risk_flags: [{ key: 'NPSが4以下', label: 'NPSが4以下', new: true, since: '2026-10-19' }, { key: '満了まで60日以内', label: '満了まで60日以内', new: false, since: '2026-10-16' }], days_left: 13, days_since_contact: 1, mtg_last: '2026-10-01', actionyotei: '2026-10-21', actionnaiyou: '電話' };
export const ongoingRiskRow: RiskRow = { ...riskRow, deal_id: '103', name: '架空の採用支援C', company: '架空の運輸C', flags: ['満了まで60日以内'], risk_flags: riskRow.risk_flags.filter(flag => !flag.new) };
export const newRiskRow: RiskRow = { ...riskRow, deal_id: '104', name: '架空の採用支援D', company: '架空の運輸D', days_left: 90, flags: ['NPSが4以下'], risk_flags: riskRow.risk_flags.filter(flag => flag.new) };
export const riskFixture: RiskResponse = { status: 'ready', data_day: '2026-10-19', previous_day: '2026-10-16', comparison: 'ready', rows: [riskRow, ongoingRiskRow, newRiskRow] };
export const scopeFixture = { status: 'ready', owner: '11', owner_field: 'consultant', notice: null, owners: [{ id: '11', name: '架空の担当A', archived: false }, { id: '22', name: '架空の担当B', archived: false }] };
export const detailFixture = { meta: { found: true }, deal: { name: riskRow.name, start: '2026-08-01', expiration: '2026-11-01', period: 3, amount: 300000 }, events: [{ kind: 'call', date: '2026-10-18', duration_sec: 120, contact: true, source_label: '電話の記録', fact: true, summary: { summary: '採用状況を確認し、次の打ち合わせを決めた。', next_action: '募集内容を確認する' } }, { kind: 'mtg', date: '2026-10-01', source_label: 'Zoom 録画（事実）', fact: true, extracted: true, todo: '求人内容の見直し', concern: '応募の減少', positive: '採用を継続', risk: '要確認' }] };
export const goodRow: RiskRow = { ...riskRow, risk_flags: [], good_flags: [
  { key: 'applications', label: '応募 3 → 7', new: false, since: null },
  { key: 'cpa', label: '採用単価 5.0万円（区分の中央値 10.0万円）', new: false, since: null },
  { key: 'positive', label: 'MTG：採用を継続し、次の募集職種を増やす方向で相談する', new: false, since: null },
] };
export const goodFixture: RiskResponse = { ...riskFixture, comparison: 'unavailable', previous_day: null, rows: [goodRow, { ...ongoingRiskRow, risk_flags: [], good_flags: (goodRow.good_flags ?? []).slice(0, 2) }, { ...newRiskRow, flags: [], risk_flags: [], good_flags: (goodRow.good_flags ?? []).slice(0, 1) }] };
