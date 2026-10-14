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
    // 総務省の番号区画の表による 3〜5 桁の市外局番
    ['0262765000', '026-276-5000'],
    ['0312345678', '03-1234-5678'],
    ['0663312200', '06-6331-2200'],
    ['0185520007', '0185-52-0007'],
    ['0261234567', '0261-23-4567'],
    ['0422123456', '0422-12-3456'],
    ['0466123456', '0466-12-3456'],
    ['0429925000', '04-2992-5000'],
    ['0471234567', '04-7123-4567'],
    ['0126721234', '01267-2-1234'],
    ['0499221234', '04992-2-1234'],
    ['+81262765000', '026-276-5000'],
    ['026-276-5000', '026-276-5000'],
    ['0185-52-0007', '0185-52-0007'],
    ['０２６２７６５０００', '026-276-5000'],
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
    // M2M・特定 IP (020 / 060) と 0990 (ダイヤル Q2)
    ['02012345678', '020-1234-5678'],
    ['06012345678', '060-1234-5678'],
    ['0990123456', '0990-123-456'],
    // 携帯等でない 11 桁は区切らない (3-4-4 にしない)
    ['01234567890', '01234567890'],
  ])('%s -> %s', (raw, want) => {
    expect(formatPhoneForDisplay(raw)).toBe(want);
  });

  it('falls back to the toDomesticPhone digits when the area code is not in the table', () => {
    // 表にない局番 (0100 など未割り当て) は区切らない
    expect(formatPhoneForDisplay('0100123456')).toBe('0100123456');
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
    for (const raw of ['0300000005', '+819012345678', '0120123456', '08001234567', '0466123456', '0262765000', '0185520007']) {
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

describe('toDomesticPhone: values that are not a domestic number are returned unchanged', () => {
  it.each([
    // 区切りを残したまま返る (数字だけにしない) ことで「国内の番号として扱っていない」と分かる
    ['0123-4567', '0123-4567'], // 8 桁 (9〜11 桁の外)
    ['0123-4567-8901', '0123-4567-8901'], // 12 桁
    ['1234-567-8901', '1234-567-8901'], // 0 も + も無い
    ['+81 123-4567', '+81 123-4567'], // +81 の後が 7 桁 (0 を補っても 8 桁)
    ['TEL 03-1234-5678', 'TEL 03-1234-5678'], // 番号以外の文字が入っている
  ])('%s -> %s', (raw, want) => {
    expect(toDomesticPhone(raw)).toBe(want);
  });
  it('the 9 and 11 digit bounds are domestic', () => {
    expect(toDomesticPhone('03-123-4567')).toBe('031234567');
    expect(toDomesticPhone('090-1234-5678')).toBe('09012345678');
  });
});
