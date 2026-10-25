import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import type { PortalAdminStatus } from '../../../generated/PortalAdminStatus';
import { CustomerRows } from './CustomersView';
import { PreviewTable, StatusBanner } from './parts';

const status = (patch: Partial<PortalAdminStatus>): PortalAdminStatus => ({
  store_kind: 'sheet',
  ready: true,
  pending: 0,
  last_error: null,
  loaded_at: '2026-10-26T01:00:00Z',
  last_saved_at: null,
  customer_login: false,
  reader_email: null,
  ...patch,
});

describe('試しに読んだ結果', () => {
  it('見出し・記入例・応募者の行を出し、セルの中のタグは文字として出す', () => {
    const html = renderToStaticMarkup(
      <PreviewTable
        preview={{
          tab_title: '応募者',
          header_row: 1,
          example_rows: 1,
          headers: ['応募日', '', '状況'],
          above: [],
          examples: [['記入例', '', '未連絡']],
          rows: [['2026/10/20', '<script>alert(1)</script>', '=IMPORTRANGE("a","b")']],
          read_at: '2026-10-26T01:00:00Z',
        }}
      />,
    );
    expect(html).toContain('<th class="border-b border-r border-pt-line px-2 py-1.5 align-top whitespace-pre-wrap break-words max-w-[240px] bg-pt-sunken font-medium text-pt-ink">応募日</th>');
    expect(html).toContain('（見出しなし）');
    expect(html).toContain('記入例</td>');
    expect(html).toContain('&lt;script&gt;alert(1)&lt;/script&gt;');
    expect(html).not.toContain('<script>');
    expect(html).toContain('=IMPORTRANGE(&quot;a&quot;,&quot;b&quot;)');
  });
  it('応募者の行が無いときは知らせる', () => {
    const html = renderToStaticMarkup(
      <PreviewTable preview={{ tab_title: 't', header_row: 1, example_rows: 0, headers: ['a'], above: [], examples: [], rows: [], read_at: '2026-10-26T01:00:00Z' }} />,
    );
    expect(html).toContain('見出しの下に応募者の行がありません');
  });
});

describe('保存先の帯', () => {
  it('未設定・読み込み中・保存待ちのときだけ出す', () => {
    expect(renderToStaticMarkup(<StatusBanner status={status({})} />)).toBe('');
    expect(renderToStaticMarkup(<StatusBanner status={status({ store_kind: 'none', ready: false })} />)).toContain('社内用シートがまだ設定されていません');
    expect(renderToStaticMarkup(<StatusBanner status={status({ ready: false, last_error: '社内用シートを開けません' })} />)).toContain('社内用シートを開けません');
    const pending = renderToStaticMarkup(<StatusBanner status={status({ pending: 3, last_error: 'Google につながりません。再試行します' })} />);
    expect(pending).toContain('保存待ちが 3 件');
    expect(pending).toContain('Google につながりません');
  });
});

describe('顧客の一覧', () => {
  it('段階・シート・アカウントの数を出す', () => {
    const html = renderToStaticMarkup(
      <CustomerRows
        customers={[
          { id: 'c-1', name: '架空の運送', hubspot_company_id: null, stage: 'both', disabled: false, created_at: '2026-10-01T00:00:00Z', updated_at: '2026-10-01T00:00:00Z', sheet_count: 2, account_count: 3, active_account_count: 1, last_login_at: null },
          { id: 'c-2', name: '架空の歯科', hubspot_company_id: '1', stage: 'sheet_only', disabled: true, created_at: '2026-10-01T00:00:00Z', updated_at: '2026-10-01T00:00:00Z', sheet_count: 0, account_count: 0, active_account_count: 0, last_login_at: null },
        ]}
      />,
    );
    expect(html).toContain('href="?view=customer&amp;id=c-1"');
    expect(html).toContain('両方');
    expect(html).toContain('2 枚');
    expect(html).toContain('（利用中 1）');
    expect(html).toContain('停止中');
    expect(html).toContain('シートだけ');
  });
});
