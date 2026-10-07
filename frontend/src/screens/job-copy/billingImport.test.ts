import { describe, expect, it } from 'vitest';
import { jobs } from './data';
import {
  BillingCsvError, DEFAULT_BILLING_COLUMN_ALIASES, billingDate, billingMappingProblems, buildBillingImport, canonicalMedia,
  decodeBillingCsv, guessBillingColumns, importBillingCsv, parseCsv,
} from './billingImport';
import { billingPeriodsForJob, billingPeriodsFromHrhPerformance } from './billingTypes';

const header = '媒体,媒体求人ID,期間開始,期間終了,金額（円・税込）';
const csv = (...lines: string[]) => [header, ...lines].join('\r\n');
const hex = (value: string) => new Uint8Array(value.match(/../gu)?.map(byte => parseInt(byte, 16)) ?? []);

describe('billing CSV import', () => {
  it('turns two 5-column rows into two periods with amounts 30000 and 45000', () => {
    const result = importBillingCsv(csv('HRハッカー,DEMO-HRH-001,2026-09-01,2026-09-14,30000', 'Airワーク,DEMO-AIR-002,2026/9/15,2026/9/30,"45,000"'), jobs);
    expect(result.counts).toEqual({ dataRows: 2, matched: 2, ambiguous: 0, notFound: 0, rejected: 0, duplicates: 0 });
    expect(result.periods.map(period => [period.jobId, period.periodStart, period.periodEnd, period.amountYen, period.taxBasis, period.sourceRow])).toEqual([
      ['demo-job-001', '2026-09-01', '2026-09-14', 30000, '税込', 2],
      ['demo-job-002', '2026-09-15', '2026-09-30', 45000, '税込', 3],
    ]);
    expect(result.periods.every(period => period.source === 'csv')).toBe(true);
  });

  it('rejects a row whose end is before its start, with the row number', () => {
    const result = importBillingCsv(csv('HRハッカー,DEMO-HRH-001,2026-09-01,2026-09-14,30000', 'HRハッカー,DEMO-HRH-003,2026-09-20,2026-09-10,1000'), jobs);
    expect(result.rejected).toEqual([{ row: 3, message: '期間終了が期間開始より前です' }]);
    expect(result.periods).toHaveLength(1);
  });

  it('keeps an empty amount as null, never 0, and warns', () => {
    const result = importBillingCsv(csv('HRハッカー,DEMO-HRH-001,2026-09-01,2026-09-14,'), jobs);
    expect(result.periods[0]?.amountYen).toBeNull();
    expect(result.periods[0]?.amountYen).not.toBe(0);
    expect(result.warnings).toEqual([{ row: 2, message: '金額が空欄です。0円ではなく「金額不明」として扱います' }]);
  });

  it('reads an Excel (CP932) CSV without garbling 東京', () => {
    const bytes = hex('947d91cc2c947d91cc8b81906c49442c8afa8ad48a4a8e6e2c8afa8ad48f4997b92c8be08a7a8169897e814590c58d9e816a2c83768389839396bc0d0a4852836e8362834a815b2c44454d4f2d4852482d3030312c323032362f392f312c323032362f392f33302c2233302c303030222c938c8b9e0d0a');
    const decoded = decodeBillingCsv(bytes);
    expect(decoded.encoding).toBe('shift_jis');
    expect(decoded.text).not.toContain('�');
    const result = importBillingCsv(decoded.text, jobs);
    expect(result.periods[0]).toMatchObject({ planName: '東京', amountYen: 30000, periodStart: '2026-09-01', periodEnd: '2026-09-30', mediaJobId: 'DEMO-HRH-001' });
  });

  it('reads UTF-8 with a BOM and reports utf-8', () => {
    const bytes = new TextEncoder().encode(`\uFEFF${csv('HRハッカー,DEMO-HRH-001,2026-09-01,2026-09-14,1')}`);
    const decoded = decodeBillingCsv(bytes);
    expect(decoded.encoding).toBe('utf-8');
    expect(decoded.text.startsWith('媒体')).toBe(true);
  });

  it('counts an ID that is not in the list as not found and leaves it out', () => {
    const result = importBillingCsv(csv('HRハッカー,DEMO-HRH-999,2026-09-01,2026-09-14,30000', 'HRハッカー,DEMO-HRH-001,2026-09-01,2026-09-14,30000'), jobs);
    expect(result.counts.notFound).toBe(1);
    expect(result.notFound[0]?.row).toBe(2);
    expect(result.periods.map(period => period.mediaJobId)).toEqual(['DEMO-HRH-001']);
  });

  it('does not match by ID alone when the media differs (exact media + ID)', () => {
    const result = importBillingCsv(csv('Airワーク,DEMO-HRH-001,2026-09-01,2026-09-14,30000'), jobs);
    expect(result.counts).toMatchObject({ matched: 0, notFound: 1 });
  });

  it('warns about a duplicated period and does not add the amounts together', () => {
    const result = importBillingCsv(csv('HRハッカー,DEMO-HRH-001,2026-09-01,2026-09-14,30000', 'HRハッカー,DEMO-HRH-001,2026-09-01,2026-09-14,30000'), jobs);
    expect(result.duplicates).toEqual([{ row: 3, message: '2行目と同じ求人・同じ期間です。合算せず、2行目だけを使います' }]);
    expect(result.periods).toHaveLength(1);
    expect(result.periods.reduce((sum, period) => sum + (period.amountYen ?? 0), 0)).toBe(30000);
  });

  it('keeps overlapping periods separate and records the overlap', () => {
    const result = importBillingCsv(csv('HRハッカー,DEMO-HRH-001,2026-09-01,2026-09-14,30000', 'HRハッカー,DEMO-HRH-001,2026-09-10,2026-09-20,10000'), jobs);
    expect(result.periods.map(period => period.overlapsSourceRows)).toEqual([[3], [2]]);
    expect(result.warnings.map(issue => issue.row)).toEqual([2, 3]);
  });

  it('does not link a row when two records share the same media + ID', () => {
    const base = jobs[0];
    if (!base) throw new Error('demo job missing');
    const records = [base, { ...base, id: 'copy-of-001' }];
    const result = importBillingCsv(csv('HRハッカー,DEMO-HRH-001,2026-09-01,2026-09-14,30000'), records);
    expect(result.counts).toMatchObject({ matched: 0, ambiguous: 1 });
  });

  it('collects several errors on one row with its number', () => {
    const result = importBillingCsv(csv('Indeed,,2026-13-01,2026-09-14,abc'), jobs);
    expect(result.rejected).toHaveLength(1);
    expect(result.rejected[0]?.row).toBe(2);
    expect(result.rejected[0]?.message).toContain('媒体「Indeed」は扱えません');
    expect(result.rejected[0]?.message).toContain('媒体求人IDが空欄です');
    expect(result.rejected[0]?.message).toContain('期間開始は');
    expect(result.rejected[0]?.message).toContain('金額が数として読めません');
  });

  it('uses the optional columns when present', () => {
    const text = '媒体,媒体求人ID,期間開始,期間終了,金額(税抜),プラン名,表示回数,クリック数,媒体応募数\nHRハッカー,DEMO-HRH-001,2026-09-01,2026-09-14,30000,スタンダード,1200,48,3';
    const [period] = importBillingCsv(text, jobs).periods;
    expect(period).toMatchObject({ taxBasis: '税抜', planName: 'スタンダード', impressions: 1200, clicks: 48, mediaApplications: 3 });
  });

  it('lets the column aliases be swapped for a different header set', () => {
    const text = 'サービス,原稿番号,from,to,請求\nAirWork,DEMO-AIR-002,2026-09-01,2026-09-30,8000';
    expect(() => importBillingCsv(text, jobs)).toThrow(BillingCsvError);
    const aliases = { ...DEFAULT_BILLING_COLUMN_ALIASES, media: ['サービス'], mediaJobId: ['原稿番号'], periodStart: ['from'], periodEnd: ['to'], amount: ['請求'] };
    const result = importBillingCsv(text, jobs, aliases);
    expect(result.periods[0]).toMatchObject({ jobId: 'demo-job-002', media: 'Airワーク', amountYen: 8000, taxBasis: '不明' });
  });

  it('accepts a manual column mapping', () => {
    const rows = parseCsv('a,b,c,d,e\nHRハッカー,DEMO-HRH-001,2026-09-01,2026-09-14,500');
    const result = buildBillingImport(rows, { media: 0, mediaJobId: 1, periodStart: 2, periodEnd: 3, amount: 4 }, jobs, '税込');
    expect(result.periods[0]?.amountYen).toBe(500);
  });

  it('reports missing and doubled columns in the mapping', () => {
    expect(billingMappingProblems({ media: 0, mediaJobId: 0, periodStart: 2, periodEnd: 3 }, 5)).toEqual([
      '「媒体」と「媒体求人ID」に同じ列が選ばれています。',
      '「金額（円）」の列を選んでください。',
    ]);
    expect(guessBillingColumns(['金額（円・税込）', '媒体', '媒体求人ID'])).toMatchObject({ amount: 0, media: 1, mediaJobId: 2 });
  });

  it('parses quoted fields, embedded newlines and escaped quotes', () => {
    expect(parseCsv('a,"b,1","c""d"\r\n"x\ny",,z')).toEqual([['a', 'b,1', 'c"d'], ['x\ny', '', 'z']]);
    expect(() => parseCsv('a,"b')).toThrow(BillingCsvError);
  });

  it('normalizes dates and media names', () => {
    expect(billingDate('2026/9/1')).toBe('2026-09-01');
    expect(billingDate('2026-02-30')).toBeNull();
    expect(billingDate('2026-09')).toBeNull();
    expect(canonicalMedia('AirWork')).toBe('Airワーク');
    expect(canonicalMedia('ＨＲハッカー')).toBe('HRハッカー');
    expect(canonicalMedia('Indeed')).toBeNull();
  });
});

