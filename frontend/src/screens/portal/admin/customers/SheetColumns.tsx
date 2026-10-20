import { useMemo, useRef, useEffect, useState } from 'react';
import { AdminLayout } from '../../ui/layouts';
import { Badge, Button, Card, Notice, Select, TextInput, cx } from '../../ui/kit';
import { ROUTES, usePreviewNav } from '../../preview';
import { initialRows } from '../../fixtures';
import type { ColumnConfig, ColumnGroup, ColumnKind } from '../../types';
import { GROUP_LABEL, GROUP_ORDER, KIND_LABEL, KIND_ORDER, cardLayout, groupOf, kindOf } from '../../columns';
import { CardBody } from '../../customer/list/ApplicantCard';
import { ShareHint } from './ShareHint';
import { CONNECTION_LABEL, CONNECTION_TONE, actions, useAdminState } from './state';

interface Col extends ColumnConfig {
  /** シートで増えた列（まだ設定していない）。 */
  isNew?: boolean;
  /** シートで見つからなくなった列。 */
  isMissing?: boolean;
}

const nameOf = (c: ColumnConfig) => c.label.trim() || c.header;
const plain = (c: Col): ColumnConfig => ({ header: c.header, label: c.label, visible: c.visible, writable: c.writable, options: c.options, multiline: c.multiline, inList: c.inList, group: groupOf(c), kind: kindOf(c) });

type Role = '' | 'title' | 'status' | 'sort';
const ROLE_LABEL: Record<Role, string> = { '': '（なし）', title: 'カードの見出し', status: '選考状況', sort: '日付で並べる' };

function Check({ on, onChange, label, disabled }: { on: boolean; onChange: (v: boolean) => void; label: string; disabled?: boolean | undefined }) {
  return <input type="checkbox" aria-label={label} checked={on} disabled={disabled} onChange={(e) => { onChange(e.target.checked); }} className="h-4 w-4 disabled:opacity-30" />;
}

/** 見出しが変わったシートの、増えた列と見つからない列を作る（試作の見本）。 */
function withHeaderChanges(cols: ColumnConfig[], changed: boolean): Col[] {
  if (!changed) return cols;
  const missingHeader = cols.find((c) => c.header === '年齢') ? '年齢' : cols[cols.length - 1]?.header;
  const base: Col[] = cols.map((c) => (c.header === missingHeader ? { ...c, isMissing: true } : c));
  return [...base, { header: '応募経路', label: '', visible: false, writable: false, options: null, multiline: false, inList: false, group: 'other', kind: 'text', isNew: true }];
}

