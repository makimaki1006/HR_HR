import { describe, expect, it } from 'vitest';
import {
  buildCommonQuery,
  buildCompetitorsQuery,
  buildGapQuery,
  EMPTY_FORM,
  JOB_TYPES,
  qs,
  validateForm,
  type DiagnosisForm,
} from './query';

const base: DiagnosisForm = {
  ...EMPTY_FORM,
  jobType: '老人福祉・介護',
  prefecture: '東京都',
  prefcode: 13,
};

const decode = (q: string): [string, string][] => [...new URLSearchParams(q).entries()];

describe('qs', () => {
  it('skips null, undefined and empty strings but keeps 0', () => {
    expect(qs([['a', 'x'], ['b', ''], ['c', null], ['d', undefined], ['e', 0]])).toBe('a=x&e=0');
  });
});

describe('common query', () => {
  it('sends job_type / emp_type / prefecture / prefcode and leaves out the empty municipality and citycode', () => {
    expect(decode(buildCommonQuery(base))).toEqual([
      ['job_type', '老人福祉・介護'],
      ['emp_type', '正社員'],
      ['prefecture', '東京都'],
      ['prefcode', '13'],
    ]);
  });

  it('adds municipality and citycode when a city is chosen, in the old parameter order', () => {
    const q = buildCommonQuery({ ...base, empType: 'パート', municipality: '新宿区', citycode: 13104 });
    expect(decode(q)).toEqual([
      ['job_type', '老人福祉・介護'],
      ['emp_type', 'パート'],
      ['prefecture', '東京都'],
      ['municipality', '新宿区'],
      ['prefcode', '13'],
      ['citycode', '13104'],
    ]);
  });

  it('percent-encodes Japanese values like URLSearchParams does', () => {
    expect(buildCommonQuery(base)).toBe(
      'job_type=%E8%80%81%E4%BA%BA%E7%A6%8F%E7%A5%89%E3%83%BB%E4%BB%8B%E8%AD%B7&emp_type=%E6%AD%A3%E7%A4%BE%E5%93%A1&prefecture=%E6%9D%B1%E4%BA%AC%E9%83%BD&prefcode=13',
    );
  });

  it('falls back to 正社員 for an empty emp_type', () => {
    expect(decode(buildCommonQuery({ ...base, empType: '' }))).toContainEqual(['emp_type', '正社員']);
  });

  it('competitors adds limit=100 at the end', () => {
    expect(buildCompetitorsQuery(base).endsWith('&prefcode=13&limit=100')).toBe(true);
  });
});

describe('condition_gap query', () => {
  it('without own conditions it equals the common query', () => {
    expect(buildGapQuery(base)).toBe(buildCommonQuery(base));
  });

  it('converts the monthly salary from man-yen to yen with rounding', () => {
    const q = decode(buildGapQuery({ ...base, ownSalaryMan: '25' }));
    expect(q).toContainEqual(['company_salary_min', '250000']);
    const q2 = decode(buildGapQuery({ ...base, ownSalaryMan: '24.56789' }));
    expect(q2).toContainEqual(['company_salary_min', '245679']);
  });

  it('salary / holidays: blank, negative and non-numeric are not sent; an entered 0 is sent as 0', () => {
    for (const v of ['-3', '', 'abc']) {
      expect(buildGapQuery({ ...base, ownSalaryMan: v })).not.toContain('company_salary_min');
    }
    expect(decode(buildGapQuery({ ...base, ownSalaryMan: '0' }))).toContainEqual(['company_salary_min', '0']);
    for (const v of ['-1', '', 'abc']) {
      expect(buildGapQuery({ ...base, ownHolidays: v })).not.toContain('company_annual_holidays');
    }
    expect(decode(buildGapQuery({ ...base, ownHolidays: '0' }))).toContainEqual(['company_annual_holidays', '0']);
    expect(decode(buildGapQuery({ ...base, ownHolidays: '120' }))).toContainEqual([
      'company_annual_holidays',
      '120',
    ]);
  });

  it('bonus is sent for 0 (boundary, >= 0) and decimals, not for blank or negative', () => {
    expect(decode(buildGapQuery({ ...base, ownBonus: '0' }))).toContainEqual(['company_bonus_months', '0']);
    expect(decode(buildGapQuery({ ...base, ownBonus: '2.5' }))).toContainEqual(['company_bonus_months', '2.5']);
    expect(buildGapQuery({ ...base, ownBonus: '' })).not.toContain('company_bonus_months');
    expect(buildGapQuery({ ...base, ownBonus: '-1' })).not.toContain('company_bonus_months');
  });
});

describe('validateForm', () => {
  it('asks for the industry first, then the prefecture, with on-screen messages', () => {
    expect(validateForm(EMPTY_FORM)).toBe('業種を選択してください');
    expect(validateForm({ ...EMPTY_FORM, jobType: '小売業' })).toBe('都道府県を選択してください');
    expect(validateForm({ ...EMPTY_FORM, jobType: '小売業', prefecture: '東京都', prefcode: null })).toBe(
      '都道府県を選択してください',
    );
    expect(validateForm(base)).toBeNull();
  });
});

describe('fixed lists', () => {
  it('has the 13 industries of the old page', () => {
    expect(JOB_TYPES).toHaveLength(13);
    expect(JOB_TYPES[0]).toBe('老人福祉・介護');
    expect(JOB_TYPES[12]).toBe('その他');
  });
});
