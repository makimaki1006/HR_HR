import { useState } from 'react';
import { AdminLayout } from '../../ui/layouts';
import { Badge, Button, Card, Field, Select, TextInput, cx, formatShortDateTime } from '../../ui/kit';
import { ROUTES, usePreviewNav } from '../../preview';
import type { CustomerStage } from '../../types';
import { CONNECTION_LABEL, STAGE_LABEL, STAGE_TONE, actions, stageWarning, useAdminState } from './state';

type StageFilter = 'all' | 'disabled' | CustomerStage;

export function CustomerList() {
  const { go } = usePreviewNav();
  const s = useAdminState();
  const [query, setQuery] = useState('');
  const [stage, setStage] = useState<StageFilter>('all');
  const [adding, setAdding] = useState(false);
  const [name, setName] = useState('');

  const rows = s.customers.filter((c) => {
    const byStage = stage === 'all' || (stage === 'disabled' ? c.disabled : c.stage === stage);
    return byStage && c.name.includes(query.trim());
  });

  const open = (id: string) => {
    actions.selectCustomer(id);
    go('admin-customer-detail');
  };
  const create = () => {
    const trimmed = name.trim();
    if (!trimmed) return;
    actions.addCustomer(trimmed);
    setName('');
    setAdding(false);
    go('admin-customer-detail');
  };

  return (
    <AdminLayout title="顧客とシート" current="customers" onNavigate={(to) => { go(to === 'customers' ? ROUTES.adminCustomers : ROUTES.adminAccounts); }}>
      <div className="mb-4 flex flex-wrap items-end gap-3">
        <div className="w-72"><Field label="顧客名で探す"><TextInput value={query} onChange={(e) => { setQuery(e.target.value); }} placeholder="顧客名" /></Field></div>
        <div className="w-60">
          <Field label="絞り込み">
            <Select value={stage} onChange={(e) => { setStage(e.target.value as StageFilter); }}>
              <option value="all">すべて</option>
              <option value="sheet_only">{STAGE_LABEL.sheet_only}</option>
              <option value="both">{STAGE_LABEL.both}</option>
              <option value="portal_only">{STAGE_LABEL.portal_only}</option>
              <option value="disabled">停止中</option>
            </Select>
          </Field>
        </div>
        <span className="pb-3 text-sm text-pt-muted">{rows.length} 社</span>
        <div className="ml-auto"><Button variant="primary" onClick={() => { setAdding(true); }}>顧客を追加</Button></div>
      </div>

      {adding && (
        <Card className="mb-4 p-4">
          <div className="flex flex-wrap items-end gap-3">
            <div className="w-96"><Field label="顧客名" hint="最初は「シートだけ」の段階で登録されます。"><TextInput autoFocus value={name} onChange={(e) => { setName(e.target.value); }} onKeyDown={(e) => { if (e.key === 'Enter') create(); }} /></Field></div>
            <Button variant="primary" disabled={!name.trim()} onClick={create}>追加する</Button>
            <Button variant="ghost" onClick={() => { setAdding(false); setName(''); }}>やめる</Button>
          </div>
        </Card>
      )}

      <Card className="overflow-hidden">
        <table className="w-full table-fixed text-sm">
          <colgroup>
            <col className="w-[26%]" /><col className="w-[22%]" /><col className="w-[20%]" /><col className="w-[14%]" /><col className="w-[18%]" />
          </colgroup>
          <thead className="bg-pt-bg text-left text-xs text-pt-muted">
            <tr>
              <th className="px-4 py-2 font-medium">顧客名</th>
              <th className="px-4 py-2 font-medium">段階</th>
              <th className="px-4 py-2 font-medium">シート</th>
              <th className="px-4 py-2 font-medium">アカウント</th>
              <th className="px-4 py-2 font-medium">最後に使われた日時</th>
            </tr>
          </thead>
          <tbody>
            {rows.map((c) => {
              const mine = s.sheets.filter((sh) => sh.customerId === c.id);
              const bad = mine.filter((sh) => sh.connection !== 'ok');
              const accts = s.accounts.filter((a) => a.customerId === c.id);
              const usable = accts.filter((a) => a.status !== 'disabled').length;
              const off = accts.length - usable;
              const first = bad[0];
              const warn = stageWarning(c.stage, mine);
              return (
                <tr key={c.id} onClick={() => { open(c.id); }} className={cx('cursor-pointer border-t border-pt-line hover:bg-pt-bg', c.disabled && 'text-pt-muted')}>
                  <td className="px-4 py-3">
                    <button type="button" onClick={(e) => { e.stopPropagation(); open(c.id); }} className="text-left font-medium text-pt-primary hover:underline">{c.name}</button>
                    {c.disabled && <span className="ml-2"><Badge tone="danger">停止中</Badge></span>}
                  </td>
                  <td className="px-4 py-3">
                    <Badge tone={STAGE_TONE[c.stage]}>{STAGE_LABEL[c.stage]}</Badge>
                    {warn && <div className="mt-1"><Badge tone="warn">{warn}</Badge></div>}
                  </td>
                  <td className="px-4 py-3">
                    {mine.length === 0 ? <span className="text-pt-faint">未登録</span> : (
                      <div>
                        <div>{mine.length} 枚</div>
                        <div className="mt-0.5">
                          {first
                            ? <Badge tone="warn">{bad.length === 1 && mine.length === 1 ? CONNECTION_LABEL[first.connection] : String(mine.length) + ' 枚中 ' + String(bad.length) + ' 枚 ' + CONNECTION_LABEL[first.connection]}</Badge>
                            : <Badge tone="ok">つながっています</Badge>}
                        </div>
                      </div>
                    )}
                  </td>
                  <td className="px-4 py-3">
                    <div>使える {usable} 件</div>
                    {off > 0 && <div className="text-xs text-pt-faint">使えない {off} 件</div>}
                  </td>
                  <td className="px-4 py-3 text-pt-muted">{c.lastUsedAt ? formatShortDateTime(c.lastUsedAt) : 'まだ使われていません'}</td>
                </tr>
              );
            })}
            {rows.length === 0 && <tr><td colSpan={5} className="px-4 py-10 text-center text-pt-muted">該当する顧客がありません。</td></tr>}
          </tbody>
        </table>
      </Card>
    </AdminLayout>
  );
}
