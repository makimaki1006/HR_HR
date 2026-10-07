import { describe, expect, it } from 'vitest';
import { formatPhoneForDisplay, toDomesticPhone } from './phone';

describe('formatPhoneForDisplay', () => {
  it.each([
    // 2 桁・3 桁の市外局番 (先頭だけで桁数が決まるもの)
    ['0300000005', '03-0000-0005'],
    ['+81312345678', '03-1234-5678'],
    ['+81(0)3-0000-0005', '03-0000-0005'],
    ['0612345678', '06-1234-5678'],
    ['0111234567', '011-123-4567'],
    ['045-123-4567', '045-123-4567'],
    ['0521234567', '052-123-4567'],
    // 携帯・IP
    ['+81 90-0000-0003', '090-0000-0003'],
    ['09000000099', '090-0000-0099'],
    ['08012345678', '080-1234-5678'],
    ['07012345678', '070-1234-5678'],
    ['+815012345678', '050-1234-5678'],
    // フリーダイヤル等
    ['0120123456', '0120-123-456'],
    ['0570-000-111', '0570-000-111'],
    ['08001234567', '0800-123-4567'],
  ])('%s -> %s', (raw, want) => {
    expect(formatPhoneForDisplay(raw)).toBe(want);
  });

  it('falls back to the toDomesticPhone digits when the area code length cannot be told from the prefix', () => {
    // 0422 (武蔵野) と 042 (八王子等) のように先頭が同じで桁数が違う局番は区切らない
    expect(formatPhoneForDisplay('0422-12-3456')).toBe('0422123456');
    expect(formatPhoneForDisplay('0466123456')).toBe('0466123456');
    // 050 なのに 10 桁 (桁数が合わない) も区切らない
    expect(formatPhoneForDisplay('+81500000006')).toBe('0500000006');
    // 9 桁
    expect(formatPhoneForDisplay('031234567')).toBe('031234567');
  });

  it('keeps non-Japanese / non-number values and null like toDomesticPhone', () => {
    expect(formatPhoneForDisplay(null)).toBeNull();
    expect(formatPhoneForDisplay('  ')).toBeNull();
    expect(formatPhoneForDisplay('+1 415-555-0100')).toBe('+1 415-555-0100');
    expect(formatPhoneForDisplay(' サンプル ')).toBe('サンプル');
  });

  it('removing the hyphens gives back exactly the toDomesticPhone value (the one used for copy)', () => {
    for (const raw of ['0300000005', '+819012345678', '0120123456', '08001234567', '0466123456']) {
      expect(formatPhoneForDisplay(raw)?.replaceAll('-', '')).toBe(toDomesticPhone(raw));
    }
  });
});

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
