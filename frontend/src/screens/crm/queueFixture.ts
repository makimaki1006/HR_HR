import type { CallQueueItem } from '../../generated/CallQueueItem';
import type { CallQueueResponse } from '../../generated/CallQueueResponse';
import { DEFAULT_PIPELINE_ID, FIXTURE_PIPELINE_ID, FIXTURE_PIPELINES, eligibleStageIds } from './queuePipelines';

/** 架空サンプルのパイプライン (ステージ名は固定) */
const fixturePipeline = (id: string) => FIXTURE_PIPELINES.find(p => p.id === id);
import type { QueueFilters } from './queueModel';

/**
 * 固定の架空データ。実データの名前・電話番号・ID は一切含まない
 * (電話番号は 0300000000 台、会社名・氏名は架空)。実データのモードとは明示的に切り替える。
 */
export const FIXTURE_TODAY = '2026-10-05';
export const FIXTURE_PAGE_SIZE = 5;
export const FIXTURE_OWNER_ME = '9001';

interface Seed {
  id: string; company: string; contact: string | null; stage: string; owner: string | null;
  next: string | null; time: string | null; last: string | null; phone: string | null; mobile?: boolean;
  /** 既定は bpo_リクロジ */
  pipeline?: string;
}

const SEEDS: Seed[] = [
  { id: 'f-1', company: '架空商事株式会社', contact: '甲野 太郎', stage: '1095387442', owner: '9001', next: null, time: null, last: null, phone: '+81300000001' },
  { id: 'f-2', company: '架空食品株式会社', contact: '乙川 花子', stage: '1095387445', owner: '9001', next: '2026-10-01', time: '10:00', last: '2026-09-28', phone: '03-0000-0002' },
  { id: 'f-3', company: 'サンプル運輸', contact: '丙田 次郎', stage: '1095387443', owner: '9002', next: '2026-10-05', time: '14:30', last: '2026-10-02', phone: '+81 90-0000-0003' },
  { id: 'f-4', company: '架空クリニック', contact: null, stage: '1095387442', owner: null, next: null, time: null, last: '2026-09-10', phone: '0300000004' },
  { id: 'f-5', company: 'ダミー建設', contact: '丁野 三郎', stage: '1095387446', owner: '9001', next: '2026-09-30', time: '16:00', last: '2026-09-25', phone: '+81(0)3-0000-0005' },
  { id: 'f-6', company: '架空物流センター', contact: '戊島 里奈', stage: '1095387442', owner: '9002', next: '2026-10-12', time: '11:00', last: '2026-10-01', phone: '+81500000006' },
  { id: 'f-7', company: '見本不動産', contact: '己山 健', stage: '1319310149', owner: '9001', next: '2026-10-04', time: '13:00', last: '2026-10-03', phone: '03-0000-0007' },
  { id: 'f-8', company: '架空介護サービス', contact: '庚村 愛', stage: '1095387442', owner: '9001', next: null, time: null, last: '2026-08-20', phone: '+81 3 0000 0008' },
  { id: 'f-9', company: 'テスト印刷', contact: '辛木 誠', stage: '1095387447', owner: '9002', next: '2026-10-20', time: '09:30', last: '2026-10-04', phone: '03-0000-0009' },
  { id: 'f-10', company: '架空自動車整備', contact: '壬谷 彩', stage: '1095387442', owner: null, next: null, time: null, last: null, phone: '+81300000010' },
  { id: 'f-11', company: '架空ホテル', contact: '癸原 剛', stage: '1095387444', owner: '9001', next: '2026-10-03', time: '15:00', last: '2026-09-29', phone: '03-0000-0011' },
  { id: 'f-12', company: '番号未登録の架空商店', contact: '子田 優', stage: '1095387442', owner: '9001', next: null, time: null, last: '2026-09-15', phone: null },
  // 2 つ目の架空パイプライン (パイプラインの切り替えの確認用)
  { id: 'f-13', company: '架空倉庫サービス', contact: '丑川 光', stage: 'fx-new', owner: '9001', next: null, time: null, last: null, phone: '03-0000-0013', pipeline: FIXTURE_PIPELINE_ID },
  { id: 'f-14', company: '架空ベーカリー', contact: '寅田 静', stage: 'fx-follow', owner: '9002', next: '2026-10-02', time: '11:30', last: '2026-09-30', phone: '03-0000-0014', pipeline: FIXTURE_PIPELINE_ID },
  { id: 'f-15', company: '架空塾', contact: '卯月 望', stage: 'fx-follow', owner: '9001', next: '2026-10-09', time: '10:00', last: '2026-10-01', phone: '03-0000-0015', pipeline: FIXTURE_PIPELINE_ID },
  { id: 'f-16', company: '架空クリーニング', contact: '辰野 晴', stage: 'fx-stop', owner: '9001', next: '2026-10-01', time: null, last: '2026-09-20', phone: '03-0000-0016', pipeline: FIXTURE_PIPELINE_ID },
];

const pipelineOf = (s: Seed) => s.pipeline ?? DEFAULT_PIPELINE_ID;

