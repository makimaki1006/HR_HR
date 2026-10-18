import { describe, expect, it } from 'vitest';
import { shortNumber } from './ListingOverview';

describe('ライフラインの1列に収める数', () => {
  it('10万未満はそのまま、10万以上は万・億で縮める', () => {
    expect(shortNumber(null)).toBe('—');
    expect(shortNumber(0, '¥')).toBe('¥0');
    expect(shortNumber(98765)).toBe('98,765');
    expect(shortNumber(123456, '¥')).toBe('¥12.3万');
    expect(shortNumber(1234567)).toBe('123万');
    expect(shortNumber(987654321, '¥')).toBe('¥9.9億');
    expect(shortNumber(-150000, '¥')).toBe('-¥15万');
  });
});
