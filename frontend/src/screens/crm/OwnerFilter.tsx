import { useCallback, useRef, useState } from 'react';
import { useDismiss } from './useDismiss';
import { ownerLabel, visibleOwners } from './ownerModel';
import type { OwnersState } from './useOwners';

interface Props {
  /** 絞り込み条件の owner ('' | 'all' | 'me' | 'unassigned' | HubSpot owner ID)。'' は既定 (管理者は全員分、それ以外は自分) */
  owner: string;
  /** owner が既定 ('') のとき、サーバが実際に使っている所有者 (応答の scope.owner)。まだ分からなければ null */
  effective: string | null;
  /** 自分の所有者を HubSpot で見つけられず、選ぶ必要がある */
  needsPick: boolean;
  onChange: (owner: string) => void;
  owners: OwnersState;
  onReload: () => void;
}

/** 所有者の絞り込み (全員が使える)。全員分 / 担当なし / 自分 / 一覧から名前で選ぶ。一覧が取れなければ ID の入力に戻す */
export function OwnerFilter({ owner, effective, needsPick, onChange, owners, onReload }: Props) {
  const current = owner !== '' ? owner : (effective ?? '');
  const isId = /^\d+$/.test(owner);
  // 一覧は「開いている間」だけ出す。選ぶ・外をクリック・Esc・フォーカスが外れる、で閉じる
  const [pickMode, setPickMode] = useState(false);
  const rootRef = useRef<HTMLFieldSetElement>(null);
  const triggerRef = useRef<HTMLSelectElement>(null);
  const [query, setQuery] = useState('');
  const [includeArchived, setIncludeArchived] = useState(false);
  // 自分の所有者が見つからないときは、最初から一覧を開いて選んでもらう
  const showPick = pickMode || (needsPick && !isId);

  const closePick = useCallback(() => { setPickMode(false); }, []);
  const closeAndFocus = useCallback(() => { setPickMode(false); triggerRef.current?.focus(); }, []);
  useDismiss(rootRef, pickMode, closePick, closeAndFocus);

  const selectValue = showPick ? 'pick' : isId ? `id:${owner}` : current;

  function chooseMode(v: string) {
    if (v === 'pick') {
      setPickMode(true);
    } else if (!v.startsWith('id:')) {
      setPickMode(false);
      onChange(v);
    }
  }
  function choose(id: string) {
    onChange(id);
    closeAndFocus();
  }

  const list = owners.phase === 'ready' ? owners.owners : [];
  const shown = visibleOwners(list, query, includeArchived, isId ? owner : '');
  const known = isId && list.some(o => o.id === owner);
  const picked = isId ? list.find(o => o.id === owner) : undefined;

  return <fieldset className="cq-owner" ref={rootRef}
    onBlur={e => { if (pickMode && e.relatedTarget instanceof Node && !e.currentTarget.contains(e.relatedTarget)) closePick(); }}>
    <legend>所有者</legend>
    <select aria-label="所有者" ref={triggerRef} value={selectValue} onChange={e => { chooseMode(e.target.value); }}>
      {current === '' && !showPick && <option value="">(確認中)</option>}
      <option value="all">全員分</option><option value="unassigned">担当者なし</option>
      <option value="me">自分</option>
      {isId && <option value={`id:${owner}`}>{picked ? ownerLabel(picked) : `ID ${owner}`}</option>}
      <option value="pick">一覧から選ぶ</option></select>

    {needsPick && !isId && <p role="status" className="cq-owner-note cq-owner-err">所有者を選んでください(あなたに対応する HubSpot の所有者が見つかりません)。</p>}

    {showPick && owners.phase === 'loading' && <p role="status" className="cq-owner-note">所有者の一覧を読み込み中…</p>}

    {showPick && owners.phase === 'error' && <div className="cq-owner-fallback">
      <p role="alert" className="cq-owner-note cq-owner-err">所有者の一覧を取得できませんでした。{owners.message}
        所有者ID(HubSpot owner ID)を直接入力することもできます。</p>
      <button type="button" onClick={onReload}>一覧を再取得</button>
      <input aria-label="所有者ID(HubSpot owner ID)" inputMode="numeric" placeholder="所有者ID(数字)"
        value={isId ? owner : ''} onChange={e => { onChange(e.target.value.replace(/\D/g, '').slice(0, 20)); }} />
    </div>}

    {showPick && owners.phase === 'ready' && <div className="cq-owner-pick">
      <input type="search" aria-label="所有者を検索" placeholder="名前・メールで検索" value={query}
        onChange={e => { setQuery(e.target.value); }} />
      <label className="cq-check"><input type="checkbox" checked={includeArchived}
        onChange={e => { setIncludeArchived(e.target.checked); }} />退職者も表示</label>
      <select aria-label="所有者を選ぶ" size={Math.min(8, Math.max(2, shown.length + (isId && !known ? 1 : 0)))}
        value={isId ? owner : ''} onChange={e => { choose(e.target.value); }}>
        {isId && !known && <option value={owner}>ID {owner}(一覧にありません)</option>}
        {shown.map(o => <option key={o.id} value={o.id}>{ownerLabel(o)}</option>)}
      </select>
      {shown.length === 0 && <p role="status" className="cq-owner-note">該当する所有者がいません。</p>}
      {!isId && !needsPick && <p className="cq-owner-note">所有者を選んでください(選ぶまでは「{current === 'me' ? '自分' : current === 'all' ? '全員分' : current === 'unassigned' ? '担当者なし' : '現在の表示'}」のままです)。</p>}
      {owners.truncated && <p className="cq-owner-note">所有者が多いため一覧の一部しか取得できていません。</p>}
    </div>}
  </fieldset>;
}