export function SheetColumns() {
  const { go } = usePreviewNav();
  const s = useAdminState();
  const sheet = s.sheets.find((x) => x.id === s.sheetId);
  const customer = s.customers.find((c) => c.id === sheet?.customerId);
  const changed = sheet?.connection === 'header_changed';
  const [cols, setCols] = useState<Col[]>(() => withHeaderChanges(sheet?.columns ?? [], changed));
  const [cardTitle, setCardTitle] = useState<string>(sheet?.titleColumn ?? '');
  const [statusCol, setStatusCol] = useState<string>(sheet?.statusColumn ?? '');
  const [sortCol, setSortCol] = useState<string>(sheet?.sortColumn ?? '');
  const [saving, setSaving] = useState(false);
  const [saveState, setSaveState] = useState<'idle' | 'saved' | 'failed'>('idle');
  const [rechecking, setRechecking] = useState(false);
  const timer = useRef<number | undefined>(undefined);
  useEffect(() => () => { window.clearTimeout(timer.current); }, []);

  const nav = (to: 'customers' | 'accounts') => { go(to === 'customers' ? ROUTES.adminCustomers : ROUTES.adminAccounts); };
  const sample = useMemo(() => initialRows.s1?.[3]?.values ?? {}, []);

  if (!sheet) return <AdminLayout title="列の設定" current="customers" onNavigate={nav}><p>シートが選ばれていません。</p></AdminLayout>;

  const touch = () => { setSaveState('idle'); };
  const patch = (i: number, p: Partial<Col>) => {
    touch();
    setCols((cur) => cur.map((c, j) => {
      if (j !== i) return c;
      const next = { ...c, ...p, isNew: false };
      // 見せない列は、書ける・一覧に出す・複数行も外す。
      return next.visible ? next : { ...next, writable: false, inList: false, multiline: false };
    }));
  };
  const move = (i: number, d: -1 | 1) => {
    touch();
    setCols((cur) => {
      const j = i + d;
      const a = cur[i];
      const b = cur[j];
      if (!a || !b) return cur;
      const copy = [...cur];
      copy[i] = b;
      copy[j] = a;
      return copy;
    });
  };
  const dropMissing = (i: number) => { touch(); setCols((cur) => cur.filter((_, j) => j !== i)); };

  const live = cols.filter((c) => !c.isMissing);
  const shown = live.filter((c) => c.visible);
  const listCols = shown.filter((c) => c.inList);
  const missing = cols.filter((c) => c.isMissing);
  const added = cols.filter((c) => c.isNew);
  const titleCol = shown.find((c) => c.header === cardTitle);
  const layout = cardLayout({ columns: live.map(plain), titleColumn: cardTitle, statusColumn: statusCol, sortColumn: sortCol });

  const problems: string[] = [];
  if (shown.length === 0) problems.push('顧客に見せる列がありません。');
  else if (listCols.length === 0) problems.push('一覧に出す列がありません。');
  if (shown.length > 0 && !titleCol) problems.push('スマホのカードの見出しにする列を選んでください。');
  if (missing.length > 0) problems.push('シートで見つからない列が ' + String(missing.length) + ' 列あります。確認して外してください。');

  const savedRoles = [sheet.titleColumn ?? '', sheet.statusColumn ?? '', sheet.sortColumn ?? ''];
  const roleChanges = [cardTitle, statusCol, sortCol].filter((r, i) => r !== savedRoles[i]).length;
  const savedPlain = sheet.columns.map((c) => plain(c));
  const dirty = JSON.stringify(cols.map(plain)) !== JSON.stringify(savedPlain) || roleChanges > 0 || missing.length > 0 || changed;
  const changeCount = cols.filter((c, i) => JSON.stringify(plain(c)) !== JSON.stringify(savedPlain[i])).length + roleChanges;

  const save = () => {
    setSaving(true);
    setSaveState('idle');
    timer.current = window.setTimeout(() => {
      setSaving(false);
      if (cols.some((c) => c.label.includes('失敗'))) { setSaveState('failed'); return; }
      const clean = cols.filter((c) => !c.isMissing).map(plain);
      actions.saveColumns(sheet.id, clean);
      actions.updateSheet(sheet.id, { titleColumn: cardTitle, statusColumn: statusCol, sortColumn: sortCol });
      if (changed) actions.updateSheet(sheet.id, { connection: 'ok' });
      setCols(clean);
      setSaveState('saved');
    }, 600);
  };
  const reset = () => {
    setCols(withHeaderChanges(sheet.columns, changed));
    setCardTitle(sheet.titleColumn ?? '');
    setStatusCol(sheet.statusColumn ?? '');
    setSortCol(sheet.sortColumn ?? '');
    touch();
  };
  const recheck = () => {
    setRechecking(true);
    timer.current = window.setTimeout(() => {
      setRechecking(false);
      if (sheet.connection === 'not_shared') actions.updateSheet(sheet.id, { connection: 'ok' });
    }, 700);
  };

  const val = (c: ColumnConfig) => sample[c.header] ?? '（サンプル）';
  const roleOf = (c: Col): Role => (c.header === cardTitle ? 'title' : c.header === statusCol ? 'status' : c.header === sortCol ? 'sort' : '');
  const setRole = (c: Col, r: Role) => {
    touch();
    if (c.header === cardTitle) setCardTitle('');
    if (c.header === statusCol) setStatusCol('');
    if (c.header === sortCol) setSortCol('');
    if (r === 'title') setCardTitle(c.header);
    if (r === 'status') setStatusCol(c.header);
    if (r === 'sort') setSortCol(c.header);
  };
  const previewValues: Record<string, string> = {};
  for (const c of live) previewValues[c.header] = val(c);

  return (
    <AdminLayout title="列の設定" current="customers" onNavigate={nav}>
      <button type="button" className="mb-3 text-sm text-pt-primary hover:underline" onClick={() => { go('admin-customer-detail'); }}>{customer?.name ?? '顧客'}の詳細に戻る</button>

      <div className="sticky top-0 z-10 -mx-1 mb-4 flex flex-wrap items-center gap-3 rounded-lg border border-pt-line bg-pt-surface px-4 py-2.5 shadow-sm">
        <span className="text-base font-semibold">{sheet.tabName}</span>
        <Badge tone={CONNECTION_TONE[sheet.connection]}>{CONNECTION_LABEL[sheet.connection]}</Badge>
        <span className="ml-auto flex flex-wrap items-center gap-3 text-sm">
          {saveState === 'saved' && <span className="text-pt-ok">保存しました。</span>}
          {saveState === 'failed' && <span className="text-pt-danger">保存できませんでした。もう一度押してください。入力はそのまま残っています。</span>}
          {problems.length > 0 && <span className="text-pt-danger">直すところが {problems.length} 件あります</span>}
          {dirty && saveState !== 'saved' && <span className="text-pt-muted">変更 {changeCount} 件</span>}
          <Button variant="ghost" size="sm" disabled={!dirty || saving} onClick={reset}>取り消し</Button>
          <Button variant="primary" disabled={problems.length > 0 || !dirty || saving} onClick={save}>{saving ? '保存しています' : '保存'}</Button>
        </span>
      </div>

      {sheet.connection === 'not_shared' || sheet.connection === 'not_found' ? (
        <div className="max-w-2xl">
          {sheet.connection === 'not_shared'
            ? <ShareHint onRecheck={recheck} checking={rechecking} />
            : <Notice tone="danger" title="シートが見つかりません" action={<Button size="sm" disabled={rechecking} onClick={recheck}>{rechecking ? '確認しています' : 'つながりを確かめ直す'}</Button>}>URL に誤りがないか、シートが削除されていないか確かめてください。</Notice>}
        </div>
      ) : cols.length === 0 ? (
        <Notice tone="warn" title="列を読み込めません">シートとのつながりを確かめてから、列を設定してください。</Notice>
      ) : (
        <>
          {(missing.length > 0 || added.length > 0) && (
            <div className="mb-4"><Notice tone="warn" title={'シートで増えた列 ' + String(added.length) + '、見つからない列 ' + String(missing.length)}>
              増えた列は、最初は顧客に見せない設定で並べています。見つからない列は、確認して外すまで保存できません。
            </Notice></div>
          )}
          {problems.length > 0 && (
            <ul className="mb-4 list-disc pl-5 text-sm text-pt-danger">{problems.map((p) => <li key={p}>{p}</li>)}</ul>
          )}
          <p className="mb-3 text-xs leading-relaxed text-pt-muted">
            見せる＝顧客の画面に出す。書ける＝顧客が画面で書き換えられる。一覧に出す＝表とカードに出す（出さない列は詳細だけ）。複数行＝長い文章を複数行で入力する。見せない列は、ほかの設定も外れます。順番は表とカードの並びになります。
            まとまり＝詳細のどの見出しに入れるか。種類＝入力欄と表示の形。役割＝カードの見出し、選考状況の絞り込み、新しい順に並べる日付を決める列です。
          </p>

          <div className="mb-5 grid grid-cols-[minmax(0,1fr)_320px] items-start gap-5">
            <Card className="p-4">
              <h2 className="mb-2 text-sm font-semibold">パソコンでの見え方（表の 1 行）</h2>
              <div className="relative">
                <div className="overflow-x-auto rounded border border-pt-line">
                  <table className="w-max min-w-full text-xs">
                    <thead className="bg-pt-bg text-left text-pt-muted"><tr>{listCols.map((c) => <th key={c.header} className="whitespace-nowrap px-2 py-1.5 font-medium">{nameOf(c)}</th>)}</tr></thead>
                    <tbody><tr className="border-t border-pt-line">{listCols.map((c) => (
                      <td key={c.header} className="whitespace-nowrap px-2 py-1.5">
                        {c.writable ? <span className="rounded border border-pt-line-strong px-1.5 py-0.5">{val(c)}</span> : val(c)}
                      </td>
                    ))}</tr></tbody>
                  </table>
                </div>
                <div className="pointer-events-none absolute inset-y-0 right-0 w-10 rounded-r bg-gradient-to-l from-pt-surface to-transparent" />
              </div>
              <p className="mt-1 text-xs text-pt-muted">右に続きます（{listCols.length} 列）。枠で囲んだ値は、顧客が書き換えられます。顧客に見せる列: {shown.length} 列 ／ 書ける列: {shown.filter((c) => c.writable).length} 列</p>
            </Card>
            <Card className="p-4">
              <h2 className="mb-2 text-sm font-semibold">スマホでの見え方（カード 1 枚）</h2>
              <div className="mx-auto w-[280px] rounded-lg border border-pt-line bg-pt-surface p-3 text-left">
                {!layout.title || listCols.length === 0 ? <p className="text-xs text-pt-muted">一覧に出す列がありません。</p> : <CardBody layout={layout} values={previewValues} />}
              </div>
              <p className="mt-2 text-xs text-pt-muted">顧客の一覧と同じ部品で表示しています。見出し・選考状況のバッジ・日付と一覧に出した文字の列・日付と時刻の列（面接日など）の順に出ます。</p>
            </Card>
          </div>

          <Card className="overflow-x-auto">
            <table className="w-full min-w-[1100px] text-sm">
              <thead className="bg-pt-bg text-left text-xs text-pt-muted">
                <tr>
                  <th className="px-2 py-2 font-medium">順</th>
                  <th className="px-2 py-2 font-medium">シートの見出し</th>
                  <th className="px-2 py-2 font-medium">画面での表示名</th>
                  <th className="whitespace-nowrap px-2 py-2 text-center font-medium">見せる</th>
                  <th className="whitespace-nowrap px-2 py-2 text-center font-medium">書ける</th>
                  <th className="whitespace-nowrap px-2 py-2 text-center font-medium">一覧に出す</th>
                  <th className="whitespace-nowrap px-2 py-2 text-center font-medium">複数行</th>
                  <th className="whitespace-nowrap px-2 py-2 font-medium">まとまり</th>
                  <th className="whitespace-nowrap px-2 py-2 font-medium">種類</th>
                  <th className="whitespace-nowrap px-2 py-2 font-medium">役割</th>
                </tr>
              </thead>
              <tbody>
                {cols.map((c, i) => (
                  <tr key={c.header} className={cx('border-t border-pt-line align-top', c.isMissing && 'bg-pt-danger-soft', c.isNew && 'bg-pt-warn-soft')}>
                    <td className="whitespace-nowrap px-2 py-2">
                      <button type="button" aria-label={c.header + 'を上へ'} disabled={i === 0} onClick={() => { move(i, -1); }} className="px-1 text-pt-muted disabled:opacity-30">上</button>
                      <button type="button" aria-label={c.header + 'を下へ'} disabled={i === cols.length - 1} onClick={() => { move(i, 1); }} className="px-1 text-pt-muted disabled:opacity-30">下</button>
                    </td>
                    <td className="px-2 py-2">
                      <div className="flex flex-wrap items-center gap-1.5 font-medium">{c.header}{c.isNew && <Badge tone="new">新しい列</Badge>}</div>
                      {c.options && <div className="mt-0.5 text-xs text-pt-muted">選択肢: {c.options.filter(Boolean).join('、')}</div>}
                      {c.isMissing && (
                        <div className="mt-1 flex items-center gap-2 text-xs text-pt-danger">シートで見つかりません
                          <Button size="sm" variant="danger" onClick={() => { dropMissing(i); }}>この列を外す</Button>
                        </div>
                      )}
                    </td>
                    <td className="px-2 py-2"><TextInput className="h-9 text-sm" disabled={c.isMissing} value={c.label} placeholder={c.header} onChange={(e) => { patch(i, { label: e.target.value }); }} /></td>
                    <td className="px-2 py-3 text-center"><Check on={c.visible} disabled={c.isMissing} label={c.header + 'を見せる'} onChange={(v) => { patch(i, { visible: v }); }} /></td>
                    <td className="px-2 py-3 text-center"><Check on={c.writable} disabled={!c.visible || c.isMissing} label={c.header + 'を書ける'} onChange={(v) => { patch(i, { writable: v }); }} /></td>
                    <td className="px-2 py-3 text-center"><Check on={c.inList} disabled={!c.visible || c.isMissing} label={c.header + 'を一覧に出す'} onChange={(v) => { patch(i, { inList: v }); }} /></td>
                    <td className="px-2 py-3 text-center"><Check on={c.multiline} disabled={!c.visible || c.isMissing} label={c.header + 'を複数行にする'} onChange={(v) => { patch(i, { multiline: v }); }} /></td>
                    <td className="px-2 py-2">
                      <Select aria-label={c.header + 'のまとまり'} className="!h-9 !w-40 text-sm" disabled={!c.visible || c.isMissing} value={groupOf(c)} onChange={(e) => { patch(i, { group: e.target.value as ColumnGroup }); }}>
                        {GROUP_ORDER.map((g) => <option key={g} value={g}>{GROUP_LABEL[g]}</option>)}
                      </Select>
                    </td>
                    <td className="px-2 py-2">
                      <Select aria-label={c.header + 'の種類'} className="!h-9 !w-32 text-sm" disabled={!c.visible || c.isMissing} value={kindOf(c)} onChange={(e) => { patch(i, { kind: e.target.value as ColumnKind }); }}>
                        {KIND_ORDER.map((k) => <option key={k} value={k}>{KIND_LABEL[k]}</option>)}
                      </Select>
                    </td>
                    <td className="px-2 py-2">
                      <Select aria-label={c.header + 'の役割'} className="!h-9 !w-36 text-sm" disabled={!c.visible || c.isMissing} value={roleOf(c)} onChange={(e) => { setRole(c, e.target.value as Role); }}>
                        {(Object.keys(ROLE_LABEL) as Role[]).map((r) => <option key={r || 'none'} value={r}>{ROLE_LABEL[r]}</option>)}
                      </Select>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </Card>
        </>
      )}
    </AdminLayout>
  );
}
