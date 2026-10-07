// @vitest-environment happy-dom
import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { useState } from 'react';
import { afterEach, describe, expect, it } from 'vitest';
import { jobs } from './data';
import { BillingImportPanel } from './BillingImportPanel';
import { JobCopyDataImport } from './JobCopyDataImport';
import type { BillingPeriod } from './billingTypes';

afterEach(cleanup);

let lastApplied: BillingPeriod[] = [];
function Harness() {
  const [applied, setApplied] = useState<BillingPeriod[]>([]);
  return <JobCopyDataImport><BillingImportPanel records={jobs} applied={applied} onApply={periods => { lastApplied = periods; setApplied(periods); }} onClear={() => { setApplied([]); }} /></JobCopyDataImport>;
}

function upload(text: string, name = 'billing.csv') {
  const file = new File([new TextEncoder().encode(text)], name, { type: 'text/csv' });
  fireEvent.change(screen.getByLabelText('課金CSVファイル'), { target: { files: [file] } });
}

describe('BillingImportPanel', () => {
  it('walks through choose file → column check → match counts → apply', async () => {
    lastApplied = [];
    render(<Harness />);
    expect(screen.getByText('データ取込')).toBeTruthy();
    expect(screen.getByRole('note').textContent).toContain('ページを再読み込みすると消えます');
    upload('媒体,媒体求人ID,期間開始,期間終了,金額（円・税込）\nHRハッカー,DEMO-HRH-001,2026-09-01,2026-09-14,30000\nAirワーク,DEMO-AIR-002,2026-09-15,2026-09-30,45000\nHRハッカー,NOPE,2026-09-01,2026-09-14,1\nHRハッカー,DEMO-HRH-001,2026-09-01,2026-09-14,30000');
    await screen.findByText('2. 列の対応を確かめる');
    expect(screen.getByLabelText<HTMLSelectElement>('媒体の列').value).toBe('0');
    expect(screen.getByLabelText<HTMLSelectElement>('金額（円）の列').value).toBe('4');
    expect(screen.getByLabelText<HTMLSelectElement>('金額の扱い').value).toBe('税込');
    fireEvent.click(screen.getByRole('button', { name: '求人と照合する' }));
    const counts = within(screen.getByLabelText('照合結果の件数'));
    expect(counts.getByText('一致').nextElementSibling?.textContent).toBe('2行');
    expect(counts.getByText('一覧に無い').nextElementSibling?.textContent).toBe('1行');
    expect(counts.getByText('同じ期間の重複').nextElementSibling?.textContent).toBe('1行');
    expect(counts.getByText('候補が複数').nextElementSibling?.textContent).toBe('0行');
    expect(screen.getByText('一覧に無い求人の行（1行）')).toBeTruthy();
    fireEvent.click(screen.getByRole('button', { name: '一致した2行を課金として反映' }));
    expect(lastApplied.map(period => period.amountYen)).toEqual([30000, 45000]);
    const status = await screen.findByText(/課金CSVの 2 期間を反映中/u);
    expect(status.textContent).toContain('2求人・合計 75,000円');
    expect(status.textContent).toContain('再読み込みすると消えます');
    fireEvent.click(screen.getByRole('button', { name: '反映した課金を外す' }));
    await waitFor(() => { expect(screen.queryByText(/期間を反映中/u)).toBeNull(); });
  });

  it('marks missing required columns in red and blocks matching until chosen', async () => {
    render(<Harness />);
    upload('サービス,原稿番号,開始日,終了日,請求\nHRハッカー,DEMO-HRH-001,2026-09-01,2026-09-14,30000');
    await screen.findByText('2. 列の対応を確かめる');
    expect(screen.getByRole('alert').textContent).toContain('「媒体」の列を選んでください。');
    const mediaRow = screen.getByLabelText('媒体の列').closest('tr');
    expect(mediaRow?.className).toBe('jc-billing-missing');
    const button = screen.getByRole<HTMLButtonElement>('button', { name: '求人と照合する' });
    expect(button.disabled).toBe(true);
    fireEvent.change(screen.getByLabelText('媒体の列'), { target: { value: '0' } });
    fireEvent.change(screen.getByLabelText('媒体求人IDの列'), { target: { value: '1' } });
    fireEvent.change(screen.getByLabelText('金額（円）の列'), { target: { value: '4' } });
    expect(button.disabled).toBe(false);
    fireEvent.click(button);
    expect(within(screen.getByLabelText('照合結果の件数')).getByText('一致').nextElementSibling?.textContent).toBe('1行');
  });

  it('shows a row-numbered reason for rejected rows and keeps 0円 out for an empty amount', async () => {
    render(<Harness />);
    upload('媒体,媒体求人ID,期間開始,期間終了,金額\nHRハッカー,DEMO-HRH-001,2026-09-14,2026-09-01,30000\nHRハッカー,DEMO-HRH-003,2026-09-01,2026-09-14,');
    await screen.findByText('2. 列の対応を確かめる');
    fireEvent.click(screen.getByRole('button', { name: '求人と照合する' }));
    expect(screen.getByText('値に誤りがあり使わない行（1行）').closest('details')?.textContent).toContain('2行目 期間終了が期間開始より前です');
    fireEvent.click(screen.getByRole('button', { name: '一致した1行を課金として反映' }));
    const status = await screen.findByText(/期間を反映中/u);
    expect(status.textContent).toContain('金額はすべて不明');
    expect(status.textContent).not.toContain('0円');
    expect(document.body.textContent).not.toMatch(/効果|確実に|必ず|100%|CP932|fixture|snapshot/u);
  });
});
