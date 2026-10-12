import { useId, useState } from 'react';
import type { CrmCatalogProperty } from '../../generated/CrmCatalogProperty';
/** 検索と選択を同じ入力で行う。内部名は候補に表示しない。 */
export function QueuePropertyPicker({ properties, value, onChange, label = '項目を検索' }: {
  properties: CrmCatalogProperty[]; value: CrmCatalogProperty | null; onChange: (p: CrmCatalogProperty | null) => void; label?: string;
}) {
  const id = useId();
  const [query, setQuery] = useState(value?.label ?? '');
  const [open, setOpen] = useState(false);
  const options = properties.filter(p => p.label.toLowerCase().includes(query.toLowerCase()));
  const choose = (p: CrmCatalogProperty) => { setQuery(p.label); onChange(p); setOpen(false); };
  return <div className="cq-property-picker"><label>{label}<input role="combobox" aria-expanded={open} aria-controls={id} aria-autocomplete="list"
    value={query} placeholder="項目名を入力" onFocus={() => { setOpen(true); }}
    onChange={e => { setQuery(e.target.value); onChange(null); setOpen(true); }}
    onKeyDown={e => { if (e.key === 'Escape') { e.stopPropagation(); setOpen(false); } if (e.key === 'ArrowDown') { e.preventDefault(); document.getElementById(id)?.querySelector<HTMLButtonElement>('button')?.focus(); } }} /></label>
    {open && <ul id={id} role="listbox" aria-label={`${label}の候補`}>{options.map(p => <li key={p.name} role="none"><button type="button" role="option" aria-selected={value?.name === p.name} onClick={() => { choose(p); }}>{p.label}</button></li>)}
      {options.length === 0 && <li role="none">一致する項目がありません</li>}</ul>}
  </div>;
}
