import { describe, expect, it } from 'vitest';
import { extractSalary, isSalaryLine, parseSalaryText, salaryLabel, sameSalary } from './salaryExtract';

describe('salary extraction from the body', () => {
  it('reads monthly ranges and hourly pay from the labelled line', () => {
    expect(extractSalary('職種：配送\n給与：月給250,000円〜280,000円\n勤務時間：8:00〜17:00')).toEqual({ kind: '月給', min: 250000, max: 280000, raw: '月給250,000円〜280,000円', inferredKind: false });
    expect(extractSalary('給与：時給1,100円')).toMatchObject({ kind: '時給', min: 1100, max: 1100 });
  });
  it('reads 万円, full-width digits and a label on its own line', () => {
    expect(extractSalary('【給与】\n月給２５万円～２８.５万円')).toMatchObject({ kind: '月給', min: 250000, max: 285000 });
    expect(extractSalary('時給 1200円')).toMatchObject({ kind: '時給', min: 1200, max: 1200 });
  });
  it('reads a bare 7-digit amount as annual pay marked as inferred, and anything else as unknown', () => {
    expect(extractSalary('給与：4000000')).toEqual({ kind: '年収', min: 4000000, max: 4000000, raw: '4000000', inferredKind: true });
    expect(salaryLabel(extractSalary('給与：4000000'))).toBe('年収400万円（推定表記）');
    expect(extractSalary('給与：経験・能力を考慮して決定')).toMatchObject({ kind: '不明', min: null, max: null });
    expect(salaryLabel(extractSalary('給与：経験・能力を考慮して決定'))).toBe('不明');
  });
  it('returns null when the body has no salary line', () => {
    expect(extractSalary('仕事内容：配送\n勤務時間：8:00〜17:00')).toBeNull();
    expect(extractSalary('')).toBeNull();
    expect(salaryLabel(null)).toBe('給与の記載なし');
  });
  it('does not read hours as pay and compares by kind and amounts', () => {
    expect(parseSalaryText('時給1,100円（1日8時間）')).toMatchObject({ min: 1100, max: 1100 });
    expect(sameSalary(extractSalary('給与：月給25万円'), extractSalary('給与：月給250,000円'))).toBe(true);
    expect(sameSalary(extractSalary('給与：月給25万円'), extractSalary('給与：時給1,500円'))).toBe(false);
    expect(salaryLabel(extractSalary('給与：月給250,000円〜280,000円'))).toBe('月給25万〜28万円');
  });
  it('reads a range only from two amounts written next to each other, never upside down', () => {
    expect(parseSalaryText('月給25万円〜 ＋交通費 月2万円まで')).toMatchObject({ kind: '月給', min: 250000, max: 250000 });
    expect(salaryLabel(parseSalaryText('月給25万円〜 ＋交通費 月2万円まで'))).toBe('月給25万円');
    expect(parseSalaryText('時給1,100円〜 ※研修期間3ヶ月は時給1,000円')).toMatchObject({ kind: '時給', min: 1100, max: 1100 });
    expect(parseSalaryText('月給28万円〜25万円')).toMatchObject({ min: 280000, max: 280000 });
    expect(parseSalaryText('月給25万円〜28万円 ＋交通費 月2万円まで')).toMatchObject({ min: 250000, max: 280000 });
    expect(extractSalary('給与：月給25万円〜 ＋交通費 月2万円まで')).toMatchObject({ min: 250000, max: 250000 });
  });
  it('does not mark a salary change when only the allowance after the salary changes', () => {
    expect(sameSalary(extractSalary('給与：月給25万円〜 ＋交通費 月2万円まで'), extractSalary('給与：月給25万円〜 ＋交通費 月3万円まで'))).toBe(true);
  });
  it('tells the salary line from other lines', () => {
    expect(isSalaryLine('給与：月給25万円')).toBe(true);
    expect(isSalaryLine('【給与】')).toBe(true);
    expect(isSalaryLine('時給 1,200円')).toBe(true);
    expect(isSalaryLine('給与体系について')).toBe(false);
    expect(isSalaryLine('仕事内容：配送')).toBe(false);
  });
});
