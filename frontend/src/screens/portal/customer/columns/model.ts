// 項目の設定画面の下書きと、変更の計算。
import type { ApplicantRow, ColumnConfig, ColumnGroup, ColumnKind, FunnelStage, SheetConfig } from '../../types';
import { kindOf } from '../../columns';

export const STAGE_LABEL: Record<FunnelStage, string> = {
  applied: '応募',
  screening: '選考中',
  interview_set: '面接設定',
  interviewed: '面接実施',
  offer: '内定',
  hired: '採用',
  rejected: '不採用',
  declined: '辞退',
};
export const STAGE_ORDER: FunnelStage[] = ['applied', 'screening', 'interview_set', 'interviewed', 'offer', 'hired', 'rejected', 'declined'];

export interface OptionRow {
  id: string;
  /** もとの選択肢の名前。新しく足したものは null。 */
  orig: string | null;
  name: string;
  stage: FunnelStage | '';
}

export interface Draft {
  label: string;
  kind: ColumnKind;
  group: ColumnGroup;
  inList: boolean;
  writable: boolean;
  multiline: boolean;
  options: OptionRow[];
  /** 空の選択肢（未入力）が元の何番目にあったか。編集の行には出さず、保存のとき元の位置に戻す。 */
  emptyAt: number | null;
}

export function makeDraft(col: ColumnConfig, sheet: SheetConfig): Draft {
  const isStatus = sheet.statusColumn === col.header;
  return {
    label: col.label,
    kind: kindOf(col),
    group: col.group ?? (col.writable ? 'customer' : 'other'),
    inList: col.inList,
    writable: col.writable,
    multiline: col.multiline,
    options: (col.options ?? []).flatMap((name, i) => (name === '' ? [] : [{ id: 'o' + String(i), orig: name, name, stage: isStatus ? (sheet.stageMap?.[name] ?? '') : '' }])),
    emptyAt: (col.options ?? []).includes('') ? (col.options ?? []).indexOf('') : null,
  };
}

export function sameDraft(a: Draft, b: Draft): boolean {
  return JSON.stringify(a) === JSON.stringify(b);
}

export function countOf(rows: ApplicantRow[], header: string, value: string): number {
  return rows.filter((r) => (r.values[header] ?? '') === value).length;
}

export interface Impact {
  orig: string;
  /** 削除、または名前の変更。 */
  type: 'delete' | 'rename';
  newName: string;
  uses: number;
}

export function impactsOf(draft: Draft, rows: ApplicantRow[], header: string): Impact[] {
  const out: Impact[] = [];
  for (const o of draft.options) {
    if (o.orig === null || o.orig === o.name) continue;
    const uses = countOf(rows, header, o.orig);
    if (uses > 0) out.push({ orig: o.orig, type: 'rename', newName: o.name, uses });
  }
  return out;
}

/** 消された（下書きに残っていない）選択肢。 */
export function deletedOf(draft: Draft, original: ColumnConfig, rows: ApplicantRow[]): Impact[] {
  const kept = new Set(draft.options.map((o) => o.orig));
  return (original.options ?? [])
    .filter((v) => v !== '' && !kept.has(v))
    .map((v) => ({ orig: v, type: 'delete' as const, newName: '', uses: countOf(rows, original.header, v) }))
    .filter((i) => i.uses > 0);
}

export function validate(draft: Draft): string | null {
  if (draft.kind !== 'choice') return null;
  const names = draft.options.map((o) => o.name.trim());
  if (names.some((n) => n === '')) return '名前が空の選択肢があります。';
  if (new Set(names).size !== names.length) return '同じ名前の選択肢があります。';
  return null;
}

/** 保存する選択肢の並び（空の選択肢は元の位置に戻す）。 */
export function namesOf(draft: Draft): string[] {
  const names = draft.options.map((o) => o.name.trim());
  if (draft.emptyAt !== null) names.splice(Math.min(draft.emptyAt, names.length), 0, '');
  return names;
}

/** 選択肢の変更を短い文にする（差分だけ）。 */
export function optionDiff(base: Draft, draft: Draft): string[] {
  const out: string[] = [];
  for (const o of draft.options) {
    const name = o.name.trim();
    if (o.orig === null) out.push(`「${name}」を追加`);
    else if (o.orig !== name) out.push(`「${o.orig}」を「${name}」に変更`);
  }
  const kept = new Set(draft.options.map((o) => o.orig));
  for (const o of base.options) if (o.orig !== null && !kept.has(o.orig)) out.push(`「${o.orig}」を削除`);
  if (out.length === 0 && JSON.stringify(base.options.map((o) => o.orig)) !== JSON.stringify(draft.options.map((o) => o.orig))) out.push('並び順を変更');
  return out;
}

/** 数字の段階の変更（変わった選択肢だけ）。 */
export function stageDiff(base: Draft, draft: Draft): string[] {
  const out: string[] = [];
  for (const o of draft.options) {
    const before = o.orig === null ? '' : (base.options.find((b) => b.orig === o.orig)?.stage ?? '');
    if (before === o.stage) continue;
    const name = o.name.trim();
    out.push(`${name}: ${before === '' ? '未設定' : STAGE_LABEL[before]} → ${o.stage === '' ? '未設定' : STAGE_LABEL[o.stage]}`);
  }
  return out;
}

export function stageMapOf(draft: Draft): Record<string, FunnelStage> {
  const map: Record<string, FunnelStage> = {};
  draft.options.forEach((o) => { if (o.stage !== '') map[o.name.trim()] = o.stage; });
  return map;
}

/** 選択肢に無いのに入っている値と人数。 */
export function strayValues(draft: Draft, rows: ApplicantRow[], header: string): { value: string; uses: number }[] {
  const known = new Set(draft.options.map((o) => o.name.trim()));
  const counts = new Map<string, number>();
  for (const r of rows) {
    const v = r.values[header] ?? '';
    if (v !== '' && !known.has(v)) counts.set(v, (counts.get(v) ?? 0) + 1);
  }
  return [...counts.entries()].map(([value, uses]) => ({ value, uses }));
}
