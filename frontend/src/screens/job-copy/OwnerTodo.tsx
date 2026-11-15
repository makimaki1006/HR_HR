import { createContext, useContext, useEffect, useId, useState } from 'react';
import { createPortal } from 'react-dom';
import { ReportFreshness } from './ReportFreshness';
import { NextTodoButton, OverdueBadge, ReasonChips } from './NextTodo';
import type { TodoReason } from './NextTodo';
import type { ChangedDeal } from './RecentChangesCard';
import { formatDateJst } from './format';
import { updateJobCopyQuery, useJobCopySearch } from './navigation';
import { InfoTip } from './InfoTip';
import { useRevealOnMount } from './revealList';
import { useSelfOwner } from './selfOwner';
import { publishOwnerNames } from './ownerNames';
import { CardOpenCue, CloseListButton, CountChip, GoLink, OutLink } from './Pressable';

export interface TodoRow { id: string; name: string | null; company_id: string | null; company: string | null; stage: string | null; content: string | null; due: string; consultant: string | null; actionyotei?: string | null; actionnaiyou?: string | null; reasons?: TodoReason[] }
export type ActionBucket = 'overdue' | 'today' | 'week' | 'later';
export interface WorkReady { status: 'ready'; hubspot_portal_id?: string; sheet_as_of?: string | null; todos_status?: 'preparing' | 'failed'; refreshing?: boolean; facts_built_at?: string | null; facts_stale?: boolean; owner: string; owner_field: string; notice: string | null; owners: { id: string; name: string; archived: boolean }[]; actions: Record<ActionBucket, TodoRow[]>; expirations: TodoRow[]; signals?: TodoRow[]; refresh_failed: boolean }
export type Work = WorkReady | { status: 'preparing' | 'failed' };
/** URL にある担当選択だけを API へ渡す。画面内の移動・戻るでも維持する。 */
export function ownerParams(search: string): string {
  const source = new URLSearchParams(search); const result = new URLSearchParams();
  for (const key of ['owner_field', 'owner']) { const value = source.get(key); if (value) result.set(key, value); }
  return result.toString();
}
/** 担当者の一覧がまだ届かない間（準備中）の 1 行。上の帯の高さを増やさないよう、選択欄の代わりに短い文だけを出す。 */
export function PendingOwnerFilter() { return <p className="cm-owner-pending" role="status">担当者の一覧を準備しています。準備ができると、担当で絞れます。</p>; }
/** undefined は元の位置、null は非表示、要素があれば共通メニューへ移す。 */
export const OwnerFilterPlacement = createContext<HTMLElement | null | undefined>(undefined);

