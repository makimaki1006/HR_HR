// 詳細条件で、選択肢がプロパティ定義の外にある項目 (取引ステージ・担当者) の「選べる値」。
// 取引ステージはパイプラインごとに違うので、いま選んでいるパイプラインのステージをパイプライン名付きで出す。
// サーバ (queue_property_filter::ValueScope) も同じ範囲だけを受け付ける。
import type { CrmCatalogProperty } from '../../generated/CrmCatalogProperty';
import { ownerLabel, visibleOwners } from './ownerModel';
import { pipelineName, stageName } from './queuePipelines';
import type { QueuePipelineDef } from './queuePipelines';
import type { OwnersState } from './useOwners';

export interface ValueChoice { value: string; label: string }

export interface ValueChoiceSource {
  /** 画面で使えるパイプライン (名前つき)。ラベルの引き当てに全部を使う */
  pipelines: readonly QueuePipelineDef[];
  /** いま選んでいるパイプラインの ID */
  selectedPipelineIds: readonly string[];
  /** 「すべての取引」: 表に無いステージも選べる */
  allDeals: boolean;
  owners: OwnersState;
}

/** 条件の項目に使わない項目: パイプラインは上の「対象のパイプライン」で選ぶ */
export const PIPELINE_PROPERTY = 'pipeline';

/** 値の候補の材料が無いとき (テスト用)。ステージは空、担当者は未読み込み */
export const NO_VALUE_CHOICES: ValueChoiceSource = { pipelines: [], selectedPipelineIds: [], allDeals: false, owners: { phase: 'idle' } };

export type ChoiceStatus = 'ready' | 'loading' | 'error';

export function stageChoices(src: ValueChoiceSource): ValueChoice[] {
  const out: ValueChoice[] = [];
  for (const p of src.pipelines) {
    if (!src.selectedPipelineIds.includes(p.id)) continue;
    const stages = [...p.stages, ...(src.allDeals ? p.unknownStages : [])];
    for (const s of stages) out.push({ value: s.id, label: `${pipelineName(p)} / ${stageName(s)}` });
  }
  return out;
}

/** 値を選ぶ一覧を定義の外から作る項目か (取引ステージ・担当者を選ぶ型) */
export function hasExternalChoices(p: CrmCatalogProperty): boolean {
  return p.name === 'dealstage' || p.owner_ref;
}

/** 選べる値。定義の外にある項目はここで作り、それ以外は定義の選択肢をそのまま使う */
export function choicesFor(p: CrmCatalogProperty, src: ValueChoiceSource): { status: ChoiceStatus; choices: ValueChoice[] } {
  if (p.name === 'dealstage') return { status: 'ready', choices: stageChoices(src) };
  if (p.owner_ref) {
    if (src.owners.phase === 'ready') {
      return { status: 'ready', choices: visibleOwners(src.owners.owners, '', false, '').map(o => ({ value: o.id, label: ownerLabel(o) })) };
    }
    return { status: src.owners.phase === 'error' ? 'error' : 'loading', choices: [] };
  }
  return { status: 'ready', choices: p.options.map(o => ({ value: o.value, label: o.label })) };
}

/** 条件のチップに出す値の表示名。分からなければ undefined (呼び出し側が定義の選択肢か値そのものに戻す) */
export function valueLabel(p: CrmCatalogProperty, value: string, src: ValueChoiceSource): string | undefined {
  if (p.name === 'dealstage') {
    for (const pl of src.pipelines) {
      const s = [...pl.stages, ...pl.unknownStages].find(x => x.id === value);
      if (s) return `${pipelineName(pl)} / ${stageName(s)}`;
    }
    return undefined;
  }
  if (p.owner_ref && src.owners.phase === 'ready') {
    const o = src.owners.owners.find(x => x.id === value);
    return o ? ownerLabel(o) : undefined;
  }
  return undefined;
}

/** 条件の項目の候補に出せるか。パイプラインは対象の選択と重なるので出さない。
 *  選べる値が定義にもなく、こちらで作ることもできない選択式は、値を選べないので出さない */
export function usableAsCondition(p: CrmCatalogProperty): boolean {
  if (p.name === PIPELINE_PROPERTY) return false;
  if (p.property_type === 'enumeration') return hasExternalChoices(p) || p.options.length > 0;
  return true;
}
