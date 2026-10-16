import type { ChangedDeal } from './RecentChangesCard';
import { updateJobCopyQuery } from './navigation';

export interface TodoRow { id: string; name: string | null; company_id: string | null; company: string | null; stage: string | null; content: string | null; due: string; consultant: string | null }
export type ActionBucket = 'overdue' | 'today' | 'week' | 'later';
export interface WorkReady { status: 'ready'; todos_status?: 'preparing' | 'failed'; owner: string; owner_field: string; notice: string | null; owners: { id: string; name: string; archived: boolean }[]; actions: Record<ActionBucket, TodoRow[]>; expirations: TodoRow[]; refresh_failed: boolean }
export type Work = WorkReady | { status: 'preparing' | 'failed' };
/** URL にある担当選択だけを API へ渡す。画面内の移動・戻るでも維持する。 */
export function ownerParams(search: string): string {
  const source = new URLSearchParams(search); const result = new URLSearchParams();
  for (const key of ['owner_field', 'owner']) { const value = source.get(key); if (value) result.set(key, value); }
  return result.toString();
}
export function OwnerFilter({ work }: { work: WorkReady }) {
  return <section className="jh-owner" aria-label="担当者で絞る"><strong>担当者で絞る</strong>
    <label>担当の項目<select value={work.owner_field} onChange={event => { updateJobCopyQuery({ owner_field: event.target.value }, 'replace'); }}><option value="consultant">コンサル担当</option><option value="hubspot_owner_id">取引の担当者</option></select></label>
    <label>担当者<select value={work.owner} onChange={event => { updateJobCopyQuery({ owner: event.target.value }, 'replace'); }}><option value="all">全員</option>
      {!work.owners.some(o => o.id === work.owner) && work.owner !== 'all' && <option value={work.owner}>選択中の担当者（該当案件なし）</option>}
      <optgroup label="在籍中">{work.owners.filter(o => !o.archived).map(o => <option key={o.id} value={o.id}>{o.name}</option>)}</optgroup>
      <optgroup label="退職者">{work.owners.filter(o => o.archived).map(o => <option key={o.id} value={o.id}>{o.name}</option>)}</optgroup>
    </select></label>
    <button type="button" className="jc-button" onClick={() => { updateJobCopyQuery({ owner: 'me' }, 'replace'); }}>自分に戻す</button>
    {work.todos_status === 'preparing' && <p role="status">担当者の候補を更新しています…</p>}
    {work.notice && <p role="status">{work.notice}</p>}
  </section>;
}
const buckets: [ActionBucket, string][] = [['overdue', '期限切れ'], ['today', '今日'], ['week', '7日以内'], ['later', 'それ以降']];
export function OwnerTodo({ work, onOpenCompany, onOpenDeal }: { work: WorkReady; onOpenCompany: (company: { id: string; name: string }) => void; onOpenDeal: (deal: ChangedDeal) => void }) {
  if (work.todos_status) return <section className="jh-todo" aria-labelledby="jh-todo-title"><h2 id="jh-todo-title">やること</h2><p role={work.todos_status === 'failed' ? 'alert' : 'status'}>{work.todos_status === 'failed' ? 'やることを読み込めませんでした。再読み込みしてください。' : 'やることを集計しています…'}</p></section>;
  const rows = (items: TodoRow[]) => items.length ? <div className="jh-scroll"><table className="jh-todo-table"><thead><tr><th scope="col">会社</th><th scope="col">案件名</th><th scope="col">ステージ</th><th scope="col">内容</th><th scope="col">期日</th><th scope="col">コンサル担当</th></tr></thead><tbody>{items.map(row => <tr key={row.id}><td>{row.company_id ? <button type="button" className="jh-link" onClick={() => { onOpenCompany({ id: row.company_id ?? '', name: row.company ?? '会社名未取得' }); }}>{row.company ?? '会社名未取得'}</button> : '—'}</td><th scope="row"><button type="button" className="jh-link" onClick={() => { onOpenDeal(row); }}>{row.name ?? '案件名未取得'}</button></th><td>{row.stage ?? 'ステージ未取得'}</td><td>{row.content ?? '内容未登録'}</td><td><time dateTime={row.due}>{row.due}</time></td><td>{row.consultant ?? '担当未登録'}</td></tr>)}</tbody></table></div> : <p className="jh-note">該当する案件はありません。</p>;
  return <section className="jh-todo" aria-labelledby="jh-todo-title"><h2 id="jh-todo-title">やること</h2><p className="jh-note">期日は日本時間です。終了した案件は除いています。</p>
    {work.refresh_failed && <p role="status">最新の情報を読み込めなかったため、前回取得した情報を表示しています。</p>}
    <h3>CSアクション <span>{Object.values(work.actions).reduce((sum, list) => sum + list.length, 0)}件</span></h3>
    {buckets.map(([key, label]) => key === 'later' ? <details key={key} className="jh-todo-group"><summary>{label}（{work.actions[key].length}件）</summary>{rows(work.actions[key])}</details> : <section key={key} className={`jh-todo-group ${key === 'overdue' ? 'jh-todo-overdue' : ''}`} aria-label={`${label}のCSアクション`}><h4>{label} <span>{work.actions[key].length}件</span></h4>{rows(work.actions[key])}</section>)}
    <section aria-label="契約満了が近い" className="jh-todo-group"><h3>契約満了が近い <span>{work.expirations.length}件</span></h3><p className="jh-note">今日から30日後まで（両端を含む）。期日は契約満了日です。</p>{rows(work.expirations)}</section>
  </section>;
}
