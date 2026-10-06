import { useState } from 'react';
import { ownerLabel, visibleOwners } from './ownerModel';
import type { OwnersState } from './useOwners';

interface Props {
  /** 絞り込み条件の owner ('' | 'all' | 'me' | 'unassigned' | HubSpot owner ID) */
  owner: string;
  onChange: (owner: string) => void;
  owners: OwnersState;
  onReload: () => void;
}

/** 管理者の担当者の絞り込み。全員 / 担当なし / 自分 / 一覧から名前で選ぶ。一覧が取れなければ ID の入力に戻す */
export function OwnerFilter({ owner, onChange, owners, onReload }: Props) {
  const isId = /^\d+$/.test(owner);
  const [pickMode, setPickMode] = useState(isId);
  const [query, setQuery] = useState('');
  const [includeArchived, setIncludeArchived] = useState(false);

  const selectValue = pickMode ? 'pick' : owner === 'all' ? '' : owner;

  function chooseMode(v: string) {
    if (v === 'pick') {
      setPickMode(true);
      if (!isId) onChange('');
    } else {
      setPickMode(false);
      onChange(v);
    }
  }

  const list = owners.phase === 'ready' ? owners.owners : [];
  const shown = visibleOwners(list, query, includeArchived, isId ? owner : '');
  const known = isId && list.some(o => o.id === owner);

  return <fieldset className="cq-owner"><legend>担当者(管理者のみ)</legend>
    <select aria-label="担当者" value={selectValue} onChange={e => { chooseMode(e.target.value); }}>
      <option value="">全員分</option><option value="unassigned">担当者なし</option>
      <option value="me">自分</option><option value="pick">一覧から選ぶ</option></select>

    {pickMode && owners.phase === 'loading' && <p role="status" className="cq-owner-note">担当者の一覧を読み込み中…</p>}

    {pickMode && owners.phase === 'error' && <div className="cq-owner-fallback">
      <p role="alert" className="cq-owner-note cq-owner-err">担当者の一覧を取得できませんでした。{owners.message}
        担当者ID(HubSpot owner ID)を直接入力することもできます。</p>
      <button type="button" onClick={onReload}>一覧を再取得</button>
      <input aria-label="担当者ID(HubSpot owner ID)" inputMode="numeric" placeholder="担当者ID(数字)"
        value={isId ? owner : ''} onChange={e => { onChange(e.target.value.replace(/\D/g, '').slice(0, 20)); }} />
    </div>}

    {pickMode && owners.phase === 'ready' && <div className="cq-owner-pick">
      <input type="search" aria-label="担当者を検索" placeholder="名前・メールで検索" value={query}
        onChange={e => { setQuery(e.target.value); }} />
      <label className="cq-check"><input type="checkbox" checked={includeArchived}
        onChange={e => { setIncludeArchived(e.target.checked); }} />退職者も表示</label>
      <select aria-label="担当者を選ぶ" size={Math.min(8, Math.max(2, shown.length + (isId && !known ? 1 : 0)))}
        value={isId ? owner : ''} onChange={e => { onChange(e.target.value); }}>
        {isId && !known && <option value={owner}>ID {owner}(一覧にありません)</option>}
        {shown.map(o => <option key={o.id} value={o.id}>{ownerLabel(o)}</option>)}
      </select>
      {shown.length === 0 && <p role="status" className="cq-owner-note">該当する担当者がいません。</p>}
      {!isId && <p className="cq-owner-note">担当者を選んでください(選ぶまでは全員分です)。</p>}
      {owners.truncated && <p className="cq-owner-note">担当者が多いため一覧の一部しか取得できていません。</p>}
    </div>}
  </fieldset>;
}
