import { describe, expect, it } from 'vitest';
import fixtures from '../../generated/jobgen/fixtures.json';
import { buildHrhackerCsv, hrhackerCsvFileName } from './csv';

describe('buildHrhackerCsv', () => {
  it('BOM 付き・ヘッダ 1 行 + データ 1 行・CRLF・カンマ/改行/引用符はダブルクォート', () => {
    const csv = buildHrhackerCsv({ a: '1', 'b,c': 'x"y', d: 'l1\nl2', e: '' });
    expect(csv).toBe('﻿a,"b,c",d,e\r\n1,"x""y","l1\nl2",\r\n');
  });

  it('fixture の 84 列は row のキー順で並ぶ (先頭 求人id、末尾 公開)', () => {
    const row = fixtures.responses.hrhacker.row as Record<string, string>;
    const csv = buildHrhackerCsv(row);
    const lines = csv.split('\r\n');
    expect(lines).toHaveLength(3);
    expect(lines[2]).toBe('');
    const header = (lines[0] ?? '').replace('﻿', '').split(',');
    expect(header).toHaveLength(84);
    expect(header[0]).toBe('求人id');
    expect(header[3]).toBe('案件名');
    expect(header[83]).toBe('公開');
    // データ行: 案件名 (4 列目) は検証済みの生成値、メリット (12 列目) は review_required で空欄。
    const values = lines[1] ?? '';
    expect(values.startsWith(',,,介護職員（特養）／高尾駅徒歩10分・年間休日110日,')).toBe(true);
    expect(values).toContain(',正社員,介護職員（特別養護老人ホーム）,');
    expect(values).not.toContain('120日');
  });

  it('ファイル名は hrhacker_84col_<ms>.csv', () => {
    expect(hrhackerCsvFileName(1_700_000_000_000)).toBe('hrhacker_84col_1700000000000.csv');
  });
});
