import { describe, expect, it } from 'vitest';
import {
  DEFAULT_FORM,
  MAX_CSV_BYTES,
  PREFECTURE_NOTICE,
  buildFormData,
  validateForm,
  type FormValues,
} from './form';

const file = (name: string, size: number): File => {
  const f = new File(['x'], name, { type: 'text/csv' });
  Object.defineProperty(f, 'size', { value: size });
  return f;
};

const ok = (over: Partial<FormValues> = {}): FormValues => ({
  ...DEFAULT_FORM,
  file: file('jobs.csv', 1000),
  ...over,
});

const PREFS = ['北海道', '大阪府'];

describe('validateForm', () => {
  it('正常な入力はエラー無し', () => {
    expect(validateForm(ok(), PREFS)).toEqual({});
  });

  it('CSV 未選択', () => {
    expect(validateForm({ ...DEFAULT_FORM, file: null }, PREFS).file).toContain('選択');
  });

  it('0 バイトの CSV は弾く (旧画面は required を通ってサーバで 400 になっていた)', () => {
    expect(validateForm(ok({ file: file('a.csv', 0) }), PREFS).file).toContain('空');
  });

  it('20MB ちょうどは通り、1 バイト超は弾く', () => {
    expect(MAX_CSV_BYTES).toBe(20 * 1024 * 1024);
    expect(validateForm(ok({ file: file('a.csv', MAX_CSV_BYTES) }), PREFS).file).toBeUndefined();
    expect(validateForm(ok({ file: file('a.csv', MAX_CSV_BYTES + 1) }), PREFS).file).toContain(
      '20MB',
    );
  });

  it('拡張子は .csv / .txt (大文字小文字不問) だけ', () => {
    expect(validateForm(ok({ file: file('A.CSV', 10) }), PREFS).file).toBeUndefined();
    expect(validateForm(ok({ file: file('a.txt', 10) }), PREFS).file).toBeUndefined();
    expect(validateForm(ok({ file: file('a.xlsx', 10) }), PREFS).file).toContain('.csv');
    expect(validateForm(ok({ file: file('csv', 10) }), PREFS).file).toContain('.csv');
  });

  it.each([
    ['', true],
    ['0', true],
    ['201', true],
    ['4.5', true],
    ['abc', true],
    ['-3', true],
    ['1e2', true],
    ['1', false],
    ['45', false],
    ['200', false],
    [' 45 ', false],
  ])('top_n=%j のエラー有無は %s', (topN, bad) => {
    const e = validateForm(ok({ topN }), PREFS);
    expect(e.topN !== undefined).toBe(bad);
  });

  it('調査名・検索語は 200 文字まで (文字数で数える)', () => {
    expect(validateForm(ok({ surveyTitle: 'あ'.repeat(200) }), PREFS).surveyTitle).toBeUndefined();
    expect(validateForm(ok({ surveyTitle: 'あ'.repeat(201) }), PREFS).surveyTitle).toContain('200');
    expect(validateForm(ok({ searchKeyword: 'a'.repeat(201) }), PREFS).searchKeyword).toContain(
      '200',
    );
  });

  it('都道府県は空 (全国) か選択肢にあるものだけ', () => {
    expect(validateForm(ok({ prefecture: '' }), PREFS).prefecture).toBeUndefined();
    expect(validateForm(ok({ prefecture: '大阪府' }), PREFS).prefecture).toBeUndefined();
    expect(validateForm(ok({ prefecture: '火星' }), PREFS).prefecture).toContain('選択');
  });

  it('選択肢がまだ無い (options 未取得) ときは都道府県を検証しない', () => {
    expect(validateForm(ok({ prefecture: '火星' }), []).prefecture).toBeUndefined();
  });

  it('全国の予告文がある', () => {
    expect(PREFECTURE_NOTICE).toContain('人口');
  });
});

describe('buildFormData', () => {
  it('旧画面と同じフィールド名で送る', () => {
    const f = file('jobs.csv', 5);
    const data = buildFormData(
      ok({
        file: f,
        surveyTitle: '大阪府・施設長',
        marketTitle: '施設長',
        prefecture: '大阪府',
        searchKeyword: '施設長 求人',
        includeGoogle: true,
        sourceType: 'indeed',
        wageMode: 'hourly',
        topN: ' 30 ',
      }),
    );
    expect(data.get('survey_title')).toBe('大阪府・施設長');
    expect(data.get('market_title')).toBe('施設長');
    expect(data.get('prefecture')).toBe('大阪府');
    expect(data.get('search_keyword')).toBe('施設長 求人');
    expect(data.get('include_google')).toBe('1');
    expect(data.get('source_type')).toBe('indeed');
    expect(data.get('wage_mode')).toBe('hourly');
    expect(data.get('top_n')).toBe('30');
    expect((data.get('csv_file') as File).name).toBe('jobs.csv');
  });

  it('Google を使わないときは include_google を送らない (チェックボックスと同じ)', () => {
    const data = buildFormData(ok({ includeGoogle: false }));
    expect(data.has('include_google')).toBe(false);
  });

  it('既定値は旧画面と同じ (SP・月給・45・Google ON)', () => {
    expect(DEFAULT_FORM.sourceType).toBe('indeed_sp');
    expect(DEFAULT_FORM.wageMode).toBe('monthly');
    expect(DEFAULT_FORM.topN).toBe('45');
    expect(DEFAULT_FORM.includeGoogle).toBe(true);
  });
});