type OwnerWork = Pick<WorkReady, 'owner' | 'owner_field' | 'notice' | 'owners'>;
/** 今だれの担当分を見ているか。本人が分からないときは unknown（帯は出さず、見出しは担当者名）。self は useSelfOwner() の値。 */
export function ownerScope(work: OwnerWork, self: string | null | undefined): { kind: 'self' | 'all' | 'other' | 'unknown'; name: string | null } {
  if (work.owner === 'all') return { kind: 'all', name: null };
  const name = work.owners.find(o => o.id === work.owner)?.name ?? null;
  if (self && self === work.owner) return { kind: 'self', name };
  return { kind: self ? 'other' : 'unknown', name };
}
export function ownerScopeTitle(scope: ReturnType<typeof ownerScope>): string {
  return scope.kind === 'self' ? 'あなたの担当分' : scope.kind === 'all' ? '全員の担当分' : scope.name ? `${scope.name}さんの担当分` : '選択中の担当者の担当分';
}
/** 担当者の候補の表示名。退職者には印を付け、同じ名前が並ぶときは「（2人目）」のように順番を添える。 */
export function ownerOptions(owners: WorkReady['owners']): { id: string; label: string }[] {
  const base = [...owners.filter(o => !o.archived), ...owners.filter(o => o.archived)].map(o => ({ id: o.id, label: o.archived && !o.name.includes('退職') ? `${o.name}（退職）` : o.name }));
  return base.map((o, index) => {
    const order = base.slice(0, index).filter(other => other.label === o.label).length;
    return order > 0 ? { ...o, label: `${o.label}（${String(order + 1)}人目）` } : o;
  });
}
const fieldNotes: Record<string, string> = {
  consultant: 'HubSpot の案件の「コンサル担当」の項目で絞っています。',
  hubspot_owner_id: 'HubSpot の案件の担当者（案件の所有者）で絞っています。',
};
const changeOwner = (owner: string) => { updateJobCopyQuery({ owner, todo_page: null, deals_page: null }, 'replace'); };
export function OwnerFilter({ work }: { work: OwnerWork }) {
  const placed = useContext(OwnerFilterPlacement);
  const listId = useId();
  const self = useSelfOwner();
  useEffect(() => { publishOwnerNames(work.owners); }, [work.owners]);
  const options = ownerOptions(work.owners);
  const currentLabel = work.owner === 'all' ? '' : options.find(o => o.id === work.owner)?.label ?? '選択中の担当者（該当案件なし）';
  // 入力中の文字。null の間は今の担当者名を出す（フォーカス中は空にして、候補を全員分出す）。
  const [text, setText] = useState<string | null>(null);
  if (placed === null) return null;
  const scope = ownerScope(work, self);
  const typed = text?.trim() ?? '';
  const notFound = typed !== '' && !options.some(o => o.label.includes(typed));
  const pick = (id: string) => { setText(null); if (id !== work.owner) changeOwner(id); };
  const filter = <><section className="jh-owner" aria-label="担当者で絞る">
    <label><span className="jh-owner-label">担当の種類</span><select value={work.owner_field} onChange={event => { updateJobCopyQuery({ owner_field: event.target.value, todo_page: null, deals_page: null }, 'replace'); }}><option value="consultant">コンサル担当</option><option value="hubspot_owner_id">案件の担当者</option></select></label>
    <InfoTip className="jh-owner-tip" iconFirst label={<span className="jc-visually-hidden">担当の種類の説明</span>}><p>{fieldNotes[work.owner_field] ?? fieldNotes.consultant}</p></InfoTip>
    <label><span className="jh-owner-label">担当者</span><input className="jh-owner-name" list={listId} value={text ?? currentLabel} placeholder={text !== null ? currentLabel || '全員（名前で探す）' : work.owner === 'all' ? '全員（名前で探す）' : '名前を入力'} autoComplete="off" aria-describedby={notFound ? `${listId}-none` : undefined}
      onFocus={() => { setText(''); }}
      onChange={event => {
        const value = event.target.value;
        setText(value);
        // 候補を選んだとき（文字の入力ではないとき）だけ切り替える。打っている途中の一致では切り替えない。
        const inputType = (event.nativeEvent as InputEvent).inputType as string | undefined;
        if (inputType === undefined || inputType === 'insertReplacementText') { const hit = options.find(o => o.label === value); if (hit) pick(hit.id); }
      }}
      onKeyDown={event => {
        // IME の変換確定の Enter は無視する（keyCode 229 は古いブラウザの変換中の印）。
        // eslint-disable-next-line @typescript-eslint/no-deprecated -- isComposing を返さないブラウザのため
        if (event.nativeEvent.isComposing || event.keyCode === 229) return;
        if (event.key === 'Escape') { setText(null); event.currentTarget.blur(); return; }
        if (event.key !== 'Enter') return;
        event.preventDefault();
        const exact = options.find(o => o.label === typed);
        const hits = options.filter(o => o.label.includes(typed));
        const only = exact ?? (typed && hits.length === 1 ? hits[0] : undefined);
        if (only) pick(only.id);
      }}
      onBlur={() => { setText(null); }} /></label>
    <datalist id={listId}>{options.map(o => <option key={o.id} value={o.label} />)}</datalist>
    <span className="jh-owner-quick" role="group" aria-label="担当の切り替え">
      <button type="button" className="jh-owner-quick-button" aria-pressed={work.owner === 'all'} onClick={() => { changeOwner('all'); }}>全員</button>
      <button type="button" className="jh-owner-quick-button" aria-pressed={scope.kind === 'self'} onClick={() => { changeOwner('me'); }}>自分</button>
    </span>
  </section>
    {notFound && <p className="jh-owner-notice" id={`${listId}-none`} role="status">「{typed}」に合う担当者が見つかりません</p>}
    {work.notice && <p className="jh-owner-notice" role="status">{work.notice}</p>}</>;
  return placed === undefined ? filter : createPortal(filter, placed);
}
/** 担当で絞れない画面でも、同じ場所に押せない欄と理由を出す（画面を切り替えても帯の形が変わらないように）。 */
export function DisabledOwnerFilter({ reason }: { reason: string }) {
  return <section className="jh-owner jh-owner-disabled" aria-label="担当者で絞る">
    <label><span className="jh-owner-label">担当の種類</span><select disabled value=""><option value="">—</option></select></label>
    <label><span className="jh-owner-label">担当者</span><input className="jh-owner-name" disabled value="" placeholder="—" /></label>
    <small className="cm-owner-note">{reason}</small>
  </section>;
}
/** 自分以外を見ている間だけ出す黄色の帯。本人の担当者情報が無い人（notice あり）には出さない。 */
export function OwnerScopeBanner({ work }: { work: OwnerWork }) {
  const scope = ownerScope(work, useSelfOwner());
  if (work.notice || scope.kind === 'self' || scope.kind === 'unknown') return null;
  return <p className="jh-scope-banner" role="status"><span>{scope.kind === 'all' ? '全員の担当分を表示中です' : `ほかの人の担当分を表示中です（${scope.name ?? '選択中の担当者'}）`}</span>
    <button type="button" className="jc-button" onClick={() => { changeOwner('me'); }}>自分の担当分に戻す</button></p>;
}
export const signalCards = [
  ['expiration', '契約満了が近い', '今日から60日後まで'],
  ['expired', '満了後も稼働中', '満了日が今日より前で、終了していないステージ（ステージはHubSpotで直す）'],
  ['contact', '接触が空いている', '最後の接触から31日以上（MTGか60秒を超える電話。メールは数えない）'],
  ['stalled', '応募が止まっている', '掲載中の求人があり、直近14日の応募が0件'],
  ['first_contract', '初回で成果が出ていない', '開始30日で5件未満、または60日で10件未満'],
  ['nps', '定期の満足度が低い', '定期の満足度が4以下'],
  ['target', '採用目標に届いていない', '開始30日を過ぎ、採用目標の半分未満'],
  ['cpa', '採用単価が高い', '同じ進捗帯の中央値の1.5倍以上'],
  ['mtg', 'MTGが途絶えている', '最後のMTGから60日以上'],
] as const;
const planCards = [
  ['overdue', '期限切れ', '今日より前の予定', 'urgent'],
  ['today', '今日', '今日の予定', 'plain'],
  ['week', '7日以内', '明日から7日後までの予定', 'plain'],
] as const;
const PAGE_SIZE = 20;
/** ホームのカードに常に出すラベル（残りは件数付きの札に畳む）。 */
export const mainSignalKeys: readonly string[] = ['expiration', 'contact', 'stalled', 'mtg'];
/** 別の画面（案件一覧）で見るもの。札にはせず「→ 案件一覧で見る」で移る。 */
const goSignalKeys: readonly string[] = ['first_contract'];
/** 件数付きの札の 1 行。押すとその一覧が開く。0 件も薄く残し、並びを日によって変えない。 */
export function CountLine({ label, items, openKey, onToggle }: { label: string; items: { key: string; label: string; count: number | string; pressedKey?: string }[]; openKey: string | null; onToggle: (key: string) => void }) {
  if (!items.length) return null;
  return <div className="jh-more" role="group" aria-label={label}><span className="jh-more-label">{label}</span>{items.map(item => <CountChip key={item.key} label={item.label} count={item.count} pressed={openKey === (item.pressedKey ?? item.key)} onClick={() => { onToggle(item.key); }} />)}</div>;
}
/** カードの色: 赤＝急ぐ（予定が過ぎた）、橙＝注意、灰＝0件。 */
export function cardToneClass(count: number, tone: 'urgent' | 'warn' | 'plain'): string {
  if (count === 0) return 'jh-zero';
  return tone === 'urgent' ? 'jh-alert' : tone === 'warn' ? 'jh-warn' : 'jh-plain';
}
export function OwnerTodo({ work, onOpenCompany, onOpenDeal, onSaved, onShowList }: { onShowList?: (() => void) | undefined; onSaved?: (() => void) | undefined; work: WorkReady; onOpenCompany: (company: { id: string; name: string }) => void; onOpenDeal: (deal: ChangedDeal) => void }) {
  const query = new URLSearchParams(useJobCopySearch());
  const reveal = useRevealOnMount();
  if (work.todos_status) return <section className="jh-todo" aria-labelledby="jh-todo-title"><h2 id="jh-todo-title">やること</h2><ReportFreshness builtAt={work.facts_built_at} stale={work.facts_stale} refreshing={work.refreshing} /><p role={work.todos_status === 'failed' ? 'alert' : 'status'}>{work.todos_status === 'failed' ? 'やることを読み込めませんでした。再読み込みしてください。' : 'やることを集計しています…'}</p></section>;
  const open = query.get('card');
  const groups = [...signalCards.map(([key, label, rule]) => ({ key, label, rule, tone: 'warn' as const, items: (work.signals ?? []).filter(row => row.reasons?.some(r => r.kind === key)) })), ...planCards.map(([key, label, rule, tone]) => ({ key, label, rule, tone, items: work.actions[key] })), { key: 'later', label: 'それ以降の予定', rule: '8日後以降の予定', tone: 'plain' as const, items: work.actions.later }];
  const selected = groups.find(group => `todo_${group.key}` === open);
  const pages = Math.max(1, Math.ceil((selected?.items.length ?? 0) / PAGE_SIZE));
  const requested = query.get('todo_page') ?? '1';
  const page = Math.min(pages, /^[1-9]\d{0,5}$/.test(requested) ? Number(requested) : 1);
  const start = (page - 1) * PAGE_SIZE;
  const items = selected?.items.slice(start, start + PAGE_SIZE) ?? [];
  const toggle = (key: string) => { if (goSignalKeys.includes(key)) { updateJobCopyQuery({ view: 'deals', deals_first: 'yes', deals_launch: 'below', deals_reason: null, deals_sort: null, deals_page: null, card: null }, 'push'); return; } if (open !== `todo_${key}`) onShowList?.(); updateJobCopyQuery({ card: open === `todo_${key}` ? null : `todo_${key}`, todo_page: null, deals_page: null }, 'push'); };
  const close = () => { updateJobCopyQuery({ card: null, todo_page: null }, 'push'); };
  const changePage = (next: number) => { updateJobCopyQuery({ todo_page: next === 1 ? null : String(next) }, 'push'); };
  const signalGroups = groups.slice(0, signalCards.length);
  const planGroups = groups.slice(signalCards.length, signalCards.length + planCards.length);
  const mainSignals = signalGroups.filter(group => mainSignalKeys.includes(group.key));
  const otherSignals = signalGroups.filter(group => !mainSignalKeys.includes(group.key) && !goSignalKeys.includes(group.key));
  const goSignals = signalGroups.filter(group => goSignalKeys.includes(group.key));
  const card = (group: (typeof groups)[number]) => { const pressed = open === `todo_${group.key}`; return <button type="button" key={group.key} className={`jh-card ${cardToneClass(group.items.length, group.tone)}`} aria-pressed={pressed} onClick={() => { toggle(group.key); }}><strong className="jh-count">{group.items.length}<small>件</small></strong><span className="jh-card-title">{group.label}</span>{group.items.length > 0 && <CardOpenCue open={pressed} />}</button>; };
  // 押した段のすぐ下に一覧を開く（予定の段・ラベルの段のどちらか）。
  const inPlan = selected ? ['overdue', 'today', 'week', 'later'].includes(selected.key) : false;
  const list = selected && <section key={selected.key} ref={reveal} className={`jh-list ${selected.key === 'overdue' ? 'jh-todo-overdue' : ''}`} aria-label={`${selected.label}の一覧`}>
      <div className="jh-list-head"><h3>{selected.label}（{selected.items.length}件）</h3><CloseListButton label={`${selected.label}の一覧`} onClick={close} /></div>
      {items.length ? <><p className="jh-note">{start + 1}〜{start + items.length}件 ／ 全{selected.items.length}件</p><div className="jh-scroll"><table className="jh-todo-table"><thead><tr><th scope="col">会社</th><th scope="col">案件名</th><th scope="col">ステージ</th><th scope="col">ラベル</th><th scope="col">次の予定</th><th scope="col">コンサル担当</th><th scope="col">次の予定を入れる</th></tr></thead><tbody>{items.map(row => <tr key={row.id}><td>{row.company_id ? <button type="button" className="jh-link" onClick={() => { onOpenCompany({ id: row.company_id ?? '', name: row.company ?? '会社名未取得' }); }}>{row.company ?? '会社名未取得'}</button> : '—'}</td><th scope="row"><button type="button" className="jh-link" onClick={() => { onOpenDeal(row); }}>{row.name ?? '案件名未取得'}</button>{selected.key === 'expired' && /^\d+$/.test(work.hubspot_portal_id ?? '') && <><br /><OutLink href={`https://app.hubspot.com/contacts/${work.hubspot_portal_id ?? ''}/record/0-3/${row.id}`} name="HubSpotで開く" /></>}</th><td>{row.stage ?? 'ステージ未取得'}</td><td><ReasonChips reasons={row.reasons ?? []} /></td><td><time>{formatDateJst(row.actionyotei ?? (selected.key === 'expiration' || signalCards.some(c => c[0] === selected.key) ? null : row.due), '予定なし')}</time><OverdueBadge due={row.actionyotei ?? (selected.key === 'expiration' || signalCards.some(c => c[0] === selected.key) ? null : row.due)} /><br />{row.content ?? '内容未登録'}</td><td>{row.consultant ?? '担当未登録'}</td><td><NextTodoButton compact dealId={row.id} hasPlan={!!row.actionyotei || planCards.some(c => c[0] === selected.key) || selected.key === 'later'} onSaved={onSaved} /></td></tr>)}</tbody></table></div></> : <p className="jh-note">該当する案件はありません。</p>}
      {pages > 1 && <nav className="jh-todo-pages" aria-label={`${selected.label}のページ`}><button type="button" className="jc-button" disabled={page === 1} onClick={() => { changePage(page - 1); }}>前の20件</button><span>{page} / {pages}ページ</span><button type="button" className="jc-button" disabled={page === pages} onClick={() => { changePage(page + 1); }}>次の20件</button></nav>}
    </section>;
  return <section className="jh-todo" aria-labelledby="jh-todo-title"><h2 id="jh-todo-title">やること</h2><ReportFreshness builtAt={work.facts_built_at} stale={work.facts_stale} refreshing={work.refreshing} />
    {work.refresh_failed && <p role="status">最新の情報を読み込めなかったため、前回取得した情報を表示しています。</p>}
    <div><div className="jh-section-head"><h3>予定 <small>担当者が入れた次の予定</small></h3></div><div className="jh-cards jh-todo-cards">{planGroups.map(group => card(group))}</div>
      <CountLine label="ほかの予定" items={[{ key: 'later', label: 'それ以降の予定', count: work.actions.later.length, pressedKey: 'todo_later' }]} openKey={open} onToggle={toggle} />
      {inPlan && list}</div>
    <div><div className="jh-section-head"><h3>ラベルが付いた案件</h3><InfoTip className="jc-infotip-aside" iconFirst label="数え方を見る"><p>ラベルは記録から自動で付き、条件が解消すると消えます。日付は日本時間、終了した案件は除いています。</p><p>色の意味：赤は予定が過ぎたもの（期限切れ）、橙は注意、灰は0件です。ラベルの赤い札は、MTGが60日以上途絶えている案件です。</p><p>MTGが途絶えているラベル：{work.sheet_as_of ? `${formatDateJst(work.sheet_as_of)} 時点（前日の集計）` : '集計した日が未取得'}。</p><ul>{signalGroups.map(group => <li key={group.key}>{group.label}：{group.rule}</li>)}</ul></InfoTip></div>
      <div className="jh-cards jh-todo-cards">{mainSignals.map(group => card(group))}</div>
      <CountLine label="ほかのラベル" items={otherSignals.map(group => ({ key: group.key, label: group.label, count: group.items.length, pressedKey: `todo_${group.key}` }))} openKey={open} onToggle={toggle} />
      {goSignals.map(group => <p key={group.key} className="jh-go-line"><span>{group.label} {group.items.length}件</span><GoLink onClick={() => { toggle(group.key); }}>案件一覧で見る</GoLink></p>)}
      {selected && !inPlan && list}</div>
  </section>;
}
