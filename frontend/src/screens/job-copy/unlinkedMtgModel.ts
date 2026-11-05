/** 未連携のMTGを取引の項目 mtg_manual_links に結び付ける行の組み立て。1 行 = 録画UUID<TAB>選んだ日時(JST)<TAB>選んだ人のメール */
export const MANUAL_LINKS_FIELD = 'mtg_manual_links';

/** JST の YYYY-MM-DD HH:MM */
export function jstStamp(now: Date): string {
  return new Date(now.getTime() + 9 * 3600_000).toISOString().slice(0, 16).replace('T', ' ');
}
function lines(base: string | null): string[] {
  return (base ?? '').split(/\r?\n/u).filter(l => l.trim() !== '');
}
/** 録画UUID の行がすでにあるか */
export function hasManualLink(base: string | null, uuid: string): boolean {
  return lines(base).some(l => l.split('\t')[0]?.trim() === uuid);
}
/** 行を足した全文。同じ録画UUID がすでにあれば null（足さない）。既存の行はそのまま残す。 */
export function addManualLink(base: string | null, uuid: string, email: string, now: Date): string | null {
  if (hasManualLink(base, uuid)) return null;
  const line = `${uuid}\t${jstStamp(now)}\t${email}`;
  const current = (base ?? '').replace(/\s+$/u, '');
  return current === '' ? line : `${current}\n${line}`;
}
/** その録画UUID の行だけを消した全文。ほかの行はそのまま。残りが無ければ空文字。 */
export function removeManualLink(base: string | null, uuid: string): string {
  return lines(base).filter(l => l.split('\t')[0]?.trim() !== uuid).join('\n');
}

export interface UnlinkedRow {
  uuid: string; recording: string; date: string; subject: string; host: string; host_owner_id: string | null;
  candidates: { id: string; name: string }[]; other_candidates: number;
  selected: { deal_id: string; deal_name: string; at: string; by: string } | null;
}
export interface UnlinkedResponse {
  status: 'ready' | 'preparing' | 'failed' | 'unavailable';
  me_email?: string; selected_known?: boolean; rows?: UnlinkedRow[]; own_deals?: Record<string, { id: string; name: string }[]>;
}
/** 選択欄に出す取引。シートの候補を先頭に、その担当者の取引が続く（重複なし）。 */
export function dealChoices(row: UnlinkedRow, own: Record<string, { id: string; name: string }[]> | undefined): { id: string; name: string; candidate: boolean }[] {
  const seen = new Set<string>(); const out: { id: string; name: string; candidate: boolean }[] = [];
  for (const c of row.candidates) if (!seen.has(c.id)) { seen.add(c.id); out.push({ ...c, candidate: true }); }
  for (const c of (row.host_owner_id ? own?.[row.host_owner_id] : undefined) ?? []) if (!seen.has(c.id)) { seen.add(c.id); out.push({ ...c, candidate: false }); }
  return out;
}