describe('billing period helpers', () => {
  it('converts HRH performance rows, keeping a missing cost as null', () => {
    const periods = billingPeriodsFromHrhPerformance('job-x', '12345678', {
      schema_version: 1, source: 'hrhacker', job_id: '12345678', captured_at: '2026-10-01T00:00:00+09:00',
      rows: [{ period_start: '2026-09-01', period_end: '2026-09-07', impressions: 100, clicks: 5, cost_yen: null, applications: 1 }],
    });
    expect(periods).toEqual([{ jobId: 'job-x', media: 'HRハッカー', mediaJobId: '12345678', periodStart: '2026-09-01', periodEnd: '2026-09-07', amountYen: null, taxBasis: '不明', planName: null, impressions: 100, clicks: 5, mediaApplications: 1, source: 'hrh_performance', sourceRow: null, overlapsSourceRows: [] }]);
    expect(billingPeriodsFromHrhPerformance('job-x', '1', undefined)).toEqual([]);
  });

  it('filters and sorts periods per job', () => {
    const result = importBillingCsv(csv('HRハッカー,DEMO-HRH-001,2026-09-15,2026-09-30,2', 'Airワーク,DEMO-AIR-002,2026-09-01,2026-09-30,9', 'HRハッカー,DEMO-HRH-001,2026-09-01,2026-09-14,1'), jobs);
    expect(billingPeriodsForJob(result.periods, 'demo-job-001').map(period => period.amountYen)).toEqual([1, 2]);
  });
});
