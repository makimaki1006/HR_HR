import { useState } from 'react';
import { SAMPLE_RECORDS } from './fixtures';
import {
  ACTIVITY_LABELS, OBJECT_LABELS, associationGroups, filterActivities, findRecords, formatDate,
} from './model';
import type { ActivityFilter, CrmRecord, ObjectType } from './model';
import './crm.css';
import { BatchCallWorkspace } from './BatchCallWorkspace';

export function RecordView({ record, records, onSelect }: {
  record: CrmRecord; records: CrmRecord[]; onSelect: (record: CrmRecord) => void;
}) {
  const [tab, setTab] = useState<'activities' | 'about'>('activities');
  const [activityType, setActivityType] = useState<ActivityFilter>('all');
  const [query, setQuery] = useState('');
  const [owner, setOwner] = useState('');
  const [since, setSince] = useState('');
  const activities = filterActivities(record.activities, activityType, query, owner, since);
  const owners = [...new Set(record.activities.map(a => a.owner))];
  const groups = associationGroups(record, records);

  return <div className="crm-record-layout">
    <aside className="crm-properties" aria-label="レコードの基本情報">
      <section className="crm-card crm-identity">
        <span className="crm-eyebrow">{OBJECT_LABELS[record.objectType]}</span>
        <div className="crm-avatar" aria-hidden="true">{record.name.slice(0, 1)}</div>
        <h1>{record.name}</h1><p>{record.subtitle}</p>
        <span className="crm-status">{record.status}</span>
        <div className="crm-action-row" aria-label="操作は実接続後に利用可能">
          {['メモ', 'Eメール', 'コール', 'タスク'].map(action =>
            <button key={action} disabled title="読み取りサンプル版では利用できません">{action}</button>)}
        </div>
        {record.deepLink ? <a href={record.deepLink} target="_blank" rel="noreferrer">HubSpotで開く</a> :
          <span className="crm-muted">HubSpotリンクは実接続後に表示</span>}
      </section>
      <section className="crm-card"><h2>基本情報</h2><dl className="crm-property-list">
        {record.properties.map(p => <div key={p.name}><dt>{p.label}</dt>
          <dd>{p.value ?? <span className="crm-muted">未設定</span>}</dd></div>)}
      </dl></section>
    </aside>

    <main className="crm-main">
      <div className="crm-main-tabs" role="tablist" aria-label="レコード詳細">
        <button role="tab" aria-selected={tab === 'activities'} id="crm-activities-tab"
          aria-controls="crm-activities-panel" tabIndex={tab === 'activities' ? 0 : -1}
          onKeyDown={e => { if (['ArrowLeft', 'ArrowRight', 'End'].includes(e.key)) {
            e.preventDefault(); setTab('about'); document.getElementById('crm-about-tab')?.focus();
          } }} onClick={() => { setTab('activities'); }}>アクティビティ</button>
        <button role="tab" aria-selected={tab === 'about'} id="crm-about-tab"
          aria-controls="crm-about-panel" tabIndex={tab === 'about' ? 0 : -1}
          onKeyDown={e => { if (['ArrowLeft', 'ArrowRight', 'Home'].includes(e.key)) {
            e.preventDefault(); setTab('activities'); document.getElementById('crm-activities-tab')?.focus();
          } }} onClick={() => { setTab('about'); }}>概要</button>
      </div>
      {tab === 'about' ? <section className="crm-about" role="tabpanel" id="crm-about-panel"
        aria-labelledby="crm-about-tab">
        <section className="crm-card"><h2>レコードの概要</h2>
          <dl className="crm-overview-grid"><div><dt>担当者</dt><dd>{record.owner}</dd></div>
            <div><dt>ステータス</dt><dd>{record.status}</dd></div>
            <div><dt>最終更新</dt><dd>{formatDate(record.updatedAt)}</dd></div>
            <div><dt>記録された活動</dt><dd>{record.activities.length}件</dd></div></dl>
        </section>
        <section className="crm-card"><h2>直近の活動</h2>
          {filterActivities(record.activities, 'all', '', '', '').slice(0, 1).map(a =>
            <div key={a.id}><strong>{a.title}</strong><p>{a.body}</p><small>{formatDate(a.occurredAt)} · {a.owner}</small></div>)}
          {record.activities.length === 0 && <p className="crm-muted">記録された活動はありません。</p>}
        </section>
      </section> : <section role="tabpanel" id="crm-activities-panel" aria-labelledby="crm-activities-tab">
        <div className="crm-activity-types" aria-label="活動種別">
          {(Object.keys(ACTIVITY_LABELS) as ActivityFilter[]).map(type =>
            <button key={type} aria-pressed={activityType === type} onClick={() => { setActivityType(type); }}>
              {ACTIVITY_LABELS[type]}</button>)}
        </div>
        <div className="crm-activity-filters">
          <label className="crm-search-label">活動を検索<input type="search" value={query}
            placeholder="活動のタイトル・本文" onChange={e => { setQuery(e.target.value); }} /></label>
          <label>担当者<select value={owner} onChange={e => { setOwner(e.target.value); }}>
            <option value="">すべての担当者</option>{owners.map(name => <option key={name}>{name}</option>)}
          </select></label>
          <label>期間<select value={since} onChange={e => { setSince(e.target.value); }}>
            <option value="">すべての期間</option><option value="2026-09-30T00:00:00+09:00">2026/09/30以降</option>
          </select></label>
        </div>
        <div className="crm-timeline">
          <p className="crm-timeline-heading" aria-live="polite">{ACTIVITY_LABELS[activityType]} · {activities.length}件</p>
          {activities.map(activity => <details className="crm-card crm-activity-card" key={activity.id} open>
            <summary><span className={`crm-activity-label crm-${activity.type}`}>{ACTIVITY_LABELS[activity.type]}</span>
              <strong>{activity.title}</strong></summary>
            <div className="crm-activity-body"><p className="crm-activity-meta">{formatDate(activity.occurredAt)} · {activity.owner}</p>
              <p>{activity.body}</p>{activity.outcome && <span className="crm-status">{activity.outcome}</span>}</div>
          </details>)}
          {activities.length === 0 && <div className="crm-empty">
            <h2>{record.activities.length === 0 ? '記録された活動はありません' : '条件に一致する活動はありません'}</h2>
            <p>{record.activities.length === 0 ? 'このサンプルレコードには活動が登録されていません。' : '検索語・活動種別・担当者・期間を変更してください。'}</p>
          </div>}
        </div>
      </section>}
    </main>

    <aside className="crm-associations" aria-label="関連レコード">
      {groups.map(group => <details className="crm-card crm-association-group" key={group.objectType} open>
        <summary><strong>{OBJECT_LABELS[group.objectType]}</strong><span>{group.records.length}件</span></summary>
        {group.records.map(({ association, record: associated }) => <div className="crm-associated-card" key={association.id}>
          {associated ? <><button className="crm-record-link" onClick={() => { onSelect(associated); }}>{associated.name}</button>
            {association.label && <span className="crm-association-label">{association.label}</span>}
            <p>{associated.subtitle}</p><small>担当: {associated.owner}</small></> : <p>関連レコードを取得できません</p>}
        </div>)}
        {group.records.length === 0 && <p className="crm-muted">関連する{OBJECT_LABELS[group.objectType]}はありません。</p>}
      </details>)}
    </aside>
  </div>;
}

