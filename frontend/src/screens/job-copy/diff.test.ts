import { describe, expect, it } from 'vitest';
import { compareCopy, type CopyDiffLine } from './diff';

function reconstruct(lines: CopyDiffLine[], side: 'before' | 'after'): string {
  return lines.filter((line) => line.kind !== (side === 'before' ? 'added' : 'removed')).map((line) => line.text).join('\n');
}

describe('job copy comparison', () => {
  it.each([null, '', ' \t\r\n'])('does not interpret missing received text %j as removal', (after) => {
    expect(compareCopy('現在掲載している求人本文', after)).toEqual({ status: 'unavailable', lines: [] });
  });

  it.each([null, '', '\n  '])('identifies a first complete text when previous content is %j', (before) => {
    expect(compareCopy(before, '仕事内容\n接客')).toEqual({ status: 'initial', lines: [
      { kind: 'added', text: '仕事内容' }, { kind: 'added', text: '接客' },
    ] });
  });

  it('keeps identical content unchanged, including a final newline', () => {
    const text = '仕事内容\n接客\n';
    const comparison = compareCopy(text, text);
    expect(comparison.status).toBe('unchanged');
    expect(comparison.lines.every((line) => line.kind === 'same')).toBe(true);
    expect(reconstruct(comparison.lines, 'after')).toBe(text);
  });

  it('classifies CRLF versus LF as format only and preserves both raw texts', () => {
    const before = '仕事内容\r\n接客\r\n';
    const after = '仕事内容\n接客\n';
    const comparison = compareCopy(before, after);
    expect(comparison.status).toBe('format_only');
    expect(reconstruct(comparison.lines, 'before')).toBe(before);
    expect(reconstruct(comparison.lines, 'after')).toBe(after);
  });

  it.each([
    ['時給1,200円', '時給1,300円'],
    ['残業あり', '残業なし'],
    ['勤務時間 9:00–18:00', '勤務時間 9:00–17:00'],
    ['給与１２万円', '給与12万円'],
    ['月給 200000 円', '月給200000円'],
    ['接客\n', '接客'],
    ['接客\r補助', '接客\n補助'],
  ])('does not normalize away a material or unsupported formatting difference: %s → %s', (before, after) => {
    const comparison = compareCopy(before, after);
    expect(comparison.status).toBe('changed');
    expect(reconstruct(comparison.lines, 'before')).toBe(before);
    expect(reconstruct(comparison.lines, 'after')).toBe(after);
  });

  it('aligns shared content around inserted and removed sections', () => {
    expect(compareCopy('職種\n旧手当\n勤務地\n勤務時間', '職種\n勤務地\n新手当\n勤務時間').lines).toEqual([
      { kind: 'same', text: '職種' },
      { kind: 'removed', text: '旧手当' },
      { kind: 'same', text: '勤務地' },
      { kind: 'added', text: '新手当' },
      { kind: 'same', text: '勤務時間' },
    ]);
  });

  it('preserves repeated lines and their original sequence', () => {
    const before = '募集\n条件\n募集\n条件\n末尾';
    const after = '募集\n募集\n条件\n条件\n末尾';
    const comparison = compareCopy(before, after);
    expect(reconstruct(comparison.lines, 'before')).toBe(before);
    expect(reconstruct(comparison.lines, 'after')).toBe(after);
    expect(comparison.lines.filter((line) => line.kind === 'removed')).toHaveLength(1);
    expect(comparison.lines.filter((line) => line.kind === 'added')).toHaveLength(1);
  });

  it('bounds work on large replacements without losing source lines or shared ends', () => {
    const before = ['共通先頭', ...Array.from({ length: 1500 }, (_, i) => `旧行${String(i)}`), '共通末尾'].join('\n');
    const after = ['共通先頭', ...Array.from({ length: 1500 }, (_, i) => `新行${String(i)}`), '共通末尾'].join('\n');
    const comparison = compareCopy(before, after);
    expect(comparison.status).toBe('changed');
    expect(comparison.lines[0]).toEqual({ kind: 'same', text: '共通先頭' });
    expect(comparison.lines.at(-1)).toEqual({ kind: 'same', text: '共通末尾' });
    expect(reconstruct(comparison.lines, 'before')).toBe(before);
    expect(reconstruct(comparison.lines, 'after')).toBe(after);
  });
});
