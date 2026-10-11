import shared from './hrhCopyColumns.json';
import draftColumns from '../../generated/jobgen/columns.json';
import { compareCopy, markInlineChanges, type CopyComparison, type MarkedDiffLine } from './diff';
const columns = [...new Set([...shared, ...draftColumns])];
export const normalizeFullBody = (text: string) => text.replaceAll('\r\n', '\n').replaceAll('\r', '\n').trim();
/** 保存形式の列名＋区切り文字だけを読む。本文の意味や保険・手当などの語では分類しない。 */
export function savedColumns(text: string): Map<string, string> {
  const result = new Map<string, string>();
  if (!text) return result;
  let key = 'その他'; let lines: string[] = [];
  const flush = () => {
    if (!lines.length) return;
    const value = lines.join('\n');
    result.set(key, result.has(key) ? `${result.get(key) ?? ''}\n${value}` : value);
  };
  for (const line of text.split('\n')) {
    const full = line.indexOf('：'); const half = line.indexOf(':');
    const separator = full < 0 ? half : half < 0 ? full : Math.min(full, half);
    const heading = separator > 0 ? line.slice(0, separator) : '';
    if (heading && columns.includes(heading)) {
      flush(); key = heading; lines = [line.slice(separator + 1)];
    } else if (heading && !line.startsWith(' ') && !line.startsWith('　')) {
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
  const a = beforeRow ? rowValues(beforeRow) : savedColumns(left ?? '');
  const b = afterRow ? rowValues(afterRow) : savedColumns(right ?? '');
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
  if (!changes.length) changes.push({ key: 'その他', before: left ?? undefined, after: right ?? undefined, kind: '変更', lines: markInlineChanges(comparison.lines) });
  return { ...comparison, changes };
}
const friendly: Record<string, string> = { '基本給与 最小': '給与の下限', '基本給与 最大': '給与の上限', '給与形態': '給与の種類', '試用・研修期の基本給与 最小': '研修中の給与の下限', '試用・研修期の基本給与 最大': '研修中の給与の上限', '試用・研修期の給与のタイプ': '研修中の給与の種類', '自由項目2の内容': '補足欄2の本文', '自由項目1の内容': '補足欄1の本文', '自由項目3の内容': '補足欄3の本文', '自由項目4の内容': '補足欄4の本文', '自由項目1のタイトル': '補足欄1の見出し', '自由項目2のタイトル': '補足欄2の見出し', '自由項目3のタイトル': '補足欄3の見出し', '自由項目4のタイトル': '補足欄4の見出し', '求人id': '求人の管理情報', '店舗id': '店舗の管理情報', '職種id': '職種の管理情報' };
function Value({ line, hidden }: { line: MarkedDiffLine | undefined; hidden: boolean }) {
  if (!line) return <span>未取得</span>;
  if (hidden) return <span>非表示</span>;
  return <pre>{line.segments ? line.segments.map((segment, index) => !segment.changed ? segment.text : line.kind === 'added' ? <ins key={index} className="jc-mark-added">{segment.text}</ins> : <del key={index} className="jc-mark-removed">{segment.text}</del>) : line.kind === 'added' ? <ins className="jc-mark-added">{line.text || '（空欄）'}</ins> : <del className="jc-mark-removed">{line.text || '（空欄）'}</del>}</pre>;
}
export function ColumnComparison({ result }: { result: ColumnResult }) {
  if (result.status === 'unavailable') return <p>比較先の本文は未取得です。</p>;
  if (result.status === 'unchanged') return <p>変更なし</p>;
  return <section className="jc-column-comparison jc-diff-lines" aria-label="変わった項目"><h3>変わった項目</h3><table><thead><tr><th>項目</th><th>変更</th><th>今の値</th><th>案の値</th></tr></thead><tbody>{result.changes.map(change => <tr key={change.key}><th>{friendly[change.key] ?? change.key}</th><td>{change.kind}</td><td><Value line={change.lines.find(line => line.kind === 'removed')} hidden={change.key.toLowerCase().endsWith('id')} /></td><td><Value line={change.lines.find(line => line.kind === 'added')} hidden={change.key.toLowerCase().endsWith('id')} /></td></tr>)}</tbody></table></section>;
}
