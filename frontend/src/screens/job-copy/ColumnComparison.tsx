import shared from './hrhCopyColumns.json';
import draftColumns from '../../generated/jobgen/columns.json';
import { compareCopy, markInlineChanges, type CopyComparison, type MarkedDiffLine } from './diff';
const columns = [...new Set([...draftColumns, ...shared])];
export const normalizeFullBody = (text: string) => text.replaceAll('\r\n', '\n').replaceAll('\r', '\n').trim();
/** 保存形式の列名＋区切り文字だけを読む。本文の意味や保険・手当などの語では分類しない。 */
export function savedColumns(text: string, referenceRow?: Readonly<Record<string, string>>): Map<string, string> {
  const result = new Map<string, string>();
  if (!text) return result;
  let key = 'その他'; let lines: string[] = [];
  const flush = () => {
    if (!lines.length) return;
    const value = lines.join('\n');
    result.set(key, result.has(key) ? `${result.get(key) ?? ''}\n${value}` : value);
  };
  const source = text.split('\n');
  const headers = source.map((line, index) => {
    const full = line.indexOf('：'); const half = line.indexOf(':');
    const separator = full < 0 ? half : half < 0 ? full : Math.min(full, half);
    const heading = separator > 0 ? line.slice(0, separator) : '';
    return { index, separator, heading, rank: columns.indexOf(heading), length: 1, next: -1 };
  });
  const candidates = headers.filter(header => header.rank >= 0);
  let boundaries = new Set(candidates.map(header => header.index));
  if (new Set(candidates.map(header => header.heading)).size !== candidates.length) {
    // 重複する列名らしい行は、保存時の列順に合う最長の列の並びを使って区切る。
    // 同じ長さなら後の境界を優先し、前の値に含まれる列名らしい行を保持する。
    const best = new Map<number, number>();
    for (let index = candidates.length - 1; index >= 0; index--) {
      const header = candidates[index]; if (!header) continue;
      for (let rank = header.rank + 1; rank < columns.length; rank++) {
        const next = best.get(rank); const candidate = next === undefined ? undefined : candidates[next];
        if (candidate && next !== undefined && candidate.length + 1 > header.length) { header.length = candidate.length + 1; header.next = next; }
      }
      const previous = best.get(header.rank);
      if (previous === undefined || header.length > (candidates[previous]?.length ?? 0)) best.set(header.rank, index);
    }
    boundaries = new Set();
    let index = 0;
    while (index >= 0) { const header = candidates[index]; if (!header) break; boundaries.add(header.index); index = header.next; }
  }
  for (const header of headers) {
    const line = source[header.index] ?? '';
    if (boundaries.has(header.index)) {
      flush(); key = header.heading; lines = [line.slice(header.separator + 1)];
    } else if (header.rank < 0 && header.heading && (key === 'その他' || !referenceRow?.[key]?.split('\n').some(valueLine => valueLine.startsWith(`${header.heading}：`) || valueLine.startsWith(`${header.heading}:`)))) {
      // 行データ内の同じ見出しの行は列値の一部として保持し、対応しない見出しは全文を残す。
      flush(); key = 'その他'; lines = [line];
    } else lines.push(line);
  }
  flush(); return result;
}
export interface ColumnChange { key: string; before: string | undefined; after: string | undefined; kind: '追加' | '削除' | '変更'; lines: MarkedDiffLine[] }
export interface ColumnResult extends CopyComparison { changes: ColumnChange[] }
export function compareColumns(before: string | null, after: string | null, beforeRow?: Readonly<Record<string, string>>, afterRow?: Readonly<Record<string, string>>): ColumnResult {
  const left = before === null ? null : normalizeFullBody(before);
  const right = after === null ? null : normalizeFullBody(after);
  const raw = compareCopy(left, right);
  const comparison: CopyComparison = left !== null && right !== null ? {
    ...raw, status: left === right ? 'unchanged' : 'changed',
    lines: right === '' && left !== '' ? left.split('\n').map(text => ({ kind: 'removed', text })) : raw.lines,
  } : raw;
  if (comparison.status === 'unchanged' || comparison.status === 'unavailable') return { ...comparison, changes: [] };
  const rowValues = (row: Readonly<Record<string, string>>) => new Map(Object.entries(row).map(([key, value]) => [key, value.replaceAll('\r\n', '\n').replaceAll('\r', '\n')]));
  const a = beforeRow ? rowValues(beforeRow) : savedColumns(left ?? '', afterRow);
  const b = afterRow ? rowValues(afterRow) : savedColumns(right ?? '', beforeRow);
  const changes = [...new Set([...a.keys(), ...b.keys()])].flatMap(key => {
    const old = a.get(key); const next = b.get(key);
    if (old === next) return [];
    const lines = markInlineChanges([
      ...(old !== undefined ? [{ kind: 'removed' as const, text: old }] : []),
      ...(next !== undefined ? [{ kind: 'added' as const, text: next }] : []),
    ]);
    return [{ key, before: old, after: next, kind: old === undefined ? '追加' as const : next === undefined ? '削除' as const : '変更' as const, lines }];
  });
  // 列の並び順や区切り文字だけが変わった場合も全文の違いを捨てない。
  if (!changes.length) changes.push({ key: 'その他', before: left ?? undefined, after: right ?? undefined, kind: '変更', lines: markInlineChanges([{ kind: 'removed', text: left ?? '' }, { kind: 'added', text: right ?? '' }]) });
  return { ...comparison, changes };
}
const friendly: Record<string, string> = { '基本給与 最小': '給与の下限', '基本給与 最大': '給与の上限', '給与形態': '給与の種類', '試用・研修期の基本給与 最小': '研修中の給与の下限', '試用・研修期の基本給与 最大': '研修中の給与の上限', '試用・研修期の給与のタイプ': '研修中の給与の種類', '自由項目2の内容': '補足欄2の本文', '自由項目1の内容': '補足欄1の本文', '自由項目3の内容': '補足欄3の本文', '自由項目4の内容': '補足欄4の本文', '自由項目1のタイトル': '補足欄1の見出し', '自由項目2のタイトル': '補足欄2の見出し', '自由項目3のタイトル': '補足欄3の見出し', '自由項目4のタイトル': '補足欄4の見出し', '求人id': '求人の管理情報', '店舗id': '店舗の管理情報', '職種id': '職種の管理情報' };
/** 表示だけを整える。全文の検知と列の値の比較には加工前の本文を使う。 */
function displayOther(text: string): string {
  return text.split('\n').map(line => {
    const full = line.indexOf('：'); const half = line.indexOf(':');
    const separator = full < 0 ? half : half < 0 ? full : Math.min(full, half);
    const heading = separator > 0 ? line.slice(0, separator) : '';
    if (!columns.includes(heading)) return line;
    const value = heading.toLowerCase().endsWith('id') ? '非表示' : line.slice(separator + 1);
    return `${friendly[heading] ?? heading}${line.charAt(separator)}${value}`;
  }).join('\n');
}
function displayLines(change: ColumnChange): MarkedDiffLine[] {
  if (change.key !== 'その他') return change.lines;
  return markInlineChanges([
    ...(change.before !== undefined ? [{ kind: 'removed' as const, text: displayOther(change.before) }] : []),
    ...(change.after !== undefined ? [{ kind: 'added' as const, text: displayOther(change.after) }] : []),
  ]);
}
function Value({ line, hidden }: { line: MarkedDiffLine | undefined; hidden: boolean }) {
  if (!line) return <span>未取得</span>;
  if (hidden) return <span>非表示</span>;
  return <pre>{line.segments ? line.segments.map((segment, index) => !segment.changed ? segment.text : line.kind === 'added' ? <ins key={index} className="jc-mark-added">{segment.text}</ins> : <del key={index} className="jc-mark-removed">{segment.text}</del>) : line.kind === 'added' ? <ins className="jc-mark-added">{line.text || '（空欄）'}</ins> : <del className="jc-mark-removed">{line.text || '（空欄）'}</del>}</pre>;
}
export function ColumnComparison({ result }: { result: ColumnResult }) {
  if (result.status === 'unavailable') return <p>比較先の本文は未取得です。</p>;
  if (result.status === 'unchanged') return <p>変更なし</p>;
  return <section className="jc-column-comparison jc-diff-lines" aria-label="変わった項目"><h3>変わった項目</h3><table><thead><tr><th>項目</th><th>変更</th><th>今の値</th><th>案の値</th></tr></thead><tbody>{result.changes.map(change => <tr key={change.key}><th>{friendly[change.key] ?? change.key}</th><td>{change.kind}</td><td><Value line={displayLines(change).find(line => line.kind === 'removed')} hidden={change.key.toLowerCase().endsWith('id')} /></td><td><Value line={displayLines(change).find(line => line.kind === 'added')} hidden={change.key.toLowerCase().endsWith('id')} /></td></tr>)}</tbody></table></section>;
}
