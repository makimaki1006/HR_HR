import { describe, expect, it } from 'vitest';
import { fixed, fmtInt, fmtNumber, fmtPct0, fmtSalary } from './format';

// 旧 HTML (Rust の format! / format_number) と同じ文字列になること。
describe('fixed (Rust の {:.N} と同じ丸め: 同点は偶数へ)', () => {
  it.each([
    [0.125, 2, '0.12'],
    [0.375, 2, '0.38'],
    [12.5, 0, '12'],
    [13.5, 0, '14'],
    [2.5, 0, '2'],
    [0.5, 0, '0'],
    [1262.5, 0, '1262'],
    [1263.5, 0, '1264'],
    [30.456, 2, '30.46'],
    [30, 2, '30.00'],
    [-0.4, 0, '-0'],
    [-12.5, 0, '-12'],
  ])('fixed(%s, %s) = %s', (x, d, want) => {
    expect(fixed(x, d)).toBe(want);
  });
});

describe('fmtInt (format_number)', () => {
  it('桁区切り・負数', () => {
    expect(fmtInt(0)).toBe('0');
    expect(fmtInt(999)).toBe('999');
    expect(fmtInt(1000)).toBe('1,000');
    expect(fmtInt(1234567)).toBe('1,234,567');
    expect(fmtInt(-1234)).toBe('-1,234');
  });
});

describe('fmtNumber (旧 number())', () => {
  it('null は — で、0 は 0 (区別する)', () => {
    expect(fmtNumber(null)).toBe('—');
    expect(fmtNumber(0)).toBe('0');
  });
  it('整数は桁区切り、小数は 2 桁', () => {
    expect(fmtNumber(12000)).toBe('12,000');
    expect(fmtNumber(3.456)).toBe('3.46');
    expect(fmtNumber(4.1)).toBe('4.10');
  });
});

describe('fmtSalary', () => {
  it('null は —、0 は 0.00 / 0', () => {
    expect(fmtSalary(null, 2)).toBe('—');
    expect(fmtSalary(0, 2)).toBe('0.00');
    expect(fmtSalary(0, 0)).toBe('0');
  });
  it('小数桁は decimals に従う', () => {
    expect(fmtSalary(25.5, 2)).toBe('25.50');
    expect(fmtSalary(1262, 0)).toBe('1262');
    expect(fmtSalary(-1.234, 2)).toBe('-1.23');
  });
});

describe('fmtPct0', () => {
  it('小数 0 桁 + %', () => {
    expect(fmtPct0(33.333)).toBe('33%');
    expect(fmtPct0(0)).toBe('0%');
    expect(fmtPct0(100)).toBe('100%');
  });
});
