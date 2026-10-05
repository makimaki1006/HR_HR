import { describe, expect, it } from 'vitest';
import { toDomesticPhone } from './phone';

describe('toDomesticPhone', () => {
  it.each([
    ['+81312345678', '0312345678'],
    ['+81 3-1234-5678', '0312345678'],
    ['+81 (0)3-1234-5678', '0312345678'],
    ['+819012345678', '09012345678'],
    ['+81-90-1234-5678', '09012345678'],
    ['+8105012345678', '05012345678'],
    ['03-1234-5678', '0312345678'],
    ['(03) 1234 5678', '0312345678'],
    ['0312345678', '0312345678'],
    ['090-1234-5678', '09012345678'],
    ['0120-123-456', '0120123456'],
    ['０３－１２３４－５６７８', '0312345678'],
    ['＋８１３１２３４５６７８', '0312345678'],
    ['  03.1234.5678  ', '0312345678'],
  ])('%s -> %s', (raw, want) => {
    expect(toDomesticPhone(raw)).toBe(want);
  });

  it('returns null for empty values', () => {
    expect(toDomesticPhone(null)).toBeNull();
    expect(toDomesticPhone(undefined)).toBeNull();
    expect(toDomesticPhone('')).toBeNull();
    expect(toDomesticPhone('   ')).toBeNull();
  });

  it('keeps foreign numbers and non-numbers as they are (trimmed)', () => {
    expect(toDomesticPhone('+1 415-555-0100')).toBe('+1 415-555-0100');
    expect(toDomesticPhone(' サンプル（発信不可） ')).toBe('サンプル（発信不可）');
    expect(toDomesticPhone('内線 123')).toBe('内線 123');
    // 国番号なしで 0 以外から始まる数字は日本の番号と断定しない
    expect(toDomesticPhone('81312345678')).toBe('81312345678');
  });

  it('does not rewrite numbers whose length is not a Japanese number', () => {
    expect(toDomesticPhone('03-123')).toBe('03-123');
    expect(toDomesticPhone('0312345678901')).toBe('0312345678901');
    expect(toDomesticPhone('+81 3')).toBe('+81 3');
  });

  it('is idempotent', () => {
    for (const raw of ['+81312345678', '03-1234-5678', '+1 415-555-0100', 'サンプル']) {
      const once = toDomesticPhone(raw);
      expect(toDomesticPhone(once)).toBe(once);
    }
  });
});
