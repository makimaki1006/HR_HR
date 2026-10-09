// 合成データ: fixture に無い入力 (担当なし・名簿に居ない担当者) を、fixture の JSON を書き換えて作る。
// tests/e2e/sales_kpi_card_breakdown.py の blank_owner_input / scenario_pick_blank と同じ作り方。
import type { SalesKpiData } from '../types';
import { loadFixture } from './load';

/** チーム未設定のうち、今月の母集団 (pool) が最も多い人の id。 */
export function pickUnsetOwner(D: SalesKpiData): string {
  const unset = D.people.filter((p) => p.team === 'チーム未設定');
  let best = unset[0];
  for (const p of unset) {
    if ((D.by_person[p.id]?.pool ?? 0) > (best ? (D.by_person[best.id]?.pool ?? 0) : -1)) best = p;
  }
  if (!best) throw new Error('fixture にチーム未設定の人が居ない');
  return best.id;
}

function rewriteId(x: unknown, from: string, to: string): unknown {
  if (Array.isArray(x)) return x.map((v) => rewriteId(v, from, to));
  if (x && typeof x === 'object') {
    return Object.fromEntries(
      Object.entries(x as Record<string, unknown>).map(([k, v]) => [k === from ? to : k, rewriteId(v, from, to)]),
    );
  }
  return x === from ? to : x;
}

/**
 * 「担当なし」: チーム未設定の 1 人 (pool 最大) の id を空文字に書き換える (ownerId が空の取引)。
 * サーバの実際の形に合わせ、架電リスト・架電・決定者の人別には担当なしを入れない (no_owner / 別扱い)。
 */
export function withBlankOwner(D: SalesKpiData = loadFixture()): { data: SalesKpiData; oldId: string } {
  const oldId = pickUnsetOwner(D);
  const d = rewriteId(JSON.parse(JSON.stringify(D)), oldId, '') as SalesKpiData;
  for (const p of d.people) {
    if (p.id === '') {
      p.name = '担当なし';
      p.team = 'チーム未設定';
    }
  }
  for (const k of ['pool', 'apo', 'cyomi'] as const) {
    for (const r of d.card_deals[k]) {
      if (r.owner === '') {
        r.ownerName = '担当なし';
        r.team = 'チーム未設定';
      }
    }
  }
  for (const list of [d.stale, d.week_deals, d.next_week_deals, d.anq_missing, d.cyomi_stale]) {
    for (const r of list) {
      if (r.owner === '') {
        r.ownerName = '担当なし';
        r.team = 'チーム未設定';
      }
    }
  }
  delete d.kaden.by_person[''];
  for (const per of Object.values(d.calls.periods)) {
    delete (per as { by_person: Record<string, unknown> }).by_person[''];
  }
  d.kettei.rows = d.kettei.rows.filter((r) => r.owner !== '');
  return { data: d, oldId };
}

/**
 * 名簿に居ない担当者の行と、名簿のチームと行のチームがずれた行を、下段の一覧 (stale / week_deals) に足す。
 * 旧画面の `inScope` は名簿のチームを先に見て、居ない人だけ行のチームを使う (A9)。
 */
export function withOffRosterRows(D: SalesKpiData = loadFixture()): SalesKpiData {
  const d = JSON.parse(JSON.stringify(D)) as SalesKpiData;
  const izubo = d.people.find((p) => p.team === '第1チーム');
  const hirata = d.people.find((p) => p.team === '第2チーム');
  if (!izubo || !hirata) throw new Error('fixture に伊壺・第2チームの人が居ない');
  const base = d.stale[0] ?? d.week_deals[0];
  if (!base) throw new Error('fixture に下段の行が無い');
  const mk = (id: string, owner: string, ownerName: string, team: string) => ({ ...base, id, owner, ownerName, team });
  d.stale.push(
    mk('off-1', 'ZZ-OFF', '名簿外の人', '第1チーム'),
    // 名簿は第2チームなのに、行は第1チームと言っている (旧画面は名簿を信じる → 第1チームには出ない)
    mk('off-2', hirata.id, hirata.name, '第1チーム'),
  );
  d.week_deals.push(mk('off-3', 'ZZ-OFF', '名簿外の人', '第1チーム'));
  return d;
}