function toItem(s: Seed): CallQueueItem {
  const label = fixturePipeline(pipelineOf(s))?.stages.find(x => x.id === s.stage)?.label ?? null;
  return {
    deal_id: s.id, deal_name: `${s.company} 採用支援`, stage_id: s.stage, stage_label: label,
    owner_id: s.owner, next_call_date: s.next, next_call_time: s.time, last_call_date: s.last,
    stop: { prohibited_reason: null, block_reason: null, unreachable_check: s.id === 'f-3' ? '通話中が続く' : null },
    contact: s.contact ? { id: `c-${s.id}`, name: s.contact, phone: s.phone, mobile: null, job_title: '採用担当', extra_count: s.id === 'f-5' ? 2 : 0 } : null,
    company: { id: `co-${s.id}`, name: s.company, phone: null },
    phone: s.phone, phone_source: s.phone ? 'contact' : null,
    deep_links: { deal: `https://example.invalid/deal/${s.id}`, contact: null, company: null },
  };
}

/** 架空の 1 行 (詳細画面の架空データの元) */
export function fixtureItem(id: string): CallQueueItem | null {
  const s = SEEDS.find(x => x.id === id);
  return s ? toItem(s) : null;
}

/** サーバ (call_queue.rs) の抽出・並びの要点だけを真似る。実サーバの代わりではなく、画面の確認用 */
export function fixtureQueuePage(f: QueueFilters, cursor: string | null): CallQueueResponse {
  const q = f.q.trim().normalize('NFKC').toLowerCase();
  const stages = f.stages.length ? f.stages : eligibleStageIds(f.pipeline);
  const rules = fixturePipeline(f.pipeline)?.stages ?? [];
  const rows = SEEDS.filter(s => {
    if (pipelineOf(s) !== f.pipeline || !stages.includes(s.stage)) return false;
    const rule = rules.find(x => x.id === s.stage)?.rule ?? 'exclude';
    const due = s.next !== null && s.next <= FIXTURE_TODAY;
    if (rule === 'exclude') return false;
    if (rule === 'due' && !due) return false; // 常に出すステージ (未済など) 以外は次回日が来たものだけ
    if (f.due === 'today' && !due) return false;
    if (s.phone === null) return false; // 電話番号なしはキューに出さない
    if (f.owner === 'unassigned' ? s.owner !== null : f.owner === 'me' ? s.owner !== FIXTURE_OWNER_ME
      : f.owner !== '' && f.owner !== 'all' ? s.owner !== f.owner : false) return false;
    if (q && !`${s.company} ${s.contact ?? ''}`.normalize('NFKC').toLowerCase().includes(q)) return false;
    if (f.nextFrom && !(s.next !== null && s.next >= f.nextFrom)) return false;
    if (f.nextTo && !(s.next !== null && s.next <= f.nextTo)) return false;
    if (f.lastFrom && !(s.last !== null && s.last >= f.lastFrom)) return false;
    if (f.lastTo && !(s.last !== null && s.last <= f.lastTo)) return false;
    return true;
  });
  const asc = (a: string, b: string) => (a < b ? -1 : a > b ? 1 : 0);
  const isDue = (s: Seed) => s.next !== null && s.next <= FIXTURE_TODAY;
  // 最終架電日の古い順。未架電 (空) が先頭
  const byLastAsc = (a: Seed, b: Seed) => (a.last === b.last ? 0 : a.last === null ? -1 : b.last === null ? 1 : asc(a.last, b.last));
  const byLastDesc = (a: Seed, b: Seed) => (a.last === b.last ? 0 : a.last === null ? 1 : b.last === null ? -1 : asc(b.last, a.last));
  const sorted = [...rows].sort((a, b) => {
    if (f.sort === 'last_call_asc') return byLastAsc(a, b);
    if (f.sort === 'last_call_desc') return byLastDesc(a, b);
    // 既定 / 次回架電日順: 次回日が来たもの (次回日順) → それ以外の未済 (未架電 → 最終架電日の古い順)
    if (isDue(a) !== isDue(b)) return isDue(a) ? -1 : 1;
    if (isDue(a) && a.next !== null && b.next !== null && a.next !== b.next) {
      return f.sort === 'next_call_desc' ? asc(b.next, a.next) : asc(a.next, b.next);
    }
    return byLastAsc(a, b);
  });
  const offset = cursor ? Number(cursor.replace(/^fx:/, '')) || 0 : 0;
  const page = sorted.slice(offset, offset + FIXTURE_PAGE_SIZE);
  const end = offset + FIXTURE_PAGE_SIZE;
  return {
    items: page.map(toItem),
    next_cursor: end < sorted.length ? `fx:${String(end)}` : null,
    total: sorted.length, truncated: false,
    scope: {
      pipeline: f.pipeline, owner: f.owner === '' ? 'all' : f.owner, role: 'admin', teams: [],
      stages: [...stages].sort(), due: f.due, sort: f.sort, q: f.q.trim() || null, limit: FIXTURE_PAGE_SIZE,
      next_from: f.nextFrom || null, next_to: f.nextTo || null, last_from: f.lastFrom || null, last_to: f.lastTo || null,
    },
    partial: { missing_contacts: page.filter(s => !s.contact).length, missing_companies: 0, failed: [], excluded: { no_phone: 0, stop_reason: 0, out_of_scope: 0 }, unknown_stages: 0 },
    generated_at: '2026-10-05T03:00:00Z',
  };
}
