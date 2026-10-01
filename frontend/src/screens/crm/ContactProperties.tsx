import type { CrmRecord } from './model';
import { SAMPLE_RECORDS } from './fixtures';
import { propertyDraftKey, selectedPropertyValues, togglePropertyValue } from './mocProperties';
import { metadataDealDefinitions, useCrmMetadata } from './liveMetadata';
import './moc-properties.css';

const editable = new Set(['email', 'phone', 'jobtitle', 'industry', 'city', 'numberofemployees']);

export function MocPropertyInput({ recordName, definitionName, value, onChange, disabled = false }: {
  recordName: string; definitionName: string; value: string; onChange: (value: string) => void; disabled?: boolean;
}) {
  const metadata = useCrmMetadata();
  const definition = metadataDealDefinitions(metadata)[definitionName];
  if (!definition) return <p className="batch-error">項目 {definitionName} は取得したHubSpot定義にありません。</p>;
  const label = `${recordName}さんの${definition.label}`;
  if (!definition.editable) return <label>{definition.label}<output aria-label={label}>{value || '未設定（MOC）'}</output></label>;
  if (definition.fieldType === 'checkbox') {
    const selected = selectedPropertyValues(value);
    const options = definition.options.filter(o => !o.hidden || selected.includes(o.value));
    const unknown = selected.filter(v => !definition.options.some(o => o.value === v));
    return <fieldset className="moc-property-checkboxes" disabled={disabled} aria-label={label}>
      <legend>{definition.label}</legend>
      {options.map(option => <label key={option.value}><input type="checkbox"
        aria-label={`${label}：${option.label}`} checked={selected.includes(option.value)}
        onChange={e => { onChange(togglePropertyValue(value, option.value, e.target.checked)); }} />{option.label}</label>)}
      {unknown.map(option => <label key={option}><input type="checkbox" checked
        aria-label={`${label}：既存値 ${option}`} onChange={() => { onChange(togglePropertyValue(value, option, false)); }} />既存値：{option}</label>)}
    </fieldset>;
  }
  if (definition.type === 'enumeration') {
    const options = definition.options.filter(o => !o.hidden || value === o.value);
    return <label>{definition.label}<select aria-label={label} value={value} disabled={disabled}
      onChange={e => { onChange(e.target.value); }}><option value="">未確認・未設定</option>
      {value && !options.some(o => o.value === value) && <option value={value}>既存値：{value}</option>}
      {options.map(option => <option key={option.value} value={option.value}>{option.label}</option>)}
    </select></label>;
  }
  if (definition.fieldType === 'textarea') return <label>{definition.label}<textarea rows={3}
    aria-label={label} value={value} disabled={disabled} placeholder="ヒアリングしながら入力"
    onChange={e => { onChange(e.target.value); }} /></label>;
  return <label>{definition.label}<input type={definition.type === 'date' ? 'date' : 'text'}
    aria-label={label} value={value} disabled={disabled} placeholder="未確認"
    onChange={e => { onChange(e.target.value); }} /></label>;
}

const groups = [
  { title: '案件の担当者・採用ヒアリング', names: ['bpo_21', 'bpo_22', 'bpo_50', 'bpo_24', 'bpo_49', 'bpo_34', 'bpo_25', 'bpo_8'] },
  { title: '商談・アポイント', names: ['bpo_23', 'bpo__', 'bpo_33'] },
  { title: '架電停止・ブロック', names: ['bpo_3', 'bpo_4', 'bpo_10'] },
  { title: '連携情報（参照のみ）', names: ['bpo_18', 'bpo_19', 'bpo_32'] },
];

export function ContactProperties({ record, values, onChange }: {
  record: CrmRecord; values: Record<string, string>; onChange: (key: string, value: string) => void;
}) {
  const metadata = useCrmMetadata();
  const company = SAMPLE_RECORDS.find(r => r.id === record.associations.find(a => a.objectType === 'companies')?.id);
  return <div className="batch-properties" id={`properties-${record.id}`}>
    <div className="batch-properties-heading"><strong>{record.name}さんの詳細プロパティ</strong><span>{metadata ? 'HubSpot API取得定義' : '固定定義スナップショット'}・架空の値・HubSpot未保存</span></div>
    <div className="batch-properties-grid">
      {[{ title: '担当者情報', source: record }, ...(company ? [{ title: '会社情報', source: company }] : [])].map(({ title, source }) =>
        <section key={source.id}><h2>{title}</h2><dl>{source.properties.map(property => {
          const key = `${source.id}:${property.name}`;
          const label = metadata ? metadata.properties.find(item => item.object_type === source.objectType && item.name === property.name)?.label ?? `${property.name}（定義なし）` : property.label;
          return <div key={key}><dt>{label}</dt><dd>{editable.has(property.name) ?
            <input aria-label={`${record.name}さんの${title}の${label}`} value={values[key] ?? property.value ?? ''}
              placeholder="未確認" maxLength={300} onChange={e => { onChange(key, e.target.value); }} /> : property.value ?? '未設定'}</dd></div>;
        })}</dl></section>)}
      {groups.map(group => <section key={group.title}><h2>{group.title}</h2>
        {group.names.map(name => <MocPropertyInput key={name} recordName={record.name} definitionName={name}
          value={values[propertyDraftKey(record, name)] ?? ''} onChange={value => { onChange(propertyDraftKey(record, name), value); }} />)}
      </section>)}
    </div>
    <p className="batch-property-note">案件項目はDeal単位で分離し、会社基本情報のみ同じ会社で共有します。担当オーナー・ステータス・連携情報は参照のみ。実レコードの値や保存処理は未接続です。</p>
  </div>;
}