export function CrmReferenceScreen() {
  const [objectType, setObjectType] = useState<ObjectType>('contacts');
  const [selectedId, setSelectedId] = useState('sample-contact-1');
  const [search, setSearch] = useState('');
  const [ascending, setAscending] = useState(true);
  const records = findRecords(SAMPLE_RECORDS, objectType, search).sort((a, b) =>
    (ascending ? 1 : -1) * a.name.localeCompare(b.name, 'ja'));
  const selected = SAMPLE_RECORDS.find(r => r.id === selectedId && r.objectType === objectType);

  function select(record: CrmRecord) {
    setObjectType(record.objectType); setSelectedId(record.id); setSearch('');
  }
  function switchObject(type: ObjectType) {
    setObjectType(type); setSearch('');
    setSelectedId(SAMPLE_RECORDS.find(r => r.objectType === type)?.id ?? '');
  }

  return <div className="crm-app">
    <header className="crm-topbar"><a className="crm-home" href="/">HR_HR</a>
      <span className="crm-topbar-divider" /><strong>CRM Workspace</strong>
      <a className="crm-topbar-right" href="?view=calling">架電ワークスペースへ</a></header>
    <div className="crm-sample-banner" role="status"><strong>サンプル版</strong>
      <span>表示内容はすべて架空です。HubSpotへの接続・保存・発信は行いません。</span>
      <a href="https://knowledge.hubspot.com/records/work-with-records" target="_blank" rel="noreferrer">UIの参照元</a>
    </div>
    <nav className="crm-object-nav" aria-label="CRMオブジェクト">
      {(Object.keys(OBJECT_LABELS) as ObjectType[]).map(type =>
        <button key={type} aria-pressed={objectType === type} onClick={() => { switchObject(type); }}>{OBJECT_LABELS[type]}</button>)}
    </nav>
    <div className="crm-workspace">
      <aside className="crm-record-list" aria-label="レコード一覧">
        <div className="crm-list-header"><h2>{OBJECT_LABELS[objectType]}</h2><span>{records.length}件</span></div>
        <label className="crm-list-search">レコードを検索<input type="search" value={search}
          placeholder="名前・会社・担当者" onChange={e => { setSearch(e.target.value); }} /></label>
        <button className="crm-sort" onClick={() => { setAscending(!ascending); }}>名前順: {ascending ? '昇順' : '降順'}</button>
        <div className="crm-list-items">{records.map(record => <button key={record.id}
          className="crm-list-record" aria-current={selectedId === record.id ? 'true' : undefined}
          onClick={() => { select(record); }}><strong>{record.name}</strong><span>{record.subtitle}</span>
          <small>{record.status} · {record.owner}</small></button>)}
          {records.length === 0 && <p className="crm-empty-list" role="status">一致するレコードはありません。</p>}
        </div>
        <p className="crm-list-footer">HubSpot標準の情報構成を参照<br />実アカウントの項目は確認待ち</p>
      </aside>
      {selected && <RecordView key={selected.id} record={selected} records={SAMPLE_RECORDS} onSelect={select} />}
    </div>
  </div>;
}

export function CrmScreen() {
  return <BatchCallWorkspace />;
}
