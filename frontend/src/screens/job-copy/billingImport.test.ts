import { describe, expect, it } from 'vitest';
import { jobs } from './data';
import type { JobCopyRecord } from './data';
import {
  BillingCsvError, DEFAULT_BILLING_COLUMN_ALIASES, billingDate, billingMappingProblems, buildBillingImport, canonicalMedia,
  decodeBillingCsv, guessBillingColumns, importBillingCsv, parseCsv,
} from './billingImport';

const header = '媒体,店舗ID,媒体求人ID,期間開始,期間終了,金額（円・税込）';
const csv = (...lines: string[]) => [header, ...lines].join('\r\n');
const hex = (value: string) => new Uint8Array(value.match(/../gu)?.map(byte => parseInt(byte, 16)) ?? []);

// HRハッカーの媒体求人IDは 8 桁（先頭の 0 を含む）。店舗ID（Airワークは口座ログインID）と組で結びつける。
const [job1, job2, job3] = jobs;
if (!job1 || !job2 || !job3) throw new Error('demo jobs missing');
const records: JobCopyRecord[] = [
  { ...job1, mediaJobId: '00123456', accountId: 'SHOP-A' },
  { ...job2, accountId: 'air-login-1' },
  { ...job3, mediaJobId: '00654321', accountId: 'SHOP-A' },
];

