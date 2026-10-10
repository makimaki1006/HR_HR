// @vitest-environment happy-dom
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import fixtures from '../../generated/jobgen/fixtures.json';
import { buildNormalizeRequest, JobgenView } from './JobgenScreen';
import { buildHrhackerCsv } from './csv';
import { ExtractSection } from './sections/ExtractSection';
import { HrhackerSection } from './sections/HrhackerSection';
import { createPipelineController } from './pipeline';
import { initialState, type PipelineState } from './state';
import { createStore } from './store';

describe('顧客資料と確認画面', () => {
  it('Excelから出したShift-JISの給与・職種・改行を保つ', async () => {
    const bytes = readFileSync(resolve('../tests/fixtures/jobgen/customer-sjis.csv'));
    const result = await buildNormalizeRequest({ kind: 'csv', file: new File([bytes], 'customer.csv'), freeText: '', url: '', pasteText: '' });
    expect(result.ok).toBe(true);
    if (!result.ok) return;
    expect(result.body.text).toContain('配送スタッフ');
    expect(result.body.text).toContain('月給300,000円');
    expect(result.body.text).not.toContain('�');
  });

  it('84列CSVは応答のキー順や不足列に左右されず、正本の順序で空欄を出す', () => {
    const csv = buildHrhackerCsv({ '公開': '非公開', '給与補足': '月給250,000円', '案件名': '倉庫スタッフ' });
    expect(csv.split('\r\n')[0]).toBe('\uFEFF' + Object.keys(fixtures.responses.hrhacker.row).join(','));
    expect(csv.split('\r\n')[1]).toContain(',,,倉庫スタッフ,');
    expect(csv).toContain('"月給250,000円"');
  });

  it('未取得の事実は空欄にせず、未知の内部キーも画面に出さない', () => {
    const html = renderToStaticMarkup(<ExtractSection facts={{ required_qualifications: { status: 'missing', value: '', evidence_quote: '' }, private_unknown_key: { status: 'missing', value: '', evidence_quote: '' } }} confirmed={false} onConfirm={() => undefined} />);
    expect(html).toContain('<td>未取得</td>');
    expect(html).not.toContain('private_unknown_key');
    expect(html).not.toContain('リジェクト');
  });

  it('確認表はCSV列名・識別番号を表示せず、確認が必要な生成項目を検証済としない', () => {
    const h = structuredClone(fixtures.responses.hrhacker);
    h.row['求人id'] = 'PRIVATE-001';
    const html = renderToStaticMarkup(<HrhackerSection h={h} confirmed={false} onConfirm={() => undefined} />);
    expect(html).not.toContain('PRIVATE-001');
    expect(html).not.toContain('求人id');
    expect(html).not.toContain('基本給与 最小');
    expect(html).not.toContain('自由項目1の内容');
    expect(html).not.toContain('unsupported_numbers:');
    expect(html).toContain('要確認');
    expect(html).toContain('未取得');
  });

  it('内容が同じ求人が2行あっても選択表示は選んだ1件だけ', () => {
    const state: PipelineState = { ...initialState(), jobs: [{ title_hint: '倉庫', source_text: '月給250000円' }, { title_hint: '倉庫', source_text: '月給250000円' }], selectedJobIndex: 1, titleHint: '倉庫', sourceText: '月給250000円' };
    const store = createStore(state);
    const ctl = createPipelineController({ store, post: () => Promise.reject(new Error('unused')) });
    const html = renderToStaticMarkup(<JobgenView s={state} ctl={ctl} />);
    expect(html.split('aria-pressed="true"')).toHaveLength(2);
    expect(html.split('aria-pressed="false"')).toHaveLength(2);
    expect(html).not.toContain('月給250000円');
  });
});
