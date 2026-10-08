// 架電キューで選べるパイプラインと、ステージごとの決まり (all = 常に出す / due = 次回日が来たら出す / exclude = 出さない)。
// ID と決まりの正本は Rust の src/crm/queue_pipelines.rs。テストがそこから src/generated/call_queue_pipelines.json を書き出す。
// 表示名は HubSpot から読む (GET /api/crm/call-queue/pipelines)。読めるまで・読めないときは表の呼び名を使う。
import config from '../../generated/call_queue_pipelines.json';
import type { CallQueuePipelinesResponse } from '../../generated/CallQueuePipelinesResponse';
import type { QueueMode } from './queueModel';

export type StageRule = 'all' | 'due' | 'exclude';

export interface QueueStageDef {
  id: string;
  rule: StageRule;
  /** HubSpot の表示名。分からなければ null */
  label: string | null;
}

export interface QueuePipelineDef {
  id: string;
  /** HubSpot から名前を読めないときの呼び名 */
  fallbackName: string;
  /** HubSpot の表示名。分からなければ null */
  label: string | null;
  /** 表にあるステージ (表の順) */
  stages: readonly QueueStageDef[];
  /** HubSpot にあって表に無いステージ (架電対象外) */
  unknownStages: readonly QueueStageDef[];
}

const asRule = (r: string): StageRule => (r === 'all' || r === 'due' ? r : 'exclude');

/** 既定のパイプライン (bpo_リクロジ) */
export const DEFAULT_PIPELINE_ID: string = config.default;

/** 実データで選べるパイプライン (表示名は未取得) */
export const LIVE_PIPELINES: readonly QueuePipelineDef[] = config.pipelines.map(p => ({
  id: p.id, fallbackName: p.fallback_name, label: null, unknownStages: [],
  stages: p.stages.map(s => ({ id: s.id, rule: asRule(s.rule), label: null })),
}));

/** 架空サンプルで使う bpo_リクロジのステージ名 (HubSpot に接続しないため固定) */
const FIXTURE_BPO_LABELS: Record<string, string> = {
  '1095387442': '未済', '1095387443': '不通', '1095387444': '受付ブロック', '1095387445': '不在',
  '1274330477': '番号検索依頼中', '1095387446': '担当者ブロック', '1409897995': '成果報酬のみ',
  '1095387447': 'ニーズなし/無料のみ', '1325087323': 'ニーズなし/有料あり', '1325087324': 'ニーズあり/無料のみ',
  '1095387448': 'ニーズあり/有料あり', '1448079987': 'SV依頼案件', '1319310149': '日程確保', '1095457875': 'アポ日確定',
  '1095457877': '案件差戻', '1095457878': '架電禁止 ※リーダーのみ変更', '1325086466': '商談実施処理',
  '1330563334': '商談未実施処理', '1369739056': 'リスト精査前',
};

/** 架空サンプルだけにある 2 つ目のパイプライン (切り替えの確認用。実データには無い) */
export const FIXTURE_PIPELINE_ID = 'fx-sample';

export const FIXTURE_PIPELINES: readonly QueuePipelineDef[] = [
  ...LIVE_PIPELINES.filter(p => p.id === DEFAULT_PIPELINE_ID).map(p => ({
    ...p, label: 'bpo_リクロジ', stages: p.stages.map(s => ({ ...s, label: FIXTURE_BPO_LABELS[s.id] ?? null })),
  })),
  {
    id: FIXTURE_PIPELINE_ID, fallbackName: '架空パイプライン', label: '架空パイプライン(確認用)', unknownStages: [],
    stages: [
      { id: 'fx-new', rule: 'all', label: '新規' },
      { id: 'fx-follow', rule: 'due', label: '追客' },
      { id: 'fx-stop', rule: 'exclude', label: '対象外' },
    ],
  },
];

export function pipelinesFor(mode: QueueMode): readonly QueuePipelineDef[] {
  return mode === 'fixture' ? FIXTURE_PIPELINES : LIVE_PIPELINES;
}

/** ID からパイプラインを引く (実データ・架空サンプルのどちらでも。ID は重ならない) */
export function findPipeline(id: string): QueuePipelineDef | undefined {
  return LIVE_PIPELINES.find(p => p.id === id) ?? FIXTURE_PIPELINES.find(p => p.id === id);
}

/** キューに出しうる (all / due の) ステージ ID (表の順)。分からないパイプラインは空 */
export function eligibleStageIds(pipelineId: string): string[] {
  return (findPipeline(pipelineId)?.stages ?? []).filter(s => s.rule !== 'exclude').map(s => s.id);
}

/** 画面に出すパイプライン名 */
export function pipelineName(p: QueuePipelineDef): string {
  return p.label ?? p.fallbackName;
}

/** 画面に出すステージ名 (名前が分からない間・読めなかったときは HubSpot のステージ ID) */
export function stageName(s: QueueStageDef): string {
  return s.label ?? `ステージ ID: ${s.id}`;
}

/** 表 (`defs`) に HubSpot の表示名と、表に無いステージを付ける。応答に無いパイプラインはそのまま */
export function withLabels(defs: readonly QueuePipelineDef[], resp: CallQueuePipelinesResponse | null): QueuePipelineDef[] {
  if (!resp) return [...defs];
  return defs.map(p => {
    const got = resp.pipelines.find(x => x.id === p.id);
    if (!got) return p;
    return {
      ...p,
      label: got.label ?? p.label,
      stages: p.stages.map(s => ({ ...s, label: got.stages.find(x => x.id === s.id)?.label ?? s.label })),
      unknownStages: got.unknown_stages.map(s => ({ id: s.id, rule: 'exclude' as const, label: s.label })),
    };
  });
}
