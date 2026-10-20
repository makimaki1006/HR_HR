import { useMemo, useState } from 'react';
import { sheetsOf, useData } from '../../data';
import { ROUTES, usePreviewNav } from '../../preview';
import { Badge, Button, Dialog, Field, IconButton, Segmented, TextInput, formatShortDateTime } from '../../ui/kit';
import { IconEye, IconPlus, IconSearch } from '../../ui/icons';
import type { CustomerStage } from '../../types';
import { AdminPage, StatusDot, TD, TH, Table } from './shared';
import { STAGE_LABEL, STAGE_TONE, actions, enterAs, isReady, stageWarning } from './state';

type Filter = 'all' | CustomerStage;

export function CustomerList() {
  const { go } = usePreviewNav();
  const d = useData();
  const [filter, setFilter] = useState<Filter>('all');
  const [query, setQuery] = useState('');
  const [adding, setAdding] = useState(false);
  const [name, setName] = useState('');

  const shown = useMemo(() => d.customers.filter((c) => (filter === 'all' || c.stage === filter) && (query.trim() === '' || c.name.includes(query.trim()))), [d.customers, filter, query]);

  const open = (id: string): void => {
    actions.selectCustomer(id);
    go('admin-customer-detail');
  };
  const view = (id: string): void => {
    enterAs(id);
    go(ROUTES.list);
  };
  const create = (): void => {
    const n = name.trim();
    if (n === '') return;
    const id = actions.addCustomer(n);
    setAdding(false);
    setName('');
    open(id);
  };

  return (
    <AdminPage
      title="顧客とシート"
      current="customers"
      actions={<Button variant="primary" size="sm" onClick={() => { setAdding(true); }}><IconPlus size={14} />顧客を追加</Button>}
    >
      <div className="mb-3 flex flex-wrap items-center gap-2">
        <Segmented<Filter>
          size="sm"
          value={filter}
          onChange={setFilter}
          options={[{ value: 'all', label: 'すべて' }, { value: 'sheet_only', label: STAGE_LABEL.sheet_only }, { value: 'both', label: STAGE_LABEL.both }, { value: 'portal_only', label: STAGE_LABEL.portal_only }]}
        />
        <span className="relative">
          <IconSearch size={14} className="pointer-events-none absolute top-1/2 left-2.5 -translate-y-1/2 text-pt-faint" />
          <TextInput value={query} onChange={(e) => { setQuery(e.target.value); }} placeholder="顧客名" aria-label="顧客名で探す" className="!w-56 !pl-8 sm:!h-8" />
        </span>
        <span className="pt-num ml-auto text-xs text-pt-muted">{String(shown.length)} 社</span>
      </div>

      <Table minWidth={980}>
        <thead>
          <tr>
            <th className={TH}>顧客</th>
            <th className={TH}>段階</th>
            <th className={TH}>シート</th>
            <th className={TH + ' text-right'}>アカウント</th>
            <th className={TH}>最後に使われた</th>
            <th className={TH + ' w-12'} />
          </tr>
        </thead>
        <tbody>
          {shown.map((c) => {
            const sh = sheetsOf(d, c.id);
            const ready = sh.filter(isReady).length;
            const broken = sh.length - ready;
            const accts = d.accounts.filter((a) => a.customerId === c.id);
            const active = accts.filter((a) => a.status === 'active').length;
            const warn = stageWarning(c.stage, sh);
            return (
              <tr key={c.id} onClick={() => { open(c.id); }} className="group cursor-pointer hover:bg-pt-primary-soft/40">
                <td className={TD}>
                  <span className="font-medium">{c.name}</span>
                  {c.disabled && <span className="ml-2"><Badge tone="neutral">停止中</Badge></span>}
                </td>
                <td className={TD}>
                  <Badge tone={STAGE_TONE[c.stage]}>{STAGE_LABEL[c.stage]}</Badge>
                  {warn && <span className="ml-2 text-xs text-pt-warn">{warn}</span>}
                </td>
                <td className={TD}>
                  {sh.length === 0 ? (
                    <span className="text-pt-faint">なし</span>
                  ) : (
                    <span className="inline-flex items-center gap-3">
                      <span className="pt-num">{String(sh.length)} 枚</span>
                      {ready > 0 && <StatusDot tone="ok"><span className="pt-num">{String(ready)}</span> つながっている</StatusDot>}
                      {broken > 0 && <StatusDot tone="warn"><span className="pt-num">{String(broken)}</span> 要確認</StatusDot>}
                    </span>
                  )}
                </td>
                <td className={TD + ' text-right'}>
                  <span className="pt-num">{String(accts.length)}</span>
                  {active !== accts.length && <span className="pt-num ml-1 text-xs text-pt-faint">（利用中 {String(active)}）</span>}
                </td>
                <td className={TD + ' pt-num text-pt-muted'}>{c.lastUsedAt ? formatShortDateTime(c.lastUsedAt) : <span className="text-pt-faint">まだ使われていない</span>}</td>
                <td className={TD + ' text-right whitespace-nowrap'}>
                  <IconButton
                    label={ready === 0 ? 'つながっているシートがないため、画面は見られない' : 'この顧客の画面を見る'}
                    disabled={ready === 0}
                    className={ready === 0 ? '' : 'opacity-0 group-hover:opacity-100 focus-visible:opacity-100'}
                    onClick={(e) => { e.stopPropagation(); view(c.id); }}
                  >
                    <IconEye size={16} />
                  </IconButton>
                </td>
              </tr>
            );
          })}
          {shown.length === 0 && <tr><td colSpan={6} className="px-3 py-10 text-center text-pt-muted">条件に合う顧客はいません</td></tr>}
        </tbody>
      </Table>

      <Dialog
        open={adding}
        title="顧客を追加"
        width="sm"
        onClose={() => { setAdding(false); }}
        footer={<><Button variant="ghost" onClick={() => { setAdding(false); }}>やめる</Button><Button variant="primary" disabled={name.trim() === ''} onClick={create}>追加</Button></>}
      >
        <Field label="顧客名" hint="追加後に、シートとアカウントを登録します。">
          <TextInput autoFocus value={name} onChange={(e) => { setName(e.target.value); }} onKeyDown={(e) => { if (e.key === 'Enter') create(); }} />
        </Field>
      </Dialog>
    </AdminPage>
  );
}