describe('billing CSV import', () => {
  it('turns two rows into two periods with amounts 30000 and 45000, matching media + account + job ID', () => {
    const result = importBillingCsv(csv('HRハッカー,SHOP-A,00123456,2026-09-01,2026-09-14,30000', 'Airワーク,air-login-1,DEMO-AIR-002,2026/9/15,2026/9/30,"45,000"'), records);
    expect(result.counts).toEqual({ dataRows: 2, matched: 2, ambiguous: 0, notFound: 0, rejected: 0, duplicates: 0 });
    expect(result.periods.map(period => [period.jobId, period.accountId, period.periodStart, period.periodEnd, period.amountYen, period.taxBasis, period.sourceRow])).toEqual([
      ['demo-job-001', 'SHOP-A', '2026-09-01', '2026-09-14', 30000, '税込', 2],
      ['demo-job-002', 'air-login-1', '2026-09-15', '2026-09-30', 45000, '税込', 3],
    ]);
    expect(result.periods.every(period => period.source === 'csv')).toBe(true);
  });

  it('requires the shop / account login ID column', () => {
    expect(billingMappingProblems(guessBillingColumns(['媒体', '媒体求人ID', '期間開始', '期間終了', '金額']), 5)).toEqual(['「媒体の店舗番号／アカウントのログイン番号」の列を選んでください。']);
    expect(() => importBillingCsv('媒体,媒体求人ID,期間開始,期間終了,金額\nHRハッカー,00123456,2026-09-01,2026-09-14,30000', records)).toThrow(BillingCsvError);
    const blank = importBillingCsv(csv('HRハッカー,,00123456,2026-09-01,2026-09-14,30000', 'Airワーク,,DEMO-AIR-002,2026-09-01,2026-09-14,1'), records);
    expect(blank.rejected).toEqual([{ row: 2, message: '媒体の店舗番号が空欄です' }, { row: 3, message: 'アカウントのログイン番号が空欄です' }]);
  });

  it('never links on the job ID alone: another shop with the same job ID is not this job', () => {
    const result = importBillingCsv(csv('HRハッカー,SHOP-B,00123456,2026-09-01,2026-09-14,30000'), records);
    expect(result.counts).toMatchObject({ matched: 0, notFound: 1 });
    expect(result.notFound[0]?.message).toBe('HRハッカー の求人は一覧にありますが、媒体の店舗番号が違います');
    // 求人側の店舗IDが未取得なら結びつけない
    const noAccount: JobCopyRecord = { ...job1, mediaJobId: '00123456' };
    delete noAccount.accountId;
    const unknownShop = importBillingCsv(csv('HRハッカー,SHOP-A,00123456,2026-09-01,2026-09-14,30000'), [noAccount]);
    expect(unknownShop.counts.matched).toBe(0);
    expect(unknownShop.notFound[0]?.message).toBe('HRハッカー の求人は一覧にありますが、求人の媒体の店舗番号が未取得のため結びつけません');
  });

  it('rejects an HRハッカー job ID that is not 8 digits and hints when leading zeros look stripped', () => {
    const result = importBillingCsv(csv('HRハッカー,SHOP-A,123456,2026-09-01,2026-09-14,30000', 'HRハッカー,SHOP-A,DEMO-1,2026-09-01,2026-09-14,1'), records);
    expect(result.counts.matched).toBe(0);
    expect(result.rejected[0]).toEqual({ row: 2, message: 'HRハッカーの媒体の求人番号は8桁の数字で入力してください。媒体の求人番号の先頭の0が消えている可能性があります。Excel で開くと先頭の0が消えることがあります' });
    expect(result.rejected[1]).toEqual({ row: 3, message: 'HRハッカーの媒体の求人番号は8桁の数字で入力してください' });
  });

  it('keeps leading zeros (compares as text) and hints on a zero-stripped shop ID', () => {
    const zeros = [{ ...job1, mediaJobId: '00123456', accountId: '0042' }];
    expect(importBillingCsv(csv('HRハッカー,0042,00123456,2026-09-01,2026-09-14,30000'), zeros).counts.matched).toBe(1);
    const stripped = importBillingCsv(csv('HRハッカー,42,00123456,2026-09-01,2026-09-14,30000'), zeros);
    expect(stripped.counts.matched).toBe(0);
    expect(stripped.notFound[0]?.message).toBe('HRハッカー の求人は一覧にありますが、媒体の店舗番号が違います。媒体の店舗番号の先頭の0が消えている可能性があります。Excel で開くと先頭の0が消えることがあります');
  });

  it('reads rows whose dates carry a time or are written in Japanese (re-saved in Excel)', () => {
    const result = importBillingCsv(csv('HRハッカー,SHOP-A,00123456,2026/9/1 0:00,2026/9/14 0:00,30000', 'Airワーク,air-login-1,DEMO-AIR-002,2026年9月15日,2026年9月30日,45000'), records);
    expect(result.counts.rejected).toBe(0);
    expect(result.periods.map(period => [period.periodStart, period.periodEnd, period.amountYen])).toEqual([['2026-09-01', '2026-09-14', 30000], ['2026-09-15', '2026-09-30', 45000]]);
  });

  it('rejects a row whose end is before its start, with the row number', () => {
    const result = importBillingCsv(csv('HRハッカー,SHOP-A,00123456,2026-09-01,2026-09-14,30000', 'HRハッカー,SHOP-A,00654321,2026-09-20,2026-09-10,1000'), records);
    expect(result.rejected).toEqual([{ row: 3, message: '期間終了が期間開始より前です' }]);
    expect(result.periods).toHaveLength(1);
  });

  it('rejects a row with more clicks than impressions (same rule as the HRハッカー実績)', () => {
    const text = '媒体,店舗ID,媒体求人ID,期間開始,期間終了,金額,表示回数,クリック数\nHRハッカー,SHOP-A,00123456,2026-09-01,2026-09-14,30000,10,11\nHRハッカー,SHOP-A,00654321,2026-09-01,2026-09-14,30000,10,10';
    const result = importBillingCsv(text, records);
    expect(result.rejected).toEqual([{ row: 2, message: 'クリック数が表示回数より多くなっています' }]);
    expect(result.periods.map(period => period.sourceRow)).toEqual([3]);
  });

  it('keeps an empty amount as null, never 0, and warns', () => {
    const result = importBillingCsv(csv('HRハッカー,SHOP-A,00123456,2026-09-01,2026-09-14,'), records);
    expect(result.periods[0]?.amountYen).toBeNull();
    expect(result.periods[0]?.amountYen).not.toBe(0);
    expect(result.warnings).toEqual([{ row: 2, message: '金額が空欄です。0円ではなく「金額不明」として扱います' }]);
  });

  it('reads an Excel (CP932) CSV without garbling 東京', () => {
    const bytes = hex('947d91cc2c935895dc49442c947d91cc8b81906c49442c8afa8ad48a4a8e6e2c8afa8ad48f4997b92c8be08a7a8169897e814590c58d9e816a2c83768389839396bc0d0a4852836e8362834a815b2c53484f502d412c30303132333435362c323032362f392f312c323032362f392f33302c2233302c303030222c938c8b9e0d0a');
    const decoded = decodeBillingCsv(bytes);
    expect(decoded.encoding).toBe('shift_jis');
    expect(decoded.text).not.toContain('�');
    const result = importBillingCsv(decoded.text, records);
    expect(result.periods[0]).toMatchObject({ planName: '東京', amountYen: 30000, periodStart: '2026-09-01', periodEnd: '2026-09-30', mediaJobId: '00123456', accountId: 'SHOP-A' });
  });

  it('reads UTF-8 with a BOM and reports utf-8', () => {
    const bytes = new TextEncoder().encode(`\uFEFF${csv('HRハッカー,SHOP-A,00123456,2026-09-01,2026-09-14,1')}`);
    const decoded = decodeBillingCsv(bytes);
    expect(decoded.encoding).toBe('utf-8');
    expect(decoded.text.startsWith('媒体')).toBe(true);
  });

  it('counts an ID that is not in the list as not found and leaves it out', () => {
    const result = importBillingCsv(csv('HRハッカー,SHOP-A,00999999,2026-09-01,2026-09-14,30000', 'HRハッカー,SHOP-A,00123456,2026-09-01,2026-09-14,30000'), records);
    expect(result.counts.notFound).toBe(1);
    expect(result.notFound[0]).toEqual({ row: 2, message: 'HRハッカー の条件に合う求人は一覧にありません' });
    expect(result.periods.map(period => period.mediaJobId)).toEqual(['00123456']);
  });

  it('does not match when the media differs (exact media + account + ID)', () => {
    const result = importBillingCsv(csv('Airワーク,SHOP-A,00123456,2026-09-01,2026-09-14,30000'), records);
    expect(result.counts).toMatchObject({ matched: 0, notFound: 1 });
  });

  it('treats an exactly repeated row as one row and never adds the amounts together', () => {
    const result = importBillingCsv(csv('HRハッカー,SHOP-A,00123456,2026-09-01,2026-09-14,30000', 'HRハッカー,SHOP-A,00123456,2026-09-01,2026-09-14,30000'), records);
    expect(result.duplicates).toEqual([{ row: 3, message: '2行目とまったく同じ内容です。1行として扱います' }]);
    expect(result.periods).toHaveLength(1);
    expect(result.periods.reduce((sum, period) => sum + (period.amountYen ?? 0), 0)).toBe(30000);
  });

  it('rejects every row of a period repeated with a different amount (does not keep the first one)', () => {
    const result = importBillingCsv(csv('HRハッカー,SHOP-A,00123456,2026-09-01,2026-09-14,30000', 'HRハッカー,SHOP-A,00123456,2026-09-01,2026-09-14,31000'), records);
    expect(result.periods).toHaveLength(0);
    expect(result.rejected.map(issue => issue.row)).toEqual([2, 3]);
    expect(result.rejected[0]?.message).toBe('2・3行目が同じ求人・同じ期間で、金額などの内容が違います。どの行が正しいか分からないため、どの行も使いません');
  });

  it('rejects overlapping periods of the same job instead of keeping them with a warning', () => {
    const result = importBillingCsv(csv('HRハッカー,SHOP-A,00123456,2026-09-01,2026-09-14,30000', 'HRハッカー,SHOP-A,00123456,2026-09-10,2026-09-20,10000', 'HRハッカー,SHOP-A,00123456,2026-09-21,2026-09-30,5000'), records);
    expect(result.rejected).toEqual([
      { row: 2, message: '3行目と期間が重なっています。重なる行はどれも使いません（期間が重ならないように直してください）' },
      { row: 3, message: '2行目と期間が重なっています。重なる行はどれも使いません（期間が重ならないように直してください）' },
    ]);
    expect(result.periods.map(period => [period.sourceRow, period.amountYen])).toEqual([[4, 5000]]);
    expect(result.warnings).toEqual([]);
  });

  it('does not link a row when two records share the same media + account + ID', () => {
    const base = records[0];
    if (!base) throw new Error('record missing');
    const result = importBillingCsv(csv('HRハッカー,SHOP-A,00123456,2026-09-01,2026-09-14,30000'), [base, { ...base, id: 'copy-of-001' }]);
    expect(result.counts).toMatchObject({ matched: 0, ambiguous: 1 });
  });

  it('collects several errors on one row with its number', () => {
    const result = importBillingCsv(csv('Indeed,X,,2026-13-01,2026-09-14,abc'), records);
    expect(result.rejected).toHaveLength(1);
    expect(result.rejected[0]?.row).toBe(2);
    expect(result.rejected[0]?.message).toContain('媒体「Indeed」は扱えません');
    expect(result.rejected[0]?.message).toContain('媒体の求人番号が空欄です');
    expect(result.rejected[0]?.message).toContain('期間開始は');
    expect(result.rejected[0]?.message).toContain('金額が数として読めません');
  });

  it('uses the optional columns when present', () => {
    const text = '媒体,店舗ID,媒体求人ID,期間開始,期間終了,金額(税抜),プラン名,表示回数,クリック数,媒体応募数\nHRハッカー,SHOP-A,00123456,2026-09-01,2026-09-14,30000,スタンダード,1200,48,3';
    const [period] = importBillingCsv(text, records).periods;
    expect(period).toMatchObject({ taxBasis: '税抜', planName: 'スタンダード', impressions: 1200, clicks: 48, mediaApplications: 3 });
  });

  it('lets the column aliases be swapped for a different header set', () => {
    const text = 'サービス,ログイン,原稿番号,from,to,請求\nAirWork,air-login-1,DEMO-AIR-002,2026-09-01,2026-09-30,8000';
    expect(() => importBillingCsv(text, records)).toThrow(BillingCsvError);
    const aliases = { ...DEFAULT_BILLING_COLUMN_ALIASES, media: ['サービス'], accountId: ['ログイン'], mediaJobId: ['原稿番号'], periodStart: ['from'], periodEnd: ['to'], amount: ['請求'] };
    const result = importBillingCsv(text, records, aliases);
    expect(result.periods[0]).toMatchObject({ jobId: 'demo-job-002', media: 'Airワーク', amountYen: 8000, taxBasis: '不明' });
  });

  it('accepts a manual column mapping', () => {
    const rows = parseCsv('a,s,b,c,d,e\nHRハッカー,SHOP-A,00123456,2026-09-01,2026-09-14,500');
    const result = buildBillingImport(rows, { media: 0, accountId: 1, mediaJobId: 2, periodStart: 3, periodEnd: 4, amount: 5 }, records, '税込');
    expect(result.periods[0]?.amountYen).toBe(500);
  });

  it('reports missing and doubled columns in the mapping', () => {
    expect(billingMappingProblems({ media: 0, accountId: 1, mediaJobId: 0, periodStart: 2, periodEnd: 3 }, 5)).toEqual([
      '「媒体」と「媒体の求人番号」に同じ列が選ばれています。',
      '「金額（円）」の列を選んでください。',
    ]);
    expect(guessBillingColumns(['金額（円・税込）', '媒体', '媒体求人ID', '口座ログインID'])).toMatchObject({ amount: 0, media: 1, mediaJobId: 2, accountId: 3 });
  });

  it('parses quoted fields, embedded newlines and escaped quotes', () => {
    expect(parseCsv('a,"b,1","c""d"\r\n"x\ny",,z')).toEqual([['a', 'b,1', 'c"d'], ['x\ny', '', 'z']]);
    expect(() => parseCsv('a,"b')).toThrow(BillingCsvError);
  });

  it('normalizes dates and media names', () => {
    expect(billingDate('2026/9/1')).toBe('2026-09-01');
    expect(billingDate('2026-02-30')).toBeNull();
    expect(billingDate('2026-09')).toBeNull();
    // Excel で保存し直した CSV は時刻が付くことがある。日本語の日付も読む。
    expect(billingDate('2026/9/1 0:00')).toBe('2026-09-01');
    expect(billingDate('2026/09/30 23:59:59')).toBe('2026-09-30');
    expect(billingDate('2026年9月1日')).toBe('2026-09-01');
    expect(billingDate('２０２６年９月１日')).toBe('2026-09-01');
    expect(billingDate('2026年2月30日')).toBeNull();
    expect(billingDate('2026/9/1 午前')).toBeNull();
    expect(canonicalMedia('AirWork')).toBe('Airワーク');
    expect(canonicalMedia('ＨＲハッカー')).toBe('HRハッカー');
    expect(canonicalMedia('Indeed')).toBeNull();
  });
});
