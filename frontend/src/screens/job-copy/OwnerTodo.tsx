import type { ChangedDeal } from './RecentChangesCard';
import { formatDateJst } from './format';
import { updateJobCopyQuery, useJobCopySearch } from './navigation';

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
    <label>担当の項目<select value={work.owner_field} onChange={event => { updateJobCopyQuery({ owner_field: event.target.value, todo_page: null }, 'replace'); }}><option value="consultant">コンサル担当</option><option value="hubspot_owner_id">取引の担当者</option></select></label>
    <label>担当者<select value={work.owner} onChange={event => { updateJobCopyQuery({ owner: event.target.value, todo_page: null }, 'replace'); }}><option value="all">全員</option>
      {!work.owners.some(o => o.id === work.owner) && work.owner !== 'all' && <option value={work.owner}>選択中の担当者（該当案件なし）</option>}
      <optgroup label="在籍中">{work.owners.filter(o => !o.archived).map(o => <option key={o.id} value={o.id}>{o.name}</option>)}</optgroup>
      <optgroup label="退職者">{work.owners.filter(o => o.archived).map(o => <option key={o.id} value={o.id}>{o.name}</option>)}</optgroup>
    </select></label>
    <button type="button" className="jc-button" onClick={() => { updateJobCopyQuery({ owner: 'me', todo_page: null }, 'replace'); }}>自分に戻す</button>
    {work.todos_status === 'preparing' && <p role="status">担当者の候補を更新しています…</p>}
    {work.notice && <p role="status">{work.notice}</p>}
  </section>;
}
const cards = [
  ['overdue', '期限切れ', '今日より前のCSアクション', 'alert'],
  ['today', '今日', '今日のCSアクション', 'plain'],
  ['week', '7日以内', '明日から7日後までのCSアクション', 'plain'],
  ['expiration', '契約満了が近い', '今日から30日後まで（両端を含む）', 'plain'],
] as const;
const PAGE_SIZE = 20;
export function OwnerTodo({ work, onOpenCompany, onOpenDeal }: { work: WorkReady; onOpenCompany: (company: { id: string; name: string }) => void; onOpenDeal: (deal: ChangedDeal) => void }) {
  const query = new URLSearchParams(useJobCopySearch());
  if (work.todos_status) return <section className="jh-todo" aria-labelledby="jh-todo-title"><h2 id="jh-todo-title">やること</h2><p role={work.todos_status === 'failed' ? 'alert' : 'status'}>{work.todos_status === 'failed' ? 'やることを読み込めませんでした。再読み込みしてください。' : 'やることを集計しています…'}</p></section>;
  const open = query.get('card');
  const groups = [...cards.map(([key, label, rule, tone]) => ({ key, label, rule, tone, items: key === 'expiration' ? work.expirations : work.actions[key] })), { key: 'later', label: 'それ以降', rule: '8日後以降のCSアクション', tone: 'plain', items: work.actions.later }];
  const selected = groups.find(group => `todo_${group.key}` === open);
  const pages = Math.max(1, Math.ceil((selected?.items.length ?? 0) / PAGE_SIZE));
  const requested = query.get('todo_page') ?? '1';
  const page = Math.min(pages, /^[1-9]\d{0,5}$/.test(requested) ? Number(requested) : 1);
  const start = (page - 1) * PAGE_SIZE;
  const items = selected?.items.slice(start, start + PAGE_SIZE) ?? [];
  const toggle = (key: string) => { updateJobCopyQuery({ card: open === `todo_${key}` ? null : `todo_${key}`, todo_page: null }, 'push'); };
  const changePage = (next: number) => { updateJobCopyQuery({ todo_page: next === 1 ? null : String(next) }, 'push'); };
  return <section className="jh-todo" aria-labelledby="jh-todo-title"><h2 id="jh-todo-title">やること</h2><p className="jh-note">CSアクションと契約満了の予定です。期日は日本時間、終了した案件は除いています。</p>
    {work.refresh_failed && <p role="status">最新の情報を読み込めなかったため、前回取得した情報を表示しています。</p>}
    <div className="jh-cards jh-todo-cards">{groups.slice(0, 4).map(group => <button type="button" key={group.key} className={`jh-card jh-${group.tone}`} aria-pressed={open === `todo_${group.key}`} onClick={() => { toggle(group.key); }}><strong className="jh-count">{group.items.length}<small>件</small></strong><span className="jh-card-title">{group.label}</span><small className="jh-rule">{group.rule}</small></button>)}</div>
    <button type="button" className="jh-link jh-todo-later" aria-pressed={open === 'todo_later'} onClick={() => { toggle('later'); }}>それ以降のCSアクション（{work.actions.later.length}件）</button>
    {selected && <section className={`jh-list ${selected.key === 'overdue' ? 'jh-todo-overdue' : ''}`} aria-label={`${selected.label}の一覧`}>
      <h3>{selected.label}（{selected.items.length}件）</h3>
      {items.length ? <><p className="jh-note">{start + 1}〜{start + items.length}件 ／ 全{selected.items.length}件</p><div className="jh-scroll"><table className="jh-todo-table"><thead><tr><th scope="col">会社</th><th scope="col">案件名</th><th scope="col">ステージ</th><th scope="col">内容</th><th scope="col">期日</th><th scope="col">コンサル担当</th></tr></thead><tbody>{items.map(row => <tr key={row.id}><td>{row.company_id ? <button type="button" className="jh-link" onClick={() => { onOpenCompany({ id: row.company_id ?? '', name: row.company ?? '会社名未取得' }); }}>{row.company ?? '会社名未取得'}</button> : '—'}</td><th scope="row"><button type="button" className="jh-link" onClick={() => { onOpenDeal(row); }}>{row.name ?? '案件名未取得'}</button></th><td>{row.stage ?? 'ステージ未取得'}</td><td>{row.content ?? '内容未登録'}</td><td><time dateTime={row.due}>{formatDateJst(row.due, '期日未取得')}</time></td><td>{row.consultant ?? '担当未登録'}</td></tr>)}</tbody></table></div></> : <p className="jh-note">該当する案件はありません。</p>}
      {pages > 1 && <nav className="jh-todo-pages" aria-label={`${selected.label}のページ`}><button type="button" className="jc-button" disabled={page === 1} onClick={() => { changePage(page - 1); }}>前の20件</button><span>{page} / {pages}ページ</span><button type="button" className="jc-button" disabled={page === pages} onClick={() => { changePage(page + 1); }}>次の20件</button></nav>}
    </section>}
  </section>;
}
